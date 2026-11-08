use super::canvas::{Align, Canvas, TextStyle};
use super::geom::{Color, Rect};
use super::theme::Theme;
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
    pub const HEIGHT: f32 = 36.0;
    const LEFT: f32 = 8.0;
    const TOP: f32 = 4.0;
    const GAP: f32 = 2.0;
    const MIN_W: f32 = 90.0;
    const MAX_W: f32 = 200.0;
    const PLUS: f32 = 26.0;
    const CLOSE: f32 = 16.0;

    pub fn new() -> Self {
        Self { hover: Hit::None }
    }

    fn tab_width(n: usize, bar_width: f32) -> f32 {
        let avail = bar_width - Self::LEFT - Self::PLUS - 16.0;
        (avail / n.max(1) as f32 - Self::GAP).clamp(Self::MIN_W, Self::MAX_W)
    }

    pub fn tab_rect(i: usize, n: usize, bar_width: f32) -> Rect {
        let w = Self::tab_width(n, bar_width);
        let x = Self::LEFT + i as f32 * (w + Self::GAP);
        Rect::new(x, Self::TOP, x + w, Self::HEIGHT)
    }

    pub fn close_rect(i: usize, n: usize, bar_width: f32) -> Rect {
        let t = Self::tab_rect(i, n, bar_width);
        let cy = (t.t + t.b) * 0.5;
        Rect::xywh(t.r - 8.0 - Self::CLOSE, cy - Self::CLOSE * 0.5, Self::CLOSE, Self::CLOSE)
    }

    pub fn plus_rect(n: usize, bar_width: f32) -> Rect {
        let w = Self::tab_width(n, bar_width);
        let x = Self::LEFT + n as f32 * (w + Self::GAP) + 4.0;
        let cy = (Self::TOP + Self::HEIGHT) * 0.5;
        Rect::xywh(x, cy - Self::PLUS * 0.5, Self::PLUS, Self::PLUS)
    }

    pub fn hit_test(x: f32, y: f32, n: usize, bar_width: f32) -> Hit {
        if y < 0.0 || y >= Self::HEIGHT {
            return Hit::None;
        }
        for i in 0..n {
            if Self::close_rect(i, n, bar_width).contains(x, y) {
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

    fn method_color(theme: &Theme, m: HttpMethod) -> Color {
        match m {
            HttpMethod::GET => theme.method_get,
            HttpMethod::POST => theme.method_post,
            HttpMethod::PUT => theme.method_put,
            HttpMethod::PATCH => theme.method_patch,
            HttpMethod::DELETE => theme.method_delete,
            _ => theme.text_dim,
        }
    }

    pub fn paint(
        &self,
        c: &mut dyn Canvas,
        theme: &Theme,
        tabs: &[TabItem],
        active: usize,
        bar_width: f32,
    ) {
        let n = tabs.len();
        c.fill_rect(Rect::new(0.0, 0.0, bar_width, Self::HEIGHT), theme.bar_bg);

        for (i, tab) in tabs.iter().enumerate() {
            let r = Self::tab_rect(i, n, bar_width);
            let is_active = i == active;
            let hovered = matches!(self.hover, Hit::Tab(h) | Hit::Close(h) if h == i);

            let bg = if is_active {
                theme.tab_active
            } else if hovered {
                theme.tab_hover
            } else {
                theme.tab_inactive
            };
            c.fill_rect(r, bg);
            if is_active {
                c.fill_rect(Rect::new(r.l, r.t, r.r, r.t + 2.0), theme.accent);
            }

            // method badge, then title (clipped before the close button)
            let badge = Rect::new(r.l + 10.0, r.t, r.l + 52.0, r.b);
            c.text(
                &tab.method.to_string(),
                badge,
                Self::method_color(theme, tab.method),
                TextStyle::new(10.0).bold(),
            );
            let close = Self::close_rect(i, n, bar_width);
            let title = Rect::new(r.l + 54.0, r.t, close.l - 4.0, r.b);
            let col = if is_active { theme.text } else { theme.text_dim };
            c.text(tab.title, title, col, TextStyle::new(13.0));

            if is_active || hovered {
                let close_hot = self.hover == Hit::Close(i);
                if close_hot {
                    c.fill_rect(close, theme.danger.alpha(0.25));
                }
                let k = close.inset(4.0);
                let col = if close_hot { theme.danger } else { theme.text_dim };
                c.line((k.l, k.t), (k.r, k.b), col, 1.2);
                c.line((k.l, k.b), (k.r, k.t), col, 1.2);
            }
        }

        let p = Self::plus_rect(n, bar_width);
        if self.hover == Hit::NewTab {
            c.fill_rect(p, theme.tab_hover);
        }
        c.text("+", p, theme.text_dim, TextStyle::new(16.0).align(Align::Center));

        // bottom border
        c.line((0.0, Self::HEIGHT - 0.5), (bar_width, Self::HEIGHT - 0.5), theme.border, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_testing_prefers_close_over_tab() {
        let (n, w) = (3, 1200.0);
        let close = TabBar::close_rect(1, n, w);
        let (cx, cy) = close.center();
        assert_eq!(TabBar::hit_test(cx, cy, n, w), Hit::Close(1));
        let t = TabBar::tab_rect(1, n, w);
        assert_eq!(TabBar::hit_test(t.l + 5.0, t.t + 5.0, n, w), Hit::Tab(1));
        let (px, py) = TabBar::plus_rect(n, w).center();
        assert_eq!(TabBar::hit_test(px, py, n, w), Hit::NewTab);
        assert_eq!(TabBar::hit_test(px, TabBar::HEIGHT + 5.0, n, w), Hit::None);
    }

    #[test]
    fn tabs_shrink_to_fit_but_not_below_min() {
        let wide = TabBar::tab_rect(0, 3, 1500.0).width();
        let narrow = TabBar::tab_rect(0, 20, 1000.0).width();
        assert_eq!(wide, 200.0);
        assert_eq!(narrow, 90.0);
    }
}
