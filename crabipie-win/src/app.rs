//! Application state and input handling. Toolkit-free: the window layer
//! feeds it DIP-space events and hands it a `Canvas` to paint on.

use crate::ui::canvas::{Align, Canvas, TextStyle};
use crate::ui::geom::Rect;
use crate::ui::tab_bar::{Hit, TabBar, TabItem};
use crate::ui::theme::Theme;
use crabipie_core::HttpMethod;

#[derive(Clone, Debug)]
pub struct TabHeader {
    pub id: usize,
    pub title: String,
    pub method: HttpMethod,
}

pub struct App {
    pub theme: Theme,
    pub tabs: Vec<TabHeader>,
    pub active: usize,
    pub next_id: usize,
    bar: TabBar,
    width: f32,
    height: f32,
}

impl App {
    pub fn new(mut tabs: Vec<TabHeader>, active: usize, next_id: usize) -> Self {
        let next_id = next_id.max(tabs.iter().map(|t| t.id + 1).max().unwrap_or(0));
        let mut app = Self {
            theme: Theme::default(),
            tabs: Vec::new(),
            active: 0,
            next_id,
            bar: TabBar::new(),
            width: 0.0,
            height: 0.0,
        };
        if tabs.is_empty() {
            tabs.push(app.fresh_tab());
        }
        app.active = active.min(tabs.len() - 1);
        app.tabs = tabs;
        app
    }

    /// Reads the existing ~/.crabipie session (read-only for now: this shell
    /// never writes it, so it can't disturb the iced app's saved tabs).
    pub fn from_disk(rt: &tokio::runtime::Runtime) -> Self {
        let (session, meta) = rt.block_on(crabipie_core::load_app_state());
        let tabs = meta
            .into_iter()
            .map(|m| TabHeader { id: m.id, title: m.title, method: m.method })
            .collect();
        let (active, next_id) = session.map(|s| (s.active_tab, s.next_tab_id)).unwrap_or((0, 0));
        Self::new(tabs, active, next_id)
    }

    fn fresh_tab(&mut self) -> TabHeader {
        let id = self.next_id;
        self.next_id += 1;
        TabHeader { id, title: format!("Request {}", id + 1), method: HttpMethod::GET }
    }

    // ── tab operations ──────────────────────────────────────────────

    pub fn new_tab(&mut self) {
        let t = self.fresh_tab();
        self.tabs.push(t);
        self.active = self.tabs.len() - 1;
    }

    pub fn select_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            // never leave the window empty: replace with a fresh tab
            let t = self.fresh_tab();
            self.tabs[0] = t;
            self.active = 0;
            return;
        }
        self.tabs.remove(i);
        if i < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    // ── events (coordinates in DIPs) ────────────────────────────────

    pub fn on_resize(&mut self, w: f32, h: f32) {
        self.width = w;
        self.height = h;
    }

    fn hit(&self, x: f32, y: f32) -> Hit {
        TabBar::hit_test(x, y, self.tabs.len(), self.width)
    }

    /// Returns true when a repaint is needed.
    pub fn on_mouse_move(&mut self, x: f32, y: f32) -> bool {
        let h = self.hit(x, y);
        self.bar.set_hover(h)
    }

    pub fn on_mouse_leave(&mut self) -> bool {
        self.bar.set_hover(Hit::None)
    }

    pub fn on_left_click(&mut self, x: f32, y: f32) -> bool {
        match self.hit(x, y) {
            Hit::Tab(i) => self.select_tab(i),
            Hit::Close(i) => self.close_tab(i),
            Hit::NewTab => self.new_tab(),
            Hit::None => return false,
        }
        self.bar.set_hover(self.hit(x, y));
        true
    }

    pub fn on_middle_click(&mut self, x: f32, y: f32) -> bool {
        match self.hit(x, y) {
            Hit::Tab(i) | Hit::Close(i) => {
                self.close_tab(i);
                true
            }
            _ => false,
        }
    }

    pub fn on_ctrl_key(&mut self, key: char) -> bool {
        match key.to_ascii_uppercase() {
            'T' => self.new_tab(),
            'W' => self.close_tab(self.active),
            _ => return false,
        }
        true
    }

    // ── painting ────────────────────────────────────────────────────

    pub fn paint(&self, c: &mut dyn Canvas) {
        c.fill_rect(Rect::new(0.0, 0.0, self.width, self.height), self.theme.bg);

        let items: Vec<TabItem> = self
            .tabs
            .iter()
            .map(|t| TabItem { title: &t.title, method: t.method })
            .collect();
        self.bar.paint(c, &self.theme, &items, self.active, self.width);

        // Placeholder until the request/response panels land (step 3).
        let body = Rect::new(0.0, TabBar::HEIGHT, self.width, self.height);
        c.text(
            &format!("{}  -  request panel goes here", self.tabs[self.active].title),
            body,
            self.theme.text_dim,
            TextStyle::new(14.0).align(Align::Center),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::geom::Color;

    fn app(n: usize) -> App {
        let tabs = (0..n)
            .map(|i| TabHeader { id: i, title: format!("T{i}"), method: HttpMethod::GET })
            .collect();
        let mut a = App::new(tabs, 0, 0);
        a.on_resize(1200.0, 800.0);
        a
    }

    #[test]
    fn empty_session_gets_one_tab_and_ids_never_collide() {
        let a = App::new(vec![], 5, 0);
        assert_eq!(a.tabs.len(), 1);
        assert_eq!(a.active, 0);
        let b = App::new(
            vec![TabHeader { id: 9, title: "x".into(), method: HttpMethod::POST }],
            0,
            0,
        );
        assert_eq!(b.next_id, 10);
    }

    #[test]
    fn close_adjusts_active_like_a_browser() {
        let mut a = app(4);
        a.select_tab(2);
        a.close_tab(0); // closing left of active shifts it
        assert_eq!(a.active, 1);
        a.close_tab(2); // closing the last (right of active)
        assert_eq!(a.active, 1);
        a.close_tab(1); // closing the active one
        assert_eq!(a.tabs.len(), 1);
        a.close_tab(0); // last tab is replaced, never empty
        assert_eq!(a.tabs.len(), 1);
        assert_eq!(a.active, 0);
    }

    #[test]
    fn clicks_and_shortcuts_drive_tabs() {
        let mut a = app(2);
        let (px, py) = TabBar::plus_rect(2, 1200.0).center();
        assert!(a.on_left_click(px, py));
        assert_eq!((a.tabs.len(), a.active), (3, 2));
        let r = TabBar::tab_rect(0, 3, 1200.0);
        assert!(a.on_left_click(r.l + 4.0, r.t + 4.0));
        assert_eq!(a.active, 0);
        assert!(a.on_ctrl_key('t'));
        assert_eq!(a.tabs.len(), 4);
        assert!(a.on_ctrl_key('W'));
        assert_eq!(a.tabs.len(), 3);
        assert!(!a.on_ctrl_key('x'));
        assert!(!a.on_left_click(600.0, 400.0)); // empty body area
    }

    #[test]
    fn hover_reports_changes_only() {
        let mut a = app(2);
        let r = TabBar::tab_rect(1, 2, 1200.0);
        assert!(a.on_mouse_move(r.l + 4.0, r.t + 4.0));
        assert!(!a.on_mouse_move(r.l + 6.0, r.t + 6.0));
        assert!(a.on_mouse_leave());
    }

    struct Rec(Vec<String>);
    impl Canvas for Rec {
        fn fill_rect(&mut self, _: Rect, _: Color) { self.0.push("fill".into()) }
        fn stroke_rect(&mut self, _: Rect, _: Color, _: f32) {}
        fn line(&mut self, _: (f32, f32), _: (f32, f32), _: Color, _: f32) { self.0.push("line".into()) }
        fn text(&mut self, s: &str, _: Rect, _: Color, _: TextStyle) { self.0.push(format!("text:{s}")) }
        fn push_clip(&mut self, _: Rect) {}
        fn pop_clip(&mut self) {}
    }

    #[test]
    fn paint_emits_every_tab_title() {
        let a = app(3);
        let mut rec = Rec(vec![]);
        a.paint(&mut rec);
        for t in ["text:T0", "text:T1", "text:T2", "text:GET", "text:+"] {
            assert!(rec.0.iter().any(|s| s == t), "missing {t}");
        }
    }
}
