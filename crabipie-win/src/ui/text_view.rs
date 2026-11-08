//! Read-only, scrollable, virtualized text viewer (response body for now).
//! Only visible lines are drawn, so huge bodies stay cheap.

use super::canvas::{Align, Canvas, TextStyle};
use super::geom::Rect;
use super::theme::Theme;

pub const LINE_H: f32 = 18.0;
pub const MONO: TextStyle = TextStyle::new(12.5).mono();
const MAX_LINE_CHARS: usize = 4000; // keeps one giant minified line from stalling layout

#[derive(Clone, Debug, Default)]
pub struct TextView {
    lines: Vec<String>,
    scroll: f32,
    rect: Rect,
}

fn clean(s: &str) -> String {
    let s = s.replace('\r', "").replace('\t', "    ");
    if s.chars().count() > MAX_LINE_CHARS {
        let mut t: String = s.chars().take(MAX_LINE_CHARS).collect();
        t.push('…');
        t
    } else {
        s
    }
}

impl TextView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_text(&mut self, s: &str) {
        self.lines = s.split('\n').map(clean).collect();
        self.scroll = 0.0;
    }

    /// Streaming: continues the last (possibly partial) line.
    pub fn append(&mut self, s: &str) {
        let mut parts = s.split('\n');
        if let Some(first) = parts.next() {
            match self.lines.last_mut() {
                Some(last) => *last = clean(&format!("{last}{first}")),
                None => self.lines.push(clean(first)),
            }
        }
        for p in parts {
            self.lines.push(clean(p));
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
    pub fn line(&self, i: usize) -> Option<&str> {
        self.lines.get(i).map(|s| s.as_str())
    }

    fn max_scroll(&self) -> f32 {
        (self.lines.len() as f32 * LINE_H - self.rect.height() + 12.0).max(0.0)
    }

    /// Positive notches scroll up (wheel away from user). True if it moved.
    pub fn wheel(&mut self, notches: f32) -> bool {
        let before = self.scroll;
        self.scroll = (self.scroll - notches * 3.0 * LINE_H).clamp(0.0, self.max_scroll());
        before != self.scroll
    }

    pub fn paint(&mut self, c: &mut dyn Canvas, theme: &Theme, rect: Rect, placeholder: &str) {
        self.rect = rect;
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        c.fill_rect(rect, theme.bar_bg);
        c.stroke_rect(rect.inset(0.5), theme.border, 1.0);

        if self.lines.is_empty() {
            c.text(placeholder, rect, theme.text_dim, TextStyle::new(13.0).align(Align::Center));
            return;
        }

        c.push_clip(rect);
        let first = (self.scroll / LINE_H) as usize;
        let count = (rect.height() / LINE_H) as usize + 2;
        let top = rect.t + 6.0 - self.scroll;
        for i in first..(first + count).min(self.lines.len()) {
            let y = top + i as f32 * LINE_H;
            c.text(&self.lines[i], Rect::new(rect.l + 10.0, y, rect.r - 14.0, y + LINE_H), theme.text, MONO);
        }

        let content = self.lines.len() as f32 * LINE_H + 12.0;
        if content > rect.height() {
            let h = (rect.height() * rect.height() / content).max(24.0);
            let y = rect.t + (rect.height() - h) * (self.scroll / self.max_scroll().max(1.0));
            c.fill_rect(Rect::new(rect.r - 8.0, y, rect.r - 3.0, y + h), theme.text_dim.alpha(0.5));
        }
        c.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::MonoCanvas;

    #[test]
    fn append_continues_partial_lines() {
        let mut v = TextView::new();
        v.set_text("a\nb");
        v.append("c\nd");
        assert_eq!((v.line_count(), v.line(1), v.line(2)), (3, Some("bc"), Some("d")));
        let mut s = TextView::new();
        s.append("data: x\r\n\r\n");
        assert_eq!(s.line(0), Some("data: x"));
    }

    #[test]
    fn scroll_is_clamped_and_virtualized() {
        let mut v = TextView::new();
        let body = (0..1000).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        v.set_text(&body);
        let mut c = MonoCanvas::default();
        let r = Rect::new(0.0, 0.0, 400.0, 180.0);
        v.paint(&mut c, &Theme::default(), r, "");
        let drawn = c.ops.iter().filter(|o| o.starts_with("text:line")).count();
        assert!(drawn <= 12, "drew {drawn} lines, expected only the visible ones");
        assert!(!v.wheel(1.0), "already at top");
        assert!(v.wheel(-5.0));
        assert!(v.wheel(-1.0e6));
        assert!(!v.wheel(-1.0), "clamped at bottom");
        let mut c = MonoCanvas::default();
        v.paint(&mut c, &Theme::default(), r, "");
        assert!(c.ops.iter().any(|o| o == "text:line 999"));
    }

    #[test]
    fn giant_line_is_truncated() {
        let mut v = TextView::new();
        v.set_text(&"x".repeat(100_000));
        assert!(v.line(0).unwrap().chars().count() <= MAX_LINE_CHARS + 1);
    }
}
