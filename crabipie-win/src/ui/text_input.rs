//! Single-line text input widget (URL bar, header values, env fields...).
//! Toolkit-free: measures and draws through `TextMeasure`/`Canvas`.

use super::canvas::{Canvas, TextMeasure, TextStyle};
use super::geom::Rect;
use super::input::{Clipboard, Key, Mods};
use super::text_edit::LineEdit;
use super::theme::Theme;

pub const TEXT: TextStyle = TextStyle::new(14.0);
const PAD: f32 = 10.0;

#[derive(Clone, Debug)]
pub struct TextInput {
    pub edit: LineEdit,
    pub focused: bool,
    placeholder: String,
    rect: Rect,
    scroll_x: f32,
    dragging: bool,
    caret_visible: bool,
}

impl TextInput {
    pub fn new(placeholder: &str) -> Self {
        Self {
            edit: LineEdit::new(),
            focused: false,
            placeholder: placeholder.to_string(),
            rect: Rect::default(),
            scroll_x: 0.0,
            dragging: false,
            caret_visible: true,
        }
    }

    fn area(&self) -> Rect {
        Rect::new(self.rect.l + PAD, self.rect.t, self.rect.r - PAD, self.rect.b)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.rect.contains(x, y)
    }

    pub fn blur(&mut self) {
        self.focused = false;
        self.dragging = false;
    }

    pub fn focus(&mut self) {
        self.focused = true;
        self.caret_visible = true;
    }

    /// Caret blink tick. Returns true when a repaint is needed.
    pub fn toggle_blink(&mut self) -> bool {
        if self.focused {
            self.caret_visible = !self.caret_visible;
            true
        } else {
            false
        }
    }

    fn index_at(&self, m: &mut dyn TextMeasure, x: f32) -> usize {
        let rel = (x - self.area().l + self.scroll_x).max(0.0);
        m.text_index_at(self.edit.text(), TEXT, rel)
    }

    // ── mouse ───────────────────────────────────────────────────────

    pub fn mouse_down(&mut self, m: &mut dyn TextMeasure, x: f32, shift: bool) {
        self.focus();
        let idx = self.index_at(m, x);
        self.edit.set_caret(idx, shift);
        self.dragging = true;
    }

    pub fn double_click(&mut self, m: &mut dyn TextMeasure, x: f32) {
        self.focus();
        let idx = self.index_at(m, x);
        self.edit.select_word_at(idx);
        self.dragging = false;
    }

    /// Returns true if the selection changed (repaint).
    pub fn mouse_move(&mut self, m: &mut dyn TextMeasure, x: f32) -> bool {
        if !self.dragging {
            return false;
        }
        let before = (self.edit.caret(), self.edit.selection());
        let idx = self.index_at(m, x);
        self.edit.set_caret(idx, true);
        before != (self.edit.caret(), self.edit.selection())
    }

    pub fn mouse_up(&mut self) {
        self.dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    // ── keyboard ────────────────────────────────────────────────────

    /// Text typed by the user (WM_CHAR). Control characters are ignored.
    pub fn char(&mut self, ch: char) -> bool {
        if !self.focused || ch.is_control() {
            return false;
        }
        let mut buf = [0u8; 4];
        let changed = self.edit.insert(ch.encode_utf8(&mut buf));
        self.caret_visible = true;
        changed
    }

    /// Navigation / editing / clipboard shortcuts. Returns true if handled.
    pub fn key(&mut self, key: Key, mods: Mods, cb: &mut dyn Clipboard) -> bool {
        if !self.focused {
            return false;
        }
        let (ctrl, shift) = (mods.ctrl, mods.shift);
        let handled = match key {
            Key::Left => {
                self.edit.move_left(shift, ctrl);
                true
            }
            Key::Right => {
                self.edit.move_right(shift, ctrl);
                true
            }
            Key::Home => {
                self.edit.home(shift);
                true
            }
            Key::End => {
                self.edit.end(shift);
                true
            }
            Key::Backspace => {
                self.edit.backspace(ctrl);
                true
            }
            Key::Delete => {
                self.edit.delete(ctrl);
                true
            }
            Key::Letter('A') if ctrl => {
                self.edit.select_all();
                true
            }
            Key::Letter('C') if ctrl => {
                if let Some(t) = self.edit.copy() {
                    cb.set_text(&t);
                }
                true
            }
            Key::Letter('X') if ctrl => {
                if let Some(t) = self.edit.cut() {
                    cb.set_text(&t);
                }
                true
            }
            Key::Letter('V') if ctrl => {
                if let Some(t) = cb.get_text() {
                    self.edit.insert(&t);
                }
                true
            }
            Key::Letter('Z') if ctrl => {
                if shift { self.edit.redo() } else { self.edit.undo() };
                true
            }
            Key::Letter('Y') if ctrl => {
                self.edit.redo();
                true
            }
            _ => false,
        };
        if handled {
            self.caret_visible = true;
        }
        handled
    }

    // ── painting ────────────────────────────────────────────────────

    pub fn paint(&mut self, c: &mut dyn Canvas, theme: &Theme, rect: Rect) {
        self.rect = rect;
        let area = self.area();
        let text = self.edit.text().to_string();
        let caret = self.edit.caret();

        // keep the caret in view
        let total = c.text_width(&text, TEXT);
        let caret_x = c.text_width(&text[..caret], TEXT);
        let view = area.width();
        if caret_x - self.scroll_x < 0.0 {
            self.scroll_x = caret_x;
        } else if caret_x - self.scroll_x > view - 2.0 {
            self.scroll_x = caret_x - view + 2.0;
        }
        if total <= view - 2.0 {
            self.scroll_x = 0.0;
        }
        self.scroll_x = self.scroll_x.max(0.0);

        c.fill_rect(rect, theme.bar_bg);
        let border = if self.focused { theme.accent } else { theme.border };
        c.stroke_rect(rect.inset(0.5), border, 1.0);

        c.push_clip(area);
        let origin = area.l - self.scroll_x;

        if let Some((a, b)) = self.edit.selection() {
            let xa = origin + c.text_width(&text[..a], TEXT);
            let xb = origin + c.text_width(&text[..b], TEXT);
            c.fill_rect(Rect::new(xa, rect.t + 6.0, xb, rect.b - 6.0), theme.accent.alpha(0.35));
        }

        if text.is_empty() {
            c.text(&self.placeholder, area, theme.text_dim, TEXT);
        } else {
            // generous width so the renderer's ellipsis trimming never kicks in
            let r = Rect::new(origin, rect.t, origin + total + 16.0, rect.b);
            c.text(&text, r, theme.text, TEXT);
        }

        if self.focused && self.caret_visible && self.edit.selection().is_none() {
            let x = origin + caret_x;
            c.line((x, rect.t + 7.0), (x, rect.b - 7.0), theme.text, 1.2);
        }
        c.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{MemClipboard, MonoCanvas, GLYPH};

    fn input(text: &str) -> (TextInput, MonoCanvas) {
        let mut t = TextInput::new("URL");
        t.edit.set_text(text);
        let mut c = MonoCanvas::default();
        t.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 200.0, 34.0));
        (t, c)
    }

    #[test]
    fn click_places_caret_at_nearest_char_and_shift_extends() {
        let (mut t, mut c) = input("hello world");
        let x0 = PAD; // text origin
        t.mouse_down(&mut c, x0 + GLYPH * 5.0 + 1.0, false);
        assert_eq!(t.edit.caret(), 5);
        assert!(t.focused);
        t.mouse_up();
        t.mouse_down(&mut c, x0 + GLYPH * 8.0, true);
        assert_eq!(t.edit.selection(), Some((5, 8)));
    }

    #[test]
    fn drag_selects_and_stops_on_mouse_up() {
        let (mut t, mut c) = input("hello world");
        t.mouse_down(&mut c, PAD, false);
        assert!(t.mouse_move(&mut c, PAD + GLYPH * 4.0));
        assert_eq!(t.edit.selected_text(), Some("hell"));
        t.mouse_up();
        assert!(!t.mouse_move(&mut c, PAD + GLYPH * 9.0));
        assert_eq!(t.edit.selected_text(), Some("hell"));
    }

    #[test]
    fn typing_and_shortcuts() {
        let (mut t, mut c) = input("");
        let mut cb = MemClipboard::default();
        let ctrl = Mods { ctrl: true, shift: false };
        t.mouse_down(&mut c, PAD, false);
        for ch in "abc".chars() {
            assert!(t.char(ch));
        }
        assert!(!t.char('\u{8}')); // control chars never inserted
        t.key(Key::Letter('A'), ctrl, &mut cb);
        t.key(Key::Letter('C'), ctrl, &mut cb);
        assert_eq!(cb.0.as_deref(), Some("abc"));
        t.key(Key::End, Mods::default(), &mut cb);
        t.key(Key::Letter('V'), ctrl, &mut cb);
        assert_eq!(t.edit.text(), "abcabc");
        t.key(Key::Letter('Z'), ctrl, &mut cb);
        assert_eq!(t.edit.text(), "abc");
        t.key(Key::Letter('X'), ctrl, &mut cb); // no selection: no-op, still handled
        t.edit.select_all();
        t.key(Key::Letter('X'), ctrl, &mut cb);
        assert_eq!(t.edit.text(), "");
        assert_eq!(cb.0.as_deref(), Some("abc"));
    }

    #[test]
    fn unfocused_input_ignores_keys() {
        let (mut t, _c) = input("x");
        let mut cb = MemClipboard::default();
        assert!(!t.char('a'));
        assert!(!t.key(Key::Backspace, Mods::default(), &mut cb));
        assert_eq!(t.edit.text(), "x");
    }

    #[test]
    fn long_text_scrolls_to_keep_caret_visible_and_clicks_account_for_it() {
        let long = "0123456789".repeat(10); // 100 chars = 800 DIPs, view is 180
        let (mut t, mut c) = input(&long);
        t.focus();
        t.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 200.0, 34.0));
        assert!(t.scroll_x > 600.0, "caret at end must be scrolled into view");
        // clicking the right edge lands near the end, not at the start
        t.mouse_down(&mut c, 195.0, false);
        assert!(t.edit.caret() > 90);
        // moving to the start scrolls back
        t.edit.home(false);
        t.paint(&mut c, &Theme::default(), Rect::new(0.0, 0.0, 200.0, 34.0));
        assert_eq!(t.scroll_x, 0.0);
    }

    #[test]
    fn paint_draws_placeholder_only_when_empty() {
        let (_t, c) = input("");
        assert!(c.ops.iter().any(|o| o == "text:URL"));
        let (_t, c) = input("abc");
        assert!(!c.ops.iter().any(|o| o == "text:URL"));
        assert!(c.ops.iter().any(|o| o == "text:abc"));
    }
}
