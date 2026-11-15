//! The drawing surface the UI talks to. `render::Renderer` implements it
//! with Direct2D/DirectWrite; tests implement it with a recorder.

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
    pub mono: bool,
}

impl TextStyle {
    pub const fn new(size: f32) -> Self {
        Self { size, bold: false, align: Align::Left, mono: false }
    }
    pub const fn bold(self) -> Self {
        Self { bold: true, ..self }
    }
    pub const fn mono(self) -> Self {
        Self { mono: true, ..self }
    }
    pub const fn align(self, align: Align) -> Self {
        Self { align, ..self }
    }
}

/// Text metrics. Usable outside a paint pass (event handlers need it for
/// click-to-caret), so it is its own trait.
pub trait TextMeasure {
    /// Advance width of `s` in DIPs, including trailing spaces.
    fn text_width(&mut self, s: &str, style: TextStyle) -> f32;
    /// Byte index of the caret position nearest to `x` (DIPs from the start of `s`).
    fn text_index_at(&mut self, s: &str, style: TextStyle, x: f32) -> usize;
}

pub trait Canvas: TextMeasure {
    fn fill_rect(&mut self, r: Rect, c: Color);
    fn stroke_rect(&mut self, r: Rect, c: Color, width: f32);
    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: Color, width: f32);
    /// Single line, vertically centered in `r`, clipped, ellipsis on overflow.
    fn text(&mut self, s: &str, r: Rect, c: Color, style: TextStyle);
    fn push_clip(&mut self, r: Rect);
    fn pop_clip(&mut self);

    /// Decodes an image (whatever the OS can: PNG, JPEG, GIF, BMP, TIFF, ICO, maybe WebP)
    /// and keeps it under `key`. Returns its size, or None if it can't be decoded.
    fn load_image(&mut self, _key: u64, _bytes: &[u8]) -> Option<ImageInfo> {
        None
    }

    /// Draws a loaded image stretched into `dst`.
    fn draw_image(&mut self, _key: u64, _dst: Rect) {}

    /// Forgets a loaded image.
    fn free_image(&mut self, _key: u64) {}
}

/// Pixel size of a decoded image. Very large images are scaled down while decoding, so
/// `w`/`h` (what is drawn) can be smaller than `full_w`/`full_h` (the file's real size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub w: u32,
    pub h: u32,
    pub full_w: u32,
    pub full_h: u32,
}
