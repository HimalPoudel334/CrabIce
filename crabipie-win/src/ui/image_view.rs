//! State and math of the image viewer in the response panel: fit / zoom / pan.
//! Decoding and drawing go through `Canvas::load_image` / `draw_image`.

use super::canvas::ImageInfo;
use super::geom::Rect;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);
/// Keys of dropped views: `App::paint` tells the canvas to forget them.
static FREED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

pub fn take_freed() -> Vec<u64> {
    std::mem::take(&mut *FREED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Recognises common image formats by their first bytes.
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("PNG")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("JPEG")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("GIF")
    } else if b.starts_with(b"BM") && b.len() > 14 {
        Some("BMP")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("WebP")
    } else if b.starts_with(&[0, 0, 1, 0]) {
        Some("ICO")
    } else if b.starts_with(&[b'I', b'I', 42, 0]) || b.starts_with(&[b'M', b'M', 0, 42]) {
        Some("TIFF")
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// As large as fits, never enlarged beyond 100%.
    Fit,
    /// As wide as fits (a PDF page: read it by scrolling down), never enlarged beyond 100%.
    FitWidth,
    Scale(f32),
}

/// Which end of the image a view starts at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
}

/// A scrollbar: its track and the thumb inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub track: Rect,
    pub thumb: Rect,
}

const SB: f32 = 10.0;

pub struct ImageView {
    pub key: u64,
    pub bytes: Arc<Vec<u8>>,
    pub kind: &'static str,
    /// Filled by the first paint (the canvas decodes lazily).
    pub info: Option<ImageInfo>,
    pub failed: bool,
    /// Where the image was last drawn: the wheel and drags need it.
    pub area: Rect,
    zoom: Zoom,
    /// What double-click returns to: `Fit`, or `FitWidth` for PDF pages.
    base_fit: Zoom,
    pan: (f32, f32),
    /// Scroll to this edge as soon as the image size is known (first paint).
    pub start: Option<Edge>,
    /// Scrollbars as last drawn (for hit testing); None when the image fits.
    pub vbar: Option<Bar>,
    pub hbar: Option<Bar>,
}

/// A clone gets its own key (the canvas decodes it again on first paint), so dropping
/// one copy never frees the other's decoded image.
impl Clone for ImageView {
    fn clone(&self) -> Self {
        let mut v = Self::new(self.bytes.clone(), self.kind);
        v.zoom = self.zoom;
        v.base_fit = self.base_fit;
        v.pan = self.pan;
        v.start = self.start;
        v
    }
}

impl std::fmt::Debug for ImageView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageView").field("key", &self.key).field("kind", &self.kind).field("bytes", &self.bytes.len()).finish()
    }
}

impl Drop for ImageView {
    fn drop(&mut self) {
        FREED.lock().unwrap_or_else(|e| e.into_inner()).push(self.key);
    }
}

impl ImageView {
    pub fn new(bytes: Arc<Vec<u8>>, kind: &'static str) -> Self {
        Self { key: NEXT_KEY.fetch_add(1, Ordering::Relaxed), bytes, kind, info: None, failed: false, area: Rect::default(), zoom: Zoom::Fit, base_fit: Zoom::Fit, pan: (0.0, 0.0), start: None, vbar: None, hbar: None }
    }

    /// A PDF page: fit to width, starting at the top.
    pub fn for_page(mut self) -> Self {
        self.zoom = Zoom::FitWidth;
        self.base_fit = Zoom::FitWidth;
        self.start = Some(Edge::Top);
        self
    }

    pub fn pan_y(&self) -> f32 {
        self.pan.1
    }

    pub fn is_fit(&self) -> bool {
        matches!(self.zoom, Zoom::Fit | Zoom::FitWidth)
    }

    pub fn scale(&self, area: Rect) -> f32 {
        let Some(i) = self.info else { return 1.0 };
        match self.zoom {
            Zoom::Scale(s) => s,
            Zoom::FitWidth => (area.width() / i.w as f32).min(1.0).max(0.01),
            Zoom::Fit => (area.width() / i.w as f32).min(area.height() / i.h as f32).min(1.0).max(0.01),
        }
    }

    /// Zoom relative to the file's real pixel size.
    pub fn percent(&self, area: Rect) -> u32 {
        match self.info {
            Some(i) => (self.scale(area) * i.w as f32 / i.full_w.max(1) as f32 * 100.0).round() as u32,
            None => 100,
        }
    }

    fn clamped_pan(&self, area: Rect, s: f32) -> (f32, f32) {
        if self.info.is_none() {
            return (0.0, 0.0);
        }
        let (mx, my) = self.margins(area, s);
        (self.pan.0.clamp(-mx, mx), self.pan.1.clamp(-my, my))
    }

    /// Half of how far the image overflows the area, per axis (the pan range is +-this).
    fn margins(&self, area: Rect, s: f32) -> (f32, f32) {
        let Some(i) = self.info else { return (0.0, 0.0) };
        (((i.w as f32 * s - area.width()) / 2.0).max(0.0), ((i.h as f32 * s - area.height()) / 2.0).max(0.0))
    }

    /// Scrollbars for the current zoom/pan: (vertical, horizontal). None on an axis where the image fits.
    pub fn bars(&self, area: Rect) -> (Option<Bar>, Option<Bar>) {
        let Some(i) = self.info else { return (None, None) };
        let s = self.scale(area);
        let (mx, my) = self.margins(area, s);
        let (w, h) = (i.w as f32 * s, i.h as f32 * s);
        let (px, py) = self.clamped_pan(area, s);
        let (has_v, has_h) = (my > 0.0, mx > 0.0);
        let vbar = has_v.then(|| {
            let track = Rect::new(area.r - SB - 2.0, area.t + 2.0, area.r - 2.0, area.b - 2.0 - if has_h { SB } else { 0.0 });
            let len = (track.height() * area.height() / h).clamp(24.0, track.height());
            let top = track.t + (my - py) / (2.0 * my) * (track.height() - len);
            Bar { track, thumb: Rect::new(track.l + 2.0, top, track.r - 2.0, top + len) }
        });
        let hbar = has_h.then(|| {
            let track = Rect::new(area.l + 2.0, area.b - SB - 2.0, area.r - 2.0 - if has_v { SB } else { 0.0 }, area.b - 2.0);
            let len = (track.width() * area.width() / w).clamp(24.0, track.width());
            let left = track.l + (mx - px) / (2.0 * mx) * (track.width() - len);
            Bar { track, thumb: Rect::new(left, track.t + 2.0, left + len, track.b - 2.0) }
        });
        (vbar, hbar)
    }

    /// Moves the scroll position so the thumb's top/left edge is at `pos` (a thumb drag).
    pub fn set_scroll_from_thumb(&mut self, area: Rect, vertical: bool, pos: f32) {
        let (vb, hb) = self.bars(area);
        let Some(b) = (if vertical { vb } else { hb }) else { return };
        let (room, start) = if vertical { (b.track.height() - b.thumb.height(), b.track.t) } else { (b.track.width() - b.thumb.width(), b.track.l) };
        let frac = ((pos - start) / room.max(1.0)).clamp(0.0, 1.0);
        let (mx, my) = self.margins(area, self.scale(area));
        if vertical {
            self.pan.1 = my - frac * 2.0 * my;
        } else {
            self.pan.0 = mx - frac * 2.0 * mx;
        }
    }

    /// Scrolls to the top or bottom edge.
    pub fn scroll_to(&mut self, area: Rect, edge: Edge) {
        let (_, my) = self.margins(area, self.scale(area));
        self.pan.1 = if edge == Edge::Top { my } else { -my };
    }

    /// Where the image is drawn: centred in `area`, offset by the pan, which can never
    /// drag the image's edge past the area's edge.
    pub fn dest(&self, area: Rect) -> Rect {
        let Some(i) = self.info else { return area };
        let s = self.scale(area);
        let (w, h) = (i.w as f32 * s, i.h as f32 * s);
        let (px, py) = self.clamped_pan(area, s);
        let (cx, cy) = area.center();
        Rect::new(cx + px - w / 2.0, cy + py - h / 2.0, cx + px + w / 2.0, cy + py + h / 2.0)
    }

    /// Zooms by 20% per wheel notch, keeping the point under the cursor in place.
    pub fn zoom_at(&mut self, area: Rect, cursor: (f32, f32), notches: f32) {
        let Some(i) = self.info else { return };
        let s0 = self.scale(area);
        let d = self.dest(area);
        let s1 = (s0 * 1.2f32.powf(notches)).clamp(0.02, 32.0);
        let (u, v) = ((cursor.0 - d.l) / s0, (cursor.1 - d.t) / s0);
        let (w1, h1) = (i.w as f32 * s1, i.h as f32 * s1);
        let (cx, cy) = area.center();
        self.pan = (cursor.0 - u * s1 - cx + w1 / 2.0, cursor.1 - v * s1 - cy + h1 / 2.0);
        self.zoom = Zoom::Scale(s1);
    }

    pub fn pan_by(&mut self, area: Rect, dx: f32, dy: f32) {
        let s = self.scale(area);
        self.pan = (self.pan.0 + dx, self.pan.1 + dy);
        self.pan = self.clamped_pan(area, s);
    }

    /// Fit <-> 100%.
    pub fn toggle_fit(&mut self) {
        self.zoom = if self.is_fit() { Zoom::Scale(1.0) } else { self.base_fit };
        self.pan = (0.0, 0.0);
        if self.base_fit == Zoom::FitWidth {
            self.start = Some(Edge::Top);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(w: u32, h: u32) -> ImageView {
        let mut v = ImageView::new(Arc::new(vec![]), "PNG");
        v.info = Some(ImageInfo { w, h, full_w: w, full_h: h });
        v
    }

    #[test]
    fn fit_shrinks_but_never_enlarges_and_centres() {
        let area = Rect::new(0.0, 0.0, 1000.0, 500.0);
        let big = view(2000, 1000);
        assert_eq!(big.scale(area), 0.5);
        assert_eq!(big.dest(area), area);
        let small = view(100, 50);
        assert_eq!(small.scale(area), 1.0);
        assert_eq!(small.dest(area), Rect::new(450.0, 225.0, 550.0, 275.0));
        assert_eq!(big.percent(area), 50);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_and_pan_stays_inside() {
        let area = Rect::new(0.0, 0.0, 500.0, 500.0);
        let mut v = view(1000, 1000);
        let cursor = (100.0, 100.0);
        let before = v.dest(area);
        let (u, w) = ((cursor.0 - before.l) / v.scale(area), (cursor.1 - before.t) / v.scale(area));
        v.zoom_at(area, cursor, 5.0);
        assert!(!v.is_fit());
        let (s, after) = (v.scale(area), v.dest(area));
        assert!(((cursor.0 - after.l) / s - u).abs() < 0.5);
        assert!(((cursor.1 - after.t) / s - w).abs() < 0.5);

        v.pan_by(area, 10_000.0, -10_000.0);
        let d = v.dest(area);
        assert!(d.l <= area.l && d.r >= area.r && d.t <= area.t && d.b >= area.b, "edges can't be dragged into view");

        v.toggle_fit();
        assert!(v.is_fit());
        v.toggle_fit();
        assert_eq!(v.scale(area), 1.0);
    }

    #[test]
    fn sniffs_common_formats() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10]), Some("PNG"));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("JPEG"));
        assert_eq!(sniff(b"GIF89a...."), Some("GIF"));
        assert_eq!(sniff(b"RIFF\x10\0\0\0WEBPVP8 "), Some("WebP"));
        assert_eq!(sniff(b"hello"), None);
    }

    #[test]
    fn scrollbars_track_the_scroll_position_and_pages_start_at_an_edge() {
        let area = Rect::new(0.0, 0.0, 500.0, 400.0);
        // a tall page: 2000 x 2800 fitted to width 500 -> scale 0.25 -> 500 x 700: overflows vertically only
        let mut v = view(2000, 2800).for_page();
        assert!(v.is_fit() && (v.scale(area) - 0.25).abs() < 1e-6);
        let (vb, hb) = v.bars(area);
        assert!(vb.is_some() && hb.is_none());
        v.scroll_to(area, Edge::Top);
        let top = v.bars(area).0.unwrap();
        assert!((top.thumb.t - top.track.t).abs() < 0.5, "at the top the thumb is at the top");
        assert_eq!(v.dest(area).t, area.t);
        v.scroll_to(area, Edge::Bottom);
        let bot = v.bars(area).0.unwrap();
        assert!((bot.thumb.b - bot.track.b).abs() < 0.5);
        assert_eq!(v.dest(area).b, area.b);

        // dragging the thumb to the middle of its range scrolls to the middle
        let mid = bot.track.t + (bot.track.height() - bot.thumb.height()) / 2.0;
        v.set_scroll_from_thumb(area, true, mid);
        let d = v.dest(area);
        assert!(((d.t + d.b) / 2.0 - (area.t + area.b) / 2.0).abs() < 1.0, "centred");

        // zooming past the area adds the horizontal bar too
        v.zoom_at(area, (250.0, 200.0), 8.0);
        assert!(v.bars(area).1.is_some());
        v.toggle_fit();
        assert!(v.is_fit() && v.start == Some(Edge::Top));
    }
}
