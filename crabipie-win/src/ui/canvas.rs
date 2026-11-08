//! The drawing surface the UI talks to. `render::Renderer` implements it
//! with Direct2D; tests implement it with a recorder.

use super::geom::{Color, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub bold: bool,
    pub align: Align,
}

impl TextStyle {
    pub const fn new(size: f32) -> Self {
        Self { size, bold: false, align: Align::Left }
    }
    pub const fn bold(self) -> Self {
        Self { bold: true, ..self }
    }
    pub const fn align(self, align: Align) -> Self {
        Self { align, ..self }
    }
}

pub trait Canvas {
    fn fill_rect(&mut self, r: Rect, c: Color);
    fn stroke_rect(&mut self, r: Rect, c: Color, width: f32);
    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: Color, width: f32);
    /// Single line, vertically centered in `r`, clipped, ellipsis on overflow.
    fn text(&mut self, s: &str, r: Rect, c: Color, style: TextStyle);
    fn push_clip(&mut self, r: Rect);
    fn pop_clip(&mut self);
}
