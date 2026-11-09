use super::canvas::{Canvas, TextStyle};
use super::geom::Rect;
use super::theme::Theme;
use super::widgets;
use crabipie_core::HttpMethod;

pub struct TabItem<'a> {
    pub title: &'a str,
    pub method: HttpMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    None,
    Tab(usize),
    Close(usize),
    NewTab,
}

pub struct TabBar {
    hover: Hit,
}

impl TabBar {
    /// Top edge and height of the tab row (absolute window coordinates).
    pub const Y: f32 = 44.0;
    pub const HEIGHT: f32 = 32.0;
    const LEFT: f32 = 10.0;
    const GAP: f32 = 2.0;
    const MIN_W: f32 = 100.0;
    const MAX_W: f32 = 190.0;
    const PLUS: f32 = 26.0;
    const CLOSE: f32 = 14.0;

    pub fn new() -> Self {
        Self { hover: Hit::None }
    }

    fn tab_width(n: usize, bar_width: f32) -> f32 {
        let avail = bar_width - Self::LEFT * 2.0 - Self::PLUS - 8.0;
        (avail / n.max(1) as f32 - Self::GAP).clamp(Self::MIN_W, Self::MAX_W)
    }

    pub fn tab_rect(i: usize, n: usize, bar_width: f32) -> Rect {
        let w = Self::tab_width(n, bar_width);
        let x = Self::LEFT + i as f32 * (w + Self::GAP);
        Rect::new(x, Self::Y, x + w, Self::Y + Self::HEIGHT)
    }

    pub fn close_rect(i: usize, n: usize, bar_width: f32) -> Rect {
        let t = Self::tab_rect(i, n, bar_width);
        let cy = (t.t + t.b) * 0.5;
        Rect::xywh(t.r - 8.0 - Self::CLOSE, cy - Self::CLOSE * 0.5, Self::CLOSE, Self::CLOSE)
    }

    pub fn plus_rect(n: usize, bar_width: f32) -> Rect {
        let w = Self::tab_width(n, bar_width);
        let x = Self::LEFT + n as f32 * (w + Self::GAP) + 6.0;
        Rect::xywh(x, Self::Y + 3.0, Self::PLUS, Self::HEIGHT - 6.0)
    }

    pub fn hit_test(x: f32, y: f32, n: usize, bar_width: f32) -> Hit {
        if y < Self::Y || y >= Self::Y + Self::HEIGHT {
            return Hit::None;
        }
        for i in 0..n {
            if Self::close_rect(i, n, bar_width).inset(-3.0).contains(x, y) {
                return Hit::Close(i);
            }
            if Self::tab_rect(i, n, bar_width).contains(x, y) {
                return Hit::Tab(i);
            }
        }
        if Self::plus_rect(n, bar_width).contains(x, y) {
            return Hit::NewTab;
        }
        Hit::None
    }

    /// Returns true if the hover target changed (caller should repaint).
    pub fn set_hover(&mut self, h: Hit) -> bool {
        let changed = self.hover != h;
        self.hover = h;
        changed
    }

    pub fn method_color(theme: &Theme, m: HttpMethod) -> super::geom::Color {
        match m {
            HttpMethod::GET => theme.method_get,
            HttpMethod::POST => theme.method_post,
            HttpMethod::PUT => theme.method_put,
            HttpMethod::PATCH => theme.method_patch,
            HttpMethod::DELETE => theme.method_delete,
            _ => theme.text_dim,
        }
    }

    pub fn paint(&self, c: &mut dyn Canvas, th: &Theme, tabs: &[TabItem], active: usize, bar_width: f32) {
        let n = tabs.len();
        c.fill_rect(Rect::new(0.0, Self::Y - 4.0, bar_width, Self::Y + Self::HEIGHT), th.tab_bar_bg);
        for (i, tab) in tabs.iter().enumerate() {
            let r = Self::tab_rect(i, n, bar_width);
            let is_active = i == active;
            let hovered = matches!(self.hover, Hit::Tab(h) | Hit::Close(h) if h == i);
            c.fill_rect(r, if is_active { th.tab_active } else if hovered { th.surface } else { th.tab_inactive });
            if is_active {
                c.fill_rect(Rect::new(r.l, r.t, r.r, r.t + 2.0), th.accent);
            }
            c.text(
                &tab.method.to_string(),
                Rect::new(r.l + 10.0, r.t, r.l + 54.0, r.b),
                Self::method_color(th, tab.method),
                TextStyle::new(10.0).bold(),
            );
            let close = Self::close_rect(i, n, bar_width);
            let col = if is_active { th.text } else { th.text_dim };
            c.text(tab.title, Rect::new(r.l + 56.0, r.t, close.l - 4.0, r.b), col, TextStyle::new(14.0));
            if is_active || hovered {
                let hot = self.hover == Hit::Close(i);
                if hot {
                    c.fill_rect(close.inset(-2.0), th.danger.alpha(0.25));
                }
                widgets::cross(c, close, if hot { th.danger } else { th.text_dim });
            }
        }
        let p = Self::plus_rect(n, bar_width);
        if self.hover == Hit::NewTab {
            c.fill_rect(p, th.surface);
        }
        c.text("+", p, th.text_dim, TextStyle::new(18.0).align(super::canvas::Align::Center));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_button_wins_over_the_tab_body() {
        let w = 1200.0;
        let (cx, cy) = TabBar::close_rect(1, 3, w).center();
        assert_eq!(TabBar::hit_test(cx, cy, 3, w), Hit::Close(1));
        let (px, py) = TabBar::plus_rect(3, w).center();
        assert_eq!(TabBar::hit_test(px, py, 3, w), Hit::NewTab);
        assert_eq!(TabBar::hit_test(px, TabBar::Y + TabBar::HEIGHT + 5.0, 3, w), Hit::None);
    }
}
