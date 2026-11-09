//! Multi-line editor (request headers / body). Monospace, so columns are
//! character counts and Up/Down need no measuring.

use super::canvas::{Canvas, TextMeasure, TextStyle};
use super::geom::Rect;
use super::input::{Clipboard, Key, Mods};
use super::text_edit::LineEdit;
use super::theme::Theme;

const PAD: f32 = 10.0;
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

fn line_of(r: &[(usize, usize)], idx: usize) -> usize {
    r.iter().position(|&(s, e)| idx >= s && idx <= e).unwrap_or(r.len() - 1)
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
        }
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
    }

    pub fn append(&mut self, s: &str) {
        self.edit.append(s);
    }

    pub fn line_count(&self) -> usize {
        line_ranges(self.edit.text()).len()
    }

    pub fn line(&self, i: usize) -> Option<&str> {
        let t = self.edit.text();
        line_ranges(t).get(i).map(|&(s, e)| &t[s..e])
    }

    fn origin_x(&self) -> f32 {
        self.rect.l + PAD
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.rect.contains(x, y)
    }
    pub fn focus(&mut self) {
        self.focused = true;
        self.caret_visible = true;
    }
    pub fn blur(&mut self) {
        self.focused = false;
        self.dragging = false;
    }
    pub fn mouse_up(&mut self) {
        self.dragging = false;
    }
    pub fn toggle_blink(&mut self) -> bool {
        if self.focused {
            self.caret_visible = !self.caret_visible;
        }
        self.focused
    }

    fn index_at(&self, m: &mut dyn TextMeasure, x: f32, y: f32) -> usize {
        let text = self.edit.text();
        let r = line_ranges(text);
        let li = (((y - self.rect.t - 6.0 + self.scroll) / LINE_H).floor().max(0.0) as usize).min(r.len() - 1);
        let (s, e) = r[li];
        s + m.text_index_at(&text[s..e], STYLE, (x - self.origin_x()).max(0.0))
    }

    pub fn mouse_down(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32, shift: bool) {
        self.focus();
        let i = self.index_at(m, x, y);
        self.edit.set_caret(i, shift);
        self.dragging = true;
        self.follow = true;
    }

    pub fn double_click(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) {
        self.focus();
        let i = self.index_at(m, x, y);
        self.edit.select_word_at(i);
        self.dragging = false;
    }

    pub fn mouse_move(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
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
        let before = self.scroll;
        let max = self.max_scroll();
        self.scroll = (self.scroll - notches * 3.0 * LINE_H).clamp(0.0, max);
        before != self.scroll
    }

    fn max_scroll(&self) -> f32 {
        let n = line_ranges(self.edit.text()).len() as f32;
        (n * LINE_H - (self.rect.height() - 12.0)).max(0.0)
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
        let text = self.edit.text();
        let r = line_ranges(text);
        let li = line_of(&r, self.edit.caret());
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
                let r = line_ranges(text);
                let (s, e) = r[line_of(&r, self.edit.caret())];
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
        let text = self.edit.text().to_string();
        let r = line_ranges(&text);
        let h = rect.height() - 12.0;
        let caret = self.edit.caret();
        let cl = line_of(&r, caret);
        if self.follow {
            let top = cl as f32 * LINE_H;
            if top < self.scroll {
                self.scroll = top;
            } else if top + LINE_H > self.scroll + h {
                self.scroll = top + LINE_H - h;
            }
            self.follow = false;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());

        c.fill_rect(rect, theme.bar_bg);
        c.stroke_rect(rect.inset(0.5), if self.focused { theme.accent } else { theme.border }, 1.0);
        c.push_clip(rect);
        let (x0, x1) = (self.origin_x(), rect.r - PAD);
        let top = rect.t + 6.0 - self.scroll;
        if text.is_empty() {
            c.text(&self.placeholder, Rect::new(x0, rect.t + 6.0, x1, rect.t + 6.0 + LINE_H), theme.text_dim, STYLE);
        }
        let sel = self.edit.selection();
        let first = (self.scroll / LINE_H) as usize;
        let n = (h / LINE_H) as usize + 2;
        for li in first..(first + n).min(r.len()) {
            let (s, e) = r[li];
            let y = top + li as f32 * LINE_H;
            let line = &text[s..e];
            if let Some((a, b)) = sel {
                if a <= e && b > s {
                    let xa = x0 + c.text_width(&line[..a.max(s) - s], STYLE);
                    let mut xb = x0 + c.text_width(&line[..b.min(e) - s], STYLE);
                    if b > e {
                        xb += 6.0; // highlights the newline too
                    }
                    c.fill_rect(Rect::new(xa, y, xb, y + LINE_H), theme.accent.alpha(0.35));
                }
            }
            if !line.is_empty() {
                c.text(line, Rect::new(x0, y, x1, y + LINE_H), theme.text, STYLE);
            }
        }
        if self.focused && !self.readonly && self.caret_visible && sel.is_none() {
            let x = x0 + c.text_width(&text[r[cl].0..caret], STYLE);
            let y = top + cl as f32 * LINE_H;
            c.line((x, y + 2.0), (x, y + LINE_H - 2.0), theme.text, 1.2);
        }
        c.pop_clip();
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
}
