//! HTTP method picker: a button plus a popup list painted over everything.

use super::canvas::{Align, Canvas, TextStyle};
use super::geom::Rect;
use super::tab_bar::TabBar;
use super::theme::Theme;
use crabipie_core::HttpMethod;

const ITEM_H: f32 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdHit {
    None,
    Button,
    Item(usize),
}

#[derive(Clone, Debug, Default)]
pub struct MethodDropdown {
    pub open: bool,
    hover: Option<usize>,
}

impl MethodDropdown {
    pub fn item_rect(button: Rect, i: usize) -> Rect {
        let t = button.b + 2.0 + i as f32 * ITEM_H;
        Rect::new(button.l, t, button.r, t + ITEM_H)
    }

    /// Only reports popup items while open.
    pub fn hit(&self, button: Rect, x: f32, y: f32) -> DdHit {
        if self.open {
            for i in 0..HttpMethod::ALL.len() {
                if Self::item_rect(button, i).contains(x, y) {
                    return DdHit::Item(i);
                }
            }
        }
        if button.contains(x, y) { DdHit::Button } else { DdHit::None }
    }

    /// True if the hovered item changed.
    pub fn set_hover(&mut self, h: Option<usize>) -> bool {
        let changed = self.hover != h;
        self.hover = h;
        changed
    }

    pub fn paint_button(&self, c: &mut dyn Canvas, theme: &Theme, button: Rect, m: HttpMethod) {
        c.fill_rect(button, theme.bar_bg);
        c.stroke_rect(button.inset(0.5), if self.open { theme.accent } else { theme.border }, 1.0);
        let label = Rect::new(button.l + 10.0, button.t, button.r - 24.0, button.b);
        c.text(&m.to_string(), label, TabBar::method_color(theme, m), TextStyle::new(13.0).bold());
        // chevron
        let (cx, cy) = (button.r - 14.0, (button.t + button.b) * 0.5);
        c.line((cx - 4.0, cy - 2.0), (cx, cy + 2.0), theme.text_dim, 1.3);
        c.line((cx, cy + 2.0), (cx + 4.0, cy - 2.0), theme.text_dim, 1.3);
    }

    /// Call last in the paint pass so it sits on top.
    pub fn paint_popup(&self, c: &mut dyn Canvas, theme: &Theme, button: Rect, current: HttpMethod) {
        if !self.open {
            return;
        }
        let n = HttpMethod::ALL.len();
        let all = Rect::new(button.l, button.b + 2.0, button.r, button.b + 2.0 + n as f32 * ITEM_H);
        c.fill_rect(all, theme.bar_bg);
        for (i, m) in HttpMethod::ALL.iter().enumerate() {
            let r = Self::item_rect(button, i);
            if self.hover == Some(i) {
                c.fill_rect(r, theme.tab_hover);
            } else if *m == current {
                c.fill_rect(r, theme.tab_hover.alpha(0.5));
            }
            c.text(
                &m.to_string(),
                Rect::new(r.l + 10.0, r.t, r.r, r.b),
                TabBar::method_color(theme, *m),
                TextStyle::new(13.0).bold().align(Align::Left),
            );
        }
        c.stroke_rect(all.inset(0.5), theme.border, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_hits_only_when_open() {
        let b = Rect::new(12.0, 48.0, 104.0, 82.0);
        let mut d = MethodDropdown::default();
        let r1 = MethodDropdown::item_rect(b, 1);
        let (x, y) = r1.center();
        assert_eq!(d.hit(b, x, y), DdHit::None);
        d.open = true;
        assert_eq!(d.hit(b, x, y), DdHit::Item(1));
        assert_eq!(d.hit(b, 50.0, 60.0), DdHit::Button);
        assert!(d.set_hover(Some(2)));
        assert!(!d.set_hover(Some(2)));
    }
}
