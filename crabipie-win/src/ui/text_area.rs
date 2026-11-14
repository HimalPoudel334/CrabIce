//! Multi-line editor (request headers / body). Monospace, so columns are
//! character counts and Up/Down need no measuring.

use super::canvas::{Canvas, TextMeasure, TextStyle};
use super::geom::Rect;
use super::input::{Clipboard, Key, Mods};
use super::text_edit::LineEdit;
use super::theme::Theme;

const PAD: f32 = 10.0;
/// Scrollbar thickness.
const SB: f32 = 10.0;
pub const LINE_H: f32 = 18.0;
const STYLE: TextStyle = TextStyle::new(12.5).mono();

#[derive(Clone, Debug)]
pub struct TextArea {
    pub edit: LineEdit,
    pub focused: bool,
    placeholder: String,
    rect: Rect,
    scroll: f32,
    dragging: bool,
    caret_visible: bool,
    follow: bool, // scroll the caret into view on next paint
    readonly: bool,
    highlight: bool,
    log_mode: bool,
    /// Line byte ranges, rebuilt only when the text changes (a response can be millions of bytes).
    lines: Vec<(usize, usize)>,
    lines_rev: u64,
    /// Longest line in characters (sets the horizontal scroll range).
    max_cols: usize,
    hscroll: f32,
    /// Visible text size, glyph width and scrollbar rects: all set by `paint`.
    view_w: f32,
    view_h: f32,
    gw: f32,
    vtrack: Rect,
    vthumb: Rect,
    htrack: Rect,
    hthumb: Rect,
    /// Scrollbar thumb being dragged: (vertical?, grab offset inside the thumb).
    grab: Option<(bool, f32)>,
}

/// Byte ranges of each line (excluding the '\n').
fn line_ranges(t: &str) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut s = 0;
    for (i, c) in t.char_indices() {
        if c == '\n' {
            v.push((s, i));
            s = i + 1;
        }
    }
    v.push((s, t.len()));
    v
}

/// Index of the line holding byte `idx` (lines are contiguous, so binary search).
fn line_of(r: &[(usize, usize)], idx: usize) -> usize {
    r.partition_point(|&(_, e)| e < idx).min(r.len() - 1)
}

/// Byte length of the first `max_chars` characters of `s`.
fn clip_end(s: &str, max_chars: usize) -> usize {
    s.char_indices().nth(max_chars).map(|(b, _)| b).unwrap_or(s.len())
}

impl TextArea {
    pub fn new(placeholder: &str) -> Self {
        Self {
            edit: LineEdit::multiline(),
            focused: false,
            placeholder: placeholder.to_string(),
            rect: Rect::default(),
            scroll: 0.0,
            dragging: false,
            caret_visible: true,
            follow: false,
            readonly: false,
            highlight: false,
            log_mode: false,
            lines: Vec::new(),
            lines_rev: u64::MAX,
            max_cols: 0,
            hscroll: 0.0,
            view_w: 0.0,
            view_h: 0.0,
            gw: 8.0,
            vtrack: Rect::default(),
            vthumb: Rect::default(),
            htrack: Rect::default(),
            hthumb: Rect::default(),
            grab: None,
        }
    }

    /// Color lines as WebSocket log entries (see `log_color`).
    pub fn log(mut self) -> Self {
        self.log_mode = true;
        self
    }

    pub fn json(mut self) -> Self {
        self.highlight = true;
        self
    }

    pub fn set_highlight(&mut self, on: bool) {
        self.highlight = on;
    }

    /// Selectable and copyable, not editable (response viewer).
    pub fn readonly(placeholder: &str) -> Self {
        Self { readonly: true, ..Self::new(placeholder) }
    }

    /// Replaces the content and scrolls to the top (read-only viewers).
    pub fn set_text(&mut self, s: &str) {
        self.edit.set_text(s);
        self.edit.set_caret(0, false);
        self.scroll = 0.0;
        self.hscroll = 0.0;
    }

    /// Appends text (a streamed response, a live log). The view follows the end
    /// while it is already at the end; if the user scrolled up it stays put.
    pub fn append(&mut self, s: &str) {
        self.ensure_lines();
        let at_end = self.view_h <= 0.0 || self.scroll >= self.max_scroll() - 1.0;
        let last_start = self.lines.last().map_or(0, |l| l.0);
        self.edit.append(s);
        // extend the line index from the old last line instead of rescanning everything
        let text = self.edit.text();
        self.lines.pop();
        for (a, e) in line_ranges(&text[last_start..]) {
            let (a, e) = (last_start + a, last_start + e);
            self.max_cols = self.max_cols.max(text[a..e].chars().count());
            self.lines.push((a, e));
        }
        self.lines_rev = self.edit.rev();
        if at_end {
            self.scroll = f32::MAX; // clamped on the next paint
        }
    }

    fn ensure_lines(&mut self) {
        if self.lines_rev != self.edit.rev() {
            let text = self.edit.text();
            self.lines = line_ranges(text);
            self.max_cols = self.lines.iter().map(|&(s, e)| text[s..e].chars().count()).max().unwrap_or(0);
            self.lines_rev = self.edit.rev();
        }
    }

    fn origin_x(&self) -> f32 {
        self.rect.l + PAD
    }
    pub fn focus(&mut self) {
        self.focused = true;
        self.caret_visible = true;
    }
    pub fn blur(&mut self) {
        self.focused = false;
        self.dragging = false;
        self.grab = None;
    }
    pub fn mouse_up(&mut self) {
        self.dragging = false;
        self.grab = None;
    }

    fn on_bar(&self, x: f32, y: f32) -> bool {
        self.vtrack.contains(x, y) || self.htrack.contains(x, y)
    }

    fn max_hscroll(&self) -> f32 {
        (self.max_cols as f32 * self.gw - self.view_w).max(0.0)
    }
    pub fn toggle_blink(&mut self) -> bool {
        if self.focused {
            self.caret_visible = !self.caret_visible;
        }
        self.focused
    }

    fn index_at(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> usize {
        self.ensure_lines();
        let text = self.edit.text();
        let r = &self.lines;
        let li = (((y - self.rect.t - 6.0 + self.scroll) / LINE_H).floor().max(0.0) as usize).min(r.len() - 1);
        let (s, line_end) = r[li];
        // only the visible columns can be clicked: never measure a multi-megabyte line
        let gw = m.text_width("0", STYLE).max(1.0);
        let start_col = (self.hscroll / gw) as usize;
        let a0 = s + clip_end(&text[s..line_end], start_col);
        let a1 = a0 + clip_end(&text[a0..line_end], (self.rect.width() / gw) as usize + 8);
        let rel = (x - self.origin_x() + self.hscroll - start_col as f32 * gw).max(0.0);
        a0 + m.text_index_at(&text[a0..a1], STYLE, rel)
    }

    pub fn mouse_down(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32, shift: bool) {
        self.focus();
        // scrollbars: grab the thumb, or page toward a click on the track
        if self.vthumb.contains(x, y) {
            self.grab = Some((true, y - self.vthumb.t));
            return;
        }
        if self.hthumb.contains(x, y) {
            self.grab = Some((false, x - self.hthumb.l));
            return;
        }
        if self.vtrack.contains(x, y) {
            let dir = if y < self.vthumb.t { -1.0 } else { 1.0 };
            self.scroll = (self.scroll + dir * self.view_h).clamp(0.0, self.max_scroll());
            return;
        }
        if self.htrack.contains(x, y) {
            let dir = if x < self.hthumb.l { -1.0 } else { 1.0 };
            self.hscroll = (self.hscroll + dir * self.view_w).clamp(0.0, self.max_hscroll());
            return;
        }
        let i = self.index_at(m, x, y);
        self.edit.set_caret(i, shift);
        self.dragging = true;
        self.follow = true;
    }

    pub fn double_click(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) {
        self.focus();
        if self.on_bar(x, y) {
            return;
        }
        let i = self.index_at(m, x, y);
        self.edit.select_word_at(i);
        self.dragging = false;
    }

    pub fn mouse_move(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
        if let Some((vertical, off)) = self.grab {
            let before = (self.scroll, self.hscroll);
            if vertical {
                let room = (self.vtrack.height() - self.vthumb.height()).max(1.0);
                self.scroll = ((y - off - self.vtrack.t) / room).clamp(0.0, 1.0) * self.max_scroll();
            } else {
                let room = (self.htrack.width() - self.hthumb.width()).max(1.0);
                self.hscroll = ((x - off - self.htrack.l) / room).clamp(0.0, 1.0) * self.max_hscroll();
            }
            return before != (self.scroll, self.hscroll);
        }
        if !self.dragging {
            return false;
        }
        let before = (self.edit.caret(), self.edit.selection());
        let i = self.index_at(m, x, y);
        self.edit.set_caret(i, true);
        self.follow = true;
        before != (self.edit.caret(), self.edit.selection())
    }

    pub fn wheel(&mut self, notches: f32) -> bool {
        self.ensure_lines();
        let before = (self.scroll, self.hscroll);
        let max = self.max_scroll();
        if max > 0.0 || self.max_hscroll() <= 0.0 {
            self.scroll = (self.scroll - notches * 3.0 * LINE_H).clamp(0.0, max);
        } else {
            // a single long line (minified JSON): the wheel scrolls sideways
            self.hscroll = (self.hscroll - notches * 8.0 * self.gw).clamp(0.0, self.max_hscroll());
        }
        before != (self.scroll, self.hscroll)
    }

    fn max_scroll(&self) -> f32 {
        let view = if self.view_h > 0.0 { self.view_h } else { self.rect.height() - 12.0 };
        (self.lines.len() as f32 * LINE_H - view).max(0.0)
    }

    pub fn char(&mut self, ch: char) -> bool {
        if !self.focused || self.readonly || ch.is_control() {
            return false;
        }
        let mut b = [0u8; 4];
        self.caret_visible = true;
        self.follow = true;
        self.edit.insert(ch.encode_utf8(&mut b))
    }

    /// Move the caret one line up/down, keeping the column (monospace).
    fn move_vertical(&mut self, down: bool, extend: bool) {
        self.ensure_lines();
        let text = self.edit.text();
        let r = &self.lines;
        let li = line_of(r, self.edit.caret());
        let target = if down { li + 1 } else { li.wrapping_sub(1) };
        if target >= r.len() {
            // first/last line: Up goes to start, Down to end (like most editors)
            if down { self.edit.end(extend) } else { self.edit.home(extend) }
            return;
        }
        let col = text[r[li].0..self.edit.caret()].chars().count();
        let (s, e) = r[target];
        let off = text[s..e].char_indices().nth(col).map(|(b, _)| b).unwrap_or(e - s);
        self.edit.set_caret(s + off, extend);
    }

    pub fn key(&mut self, key: Key, mods: Mods, cb: &mut dyn Clipboard) -> bool {
        if !self.focused {
            return false;
        }
        self.ensure_lines();
        let navigation = matches!(
            key,
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End | Key::Letter('A') | Key::Letter('C')
        );
        if self.readonly && !navigation {
            return false;
        }
        let (ctrl, shift) = (mods.ctrl, mods.shift);
        let handled = match key {
            Key::Left => { self.edit.move_left(shift, ctrl); true }
            Key::Right => { self.edit.move_right(shift, ctrl); true }
            Key::Up => { self.move_vertical(false, shift); true }
            Key::Down => { self.move_vertical(true, shift); true }
            Key::Home | Key::End => {
                let text = self.edit.text();
                let r = &self.lines;
                let (s, e) = r[line_of(r, self.edit.caret())];
                let to = match (key, ctrl) {
                    (Key::Home, true) => 0,
                    (Key::Home, false) => s,
                    (_, true) => text.len(),
                    _ => e,
                };
                self.edit.set_caret(to, shift);
                true
            }
            Key::Enter => { self.edit.insert("\n"); true }
            Key::Tab => { self.edit.insert("  "); true }
            Key::Backspace => { self.edit.backspace(ctrl); true }
            Key::Delete => { self.edit.delete(ctrl); true }
            Key::Letter('A') if ctrl => { self.edit.select_all(); true }
            Key::Letter('C') if ctrl => {
                if let Some(t) = self.edit.copy() { cb.set_text(&t); }
                true
            }
            Key::Letter('X') if ctrl => {
                if let Some(t) = self.edit.cut() { cb.set_text(&t); }
                true
            }
            Key::Letter('V') if ctrl => {
                if let Some(t) = cb.get_text() { self.edit.insert(&t); }
                true
            }
            Key::Letter('Z') if ctrl => { if shift { self.edit.redo() } else { self.edit.undo() }; true }
            Key::Letter('Y') if ctrl => { self.edit.redo(); true }
            _ => false,
        };
        if handled {
            self.caret_visible = true;
            self.follow = true;
        }
        handled
    }

    pub fn paint(&mut self, c: &mut dyn Canvas, theme: &Theme, rect: Rect) {
        self.rect = rect;
        self.ensure_lines();
        let gw = c.text_width("0", STYLE).max(1.0);
        self.gw = gw;
        // which scrollbars are needed (each one takes room from the other axis)
        let total_h = self.lines.len() as f32 * LINE_H;
        let total_w = self.max_cols as f32 * gw;
        let (mut vbar, mut hbar) = (false, false);
        for _ in 0..2 {
            vbar = total_h > rect.height() - 12.0 - if hbar { SB } else { 0.0 };
            hbar = total_w > rect.width() - 2.0 * PAD - if vbar { SB } else { 0.0 };
        }
        let h = rect.height() - 12.0 - if hbar { SB } else { 0.0 };
        let view_w = rect.width() - 2.0 * PAD - if vbar { SB } else { 0.0 };
        self.view_h = h;
        self.view_w = view_w;

        let text = self.edit.text();
        let r = &self.lines;
        let caret = self.edit.caret();
        let cl = line_of(r, caret);
        if self.follow {
            let top = cl as f32 * LINE_H;
            if top < self.scroll {
                self.scroll = top;
            } else if top + LINE_H > self.scroll + h {
                self.scroll = top + LINE_H - h;
            }
            let col = text[r[cl].0..caret].chars().count() as f32 * gw;
            if col < self.hscroll {
                self.hscroll = col;
            } else if col + gw > self.hscroll + view_w {
                self.hscroll = col + gw - view_w;
            }
            self.follow = false;
        }
        self.scroll = self.scroll.clamp(0.0, (total_h - h).max(0.0));
        self.hscroll = self.hscroll.clamp(0.0, (total_w - view_w).max(0.0));

        c.fill_rect(rect, theme.bar_bg);
        c.stroke_rect(rect.inset(0.5), if self.focused { theme.accent } else { theme.border }, 1.0);
        let clip = Rect::new(rect.l + 1.0, rect.t + 1.0, rect.r - 1.0 - if vbar { SB } else { 0.0 }, rect.b - 1.0 - if hbar { SB } else { 0.0 });
        c.push_clip(clip);
        let top = rect.t + 6.0 - self.scroll;
        if text.is_empty() {
            c.text(&self.placeholder, Rect::new(self.origin_x(), rect.t + 6.0, rect.r - PAD, rect.t + 6.0 + LINE_H), theme.text_dim, STYLE);
        }
        let sel = self.edit.selection();
        // Only the visible columns of each line are measured, tokenized and drawn
        // (columns assume a monospace font), so a multi-megabyte line costs no more than a short one.
        let start_col = (self.hscroll / gw) as usize;
        let xo = self.origin_x() - (self.hscroll - start_col as f32 * gw);
        let max_cols = (view_w / gw) as usize + 8;
        let visible = |s: usize, line_end: usize| {
            let a0 = s + clip_end(&text[s..line_end], start_col);
            (a0, a0 + clip_end(&text[a0..line_end], max_cols))
        };
        let first = (self.scroll / LINE_H) as usize;
        let n = (h / LINE_H) as usize + 2;
        for li in first..(first + n).min(r.len()) {
            let (s, line_end) = r[li];
            let (a0, a1) = visible(s, line_end);
            let y = top + li as f32 * LINE_H;
            let line = &text[a0..a1];
            if let Some((a, b)) = sel {
                if a <= line_end && b > s {
                    let xa = xo + c.text_width(&text[a0..a.clamp(a0, a1)], STYLE);
                    let mut xb = xo + c.text_width(&text[a0..b.clamp(a0, a1)], STYLE);
                    if b > line_end {
                        xb += 6.0; // highlights the newline too
                    }
                    c.fill_rect(Rect::new(xa, y, xb, y + LINE_H), theme.accent.alpha(0.35));
                }
            }
            let colored: Option<Vec<(usize, usize, super::geom::Color)>> = if line.is_empty() {
                None
            } else if self.log_mode {
                Some(super::log_color::tokens(line, theme))
            } else if self.highlight {
                Some(super::json_color::tokenize(line).into_iter().map(|(a, b, t)| (a, b, super::json_color::color(t))).collect())
            } else {
                None
            };
            if let Some(toks) = colored {
                let (mut cols, mut at) = (0usize, 0usize); // running column: never recount from the line start
                for (a, b, col) in toks {
                    cols += line[at..a].chars().count();
                    let len = line[a..b].chars().count();
                    let xx = xo + gw * cols as f32;
                    c.text(&line[a..b], Rect::new(xx, y, xx + gw * len as f32 + 6.0, y + LINE_H), col, STYLE);
                    cols += len;
                    at = b;
                }
            } else if !line.is_empty() {
                c.text(line, Rect::new(xo, y, xo + gw * (max_cols as f32 + 2.0), y + LINE_H), theme.text, STYLE);
            }
        }
        if self.focused && !self.readonly && self.caret_visible && sel.is_none() {
            let (a0, a1) = visible(r[cl].0, r[cl].1);
            if caret >= a0 && caret <= a1 {
                let x = xo + c.text_width(&text[a0..caret], STYLE);
                let y = top + cl as f32 * LINE_H;
                c.line((x, y + 2.0), (x, y + LINE_H - 2.0), theme.text, 1.2);
            }
        }
        c.pop_clip();

        // scrollbars
        let dragging = self.grab.is_some();
        let thumb_col = theme.text_dim.alpha(if dragging { 0.7 } else { 0.4 });
        self.vtrack = Rect::default();
        self.vthumb = Rect::default();
        self.htrack = Rect::default();
        self.hthumb = Rect::default();
        if vbar {
            let tr = Rect::new(rect.r - SB - 2.0, rect.t + 2.0, rect.r - 2.0, rect.b - 2.0 - if hbar { SB } else { 0.0 });
            let len = (tr.height() * h / total_h).clamp(24.0, tr.height());
            let max = (total_h - h).max(1.0);
            let y = tr.t + (self.scroll / max) * (tr.height() - len);
            self.vtrack = tr;
            self.vthumb = Rect::new(tr.l + 2.0, y, tr.r - 2.0, y + len);
            c.fill_rect(self.vthumb, thumb_col);
        }
        if hbar {
            let tr = Rect::new(rect.l + 2.0, rect.b - SB - 2.0, rect.r - 2.0 - if vbar { SB } else { 0.0 }, rect.b - 2.0);
            let len = (tr.width() * view_w / total_w).clamp(24.0, tr.width());
            let max = (total_w - view_w).max(1.0);
            let x = tr.l + (self.hscroll / max) * (tr.width() - len);
            self.htrack = tr;
            self.hthumb = Rect::new(x, tr.t + 2.0, x + len, tr.b - 2.0);
            c.fill_rect(self.hthumb, thumb_col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{MemClipboard, MonoCanvas, GLYPH};

    #[test]
    fn enter_up_down_and_click_work_across_lines() {
        let mut a = TextArea::new("");
        let mut c = MonoCanvas::default();
        let mut cb = MemClipboard::default();
        a.edit.set_text("hello\nhi\nworld");
        a.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 300.0, 100.0));
        a.focus();
        a.edit.end(false); // end of "world" (col 5)
        a.key(Key::Up, Mods::default(), &mut cb); // "hi" is shorter: clamps to its end
        assert_eq!(a.edit.caret(), "hello\nhi".len());
        a.key(Key::Up, Mods::default(), &mut cb);
        assert_eq!(a.edit.caret(), 2); // goal column was lost at "hi": col 2 on "hello"
        a.key(Key::Enter, Mods::default(), &mut cb);
        assert_eq!(a.edit.text(), "he\nllo\nhi\nworld");
        // click on third line (y = top pad 6 + 2*18 + a bit), col 1
        a.mouse_down(&mut c, PAD + GLYPH, 6.0 + LINE_H * 2.0 + 4.0, false);
        assert_eq!(a.edit.caret(), "he\nllo\n".len() + 1);
    }

    #[test]
    fn line_cache_follows_edits_and_huge_lines_paint_only_what_is_visible() {
        let mut a = TextArea::new("");
        let mut c = MonoCanvas::default();
        let mut cb = MemClipboard::default();
        a.edit.set_text("a\nb");
        a.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 300.0, 100.0));
        assert_eq!(a.lines.len(), 2);
        a.focus();
        a.key(Key::Enter, Mods::default(), &mut cb);
        a.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 300.0, 100.0));
        assert_eq!(a.lines.len(), 3);

        // a minified 3 MB response on one line used to hang the painter (quadratic column counting)
        let mut v = TextArea::readonly("").json();
        v.edit.set_text(&"{\"k\":12,\"s\":\"x\"},".repeat(150_000));
        v.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 600.0, 300.0));
        v.mouse_down(&mut c, 100.0, 10.0, false);
        v.edit.select_all();
        v.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 600.0, 300.0));
    }

    #[test]
    fn scrollbars_appear_and_drag() {
        let mut c = MonoCanvas::default();
        let th = Theme::default();
        let rect = Rect::new(0.0, 0.0, 300.0, 120.0);
        let mut v = TextArea::readonly("");
        v.edit.set_text(&"line\n".repeat(200));
        v.paint(&mut c, &th, rect);
        assert!(v.vthumb.height() > 0.0, "tall text gets a vertical bar");
        assert_eq!(v.hthumb.width(), 0.0, "short lines need no horizontal bar");

        // drag the thumb to the bottom
        let (x, y) = (v.vthumb.l + 2.0, v.vthumb.t + 2.0);
        v.mouse_down(&mut c, x, y, false);
        assert!(v.grab.is_some());
        v.mouse_move(&mut c, x, 10_000.0);
        v.mouse_up();
        v.paint(&mut c, &th, rect);
        assert!((v.scroll - v.max_scroll()).abs() < 0.5);
        assert!(v.grab.is_none());

        // one very long line: horizontal bar, and the wheel scrolls sideways
        let mut w = TextArea::readonly("");
        w.edit.set_text(&"x".repeat(5_000));
        w.paint(&mut c, &th, rect);
        assert!(w.hthumb.width() > 0.0 && w.vthumb.height() == 0.0);
        assert!(w.wheel(-1.0));
        assert!(w.hscroll > 0.0);
    }

    #[test]
    fn append_follows_the_end_only_while_the_view_is_at_the_end() {
        let mut c = MonoCanvas::default();
        let th = Theme::default();
        let rect = Rect::new(0.0, 0.0, 300.0, 120.0);
        let mut a = TextArea::readonly("");
        a.paint(&mut c, &th, rect);
        for i in 0..100 {
            a.append(&format!("line {i}\n"));
        }
        a.paint(&mut c, &th, rect);
        assert_eq!(a.lines.len(), 101);
        assert!((a.scroll - a.max_scroll()).abs() < 0.5, "followed the stream to the bottom");

        // the user scrolls up: new data must not yank the view back
        assert!(a.wheel(5.0));
        a.paint(&mut c, &th, rect);
        let held = a.scroll;
        a.append("more\nlines\n");
        a.paint(&mut c, &th, rect);
        assert_eq!(a.scroll, held);

        // back at the bottom: following resumes
        a.scroll = a.max_scroll();
        a.append("tail\n");
        a.paint(&mut c, &th, rect);
        assert!((a.scroll - a.max_scroll()).abs() < 0.5);
        assert_eq!(a.lines.len(), 104);
    }
}
