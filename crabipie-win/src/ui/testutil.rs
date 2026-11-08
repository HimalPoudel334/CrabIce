//! Test doubles: a fixed-width canvas and an in-memory clipboard.

use super::canvas::{Canvas, TextMeasure, TextStyle};
use super::geom::{Color, Rect};
use super::input::Clipboard;

/// Every character is this wide, regardless of style.
pub const GLYPH: f32 = 8.0;

#[derive(Default)]
pub struct MonoCanvas {
    pub ops: Vec<String>,
}

impl TextMeasure for MonoCanvas {
    fn text_width(&mut self, s: &str, _: TextStyle) -> f32 {
        s.chars().count() as f32 * GLYPH
    }
    fn text_index_at(&mut self, s: &str, _: TextStyle, x: f32) -> usize {
        let n = (x / GLYPH).round().max(0.0) as usize;
        s.char_indices().nth(n).map(|(b, _)| b).unwrap_or(s.len())
    }
}

impl Canvas for MonoCanvas {
    fn fill_rect(&mut self, _: Rect, _: Color) {
        self.ops.push("fill".into());
    }
    fn stroke_rect(&mut self, _: Rect, _: Color, _: f32) {
        self.ops.push("stroke".into());
    }
    fn line(&mut self, _: (f32, f32), _: (f32, f32), _: Color, _: f32) {
        self.ops.push("line".into());
    }
    fn text(&mut self, s: &str, _: Rect, _: Color, _: TextStyle) {
        self.ops.push(format!("text:{s}"));
    }
    fn push_clip(&mut self, _: Rect) {}
    fn pop_clip(&mut self) {}
}

#[derive(Default)]
pub struct MemClipboard(pub Option<String>);

impl Clipboard for MemClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.0.clone()
    }
    fn set_text(&mut self, s: &str) {
        self.0 = Some(s.to_string());
    }
}
