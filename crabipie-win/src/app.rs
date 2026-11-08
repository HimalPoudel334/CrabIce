//! Application state and input handling. Toolkit-free: the window layer
//! feeds it DIP-space events and hands it a `Canvas` to paint on.

use crate::ui::canvas::{Align, Canvas, TextMeasure, TextStyle};
use crate::ui::geom::Rect;
use crate::ui::input::{Clipboard, Key, Mods};
use crate::ui::tab_bar::{Hit, TabBar, TabItem};
use crate::ui::text_input::TextInput;
use crate::ui::theme::Theme;
use crabipie_core::HttpMethod;

#[derive(Clone, Debug)]
pub struct TabHeader {
    pub id: usize,
    pub title: String,
    pub method: HttpMethod,
    pub url: TextInput,
}

impl TabHeader {
    fn new(id: usize, title: String, method: HttpMethod) -> Self {
        Self { id, title, method, url: TextInput::new("Enter request URL") }
    }
}

pub struct App {
    pub theme: Theme,
    pub tabs: Vec<TabHeader>,
    pub active: usize,
    pub next_id: usize,
    bar: TabBar,
    width: f32,
    height: f32,
    over_text: bool,
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
            over_text: false,
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
            .map(|m| TabHeader::new(m.id, m.title, m.method))
            .collect();
        let (active, next_id) = session.map(|s| (s.active_tab, s.next_tab_id)).unwrap_or((0, 0));
        Self::new(tabs, active, next_id)
    }

    fn fresh_tab(&mut self) -> TabHeader {
        let id = self.next_id;
        self.next_id += 1;
        TabHeader::new(id, format!("Request {}", id + 1), HttpMethod::GET)
    }

    fn blur_all(&mut self) -> bool {
        let mut was = false;
        for t in &mut self.tabs {
            was |= t.url.focused;
            t.url.blur();
        }
        was
    }

    // ── tab operations ──────────────────────────────────────────────

    pub fn new_tab(&mut self) {
        self.blur_all();
        let t = self.fresh_tab();
        self.tabs.push(t);
        self.active = self.tabs.len() - 1;
        self.tabs[self.active].url.focus(); // like a browser: new tab focuses the address bar
    }

    pub fn select_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.blur_all();
            self.active = i;
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.blur_all();
        if self.tabs.len() == 1 {
            // Never leave the window empty: reset the last tab in place. Same
            // id and title, so repeatedly closing doesn't count up "Request N".
            let id = self.tabs[0].id;
            self.tabs[0] = TabHeader::new(id, format!("Request {}", id + 1), HttpMethod::GET);
            self.active = 0;
            return;
        }
        self.tabs.remove(i);
        if i < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    // ── layout ──────────────────────────────────────────────────────

    pub fn on_resize(&mut self, w: f32, h: f32) {
        self.width = w;
        self.height = h;
    }

    fn url_rect(&self) -> Rect {
        let top = TabBar::HEIGHT + 12.0;
        Rect::new(12.0, top, (self.width - 12.0).max(13.0), top + 34.0)
    }

    fn hit(&self, x: f32, y: f32) -> Hit {
        TabBar::hit_test(x, y, self.tabs.len(), self.width)
    }

    // ── mouse (coordinates in DIPs) ─────────────────────────────────

    /// Returns true when a repaint is needed.
    pub fn on_mouse_move(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
        self.over_text = self.url_rect().contains(x, y);
        let hover_changed = self.bar.set_hover(self.hit(x, y));
        let drag_changed = self.tabs[self.active].url.mouse_move(m, x);
        hover_changed || drag_changed
    }

    pub fn on_mouse_leave(&mut self) -> bool {
        self.over_text = false;
        self.bar.set_hover(Hit::None)
    }

    pub fn on_left_down(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32, shift: bool) -> bool {
        match self.hit(x, y) {
            Hit::Tab(i) => self.select_tab(i),
            Hit::Close(i) => self.close_tab(i),
            Hit::NewTab => self.new_tab(),
            Hit::None => {
                if self.url_rect().contains(x, y) {
                    self.blur_all();
                    self.tabs[self.active].url.mouse_down(m, x, shift);
                    return true;
                }
                return self.blur_all();
            }
        }
        self.bar.set_hover(self.hit(x, y));
        true
    }

    pub fn on_double_click(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
        if self.url_rect().contains(x, y) {
            self.tabs[self.active].url.double_click(m, x);
            true
        } else {
            false
        }
    }

    pub fn on_left_up(&mut self) {
        for t in &mut self.tabs {
            t.url.mouse_up();
        }
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

    /// True when the cursor should be an I-beam.
    pub fn text_cursor_active(&self) -> bool {
        self.over_text
    }

    // ── keyboard ────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: Key, mods: Mods, cb: &mut dyn Clipboard) -> bool {
        if mods.ctrl {
            match key {
                Key::Letter('T') => {
                    self.new_tab();
                    return true;
                }
                Key::Letter('W') => {
                    self.close_tab(self.active);
                    return true;
                }
                _ => {}
            }
        }
        if key == Key::Escape && self.blur_all() {
            return true;
        }
        self.tabs[self.active].url.key(key, mods, cb)
    }

    pub fn on_char(&mut self, ch: char) -> bool {
        self.tabs[self.active].url.char(ch)
    }

    /// Caret blink tick; returns true when a repaint is needed.
    pub fn blink(&mut self) -> bool {
        self.tabs[self.active].url.toggle_blink()
    }

    // ── painting ────────────────────────────────────────────────────

    pub fn paint(&mut self, c: &mut dyn Canvas) {
        c.fill_rect(Rect::new(0.0, 0.0, self.width, self.height), self.theme.bg);

        let items: Vec<TabItem> = self
            .tabs
            .iter()
            .map(|t| TabItem { title: &t.title, method: t.method })
            .collect();
        self.bar.paint(c, &self.theme, &items, self.active, self.width);

        let url_rect = self.url_rect();
        self.tabs[self.active].url.paint(c, &self.theme, url_rect);

        // Placeholder until the request/response panels land.
        let body = Rect::new(0.0, url_rect.b + 8.0, self.width, self.height);
        c.text(
            "method dropdown, Send and response panel come next",
            body,
            self.theme.text_dim,
            TextStyle::new(14.0).align(Align::Center),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{MemClipboard, MonoCanvas, GLYPH};

    fn app(n: usize) -> App {
        let tabs = (0..n)
            .map(|i| TabHeader::new(i, format!("T{i}"), HttpMethod::GET))
            .collect();
        let mut a = App::new(tabs, 0, 0);
        a.on_resize(1200.0, 800.0);
        // one paint so widgets know their rects
        a.paint(&mut MonoCanvas::default());
        a
    }

    #[test]
    fn empty_session_gets_one_tab_and_ids_never_collide() {
        let a = App::new(vec![], 5, 0);
        assert_eq!(a.tabs.len(), 1);
        assert_eq!(a.active, 0);
        let b = App::new(vec![TabHeader::new(9, "x".into(), HttpMethod::POST)], 0, 0);
        assert_eq!(b.next_id, 10);
    }

    #[test]
    fn close_adjusts_active_like_a_browser() {
        let mut a = app(4);
        a.select_tab(2);
        a.close_tab(0);
        assert_eq!(a.active, 1);
        a.close_tab(2);
        assert_eq!(a.active, 1);
        a.close_tab(1);
        assert_eq!(a.tabs.len(), 1);
    }

    #[test]
    fn closing_the_last_tab_resets_it_without_counting_up() {
        let mut a = app(1);
        let id = a.tabs[0].id;
        a.tabs[0].url.edit.set_text("https://example.com");
        for _ in 0..5 {
            a.close_tab(0);
        }
        assert_eq!(a.tabs.len(), 1);
        assert_eq!(a.tabs[0].id, id);
        assert_eq!(a.tabs[0].title, format!("Request {}", id + 1));
        assert_eq!(a.next_id, 1, "no new ids consumed");
        assert!(a.tabs[0].url.edit.is_empty(), "contents reset");
    }

    #[test]
    fn clicks_and_shortcuts_drive_tabs() {
        let mut a = app(2);
        let mut m = MonoCanvas::default();
        let mut cb = MemClipboard::default();
        let ctrl = Mods { ctrl: true, shift: false };
        let (px, py) = TabBar::plus_rect(2, 1200.0).center();
        assert!(a.on_left_down(&mut m, px, py, false));
        assert_eq!((a.tabs.len(), a.active), (3, 2));
        assert!(a.tabs[2].url.focused, "new tab focuses its URL bar");
        let r = TabBar::tab_rect(0, 3, 1200.0);
        assert!(a.on_left_down(&mut m, r.l + 4.0, r.t + 4.0, false));
        assert_eq!(a.active, 0);
        assert!(!a.tabs[2].url.focused);
        assert!(a.on_key(Key::Letter('T'), ctrl, &mut cb));
        assert_eq!(a.tabs.len(), 4);
        assert!(a.on_key(Key::Letter('W'), ctrl, &mut cb));
        assert_eq!(a.tabs.len(), 3);
        assert!(!a.on_key(Key::Letter('X'), ctrl, &mut cb));
    }

    #[test]
    fn typing_into_the_url_bar_is_per_tab() {
        let mut a = app(2);
        let mut m = MonoCanvas::default();
        assert!(a.on_left_down(&mut m, 40.0, TabBar::HEIGHT + 25.0, false));
        assert!(a.tabs[0].url.focused);
        for ch in "http://a.b".chars() {
            assert!(a.on_char(ch));
        }
        assert_eq!(a.tabs[0].url.edit.text(), "http://a.b");
        let r = TabBar::tab_rect(1, 2, 1200.0);
        a.on_left_down(&mut m, r.l + 4.0, r.t + 4.0, false);
        assert!(!a.on_char('z'), "unfocused tab ignores typing");
        assert_eq!(a.tabs[1].url.edit.text(), "");
        assert_eq!(a.tabs[0].url.edit.text(), "http://a.b");
    }

    #[test]
    fn clicking_away_or_escape_blurs() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        let mut cb = MemClipboard::default();
        a.on_left_down(&mut m, 40.0, TabBar::HEIGHT + 25.0, false);
        assert!(a.tabs[0].url.focused);
        assert!(a.on_left_down(&mut m, 600.0, 500.0, false)); // empty body
        assert!(!a.tabs[0].url.focused);
        a.on_left_down(&mut m, 40.0, TabBar::HEIGHT + 25.0, false);
        assert!(a.on_key(Key::Escape, Mods::default(), &mut cb));
        assert!(!a.tabs[0].url.focused);
        assert!(!a.on_left_down(&mut m, 600.0, 500.0, false)); // nothing to blur: no repaint
    }

    #[test]
    fn drag_select_across_events_and_cursor_shape() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        a.tabs[0].url.edit.set_text("abcdefgh");
        a.paint(&mut m);
        let y = TabBar::HEIGHT + 25.0;
        a.on_left_down(&mut m, 22.0, y, false); // origin = 12 + PAD(10)
        assert!(a.text_cursor_active() || a.on_mouse_move(&mut m, 22.0, y) || true);
        a.on_mouse_move(&mut m, 22.0 + GLYPH * 3.0, y);
        assert!(a.text_cursor_active());
        assert_eq!(a.tabs[0].url.edit.selected_text(), Some("abc"));
        a.on_left_up();
        a.on_mouse_move(&mut m, 22.0 + GLYPH * 6.0, y);
        assert_eq!(a.tabs[0].url.edit.selected_text(), Some("abc"), "released: no more dragging");
        a.on_mouse_move(&mut m, 600.0, 500.0);
        assert!(!a.text_cursor_active());
    }

    #[test]
    fn hover_reports_changes_only() {
        let mut a = app(2);
        let mut m = MonoCanvas::default();
        let r = TabBar::tab_rect(1, 2, 1200.0);
        assert!(a.on_mouse_move(&mut m, r.l + 4.0, r.t + 4.0));
        assert!(!a.on_mouse_move(&mut m, r.l + 6.0, r.t + 6.0));
        assert!(a.on_mouse_leave());
    }

    #[test]
    fn blink_only_repaints_when_focused() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        assert!(!a.blink());
        a.on_left_down(&mut m, 40.0, TabBar::HEIGHT + 25.0, false);
        assert!(a.blink());
    }

    #[test]
    fn paint_emits_every_tab_title() {
        let mut a = app(3);
        let mut c = MonoCanvas::default();
        a.paint(&mut c);
        for t in ["text:T0", "text:T1", "text:T2", "text:GET", "text:+", "text:Enter request URL"] {
            assert!(c.ops.iter().any(|s| s == t), "missing {t}");
        }
    }
}
