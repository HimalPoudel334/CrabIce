//! Application state and input handling. Toolkit-free: the window layer
//! feeds it DIP-space events and hands it a `Canvas` to paint on.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::ui::canvas::{Align, Canvas, TextMeasure, TextStyle};
use crate::ui::dropdown::{DdHit, MethodDropdown};
use crate::ui::geom::Rect;
use crate::ui::input::{Clipboard, Key, Mods};
use crate::ui::json_fmt;
use crate::ui::tab_bar::{Hit, TabBar, TabItem};
use crate::ui::text_area::TextArea;
use crate::ui::text_input::TextInput;
use crate::ui::theme::Theme;
use crabipie_core::{
    format_duration, merge_set_cookies, raw_to_headers, ContentType, CookieEntry, HttpMethod, HttpResponse,
    ResponseEvent, SavedState,
};

pub const DEFAULT_URL: &str = "https://jsonplaceholder.typicode.com/posts";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReqTab {
    Headers,
    Body,
}

/// What the window layer must run: build + execute this request on the runtime.
pub struct SendJob {
    pub tab_id: usize,
    pub state: SavedState,
    pub cancel: Arc<AtomicBool>,
    pub jar: HashMap<String, Vec<CookieEntry>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RespTab {
    Body,
    Headers,
    Cookies,
}

#[derive(Clone, Debug)]
pub struct RespState {
    pub status: String,
    pub elapsed: Option<Duration>,
    pub size: usize,
    pub tab: RespTab,
    pub body: TextArea,
    pub headers: TextArea,
    pub cookies: TextArea,
}

impl RespState {
    fn empty(status: &str) -> Self {
        Self {
            status: status.into(),
            elapsed: None,
            size: 0,
            tab: RespTab::Body,
            body: TextArea::readonly("(empty body)"),
            headers: TextArea::readonly("(no headers)"),
            cookies: TextArea::readonly("(this response set no cookies)"),
        }
    }

    fn area_mut(&mut self) -> &mut TextArea {
        match self.tab {
            RespTab::Body => &mut self.body,
            RespTab::Headers => &mut self.headers,
            RespTab::Cookies => &mut self.cookies,
        }
    }

    fn areas_mut(&mut self) -> [&mut TextArea; 3] {
        [&mut self.body, &mut self.headers, &mut self.cookies]
    }
}

fn human_size(n: usize) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1048575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1048576.0),
    }
}

#[derive(Clone, Debug)]
pub struct TabHeader {
    pub id: usize,
    pub title: String,
    pub method: HttpMethod,
    pub url: TextInput,
    pub req_tab: ReqTab,
    pub headers: TextArea,
    pub body: TextArea,
    pub sending: bool,
    pub streaming: bool,
    pub resp: Option<RespState>,
    send_requested: bool,
    sent_url: String,
    cancel: Arc<AtomicBool>,
}

impl TabHeader {
    fn area_mut(&mut self) -> &mut TextArea {
        match self.req_tab {
            ReqTab::Headers => &mut self.headers,
            ReqTab::Body => &mut self.body,
        }
    }

    fn new(id: usize, title: String, method: HttpMethod) -> Self {
        let mut url = TextInput::new("Enter request URL");
        url.edit.set_text(DEFAULT_URL);
        Self {
            id,
            title,
            method,
            url,
            req_tab: ReqTab::Headers,
            headers: TextArea::new("Header-Name: value        (start a line with # to disable it)"),
            body: TextArea::new("Request body (sent as JSON for POST / PUT / PATCH)"),
            sending: false,
            streaming: false,
            resp: None,
            send_requested: false,
            sent_url: String::new(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
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
    dd: MethodDropdown,
    mouse: (f32, f32),
    jar: HashMap<String, Vec<CookieEntry>>,
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
            dd: MethodDropdown::default(),
            mouse: (0.0, 0.0),
            jar: HashMap::new(),
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
            was |= t.url.focused | t.headers.focused | t.body.focused;
            t.url.blur();
            t.headers.blur();
            t.body.blur();
            if let Some(r) = t.resp.as_mut() {
                for a in r.areas_mut() {
                    was |= a.focused;
                    a.blur();
                }
            }
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
        self.tabs[i].cancel.store(true, Ordering::Relaxed);
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

    fn row_top(&self) -> f32 {
        TabBar::HEIGHT + 12.0
    }
    fn method_rect(&self) -> Rect {
        Rect::xywh(12.0, self.row_top(), 92.0, 34.0)
    }
    fn send_rect(&self) -> Rect {
        let r = (self.width - 12.0).max(300.0);
        Rect::new(r - 84.0, self.row_top(), r, self.row_top() + 34.0)
    }
    fn url_rect(&self) -> Rect {
        Rect::new(112.0, self.row_top(), self.send_rect().l - 8.0, self.row_top() + 34.0)
    }
    fn req_strip_top(&self) -> f32 {
        self.row_top() + 34.0 + 10.0
    }
    fn req_tab_rect(&self, i: usize) -> Rect {
        Rect::xywh(12.0 + i as f32 * 92.0, self.req_strip_top(), 90.0, 28.0)
    }
    fn editor_rect(&self) -> Rect {
        let t = self.req_strip_top() + 28.0;
        Rect::new(12.0, t, (self.width - 12.0).max(13.0), t + 140.0)
    }
    fn resp_tab_rect(&self, i: usize) -> Rect {
        let right = (self.width - 12.0).max(300.0);
        Rect::xywh(right - 3.0 * 84.0 + i as f32 * 84.0, self.status_rect().t - 2.0, 84.0, 26.0)
    }
    fn status_rect(&self) -> Rect {
        let t = self.editor_rect().b + 10.0;
        Rect::new(12.0, t, (self.width - 12.0).max(13.0), t + 22.0)
    }
    fn body_rect(&self) -> Rect {
        let t = self.status_rect().b + 6.0;
        Rect::new(12.0, t, (self.width - 12.0).max(13.0), (self.height - 12.0).max(t + 1.0))
    }

    fn hit(&self, x: f32, y: f32) -> Hit {
        TabBar::hit_test(x, y, self.tabs.len(), self.width)
    }

    // ── mouse (coordinates in DIPs) ─────────────────────────────────

    /// Returns true when a repaint is needed.
    pub fn on_mouse_move(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
        self.mouse = (x, y);
        self.over_text = self.url_rect().contains(x, y)
            || self.editor_rect().contains(x, y)
            || self.body_rect().contains(x, y);
        let hover_changed = self.bar.set_hover(self.hit(x, y));
        let t = &mut self.tabs[self.active];
        let drag_changed = t.url.mouse_move(m, x)
            | t.area_mut().mouse_move(m, x, y)
            | t.resp.as_mut().map_or(false, |r| r.area_mut().mouse_move(m, x, y));
        let dd_hover = match self.dd.hit(self.method_rect(), x, y) {
            DdHit::Item(i) => Some(i),
            _ => None,
        };
        let dd_changed = self.dd.set_hover(dd_hover);
        hover_changed || drag_changed || dd_changed
    }

    pub fn on_mouse_leave(&mut self) -> bool {
        self.over_text = false;
        self.bar.set_hover(Hit::None)
    }

    pub fn on_left_down(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32, shift: bool) -> bool {
        // An open popup owns the click, wherever it lands.
        if self.dd.open {
            match self.dd.hit(self.method_rect(), x, y) {
                DdHit::Item(i) => self.tabs[self.active].method = HttpMethod::ALL[i],
                _ => {}
            }
            self.dd.open = false;
            self.dd.set_hover(None);
            return true;
        }
        match self.hit(x, y) {
            Hit::Tab(i) => self.select_tab(i),
            Hit::Close(i) => self.close_tab(i),
            Hit::NewTab => self.new_tab(),
            Hit::None => {
                for (i, tab) in [ReqTab::Headers, ReqTab::Body].into_iter().enumerate() {
                    if self.req_tab_rect(i).contains(x, y) {
                        self.blur_all();
                        self.tabs[self.active].req_tab = tab;
                        return true;
                    }
                }
                if self.editor_rect().contains(x, y) {
                    self.blur_all();
                    self.tabs[self.active].area_mut().mouse_down(m, x, y, shift);
                    return true;
                }
                if self.tabs[self.active].resp.is_some() {
                    for (i, tab) in [RespTab::Body, RespTab::Headers, RespTab::Cookies].into_iter().enumerate() {
                        if self.resp_tab_rect(i).contains(x, y) {
                            self.blur_all();
                            self.tabs[self.active].resp.as_mut().unwrap().tab = tab;
                            return true;
                        }
                    }
                    if self.body_rect().contains(x, y) {
                        self.blur_all();
                        self.tabs[self.active].resp.as_mut().unwrap().area_mut().mouse_down(m, x, y, shift);
                        return true;
                    }
                }
                if self.method_rect().contains(x, y) {
                    self.blur_all();
                    self.dd.open = true;
                    return true;
                }
                if self.send_rect().contains(x, y) {
                    self.blur_all();
                    self.trigger_send();
                    return true;
                }
                if self.url_rect().contains(x, y) {
                    self.blur_all();
                    self.tabs[self.active].url.mouse_down(m, x, shift);
                    return true;
                }
                return self.blur_all();
            }
        }
        self.dd.open = false;
        self.bar.set_hover(self.hit(x, y));
        true
    }

    pub fn on_double_click(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
        if self.url_rect().contains(x, y) {
            self.tabs[self.active].url.double_click(m, x);
            true
        } else if self.editor_rect().contains(x, y) {
            self.tabs[self.active].area_mut().double_click(m, x, y);
            true
        } else if self.body_rect().contains(x, y) && self.tabs[self.active].resp.is_some() {
            self.tabs[self.active].resp.as_mut().unwrap().area_mut().double_click(m, x, y);
            true
        } else {
            false
        }
    }

    pub fn on_left_up(&mut self) {
        for t in &mut self.tabs {
            t.url.mouse_up();
            t.headers.mouse_up();
            t.body.mouse_up();
            if let Some(r) = t.resp.as_mut() {
                for a in r.areas_mut() {
                    a.mouse_up();
                }
            }
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
        if key == Key::Escape && self.dd.open {
            self.dd.open = false;
            return true;
        }
        if key == Key::Enter && self.tabs[self.active].url.focused {
            self.trigger_send();
            return true;
        }
        if key == Key::Escape && self.blur_all() {
            return true;
        }
        let t = &mut self.tabs[self.active];
        t.url.key(key, mods, cb)
            || t.area_mut().key(key, mods, cb)
            || t.resp.as_mut().map_or(false, |r| r.area_mut().key(key, mods, cb))
    }

    pub fn on_char(&mut self, ch: char) -> bool {
        let t = &mut self.tabs[self.active];
        t.url.char(ch) || t.area_mut().char(ch)
    }

    /// Caret blink tick; returns true when a repaint is needed.
    pub fn blink(&mut self) -> bool {
        let t = &mut self.tabs[self.active];
        t.url.toggle_blink() | t.area_mut().toggle_blink()
    }

    // ── requests ────────────────────────────────────────────────────

    /// Send, or stop a streaming response. Ignored while a plain request is in flight.
    fn trigger_send(&mut self) {
        let t = &mut self.tabs[self.active];
        if t.sending {
            if t.streaming {
                t.cancel.store(true, Ordering::Relaxed);
            }
            return;
        }
        if t.url.edit.text().trim().is_empty() {
            return;
        }
        t.send_requested = true;
        t.sending = true;
        t.streaming = false;
        t.resp = Some(RespState::empty("Sending…"));
    }

    /// The window layer polls this after each input event.
    pub fn take_send(&mut self) -> Option<SendJob> {
        let t = self.tabs.iter_mut().find(|t| t.send_requested)?;
        t.send_requested = false;
        t.cancel.store(false, Ordering::Relaxed);
        let state = SavedState {
            url: t.url.edit.text().trim().to_string(),
            method: t.method,
            headers: raw_to_headers(t.headers.edit.text()),
            body: t.body.edit.text().to_string(),
            content_type: ContentType::Json,
            ..SavedState::default()
        };
        let mut u = state.url.clone();
        if !u.contains("://") {
            u = format!("https://{u}");
        }
        t.sent_url = u;
        Some(SendJob { tab_id: t.id, state, cancel: t.cancel.clone(), jar: self.jar.clone() })
    }

    /// Engine events arrive here (posted from the runtime to the UI thread).
    /// Returns true when a repaint is needed. Events for closed tabs are dropped.
    pub fn on_response(&mut self, tab_id: usize, ev: ResponseEvent) -> bool {
        let Some(ix) = self.tabs.iter().position(|t| t.id == tab_id) else { return false };
        match ev {
            ResponseEvent::Response(r) => {
                let url = self.tabs[ix].sent_url.clone();
                merge_set_cookies(&mut self.jar, &url, &r.set_cookies);
                let t = &mut self.tabs[ix];
                t.sending = r.is_streaming;
                t.streaming = r.is_streaming;
                t.resp = Some(Self::resp_state(&r));
            }
            ResponseEvent::Chunk(s) => {
                if let Some(rs) = self.tabs[ix].resp.as_mut() {
                    rs.size += s.len();
                    rs.body.append(&s);
                }
            }
            ResponseEvent::Done => {
                let t = &mut self.tabs[ix];
                t.sending = false;
                t.streaming = false;
            }
        }
        true
    }

    fn resp_state(r: &HttpResponse) -> RespState {
        let text = if r.content_type.contains("json") {
            json_fmt::pretty(&r.body).unwrap_or_else(|| r.body.clone())
        } else {
            r.body.clone()
        };
        let mut rs = RespState::empty(&r.status);
        rs.body.set_text(&text);
        rs.headers.set_text(&r.headers);
        rs.cookies.set_text(&r.set_cookies.join("\n"));
        rs.elapsed = r.response_time;
        rs.size = if r.is_binary { r.bytes.len() } else { r.body.len() };
        rs
    }

    pub fn on_wheel(&mut self, notches: f32) -> bool {
        if self.editor_rect().contains(self.mouse.0, self.mouse.1) {
            return self.tabs[self.active].area_mut().wheel(notches);
        }
        match self.tabs[self.active].resp.as_mut() {
            Some(rs) => rs.area_mut().wheel(notches),
            None => false,
        }
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

        let (method_rect, url_rect, send_rect) = (self.method_rect(), self.url_rect(), self.send_rect());
        let (status_rect, body_rect, editor_rect) = (self.status_rect(), self.body_rect(), self.editor_rect());
        let strips = [self.req_tab_rect(0), self.req_tab_rect(1)];
        let resp_tabs = [self.resp_tab_rect(0), self.resp_tab_rect(1), self.resp_tab_rect(2)];
        let tab = &mut self.tabs[self.active];
        self.dd.paint_button(c, &self.theme, method_rect, tab.method);
        tab.url.paint(c, &self.theme, url_rect);

        for (r, (label, which)) in strips.iter().zip([("Headers", ReqTab::Headers), ("Body", ReqTab::Body)]) {
            let on = tab.req_tab == which;
            c.text(label, *r, if on { self.theme.text } else { self.theme.text_dim }, TextStyle::new(13.0).bold().align(Align::Center));
            if on {
                c.fill_rect(Rect::new(r.l + 12.0, r.b - 2.0, r.r - 12.0, r.b), self.theme.accent);
            }
        }
        let which = tab.req_tab;
        let area = tab.area_mut();
        area.paint(c, &self.theme, editor_rect);
        let _ = which;

        let (label, color) = match (tab.sending, tab.streaming) {
            (true, true) => ("Stop", self.theme.danger),
            (true, false) => ("Sending…", self.theme.text_dim),
            _ => ("Send", self.theme.accent),
        };
        c.fill_rect(send_rect, color);
        c.text(label, send_rect, self.theme.bg, TextStyle::new(13.0).bold().align(Align::Center));

        match tab.resp.as_mut() {
            Some(rs) => {
                let code = rs.status.split_whitespace().next().and_then(|s| s.parse::<u16>().ok());
                let col = match code {
                    Some(200..=299) => self.theme.method_get,
                    Some(300..=399) => self.theme.method_post,
                    Some(_) => self.theme.danger,
                    None if rs.status == "Error" => self.theme.danger,
                    None => self.theme.text_dim,
                };
                let mut line = rs.status.clone();
                if let Some(d) = rs.elapsed {
                    line = format!("{line}    {}", format_duration(d));
                }
                if rs.size > 0 {
                    line = format!("{line}    {}", human_size(rs.size));
                }
                let status_w = Rect::new(status_rect.l, status_rect.t, resp_tabs[0].l - 8.0, status_rect.b);
                c.text(&line, status_w, col, TextStyle::new(13.0).bold());
                for (r, (label, which)) in resp_tabs.iter().zip([("Body", RespTab::Body), ("Headers", RespTab::Headers), ("Cookies", RespTab::Cookies)]) {
                    let on = rs.tab == which;
                    c.text(label, *r, if on { self.theme.text } else { self.theme.text_dim }, TextStyle::new(13.0).bold().align(Align::Center));
                    if on {
                        c.fill_rect(Rect::new(r.l + 12.0, r.b - 2.0, r.r - 12.0, r.b), self.theme.accent);
                    }
                }
                rs.area_mut().paint(c, &self.theme, body_rect);
            }
            None => {
                c.fill_rect(body_rect, self.theme.bar_bg);
                c.stroke_rect(body_rect.inset(0.5), self.theme.border, 1.0);
                c.text("Enter a URL and press Send", body_rect, self.theme.text_dim, TextStyle::new(13.0).align(Align::Center));
            }
        }

        // popup last so it covers everything below it
        self.dd.paint_popup(c, &self.theme, method_rect, tab.method);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{MemClipboard, MonoCanvas, GLYPH};
    use crabipie_core::HttpResponse;

    fn click_url(a: &mut App, m: &mut MonoCanvas) {
        let (x, y) = a.url_rect().center();
        a.on_left_down(m, x, y, false);
    }

    fn app(n: usize) -> App {
        let tabs = (0..n)
            .map(|i| TabHeader::new(i, format!("T{i}"), HttpMethod::GET))
            .collect();
        let mut a = App::new(tabs, 0, 0);
        for t in &mut a.tabs {
            t.url.edit.set_text(""); // most tests start from an empty URL bar
        }
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
        assert_eq!(a.tabs[0].url.edit.text(), DEFAULT_URL, "contents reset to the default");
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
        click_url(&mut a, &mut m);
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
        click_url(&mut a, &mut m);
        assert!(a.tabs[0].url.focused);
        assert!(a.on_left_down(&mut m, 600.0, 500.0, false)); // empty body
        assert!(!a.tabs[0].url.focused);
        click_url(&mut a, &mut m);
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
        let y = a.url_rect().center().1;
        let x0 = a.url_rect().l + 10.0; // text origin = rect + PAD
        a.on_left_down(&mut m, x0, y, false);
        a.on_mouse_move(&mut m, x0 + GLYPH * 3.0, y);
        assert!(a.text_cursor_active());
        assert_eq!(a.tabs[0].url.edit.selected_text(), Some("abc"));
        a.on_left_up();
        a.on_mouse_move(&mut m, x0 + GLYPH * 6.0, y);
        assert_eq!(a.tabs[0].url.edit.selected_text(), Some("abc"), "released: no more dragging");
        a.on_mouse_move(&mut m, 600.0, 5.0); // empty part of the tab bar
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
        click_url(&mut a, &mut m);
        assert!(a.blink());
    }

    #[test]
    fn paint_emits_every_tab_title() {
        let mut a = app(3);
        let mut c = MonoCanvas::default();
        a.paint(&mut c);
        for t in ["text:T0", "text:T1", "text:T2", "text:GET", "text:+", "text:Enter request URL", "text:Send", "text:Enter a URL and press Send"] {
            assert!(c.ops.iter().any(|s| s == t), "missing {t}");
        }
    }

    fn resp(body: &str, ct: &str) -> HttpResponse {
        HttpResponse { status: "200 OK".into(), body: body.into(), content_type: ct.into(), ..Default::default() }
    }

    fn type_url(a: &mut App, m: &mut MonoCanvas, url: &str) {
        click_url(a, m);
        a.tabs[a.active].url.edit.select_all();
        for ch in url.chars() {
            a.on_char(ch);
        }
    }

    #[test]
    fn dropdown_selects_a_method_and_popup_swallows_outside_clicks() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        let (bx, by) = a.method_rect().center();
        assert!(a.on_left_down(&mut m, bx, by, false));
        assert!(a.dd.open);
        let (ix, iy) = MethodDropdown::item_rect(a.method_rect(), 1).center();
        assert!(a.on_left_down(&mut m, ix, iy, false));
        assert_eq!((a.tabs[0].method, a.dd.open), (HttpMethod::POST, false));
        // open again, click elsewhere: closes, method unchanged, tab bar not triggered
        a.on_left_down(&mut m, bx, by, false);
        let r = TabBar::tab_rect(0, 1, 1200.0);
        a.on_left_down(&mut m, r.l + 4.0, r.t + 4.0, false);
        assert!(!a.dd.open);
        assert_eq!(a.tabs[0].method, HttpMethod::POST);
        a.on_left_down(&mut m, bx, by, false);
        assert!(a.on_key(Key::Escape, Mods::default(), &mut MemClipboard::default()));
        assert!(!a.dd.open);
    }

    #[test]
    fn enter_and_send_button_queue_exactly_one_job() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        let mut cb = MemClipboard::default();
        assert!(a.take_send().is_none());
        a.on_key(Key::Enter, Mods::default(), &mut cb); // empty URL: nothing
        assert!(a.take_send().is_none());
        type_url(&mut a, &mut m, "  example.com/x  ");
        a.tabs[0].method = HttpMethod::PUT;
        assert!(a.on_key(Key::Enter, Mods::default(), &mut cb));
        let job = a.take_send().expect("job");
        assert_eq!((job.state.url.as_str(), job.state.method, job.tab_id), ("example.com/x", HttpMethod::PUT, 0));
        assert!(a.take_send().is_none());
        assert!(a.tabs[0].sending);
        // clicking Send while a plain request is in flight does nothing
        let (sx, sy) = a.send_rect().center();
        a.on_left_down(&mut m, sx, sy, false);
        assert!(a.take_send().is_none());
    }

    #[test]
    fn response_flow_pretty_prints_json_and_unlocks_send() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        type_url(&mut a, &mut m, "x.dev");
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        a.take_send();
        assert!(a.on_response(0, ResponseEvent::Response(resp(r#"{"b":1,"a":[1,2]}"#, "application/json"))));
        assert!(!a.tabs[0].sending);
        let rs = a.tabs[0].resp.as_ref().unwrap();
        assert_eq!(rs.status, "200 OK");
        assert!(rs.body.line_count() > 3, "pretty-printed over several lines");
        assert_eq!(rs.body.line(1), Some("  \"b\": 1,"));
        let mut c = MonoCanvas::default();
        a.paint(&mut c);
        assert!(c.ops.iter().any(|o| o.starts_with("text:200 OK")));
        // can send again
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        assert!(a.take_send().is_some());
    }

    #[test]
    fn streaming_chunks_append_and_stop_sets_cancel() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        type_url(&mut a, &mut m, "x.dev/sse");
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        let job = a.take_send().unwrap();
        let mut r = resp("", "text/event-stream");
        r.is_streaming = true;
        a.on_response(0, ResponseEvent::Response(r));
        assert!(a.tabs[0].sending && a.tabs[0].streaming);
        a.on_response(0, ResponseEvent::Chunk("data: 1\nda".into()));
        a.on_response(0, ResponseEvent::Chunk("ta: 2\n".into()));
        assert_eq!(a.tabs[0].resp.as_ref().unwrap().body.line(1), Some("data: 2"));
        let (sx, sy) = a.send_rect().center(); // "Stop"
        a.on_left_down(&mut m, sx, sy, false);
        assert!(job.cancel.load(Ordering::Relaxed));
        a.on_response(0, ResponseEvent::Done);
        assert!(!a.tabs[0].sending);
    }

    #[test]
    fn late_events_for_closed_tabs_are_dropped_and_close_cancels() {
        let mut a = app(2);
        let mut m = MonoCanvas::default();
        type_url(&mut a, &mut m, "x.dev");
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        let job = a.take_send().unwrap();
        a.close_tab(0);
        assert!(job.cancel.load(Ordering::Relaxed));
        assert!(!a.on_response(job.tab_id, ResponseEvent::Done));
        assert_eq!(a.tabs.len(), 1);
    }

    #[test]
    fn error_responses_show_in_red_and_wheel_scrolls_body() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        type_url(&mut a, &mut m, "x.dev");
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        a.take_send();
        let mut r = resp(&(0..500).map(|i| i.to_string()).collect::<Vec<_>>().join("\n"), "text/plain");
        r.status = "Error".into();
        a.on_response(0, ResponseEvent::Response(r));
        assert!(!a.tabs[0].sending, "errors must unlock Send");
        a.paint(&mut MonoCanvas::default()); // lets the view learn its rect
        assert!(a.on_wheel(-3.0));
    }

    #[test]
    fn new_tabs_default_the_url_and_send_carries_headers_and_body() {
        let a = App::new(vec![], 0, 0);
        assert_eq!(a.tabs[0].url.edit.text(), DEFAULT_URL);

        let mut a = app(1);
        let mut m = MonoCanvas::default();
        a.tabs[0].url.edit.set_text("x.dev");
        a.tabs[0].headers.edit.set_text("X-A: 1\n# X-B: 2");
        a.tabs[0].body.edit.set_text("{\"k\":1}");
        a.tabs[0].method = HttpMethod::POST;
        let (sx, sy) = a.send_rect().center();
        a.on_left_down(&mut m, sx, sy, false);
        let job = a.take_send().unwrap();
        assert_eq!(job.state.headers.len(), 2);
        assert!(job.state.headers[0].enabled && !job.state.headers[1].enabled);
        assert_eq!(job.state.body, "{\"k\":1}");
    }

    #[test]
    fn request_tabs_switch_editors_and_enter_in_editor_adds_a_line() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        let mut cb = MemClipboard::default();
        let (bx, by) = a.req_tab_rect(1).center();
        a.on_left_down(&mut m, bx, by, false);
        assert_eq!(a.tabs[0].req_tab, ReqTab::Body);
        let (ex, ey) = a.editor_rect().center();
        a.on_left_down(&mut m, ex, ey, false);
        assert!(a.tabs[0].body.focused);
        a.on_char('a');
        a.on_key(Key::Enter, Mods::default(), &mut cb); // newline, must NOT send
        a.on_char('b');
        assert_eq!(a.tabs[0].body.edit.text(), "a\nb");
        assert!(a.take_send().is_none());
    }

    #[test]
    fn response_tabs_show_headers_and_cookies_and_feed_the_jar() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        type_url(&mut a, &mut m, "x.dev");
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        a.take_send();
        let mut r = resp("hi", "text/plain");
        r.headers = "a: 1\nb: 2".into();
        r.set_cookies = vec!["sid=9; Path=/".into()];
        a.on_response(0, ResponseEvent::Response(r));
        a.paint(&mut MonoCanvas::default());
        let (hx, hy) = a.resp_tab_rect(1).center();
        a.on_left_down(&mut m, hx, hy, false);
        assert_eq!(a.tabs[0].resp.as_ref().unwrap().headers.line_count(), 2);
        let (cx, cy) = a.resp_tab_rect(2).center();
        a.on_left_down(&mut m, cx, cy, false);
        assert_eq!(a.tabs[0].resp.as_ref().unwrap().cookies.line(0), Some("sid=9; Path=/"));
        // the cookie rides along on the next request
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        a.tabs[0].url.focus();
        a.on_key(Key::Enter, Mods::default(), &mut MemClipboard::default());
        let job = a.take_send().unwrap();
        assert!(job.jar.values().flatten().any(|c| c.name == "sid" && c.value == "9"));
    }

    #[test]
    fn response_body_is_selectable_and_copyable_but_not_editable() {
        let mut a = app(1);
        let mut m = MonoCanvas::default();
        let mut cb = MemClipboard::default();
        let ctrl = Mods { ctrl: true, shift: false };
        type_url(&mut a, &mut m, "x.dev");
        a.on_key(Key::Enter, Mods::default(), &mut cb);
        a.take_send();
        a.on_response(0, ResponseEvent::Response(resp("hello world", "text/plain")));
        a.paint(&mut MonoCanvas::default());
        let (bx, by) = a.body_rect().center();
        a.on_left_down(&mut m, bx, by, false);
        assert!(!a.on_char('x'));
        assert!(!a.on_key(Key::Backspace, Mods::default(), &mut cb));
        assert!(a.on_key(Key::Letter('A'), ctrl, &mut cb));
        assert!(a.on_key(Key::Letter('C'), ctrl, &mut cb));
        assert_eq!(cb.0.as_deref(), Some("hello world"));
        assert_eq!(a.tabs[0].resp.as_ref().unwrap().body.edit.text(), "hello world");
    }
}
