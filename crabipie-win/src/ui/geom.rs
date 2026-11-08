//! Toolkit-free geometry and color. All coordinates are in DIPs
//! (device-independent pixels, 1 DIP = 1/96 inch); the renderer owns DPI.

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

impl Rect {
    pub const fn new(l: f32, t: f32, r: f32, b: f32) -> Self {
        Self { l, t, r, b }
    }
    pub const fn xywh(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { l: x, t: y, r: x + w, b: y + h }
    }
    pub fn width(&self) -> f32 {
        self.r - self.l
    }
    pub fn height(&self) -> f32 {
        self.b - self.t
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.l + d, self.t + d, self.r - d, self.b - d)
    }
    pub fn center(&self) -> (f32, f32) {
        ((self.l + self.r) * 0.5, (self.t + self.b) * 0.5)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// 0xRRGGBB, fully opaque.
    pub const fn hex(h: u32) -> Self {
        Self {
            r: ((h >> 16) & 0xFF) as f32 / 255.0,
            g: ((h >> 8) & 0xFF) as f32 / 255.0,
            b: (h & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }
    pub const fn alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}
