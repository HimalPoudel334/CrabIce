//! Stateless drawing helpers for the small controls (pickers, buttons, ...).

use super::canvas::{Align, Canvas, TextStyle};
use super::geom::{Color, Rect};
use super::theme::Theme;

pub const UI: TextStyle = TextStyle::new(14.0);

pub fn label(c: &mut dyn Canvas, th: &Theme, r: Rect, s: &str) {
    c.text(s, r, th.text, UI);
}

pub fn picker(c: &mut dyn Canvas, th: &Theme, r: Rect, label: &str, open: bool) {
    c.fill_rect(r, th.surface);
    c.stroke_rect(r.inset(0.5), if open { th.accent } else { th.border }, 1.0);
    c.text(label, Rect::new(r.l + 10.0, r.t, r.r - 26.0, r.b), th.text, UI);
    let (cx, cy) = (r.r - 14.0, (r.t + r.b) * 0.5);
    c.line((cx - 4.0, cy - 2.0), (cx, cy + 2.0), th.text, 1.3);
    c.line((cx, cy + 2.0), (cx + 4.0, cy - 2.0), th.text, 1.3);
}

pub fn button(c: &mut dyn Canvas, th: &Theme, r: Rect, label: &str, primary: bool) {
    let (bg, fg) = if primary { (th.accent, th.on_accent) } else { (th.surface, th.text) };
    c.fill_rect(r, bg);
    c.text(label, r, fg, UI.align(Align::Center));
}

pub fn checkbox(c: &mut dyn Canvas, th: &Theme, r: Rect, checked: bool) {
    if checked {
        c.fill_rect(r, th.accent);
        c.line((r.l + 3.5, (r.t + r.b) * 0.5), (r.l + 6.5, r.b - 4.5), th.on_accent, 2.0);
        c.line((r.l + 6.5, r.b - 4.5), (r.r - 3.0, r.t + 4.0), th.on_accent, 2.0);
    } else {
        c.stroke_rect(r.inset(0.5), th.accent, 1.5);
    }
}

pub fn cross(c: &mut dyn Canvas, r: Rect, color: Color) {
    let k = r.inset(r.width() * 0.28);
    c.line((k.l, k.t), (k.r, k.b), color, 1.6);
    c.line((k.l, k.b), (k.r, k.t), color, 1.6);
}

pub fn seg_rect(r: Rect, n: usize, i: usize) -> Rect {
    let w = r.width() / n as f32;
    Rect::new(r.l + w * i as f32, r.t, r.l + w * (i + 1) as f32, r.b)
}

/// Full-width segmented control (the Query / Body / Headers / Auth strip).
pub fn segmented(c: &mut dyn Canvas, th: &Theme, r: Rect, labels: &[&str], active: usize) {
    for (i, l) in labels.iter().enumerate() {
        let s = seg_rect(r, labels.len(), i);
        let on = i == active;
        c.fill_rect(s, if on { th.accent } else { th.surface });
        c.stroke_rect(s.inset(0.5), th.border, 1.0);
        c.text(l, s, if on { th.on_accent } else { th.text }, UI.align(Align::Center));
    }
}

pub fn rule(c: &mut dyn Canvas, th: &Theme, x0: f32, x1: f32, y: f32) {
    c.line((x0, y), (x1, y), th.border, 1.0);
}
