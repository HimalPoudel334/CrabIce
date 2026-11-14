//! Small line icons drawn with canvas primitives (no icon font, so nothing can
//! render as a missing-glyph box). Each is designed on a 16x16 grid and centred in the given rect.

use super::canvas::Canvas;
use super::geom::{Color, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Floppy disk: save to a file.
    Save,
    /// Open folder: open from a file.
    Open,
    /// Folder with a plus.
    NewFolder,
    /// Two chevrons pointing up: collapse everything.
    Collapse,
    /// Bookmark: save into the collection.
    Bookmark,
    /// Checkbox with a cross: clear the selection.
    Clear,
    /// Circular arrow: reload.
    Refresh,
}

pub fn draw(c: &mut dyn Canvas, icon: Icon, r: Rect, col: Color) {
    let x = (r.l + (r.width() - 16.0) / 2.0).round();
    let y = (r.t + (r.height() - 16.0) / 2.0).round();
    let w = 1.4;
    let p = |dx: f32, dy: f32| (x + dx, y + dy);
    let box_ = |c: &mut dyn Canvas, l: f32, t: f32, rr: f32, b: f32| c.stroke_rect(Rect::new(x + l, y + t, x + rr, y + b), col, w);
    let path = |c: &mut dyn Canvas, pts: &[(f32, f32)], closed: bool| {
        for pair in pts.windows(2) {
            c.line(p(pair[0].0, pair[0].1), p(pair[1].0, pair[1].1), col, w);
        }
        if closed && pts.len() > 2 {
            let (a, b) = (pts[pts.len() - 1], pts[0]);
            c.line(p(a.0, a.1), p(b.0, b.1), col, w);
        }
    };
    match icon {
        Icon::Save => {
            path(c, &[(1.5, 1.5), (11.5, 1.5), (14.5, 4.5), (14.5, 14.5), (1.5, 14.5)], true);
            box_(c, 4.5, 1.5, 10.5, 5.5);
            box_(c, 4.0, 9.0, 12.0, 14.5);
        }
        Icon::Open => {
            path(c, &[(1.5, 13.5), (1.5, 3.5), (6.0, 3.5), (7.5, 5.5), (13.0, 5.5), (13.0, 7.5)], false);
            path(c, &[(1.5, 13.5), (4.0, 7.5), (15.0, 7.5), (12.5, 13.5)], true);
        }
        Icon::NewFolder => {
            path(c, &[(1.5, 3.5), (6.0, 3.5), (7.5, 5.5), (14.5, 5.5), (14.5, 13.5), (1.5, 13.5)], true);
            path(c, &[(8.0, 7.5), (8.0, 12.0)], false);
            path(c, &[(5.8, 9.75), (10.2, 9.75)], false);
        }
        Icon::Collapse => {
            path(c, &[(3.0, 7.0), (8.0, 3.0), (13.0, 7.0)], false);
            path(c, &[(3.0, 13.0), (8.0, 9.0), (13.0, 13.0)], false);
        }
        Icon::Bookmark => {
            path(c, &[(3.5, 1.5), (12.5, 1.5), (12.5, 14.5), (8.0, 10.5), (3.5, 14.5)], true);
        }
        Icon::Clear => {
            box_(c, 1.5, 1.5, 14.5, 14.5);
            path(c, &[(5.0, 5.0), (11.0, 11.0)], false);
            path(c, &[(11.0, 5.0), (5.0, 11.0)], false);
        }
        Icon::Refresh => {
            // 280 degree arc, then an arrow head at its end
            let (cx, cy, rad) = (8.0f32, 8.0f32, 5.5f32);
            let (a0, a1) = (-1.0f32, 3.9f32);
            let steps = 16;
            let at = |a: f32| (cx + rad * a.cos(), cy + rad * a.sin());
            for i in 0..steps {
                let (s, e) = (a0 + (a1 - a0) * i as f32 / steps as f32, a0 + (a1 - a0) * (i + 1) as f32 / steps as f32);
                let (ps, pe) = (at(s), at(e));
                c.line(p(ps.0, ps.1), p(pe.0, pe.1), col, w);
            }
            let tip = at(a1);
            let (tx, ty) = (-a1.sin(), a1.cos()); // direction of travel
            let (nx, ny) = (a1.cos(), a1.sin());
            for side in [-1.0f32, 1.0] {
                let hx = tip.0 - tx * 3.5 + nx * 2.6 * side;
                let hy = tip.1 - ty * 3.5 + ny * 2.6 * side;
                c.line(p(tip.0, tip.1), p(hx, hy), col, w);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::MonoCanvas;

    #[test]
    fn every_icon_draws_something() {
        for icon in [Icon::Save, Icon::Open, Icon::NewFolder, Icon::Collapse, Icon::Bookmark, Icon::Clear, Icon::Refresh] {
            let mut c = MonoCanvas::default();
            draw(&mut c, icon, Rect::new(0.0, 0.0, 30.0, 26.0), Color::hex(0xffffff));
            assert!(!c.ops.is_empty(), "{icon:?} drew nothing");
        }
    }
}
