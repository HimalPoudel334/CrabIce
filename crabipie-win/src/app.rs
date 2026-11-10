//! Application state, layout and input handling. Toolkit-free: the window
//! layer feeds it DIP-space events and hands it a `Canvas` to paint on.
//!
//! Painting registers clickable regions (`hot`) as it goes; mouse events are
//! resolved against that list, so layout lives in exactly one place.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::ui::canvas::{Align, Canvas, TextMeasure, TextStyle};
use crate::ui::geom::{Color, Rect};
use crate::ui::input::{Clipboard, Key, Mods};
use crate::ui::json_fmt;
use crate::ui::tab_bar::{Hit, TabBar, TabItem};
use crate::ui::text_area::TextArea;
use crate::ui::text_input::TextInput;
use crate::ui::theme::Theme;
use crate::ui::widgets as w;
use crabipie_core::{
    apply_auth_refresh_outcome, extract_domain, form_data_to_raw, format_duration, headers_to_raw, merge_set_cookies, parse_url_query, raw_to_form_data,
    raw_to_headers, rebuild_url, ApiKeyPosition, AuthType, ContentType, CookieEntry, FormField, FormFieldType,
    AuthBodyFormat, AuthHttpMethod, AuthRefreshConfig, AuthRefreshMode, AuthRefreshOutcome, EnvVar, Environment,
    HttpMethod, HttpResponse, QueryParam, WsBinaryMessageType, WsConnection, WsEvent, WsMessageType, WsPayload,
    encode_ws_payload, hex_dump, RequestHeaders, RequestType, ResponseEvent, SavedState,
    RAW_FORM_PLACEHOLDER,
};

pub const DEFAULT_URL: &str = "https://jsonplaceholder.typicode.com/posts";
const ROW_H: f32 = 34.0;
const ROW_STEP: f32 = 42.0;

// ───────────────────────────── model ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReqTabKind { Query, Body, Headers, Auth }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RespTab { Body, Header }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind { Param, Header, Form, EnvVar }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CfgField { LoginEndpoint, RefreshEndpoint, AccessVar, RefreshVar, AccessPath, RefreshPath, ExpiryPath, Ttl }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputId { CkDomain, CkName, CkValue, WsMsg, GqlOp, EnvName, Cfg(CfgField), Url, Key(RowKind, usize), Val(RowKind, usize), Bearer, User, Pass, ApiName, ApiVal }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaId { WsLog, GqlQuery, GqlVars, Body, RawHeaders, RawForm, Resp, LoginBody, RefreshBody }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerKind { AppTheme, WsType, WsBin, TabEnv, EnvSelect, AuthMode, LoginMethod, LoginFormat, RefreshMethod, RefreshFormat, ReqType, Method, Content, AuthType, ApiPos, HeadersView, FieldType(usize) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus { None, Input(InputId), Area(AreaId) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Stub,
    Tab(usize),
    Close(usize),
    NewTab,
    Picker(PickerKind),
    Send,
    ReqTab(ReqTabKind),
    RespTab(RespTab),
    Check(RowKind, usize),
    Remove(RowKind, usize),
    Add(RowKind),
    Input(InputId),
    Area(AreaId),
    PrettifyBody,
    ToggleFormView,
    CopyResp,
    PrettifyResp,
    ClearResp,
    ChooseFile(usize),
    WsSend,
    OpenCookies,
    CloseCookies,
    CookieAdd,
    CookiesClear,
    CookieCheck(usize, usize),
    CookieRemove(usize, usize),
    OpenEnv,
    CloseEnv,
    NewEnv,
    DeleteEnv,
    ToggleAuthSection,
    ToggleAutoRefresh,
}

struct Modal { sel: usize, auth_open: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsState { Disconnected, Connecting, Connected }

/// Work for the window layer, which owns the runtime and the socket tasks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WsCmd {
    Connect { tab_id: usize, url: String },
    Disconnect { tab_id: usize },
}

struct Hot { rect: Rect, act: Act }

struct Popup { kind: PickerKind, button: Rect, items: Vec<String>, current: usize, hover: Option<usize>, up: bool }

impl Popup {
    fn item_rect(&self, i: usize) -> Rect {
        let n = self.items.len() as f32;
        let t = if self.up {
            self.button.t - 2.0 - n * 28.0 + i as f32 * 28.0
        } else {
            self.button.b + 2.0 + i as f32 * 28.0
        };
        Rect::new(self.button.l, t, self.button.l + self.button.width().max(130.0), t + 28.0)
    }
    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        (0..self.items.len()).find(|&i| self.item_rect(i).contains(x, y))
    }
}

#[derive(Clone, Debug)]
pub struct KvRow {
    pub enabled: bool,
    pub key: TextInput,
    pub value: TextInput,
    pub ftype: FormFieldType,
    pub files: Vec<String>,
}

impl KvRow {
    fn new(k: &str, v: &str, enabled: bool) -> Self {
        let mut key = TextInput::new("key");
        key.edit.set_text(k);
        let mut value = TextInput::new("value");
        value.edit.set_text(v);
        Self { enabled, key, value, ftype: FormFieldType::Text, files: vec![] }
    }
    fn empty() -> Self {
        Self::new("", "", true)
    }
    fn blank(&self) -> bool {
        self.key.edit.is_empty() && self.value.edit.is_empty() && self.files.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct RespState {
    pub status: String,
    pub elapsed: Option<std::time::Duration>,
    pub tab: RespTab,
    pub body: TextArea,
    pub headers: TextArea,
}

impl RespState {
    fn empty(status: &str) -> Self {
        Self {
            status: status.into(),
            elapsed: None,
            tab: RespTab::Body,
            body: TextArea::readonly("(empty body)"),
            headers: TextArea::readonly("(no headers)"),
        }
    }
    fn area_mut(&mut self) -> &mut TextArea {
        match self.tab {
            RespTab::Body => &mut self.body,
            RespTab::Header => &mut self.headers,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: usize,
    pub title: String,
    pub request_type: RequestType,
    pub method: HttpMethod,
    pub env_id: Option<usize>,
    pub url: TextInput,
    pub req_tab: ReqTabKind,
    pub params: Vec<KvRow>,
    pub headers: Vec<KvRow>,
    pub headers_raw: bool,
    pub headers_area: TextArea,
    pub content_type: ContentType,
    pub body: TextArea,
    pub gql_query: TextArea,
    pub gql_vars: TextArea,
    pub gql_op: TextInput,
    pub ws_state: WsState,
    pub ws_conn: Option<WsConnection>,
    pub ws_log: TextArea,
    pub ws_msg: TextInput,
    pub ws_type: WsMessageType,
    pub ws_bin: WsBinaryMessageType,
    pub ws_sent: usize,
    pub ws_recv: usize,
    pub form: Vec<KvRow>,
    pub form_raw: bool,
    pub form_area: TextArea,
    pub auth_type: AuthType,
    pub bearer: TextInput,
    pub username: TextInput,
    pub password: TextInput,
    pub api_name: TextInput,
    pub api_val: TextInput,
    pub api_pos: ApiKeyPosition,
    pub sending: bool,
    pub streaming: bool,
    pub resp: Option<RespState>,
    send_requested: bool,
    sent_url: String,
    cancel: Arc<AtomicBool>,
}

fn input(placeholder: &str) -> TextInput {
    TextInput::new(placeholder)
}

impl Tab {
    fn new(id: usize) -> Self {
        let mut url = input("https://api.example.com/endpoint");
        url.edit.set_text(DEFAULT_URL);
        let mut t = Self {
            id,
            title: format!("Request {}", id + 1),
            request_type: RequestType::HTTP,
            method: HttpMethod::GET,
            env_id: None,
            url,
            req_tab: ReqTabKind::Query,
            params: vec![],
            headers: vec![KvRow::empty()],
            headers_raw: false,
            headers_area: TextArea::new("Header: Value"),
            content_type: ContentType::Json,
            body: TextArea::new("Request body").json(),
            gql_query: TextArea::new("query { ... }"),
            gql_vars: TextArea::new("{ \"id\": 1 }").json(),
            gql_op: input("Operation name (optional)"),
            ws_state: WsState::Disconnected,
            ws_conn: None,
            ws_log: TextArea::readonly("(no messages yet)").log(),
            ws_msg: input("Type message..."),
            ws_type: WsMessageType::Text,
            ws_bin: WsBinaryMessageType::Base64,
            ws_sent: 0,
            ws_recv: 0,
            form: vec![KvRow::empty()],
            form_raw: false,
            form_area: TextArea::new(RAW_FORM_PLACEHOLDER),
            auth_type: AuthType::None,
            bearer: input("Bearer token"),
            username: input("Username"),
            password: input("Password"),
            api_name: input("Key"),
            api_val: input("Value"),
            api_pos: ApiKeyPosition::Header,
            sending: false,
            streaming: false,
            resp: None,
            send_requested: false,
            sent_url: String::new(),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        t.params_from_url();
        t
    }

    /// Tab strip label: the URL without its scheme, like a browser tab.
    pub fn label(&self) -> String {
        let u = self.url.edit.text().trim();
        let u = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).unwrap_or(u);
        if u.is_empty() { self.title.clone() } else { u.to_string() }
    }

    fn rows_mut(&mut self, k: RowKind) -> &mut Vec<KvRow> {
        match k {
            RowKind::Param => &mut self.params,
            RowKind::Header => &mut self.headers,
            RowKind::Form => &mut self.form,
            RowKind::EnvVar => unreachable!("environment rows live on App"),
        }
    }

    fn rows(&self, k: RowKind) -> &Vec<KvRow> {
        match k {
            RowKind::Param => &self.params,
            RowKind::Header => &self.headers,
            RowKind::Form => &self.form,
            RowKind::EnvVar => unreachable!("environment rows live on App"),
        }
    }

    /// URL edited -> rebuild the query table (disabled rows are kept).
    fn params_from_url(&mut self) {
        let parsed = parse_url_query(self.url.edit.text());
        let disabled: Vec<KvRow> = self.params.iter().filter(|r| !r.enabled).cloned().collect();
        self.params = parsed.into_iter().map(|p| KvRow::new(&p.key, &p.value, true)).collect();
        self.params.extend(disabled);
        if self.params.is_empty() {
            self.params.push(KvRow::empty());
        }
    }

    /// Query table edited -> rewrite the URL's query string.
    fn url_from_params(&mut self) {
        let q: Vec<QueryParam> = self
            .params
            .iter()
            .map(|r| QueryParam { key: r.key.edit.text().into(), value: r.value.edit.text().into(), enabled: r.enabled })
            .collect();
        let new = rebuild_url(self.url.edit.text(), &q);
        self.url.edit.set_text(&new);
    }

    fn header_list(&self) -> Vec<RequestHeaders> {
        if self.headers_raw {
            return raw_to_headers(self.headers_area.edit.text());
        }
        self.headers
            .iter()
            .filter(|r| !r.blank())
            .map(|r| RequestHeaders { key: r.key.edit.text().into(), value: r.value.edit.text().into(), enabled: r.enabled })
            .collect()
    }

    fn form_list(&self) -> Vec<FormField> {
        if self.form_raw {
            return raw_to_form_data(self.form_area.edit.text());
        }
        self.form
            .iter()
            .filter(|r| !r.blank())
            .map(|r| FormField {
                enabled: r.enabled,
                key: r.key.edit.text().into(),
                value: r.value.edit.text().into(),
                files: r.files.clone(),
                field_type: r.ftype,
            })
            .collect()
    }

    fn set_headers_raw(&mut self, raw: bool) {
        if raw == self.headers_raw {
            return;
        }
        if raw {
            let text = headers_to_raw(&self.header_list());
            self.headers_area.set_text(&text);
        } else {
            let rows = raw_to_headers(self.headers_area.edit.text());
            self.headers = rows.iter().map(|h| KvRow::new(&h.key, &h.value, h.enabled)).collect();
            if self.headers.is_empty() {
                self.headers.push(KvRow::empty());
            }
        }
        self.headers_raw = raw;
    }

    fn set_form_raw(&mut self, raw: bool) {
        if raw == self.form_raw {
            return;
        }
        if raw {
            let text = form_data_to_raw(&self.form_list());
            self.form_area.set_text(&text);
        } else {
            let rows = raw_to_form_data(self.form_area.edit.text());
            self.form = rows.iter().map(|f| KvRow::new(&f.key, &f.value, f.enabled)).collect();
            if self.form.is_empty() {
                self.form.push(KvRow::empty());
            }
        }
        self.form_raw = raw;
    }
}

/// What the window layer must run: build + execute this request on the runtime.
pub struct SendJob {
    pub tab_id: usize,
    pub state: SavedState,
    pub cancel: Arc<AtomicBool>,
    pub jar: HashMap<String, Vec<CookieEntry>>,
    pub env: Option<Environment>,
}

// ───────────────────────────── app ─────────────────────────────

pub struct App {
    pub theme: Theme,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub next_id: usize,
    bar: TabBar,
    width: f32,
    height: f32,
    hot: Vec<Hot>,
    clip: Option<Rect>,
    popup: Option<Popup>,
    focus: Focus,
    mouse: (f32, f32),
    req_panel: Rect,
    resp_panel: Rect,
    req_scroll: f32,
    req_content_h: f32,
    req_view_h: f32,
    jar: HashMap<String, Vec<CookieEntry>>,
    file_request: Option<usize>,
    pub envs: Vec<Environment>,
    next_env_id: usize,
    modal: Option<Modal>,
    modal_hot_from: usize,
    env_name: TextInput,
    env_rows: Vec<KvRow>,
    cfg_in: Vec<TextInput>,
    login_area: TextArea,
    refresh_area: TextArea,
    modal_scroll: f32,
    modal_content_h: f32,
    modal_view_h: f32,
    ws_cmds: Vec<WsCmd>,
    cookie_modal: bool,
    ck_domain: TextInput,
    ck_name: TextInput,
    ck_value: TextInput,
    clock: fn() -> (u8, u8, u8, u16),
}

/// Default clock (UTC); the window layer swaps in local time.
fn utc_clock() -> (u8, u8, u8, u16) {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let s = d.as_secs() % 86_400;
    ((s / 3600) as u8, ((s % 3600) / 60) as u8, (s % 60) as u8, d.subsec_millis() as u16)
}

impl App {
    pub fn new(mut tabs: Vec<Tab>, active: usize, next_id: usize) -> Self {
        let next_id = next_id.max(tabs.iter().map(|t| t.id + 1).max().unwrap_or(0));
        if tabs.is_empty() {
            tabs.push(Tab::new(next_id));
        }
        let next_id = next_id.max(tabs.iter().map(|t| t.id + 1).max().unwrap_or(0));
        Self {
            theme: Theme::default(),
            active: active.min(tabs.len() - 1),
            tabs,
            next_id,
            bar: TabBar::new(),
            width: 0.0,
            height: 0.0,
            hot: Vec::new(),
            clip: None,
            popup: None,
            focus: Focus::None,
            mouse: (0.0, 0.0),
            req_panel: Rect::default(),
            resp_panel: Rect::default(),
            req_scroll: 0.0,
            req_content_h: 0.0,
            req_view_h: 0.0,
            jar: HashMap::new(),
            file_request: None,
            envs: Vec::new(),
            next_env_id: 0,
            modal: None,
            modal_hot_from: 0,
            env_name: input("Environment name"),
            env_rows: Vec::new(),
            cfg_in: ["https://api.example.com/login", "https://api.example.com/refresh", "access_token", "refresh_token", "access_token", "refresh_token", "expires_in", "3600"]
                .iter()
                .map(|p| input(p))
                .collect(),
            login_area: TextArea::new("Login body template"),
            refresh_area: TextArea::new("Refresh body template"),
            modal_scroll: 0.0,
            modal_content_h: 0.0,
            modal_view_h: 0.0,
            ws_cmds: Vec::new(),
            cookie_modal: false,
            ck_domain: input("example.com"),
            ck_name: input("name"),
            ck_value: input("value"),
            clock: utc_clock,
        }
    }

    /// Reads the existing ~/.crabipie session (read-only for now: this shell
    /// never writes it, so it can't disturb the iced app's saved tabs).
    pub fn from_disk(rt: &tokio::runtime::Runtime) -> Self {
        let (session, meta) = rt.block_on(crabipie_core::load_app_state());
        let tabs = meta
            .into_iter()
            .map(|m| {
                let mut t = Tab::new(m.id);
                t.title = m.title;
                t.method = m.method;
                t
            })
            .collect();
        let (active, next_id) = session.as_ref().map(|s| (s.active_tab, s.next_tab_id)).unwrap_or((0, 0));
        let mut app = Self::new(tabs, active, next_id);
        if let Some(s) = session {
            app.next_env_id = s.next_environment_id.max(s.environments.iter().map(|e| e.id + 1).max().unwrap_or(0));
            app.envs = s.environments;
        }
        app
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }
    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    // ── focus ───────────────────────────────────────────────────────

    fn input_mut(&mut self, id: InputId) -> Option<&mut TextInput> {
        match id {
            InputId::EnvName => return Some(&mut self.env_name),
            InputId::CkDomain => return Some(&mut self.ck_domain),
            InputId::CkName => return Some(&mut self.ck_name),
            InputId::CkValue => return Some(&mut self.ck_value),
            InputId::Cfg(f) => return self.cfg_in.get_mut(f as usize),
            InputId::Key(RowKind::EnvVar, i) => return self.env_rows.get_mut(i).map(|r| &mut r.key),
            InputId::Val(RowKind::EnvVar, i) => return self.env_rows.get_mut(i).map(|r| &mut r.value),
            _ => {}
        }
        let t = &mut self.tabs[self.active];
        match id {
            InputId::Url => Some(&mut t.url),
            InputId::Key(k, i) => t.rows_mut(k).get_mut(i).map(|r| &mut r.key),
            InputId::Val(k, i) => t.rows_mut(k).get_mut(i).map(|r| &mut r.value),
            InputId::Bearer => Some(&mut t.bearer),
            InputId::User => Some(&mut t.username),
            InputId::Pass => Some(&mut t.password),
            InputId::ApiName => Some(&mut t.api_name),
            InputId::ApiVal => Some(&mut t.api_val),
            InputId::GqlOp => Some(&mut t.gql_op),
            InputId::WsMsg => Some(&mut t.ws_msg),
            InputId::EnvName | InputId::Cfg(_) | InputId::CkDomain | InputId::CkName | InputId::CkValue => None, // handled above
        }
    }

    fn area_mut(&mut self, id: AreaId) -> Option<&mut TextArea> {
        let t = &mut self.tabs[self.active];
        match id {
            AreaId::Body => Some(&mut t.body),
            AreaId::GqlQuery => Some(&mut t.gql_query),
            AreaId::WsLog => Some(&mut t.ws_log),
            AreaId::GqlVars => Some(&mut t.gql_vars),
            AreaId::RawHeaders => Some(&mut t.headers_area),
            AreaId::RawForm => Some(&mut t.form_area),
            AreaId::Resp => t.resp.as_mut().map(|r| r.area_mut()),
            AreaId::LoginBody => Some(&mut self.login_area),
            AreaId::RefreshBody => Some(&mut self.refresh_area),
        }
    }

    fn set_focus(&mut self, f: Focus) -> bool {
        let was = self.focus != Focus::None;
        match self.focus {
            Focus::Input(id) => {
                if let Some(i) = self.input_mut(id) {
                    i.blur();
                }
            }
            Focus::Area(id) => {
                if let Some(a) = self.area_mut(id) {
                    a.blur();
                }
            }
            Focus::None => {}
        }
        self.focus = f;
        match f {
            Focus::Input(id) => {
                if let Some(i) = self.input_mut(id) {
                    i.focus();
                }
            }
            Focus::Area(id) => {
                if let Some(a) = self.area_mut(id) {
                    a.focus();
                }
            }
            Focus::None => {}
        }
        was
    }

    fn focused_text(&mut self) -> Option<String> {
        match self.focus {
            Focus::Input(id) => self.input_mut(id).map(|i| i.edit.text().to_string()),
            Focus::Area(id) => self.area_mut(id).map(|a| a.edit.text().to_string()),
            Focus::None => None,
        }
    }

    /// After a text edit: keep the URL and the query table in sync.
    fn after_edit(&mut self, before: Option<String>) {
        let Some(before) = before else { return };
        let Some(now) = self.focused_text() else { return };
        if now == before {
            return;
        }
        match self.focus {
            Focus::Input(InputId::Url) => self.tab_mut().params_from_url(),
            Focus::Input(InputId::Key(RowKind::Param, _)) | Focus::Input(InputId::Val(RowKind::Param, _)) => {
                self.tab_mut().url_from_params()
            }
            Focus::Input(InputId::EnvName)
            | Focus::Input(InputId::Key(RowKind::EnvVar, _))
            | Focus::Input(InputId::Val(RowKind::EnvVar, _)) => self.store_env_basic(),
            Focus::Input(InputId::Cfg(_)) | Focus::Area(AreaId::LoginBody) | Focus::Area(AreaId::RefreshBody) => self.store_cfg(),
            _ => {}
        }
    }

    // ── environments (modal state <-> Environment) ───────────────────

    fn rows_of(&mut self, k: RowKind) -> &mut Vec<KvRow> {
        if k == RowKind::EnvVar {
            &mut self.env_rows
        } else {
            self.tabs[self.active].rows_mut(k)
        }
    }

    fn rows_ref(&self, k: RowKind) -> &Vec<KvRow> {
        if k == RowKind::EnvVar {
            &self.env_rows
        } else {
            self.tabs[self.active].rows(k)
        }
    }

    fn cfg_mut(&mut self) -> Option<&mut AuthRefreshConfig> {
        let sel = self.modal.as_ref()?.sel;
        self.envs.get_mut(sel).map(|e| e.auth_config.get_or_insert_with(Default::default))
    }

    fn cfg_ref(&self) -> AuthRefreshConfig {
        self.modal
            .as_ref()
            .and_then(|m| self.envs.get(m.sel))
            .and_then(|e| e.auth_config.clone())
            .unwrap_or_default()
    }

    fn load_env_widgets(&mut self) {
        let Some(env) = self.modal.as_ref().and_then(|m| self.envs.get(m.sel)).cloned() else {
            self.env_rows.clear();
            return;
        };
        self.env_name.edit.set_text(&env.name);
        self.env_rows = env.variables.iter().map(|v| KvRow::new(&v.key, &v.value, v.enabled)).collect();
        let c = env.auth_config.clone().unwrap_or_default();
        let texts = [
            c.login_endpoint.clone(),
            c.refresh_endpoint.clone(),
            c.access_token_var.clone(),
            c.refresh_token_var.clone(),
            c.access_token_path.clone(),
            c.refresh_token_path.clone(),
            c.expiry_path.clone(),
            c.default_ttl_secs.to_string(),
        ];
        for (i, t) in texts.iter().enumerate() {
            self.cfg_in[i].edit.set_text(t);
        }
        self.login_area.set_text(&c.login_body_template);
        self.refresh_area.set_text(&c.refresh_body_template);
    }

    fn store_env_basic(&mut self) {
        let Some(sel) = self.modal.as_ref().map(|m| m.sel) else { return };
        let name = self.env_name.edit.text().to_string();
        let vars: Vec<EnvVar> = self
            .env_rows
            .iter()
            .filter(|r| !r.blank())
            .map(|r| EnvVar { key: r.key.edit.text().into(), value: r.value.edit.text().into(), enabled: r.enabled })
            .collect();
        if let Some(env) = self.envs.get_mut(sel) {
            env.name = name;
            env.variables = vars;
        }
    }

    fn store_cfg(&mut self) {
        let t: Vec<String> = self.cfg_in.iter().map(|i| i.edit.text().to_string()).collect();
        let (lb, rb) = (self.login_area.edit.text().to_string(), self.refresh_area.edit.text().to_string());
        if let Some(c) = self.cfg_mut() {
            c.login_endpoint = t[0].clone();
            c.refresh_endpoint = t[1].clone();
            c.access_token_var = t[2].clone();
            c.refresh_token_var = t[3].clone();
            c.access_token_path = t[4].clone();
            c.refresh_token_path = t[5].clone();
            c.expiry_path = t[6].clone();
            c.default_ttl_secs = t[7].trim().parse().unwrap_or(c.default_ttl_secs);
            c.login_body_template = lb;
            c.refresh_body_template = rb;
        }
    }

    /// A login/refresh finished on the runtime: store the token in our copy of the env.
    pub fn on_token(&mut self, env_id: usize, res: Result<AuthRefreshOutcome, String>) -> bool {
        let Some(env) = self.envs.iter_mut().find(|e| e.id == env_id) else { return false };
        match res {
            Ok(o) => apply_auth_refresh_outcome(env, &o),
            Err(e) => env.auth_last_error = Some(e),
        }
        if self.modal.is_some() && self.focus == Focus::None {
            self.load_env_widgets();
        }
        true
    }

    // ── tabs ────────────────────────────────────────────────────────

    pub fn new_tab(&mut self) {
        self.set_focus(Focus::None);
        let t = Tab::new(self.next_id);
        self.next_id += 1;
        self.tabs.push(t);
        self.active = self.tabs.len() - 1;
        self.req_scroll = 0.0;
        self.set_focus(Focus::Input(InputId::Url));
    }

    pub fn select_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.set_focus(Focus::None);
            self.active = i;
            self.req_scroll = 0.0;
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.set_focus(Focus::None);
        self.tabs[i].cancel.store(true, Ordering::Relaxed);
        self.ws_disconnect_ix(i);
        if self.tabs.len() == 1 {
            // Never leave the window empty: reset in place (same id, no counting up).
            let id = self.tabs[0].id;
            self.tabs[0] = Tab::new(id);
            self.active = 0;
            return;
        }
        self.tabs.remove(i);
        if i < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.req_scroll = 0.0;
    }

    pub fn on_resize(&mut self, w: f32, h: f32) {
        self.width = w;
        self.height = h;
    }

    // ── requests ────────────────────────────────────────────────────

    fn trigger_send(&mut self) {
        if self.tab().request_type == RequestType::WebSocket {
            self.ws_toggle();
            return;
        }
        let t = self.tab_mut();
        if t.sending {
            if t.streaming {
                t.cancel.store(true, Ordering::Relaxed);
            }
            return;
        }
        if t.url.edit.text().trim().is_empty() {
            return;
        }
        if t.request_type == RequestType::GRPC {
            let mut rs = RespState::empty("Not available yet");
            rs.body.set_text("gRPC support is not implemented yet.");
            t.resp = Some(rs);
            return;
        }
        t.send_requested = true;
        t.sending = true;
        t.streaming = false;
        t.resp = Some(RespState::empty("Sending…"));
    }

    /// The window layer polls this after each input event.
    pub fn take_send(&mut self) -> Option<SendJob> {
        let jar = self.jar.clone();
        let ix = self.tabs.iter().position(|t| t.send_requested)?;
        let env = self.tabs[ix].env_id.and_then(|id| self.envs.iter().find(|e| e.id == id).cloned());
        let t = &mut self.tabs[ix];
        t.send_requested = false;
        t.cancel.store(false, Ordering::Relaxed);
        let state = SavedState {
            url: t.url.edit.text().trim().to_string(),
            method: t.method,
            request_type: t.request_type,
            graphql_query: t.gql_query.edit.text().to_string(),
            graphql_variables: t.gql_vars.edit.text().to_string(),
            graphql_operation: t.gql_op.edit.text().to_string(),
            headers: t.header_list(),
            body: t.body.edit.text().to_string(),
            content_type: t.content_type,
            form_data: t.form_list(),
            auth_type: t.auth_type,
            username: t.username.edit.text().to_string(),
            password: t.password.edit.text().to_string(),
            bearer_token: t.bearer.edit.text().to_string(),
            api_key_name: t.api_name.edit.text().to_string(),
            api_key: t.api_val.edit.text().to_string(),
            api_key_position: t.api_pos,
            ..SavedState::default()
        };
        let mut u = state.url.clone();
        if !u.contains("://") {
            u = format!("https://{u}");
        }
        t.sent_url = u;
        Some(SendJob { tab_id: t.id, state, cancel: t.cancel.clone(), jar, env })
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
        let is_json = r.content_type.contains("json");
        let pretty = if is_json { json_fmt::pretty(&r.body) } else { None };
        let mut rs = RespState::empty(&r.status);
        rs.body.set_text(pretty.as_deref().unwrap_or(&r.body));
        rs.body.set_highlight(is_json);
        rs.headers.set_text(&r.headers);
        rs.elapsed = r.response_time;
        rs
    }

    // ── cookie manager ──────────────────────────────────────────────

    fn any_modal(&self) -> bool {
        self.modal.is_some() || self.cookie_modal
    }

    fn sorted_domains(&self) -> Vec<String> {
        let mut v: Vec<String> = self.jar.keys().cloned().collect();
        v.sort();
        v
    }

    fn cookie_at(&mut self, d: usize, i: usize) -> Option<&mut CookieEntry> {
        let dom = self.sorted_domains().get(d)?.clone();
        self.jar.get_mut(&dom)?.get_mut(i)
    }

    /// Adds (or replaces, by name) a cookie for the typed domain.
    fn cookie_add(&mut self) {
        let raw = self.ck_domain.edit.text().trim().to_string();
        let name = self.ck_name.edit.text().trim().to_string();
        if raw.is_empty() || name.is_empty() {
            return;
        }
        let domain = if raw.contains("://") {
            extract_domain(&raw).unwrap_or_else(|| raw.clone())
        } else {
            raw.trim_matches('/').to_string()
        };
        let entry = CookieEntry {
            name: name.clone(),
            value: self.ck_value.edit.text().to_string(),
            enabled: true,
            domain: domain.clone(),
            path: "/".into(),
            expires: None,
        };
        let list = self.jar.entry(domain).or_default();
        match list.iter_mut().find(|c| c.name == name) {
            Some(existing) => *existing = entry,
            None => list.push(entry),
        }
        self.ck_name.edit.set_text("");
        self.ck_value.edit.set_text("");
    }

    // ── websocket ───────────────────────────────────────────────────

    pub fn set_clock(&mut self, f: fn() -> (u8, u8, u8, u16)) {
        self.clock = f;
    }

    fn stamp(&self) -> String {
        let (h, m, s, ms) = (self.clock)();
        format!("{h:02}:{m:02}:{s:02}.{ms:03}")
    }

    fn log_line(t: &mut Tab, line: &str) {
        t.ws_log.append(line);
        t.ws_log.append("\n");
        t.ws_log.scroll_to_end();
    }

    fn ws_toggle(&mut self) {
        let ix = self.active;
        if self.tabs[ix].ws_state != WsState::Disconnected {
            self.ws_disconnect_ix(ix);
            return;
        }
        let raw = self.tabs[ix].url.edit.text().trim().to_string();
        if raw.is_empty() {
            return;
        }
        let url = if raw.contains("://") { raw } else { format!("wss://{raw}") };
        let ts = self.stamp();
        let t = &mut self.tabs[ix];
        t.ws_state = WsState::Connecting;
        Self::log_line(t, &format!("{ts} … Connecting to {url}"));
        let tab_id = t.id;
        self.ws_cmds.push(WsCmd::Connect { tab_id, url });
    }

    fn ws_disconnect_ix(&mut self, ix: usize) {
        if self.tabs[ix].ws_state == WsState::Disconnected {
            return;
        }
        let ts = self.stamp();
        let t = &mut self.tabs[ix];
        t.ws_state = WsState::Disconnected;
        t.ws_conn = None;
        Self::log_line(t, &format!("{ts} ○ Disconnected"));
        let tab_id = t.id;
        self.ws_cmds.push(WsCmd::Disconnect { tab_id });
    }

    /// Sends the composer's text, encoded per the chosen message type.
    fn ws_send(&mut self) {
        let ts = self.stamp();
        let t = self.tab_mut();
        if t.ws_state != WsState::Connected {
            return;
        }
        let text = t.ws_msg.edit.text().to_string();
        if text.is_empty() {
            return;
        }
        let payload = match encode_ws_payload(t.ws_type, t.ws_bin, &text) {
            Ok(p) => p,
            Err(e) => {
                Self::log_line(t, &format!("{ts} ! {e}"));
                return;
            }
        };
        let sent: Result<(), String> = match (&payload, t.ws_conn.as_mut()) {
            (_, None) => Err("not connected".to_string()),
            (WsPayload::Text(s), Some(c)) => c.send(s.clone()),
            (WsPayload::Binary(b), Some(c)) => c.send_binary(b.clone()),
        };
        match sent {
            Ok(()) => {
                let shown = match &payload {
                    WsPayload::Binary(b) => format!("[Binary] {} bytes: {}", b.len(), hex_dump(b, 32)),
                    WsPayload::Text(s) => match t.ws_type {
                        WsMessageType::Json => format!("[Json] {s}"),
                        WsMessageType::HTML => format!("[HTML] {s}"),
                        _ => s.clone(),
                    },
                };
                Self::log_line(t, &format!("{ts} → {shown}"));
                t.ws_sent += 1;
                t.ws_msg.edit.set_text("");
            }
            Err(e) => Self::log_line(t, &format!("{ts} ! send failed: {e}")),
        }
    }

    /// Socket commands the window layer must carry out.
    pub fn take_ws_cmds(&mut self) -> Vec<WsCmd> {
        std::mem::take(&mut self.ws_cmds)
    }

    /// Events from a socket task. Late events after a user disconnect are ignored.
    pub fn on_ws_event(&mut self, tab_id: usize, ev: WsEvent) -> bool {
        let ts = self.stamp();
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) else { return false };
        if t.ws_state == WsState::Disconnected {
            return false;
        }
        match ev {
            WsEvent::Connected(conn) => {
                t.ws_state = WsState::Connected;
                t.ws_conn = Some(conn);
                Self::log_line(t, &format!("{ts} ● Connected"));
            }
            WsEvent::MessageReceived(s) => {
                t.ws_recv += 1;
                let body = json_fmt::pretty(&s).unwrap_or(s);
                Self::log_line(t, &format!("{ts} ← {body}"));
            }
            WsEvent::BinaryReceived(b) => {
                t.ws_recv += 1;
                Self::log_line(t, &format!("{ts} ← [binary {} bytes] {}", b.len(), hex_dump(&b, 32)));
            }
            WsEvent::Disconnected(r) => {
                t.ws_state = WsState::Disconnected;
                t.ws_conn = None;
                Self::log_line(t, &format!("{ts} ○ Disconnected: {r}"));
            }
            WsEvent::Error(e) => Self::log_line(t, &format!("{ts} ! {e}")),
        }
        true
    }

    /// The socket task finished (retries exhausted or connection closed).
    pub fn on_ws_ended(&mut self, tab_id: usize) -> bool {
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) else { return false };
        if t.ws_state == WsState::Disconnected {
            return false;
        }
        t.ws_state = WsState::Disconnected;
        t.ws_conn = None;
        true
    }

    pub fn take_file_request(&mut self) -> Option<usize> {
        self.file_request.take()
    }

    pub fn set_form_file(&mut self, row: usize, path: String) {
        if let Some(r) = self.tab_mut().form.get_mut(row) {
            r.files = vec![path];
        }
    }

    // ── actions ─────────────────────────────────────────────────────

    fn picker_items(&self, k: PickerKind) -> (Vec<String>, usize) {
        let t = self.tab();
        fn names<T: ToString>(all: &[T]) -> Vec<String> {
            all.iter().map(|x| x.to_string()).collect()
        }
        match k {
            PickerKind::ReqType => (names(&RequestType::ALL), RequestType::ALL.iter().position(|x| *x == t.request_type).unwrap_or(0)),
            PickerKind::Method => (names(&HttpMethod::ALL), HttpMethod::ALL.iter().position(|x| *x == t.method).unwrap_or(0)),
            PickerKind::Content => (names(&ContentType::ALL), ContentType::ALL.iter().position(|x| *x == t.content_type).unwrap_or(0)),
            PickerKind::AuthType => (names(&AuthType::ALL), AuthType::ALL.iter().position(|x| *x == t.auth_type).unwrap_or(0)),
            PickerKind::ApiPos => (names(&ApiKeyPosition::ALL), ApiKeyPosition::ALL.iter().position(|x| *x == t.api_pos).unwrap_or(0)),
            PickerKind::HeadersView => (vec!["Formatted".into(), "Raw".into()], t.headers_raw as usize),
            PickerKind::AppTheme => (Theme::NAMES.iter().map(|n| n.to_string()).collect(), Theme::NAMES.iter().position(|n| *n == self.theme.name).unwrap_or(0)),
            PickerKind::WsType => (names(&WsMessageType::ALL), WsMessageType::ALL.iter().position(|x| *x == t.ws_type).unwrap_or(0)),
            PickerKind::WsBin => (names(&WsBinaryMessageType::ALL), WsBinaryMessageType::ALL.iter().position(|x| *x == t.ws_bin).unwrap_or(0)),
            PickerKind::TabEnv => {
                let mut v = vec!["No Environment".to_string()];
                v.extend(self.envs.iter().map(|e| e.name.clone()));
                let cur = t.env_id.and_then(|id| self.envs.iter().position(|e| e.id == id)).map_or(0, |i| i + 1);
                (v, cur)
            }
            PickerKind::EnvSelect => (self.envs.iter().map(|e| e.name.clone()).collect(), self.modal.as_ref().map_or(0, |m| m.sel)),
            PickerKind::AuthMode => (names(&AuthRefreshMode::ALL), AuthRefreshMode::ALL.iter().position(|x| *x == self.cfg_ref().mode).unwrap_or(0)),
            PickerKind::LoginMethod => (names(&AuthHttpMethod::ALL), AuthHttpMethod::ALL.iter().position(|x| *x == self.cfg_ref().login_method).unwrap_or(0)),
            PickerKind::LoginFormat => (names(&AuthBodyFormat::ALL), AuthBodyFormat::ALL.iter().position(|x| *x == self.cfg_ref().login_body_format).unwrap_or(0)),
            PickerKind::RefreshMethod => (names(&AuthHttpMethod::ALL), AuthHttpMethod::ALL.iter().position(|x| *x == self.cfg_ref().refresh_method).unwrap_or(0)),
            PickerKind::RefreshFormat => (names(&AuthBodyFormat::ALL), AuthBodyFormat::ALL.iter().position(|x| *x == self.cfg_ref().refresh_body_format).unwrap_or(0)),
            PickerKind::FieldType(i) => (
                names(&FormFieldType::ALL),
                t.form.get(i).and_then(|r| FormFieldType::ALL.iter().position(|x| *x == r.ftype)).unwrap_or(0),
            ),
        }
    }

    fn apply_picker(&mut self, k: PickerKind, idx: usize) {
        match k {
            PickerKind::AppTheme => {
                self.theme = Theme::named(Theme::NAMES[idx]);
                return;
            }
            PickerKind::WsType => {
                self.tab_mut().ws_type = WsMessageType::ALL[idx];
                return;
            }
            PickerKind::WsBin => {
                self.tab_mut().ws_bin = WsBinaryMessageType::ALL[idx];
                return;
            }
            PickerKind::TabEnv => {
                let id = if idx == 0 { None } else { self.envs.get(idx - 1).map(|e| e.id) };
                self.tab_mut().env_id = id;
                return;
            }
            PickerKind::ReqType => {
                let ix = self.active;
                let new = RequestType::ALL[idx];
                if self.tabs[ix].request_type == RequestType::WebSocket && new != RequestType::WebSocket {
                    self.ws_disconnect_ix(ix);
                }
                self.tabs[ix].request_type = new;
                return;
            }
            PickerKind::EnvSelect => {
                if let Some(m) = self.modal.as_mut() {
                    m.sel = idx;
                }
                self.modal_scroll = 0.0;
                self.load_env_widgets();
                return;
            }
            PickerKind::AuthMode | PickerKind::LoginMethod | PickerKind::LoginFormat | PickerKind::RefreshMethod | PickerKind::RefreshFormat => {
                if let Some(c) = self.cfg_mut() {
                    match k {
                        PickerKind::AuthMode => c.mode = AuthRefreshMode::ALL[idx],
                        PickerKind::LoginMethod => c.login_method = AuthHttpMethod::ALL[idx],
                        PickerKind::LoginFormat => c.login_body_format = AuthBodyFormat::ALL[idx],
                        PickerKind::RefreshMethod => c.refresh_method = AuthHttpMethod::ALL[idx],
                        _ => c.refresh_body_format = AuthBodyFormat::ALL[idx],
                    }
                }
                return;
            }
            _ => {}
        }
        let t = self.tab_mut();
        match k {
            PickerKind::ReqType => t.request_type = RequestType::ALL[idx],
            PickerKind::Method => t.method = HttpMethod::ALL[idx],
            PickerKind::Content => t.content_type = ContentType::ALL[idx],
            PickerKind::AuthType => t.auth_type = AuthType::ALL[idx],
            PickerKind::ApiPos => t.api_pos = ApiKeyPosition::ALL[idx],
            PickerKind::HeadersView => t.set_headers_raw(idx == 1),
            PickerKind::FieldType(i) => {
                if let Some(r) = t.form.get_mut(i) {
                    r.ftype = FormFieldType::ALL[idx];
                }
            }
            _ => {}
        }
    }

    fn run(&mut self, act: Act, rect: Rect, cb: &mut dyn Clipboard) {
        match act {
            Act::Stub | Act::Input(_) | Act::Area(_) => {}
            Act::Tab(i) => self.select_tab(i),
            Act::Close(i) => self.close_tab(i),
            Act::NewTab => self.new_tab(),
            Act::Picker(k) => {
                let (items, current) = self.picker_items(k);
                let up = rect.b + 2.0 + items.len() as f32 * 28.0 > self.height;
                self.popup = Some(Popup { kind: k, button: rect, items, current, hover: None, up });
            }
            Act::Send => self.trigger_send(),
            Act::ReqTab(k) => {
                self.tab_mut().req_tab = k;
                self.req_scroll = 0.0;
            }
            Act::RespTab(k) => {
                if let Some(r) = self.tab_mut().resp.as_mut() {
                    r.tab = k;
                }
            }
            Act::Check(k, i) => {
                if let Some(r) = self.rows_of(k).get_mut(i) {
                    r.enabled = !r.enabled;
                }
                match k {
                    RowKind::Param => self.tab_mut().url_from_params(),
                    RowKind::EnvVar => self.store_env_basic(),
                    _ => {}
                }
            }
            Act::Remove(k, i) => {
                let rows = self.rows_of(k);
                if i < rows.len() {
                    rows.remove(i);
                }
                match k {
                    RowKind::Param => self.tab_mut().url_from_params(),
                    RowKind::EnvVar => self.store_env_basic(),
                    _ => {}
                }
            }
            Act::Add(k) => self.rows_of(k).push(KvRow::empty()),
            Act::WsSend => self.ws_send(),
            Act::OpenCookies => {
                self.modal = None;
                self.cookie_modal = true;
                self.modal_scroll = 0.0;
            }
            Act::CloseCookies => self.cookie_modal = false,
            Act::CookieAdd => self.cookie_add(),
            Act::CookiesClear => self.jar.clear(),
            Act::CookieCheck(d, i) => {
                if let Some(c) = self.cookie_at(d, i) {
                    c.enabled = !c.enabled;
                }
            }
            Act::CookieRemove(d, i) => {
                if let Some(dom) = self.sorted_domains().get(d).cloned() {
                    if let Some(v) = self.jar.get_mut(&dom) {
                        if i < v.len() {
                            v.remove(i);
                        }
                        if v.is_empty() {
                            self.jar.remove(&dom);
                        }
                    }
                }
            }
            Act::OpenEnv => {
                self.cookie_modal = false;
                let sel = self.tab().env_id.and_then(|id| self.envs.iter().position(|e| e.id == id)).unwrap_or(0);
                self.modal = Some(Modal { sel, auth_open: false });
                self.modal_scroll = 0.0;
                self.load_env_widgets();
            }
            Act::CloseEnv => self.modal = None,
            Act::NewEnv => {
                let id = self.next_env_id;
                self.next_env_id += 1;
                let n = self.envs.len() + 1;
                self.envs.push(Environment::new(id, format!("Environment {n}")));
                let last = self.envs.len() - 1;
                if let Some(m) = self.modal.as_mut() {
                    m.sel = last;
                }
                self.modal_scroll = 0.0;
                self.load_env_widgets();
            }
            Act::DeleteEnv => {
                if let Some(sel) = self.modal.as_ref().map(|m| m.sel) {
                    if sel < self.envs.len() {
                        let gone = self.envs.remove(sel).id;
                        for t in &mut self.tabs {
                            if t.env_id == Some(gone) {
                                t.env_id = None;
                            }
                        }
                        let max = self.envs.len().saturating_sub(1);
                        if let Some(m) = self.modal.as_mut() {
                            m.sel = m.sel.min(max);
                        }
                        self.load_env_widgets();
                    }
                }
            }
            Act::ToggleAuthSection => {
                if let Some(m) = self.modal.as_mut() {
                    m.auth_open = !m.auth_open;
                }
            }
            Act::ToggleAutoRefresh => {
                if let Some(c) = self.cfg_mut() {
                    c.auto_refresh_enabled = !c.auto_refresh_enabled;
                }
            }
            Act::PrettifyBody => {
                let t = self.tab_mut();
                if let Some(p) = json_fmt::pretty(t.body.edit.text()) {
                    t.body.edit.select_all();
                    t.body.edit.insert(&p);
                }
            }
            Act::ToggleFormView => {
                let t = self.tab_mut();
                let raw = !t.form_raw;
                t.set_form_raw(raw);
            }
            Act::CopyResp => {
                if self.tab().request_type == RequestType::WebSocket {
                    cb.set_text(self.tab().ws_log.edit.text());
                } else if let Some(r) = self.tab().resp.as_ref() {
                    cb.set_text(r.body.edit.text());
                }
            }
            Act::PrettifyResp => {
                if let Some(r) = self.tab_mut().resp.as_mut() {
                    if let Some(p) = json_fmt::pretty(r.body.edit.text()) {
                        r.body.set_text(&p);
                        r.body.set_highlight(true);
                    }
                }
            }
            Act::ClearResp => {
                let t = self.tab_mut();
                if t.request_type == RequestType::WebSocket {
                    t.ws_log.set_text("");
                    t.ws_sent = 0;
                    t.ws_recv = 0;
                } else if !t.sending {
                    t.resp = None;
                }
            }
            Act::ChooseFile(i) => self.file_request = Some(i),
        }
    }

    // ── mouse (coordinates in DIPs) ─────────────────────────────────

    fn hot_at(&self, x: f32, y: f32) -> Option<(Act, Rect)> {
        let from = if self.any_modal() { self.modal_hot_from.min(self.hot.len()) } else { 0 };
        self.hot[from..].iter().rev().find(|h| h.rect.contains(x, y)).map(|h| (h.act, h.rect))
    }

    /// Returns true when a repaint is needed.
    pub fn on_mouse_move(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
        self.mouse = (x, y);
        let mut changed = self.bar.set_hover(TabBar::hit_test(x, y, self.tabs.len(), self.width));
        if let Some(p) = self.popup.as_mut() {
            let h = p.hit(x, y);
            if h != p.hover {
                p.hover = h;
                changed = true;
            }
        }
        match self.focus {
            Focus::Input(id) => {
                if let Some(i) = self.input_mut(id) {
                    changed |= i.mouse_move(m, x);
                }
            }
            Focus::Area(id) => {
                if let Some(a) = self.area_mut(id) {
                    changed |= a.mouse_move(m, x, y);
                }
            }
            Focus::None => {}
        }
        changed
    }

    pub fn on_mouse_leave(&mut self) -> bool {
        self.bar.set_hover(Hit::None)
    }

    pub fn on_left_down(&mut self, m: &mut dyn TextMeasure, cb: &mut dyn Clipboard, x: f32, y: f32, shift: bool) -> bool {
        // An open popup owns the click, wherever it lands.
        if let Some(p) = self.popup.take() {
            if let Some(i) = p.hit(x, y) {
                self.apply_picker(p.kind, i);
            }
            return true;
        }
        match self.hot_at(x, y) {
            Some((Act::Input(id), _)) => {
                self.set_focus(Focus::Input(id));
                if let Some(i) = self.input_mut(id) {
                    i.mouse_down(m, x, shift);
                }
                true
            }
            Some((Act::Area(id), _)) => {
                self.set_focus(Focus::Area(id));
                if let Some(a) = self.area_mut(id) {
                    a.mouse_down(m, x, y, shift);
                }
                true
            }
            Some((act, rect)) => {
                self.set_focus(Focus::None);
                self.run(act, rect, cb);
                true
            }
            None => self.set_focus(Focus::None),
        }
    }

    pub fn on_double_click(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
        match self.hot_at(x, y) {
            Some((Act::Input(id), _)) => {
                self.set_focus(Focus::Input(id));
                if let Some(i) = self.input_mut(id) {
                    i.double_click(m, x);
                }
                true
            }
            Some((Act::Area(id), _)) => {
                self.set_focus(Focus::Area(id));
                if let Some(a) = self.area_mut(id) {
                    a.double_click(m, x, y);
                }
                true
            }
            _ => false,
        }
    }

    pub fn on_left_up(&mut self) {
        match self.focus {
            Focus::Input(id) => {
                if let Some(i) = self.input_mut(id) {
                    i.mouse_up();
                }
            }
            Focus::Area(id) => {
                if let Some(a) = self.area_mut(id) {
                    a.mouse_up();
                }
            }
            Focus::None => {}
        }
    }

    pub fn on_middle_click(&mut self, x: f32, y: f32) -> bool {
        match TabBar::hit_test(x, y, self.tabs.len(), self.width) {
            Hit::Tab(i) | Hit::Close(i) => {
                self.close_tab(i);
                true
            }
            _ => false,
        }
    }

    pub fn on_wheel(&mut self, notches: f32) -> bool {
        let (x, y) = self.mouse;
        if let Some((Act::Area(id), _)) = self.hot_at(x, y) {
            return self.area_mut(id).map_or(false, |a| a.wheel(notches));
        }
        if self.any_modal() {
            let max = (self.modal_content_h - self.modal_view_h).max(0.0);
            let before = self.modal_scroll;
            self.modal_scroll = (self.modal_scroll - notches * 54.0).clamp(0.0, max);
            return before != self.modal_scroll;
        }
        if self.req_panel.contains(x, y) {
            let max = (self.req_content_h - self.req_view_h).max(0.0);
            let before = self.req_scroll;
            self.req_scroll = (self.req_scroll - notches * 54.0).clamp(0.0, max);
            return before != self.req_scroll;
        }
        false
    }

    /// True when the cursor should be an I-beam.
    pub fn text_cursor_active(&self) -> bool {
        matches!(self.hot_at(self.mouse.0, self.mouse.1), Some((Act::Input(_) | Act::Area(_), _))) && self.popup.is_none()
    }

    // ── keyboard ────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: Key, mods: Mods, cb: &mut dyn Clipboard) -> bool {
        if mods.ctrl && !self.any_modal() {
            match key {
                Key::Letter('T') => {
                    self.new_tab();
                    return true;
                }
                Key::Letter('W') => {
                    let i = self.active;
                    self.close_tab(i);
                    return true;
                }
                _ => {}
            }
        }
        if key == Key::Escape {
            if self.popup.take().is_some() {
                return true;
            }
            if self.any_modal() {
                if !self.set_focus(Focus::None) {
                    self.modal = None;
                    self.cookie_modal = false;
                }
                return true;
            }
            return self.set_focus(Focus::None);
        }
        if key == Key::Enter && self.focus == Focus::Input(InputId::WsMsg) {
            self.ws_send();
            return true;
        }
        if key == Key::Enter && self.focus == Focus::Input(InputId::Url) {
            self.trigger_send();
            return true;
        }
        let before = self.focused_text();
        let handled = match self.focus {
            Focus::Input(id) => self.input_mut(id).map_or(false, |i| i.key(key, mods, cb)),
            Focus::Area(id) => self.area_mut(id).map_or(false, |a| a.key(key, mods, cb)),
            Focus::None => false,
        };
        self.after_edit(before);
        handled
    }

    pub fn on_char(&mut self, ch: char) -> bool {
        let before = self.focused_text();
        let handled = match self.focus {
            Focus::Input(id) => self.input_mut(id).map_or(false, |i| i.char(ch)),
            Focus::Area(id) => self.area_mut(id).map_or(false, |a| a.char(ch)),
            Focus::None => false,
        };
        self.after_edit(before);
        handled
    }

    /// Caret blink tick; returns true when a repaint is needed.
    pub fn blink(&mut self) -> bool {
        match self.focus {
            Focus::Input(id) => self.input_mut(id).map_or(false, |i| i.toggle_blink()),
            Focus::Area(id) => self.area_mut(id).map_or(false, |a| a.toggle_blink()),
            Focus::None => false,
        }
    }

    // ── painting ────────────────────────────────────────────────────

    fn push_hot(&mut self, rect: Rect, act: Act) {
        if let Some(cl) = self.clip {
            let (cx, cy) = rect.center();
            if !cl.contains(cx, cy) {
                return;
            }
        }
        self.hot.push(Hot { rect, act });
    }

    fn paint_input(&mut self, c: &mut dyn Canvas, id: InputId, rect: Rect) {
        let th = self.theme.clone();
        if let Some(i) = self.input_mut(id) {
            i.paint(c, &th, rect);
        }
        self.push_hot(rect, Act::Input(id));
    }

    fn paint_area(&mut self, c: &mut dyn Canvas, id: AreaId, rect: Rect) {
        let th = self.theme.clone();
        if let Some(a) = self.area_mut(id) {
            a.paint(c, &th, rect);
        }
        self.push_hot(rect, Act::Area(id));
    }

    fn paint_picker(&mut self, c: &mut dyn Canvas, kind: PickerKind, rect: Rect) {
        let th = self.theme.clone();
        let (items, cur) = self.picker_items(kind);
        let open = self.popup.as_ref().map_or(false, |p| p.kind == kind);
        w::picker(c, &th, rect, &items[cur], open);
        self.push_hot(rect, Act::Picker(kind));
    }

    pub fn paint(&mut self, c: &mut dyn Canvas) {
        self.hot.clear();
        self.clip = None;
        let th = self.theme.clone();
        let (wd, ht) = (self.width, self.height);
        c.fill_rect(Rect::new(0.0, 0.0, wd, ht), th.bg);

        self.paint_title(c, &th);

        // tab strip
        let labels: Vec<String> = self.tabs.iter().map(|t| t.label()).collect();
        let items: Vec<TabItem> = self.tabs.iter().zip(&labels).map(|(t, l)| TabItem { title: l, method: t.method }).collect();
        self.bar.paint(c, &th, &items, self.active, wd);
        let n = self.tabs.len();
        for i in 0..n {
            self.push_hot(TabBar::tab_rect(i, n, wd), Act::Tab(i));
            self.push_hot(TabBar::close_rect(i, n, wd).inset(-3.0), Act::Close(i));
        }
        self.push_hot(TabBar::plus_rect(n, wd), Act::NewTab);
        w::rule(c, &th, 10.0, wd - 10.0, 82.0);

        // request row
        self.paint_picker(c, PickerKind::ReqType, Rect::xywh(10.0, 92.0, 110.0, 38.0));
        let ws = self.tab().request_type == RequestType::WebSocket;
        if !ws {
            self.paint_picker(c, PickerKind::Method, Rect::xywh(130.0, 92.0, 100.0, 38.0));
        }
        let send = Rect::new((wd - 110.0).max(240.0), 92.0, (wd - 10.0).max(340.0), 130.0);
        let url_l = if ws { 130.0 } else { 240.0 };
        self.paint_input(c, InputId::Url, Rect::new(url_l, 92.0, (send.l - 10.0).max(url_l + 10.0), 130.0));
        let (label, primary) = if ws {
            match self.tab().ws_state {
                WsState::Disconnected => ("Connect", true),
                WsState::Connecting => ("Connecting…", false),
                WsState::Connected => ("Disconnect", false),
            }
        } else {
            match (self.tab().sending, self.tab().streaming) {
                (true, true) => ("Cancel", false),
                (true, false) => ("Sending…", false),
                _ => ("Send", true),
            }
        };
        if ws && self.tab().ws_state == WsState::Connected {
            c.fill_rect(send, th.danger);
            c.text(label, send, th.on_accent, w::UI.align(Align::Center));
        } else {
            w::button(c, &th, send, label, primary);
        }
        self.push_hot(send, Act::Send);
        // panels
        let (top, bottom, mid) = (142.0, (ht - 10.0).max(200.0), wd / 2.0);
        self.req_panel = Rect::new(10.0, top, (mid - 5.0).max(200.0), bottom);
        self.resp_panel = Rect::new((mid + 5.0).max(210.0), top, (wd - 10.0).max(400.0), bottom);
        if ws {
            self.paint_ws_view(c, &th);
        } else {
            let (rp, sp) = (self.req_panel, self.resp_panel);
            self.paint_request_panel(c, &th, rp);
            self.paint_response_panel(c, &th, sp);
        }

        if self.modal.is_some() {
            self.paint_modal(c, &th);
        } else if self.cookie_modal {
            self.paint_cookie_modal(c, &th);
        }

        // popup last, on top of everything
        if let Some(p) = &self.popup {
            let all = Rect::new(p.item_rect(0).l, p.item_rect(0).t, p.item_rect(0).r, p.item_rect(p.items.len() - 1).b);
            c.fill_rect(all, th.surface);
            for (i, name) in p.items.iter().enumerate() {
                let r = p.item_rect(i);
                if p.hover == Some(i) {
                    c.fill_rect(r, th.tab_hover);
                } else if i == p.current {
                    c.fill_rect(r, th.tab_hover.alpha(0.5));
                }
                let col = if p.kind == PickerKind::Method {
                    TabBar::method_color(&th, HttpMethod::ALL[i])
                } else {
                    th.text
                };
                c.text(name, Rect::new(r.l + 10.0, r.t, r.r, r.b), col, w::UI);
            }
            c.stroke_rect(all.inset(0.5), th.border, 1.0);
        }
    }

    fn paint_modal(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let (wd, ht) = (self.width, self.height);
        c.fill_rect(Rect::new(0.0, 0.0, wd, ht), Color::hex(0x000000).alpha(0.6));
        let d = Rect::xywh((wd - (wd - 80.0).clamp(480.0, 900.0)) / 2.0, (ht - (ht - 80.0).clamp(300.0, 720.0)) / 2.0, (wd - 80.0).clamp(480.0, 900.0), (ht - 80.0).clamp(300.0, 720.0));
        self.modal_hot_from = self.hot.len();
        self.push_hot(d, Act::Stub); // swallows clicks on the dialog body
        c.fill_rect(d, th.bg);
        c.stroke_rect(d.inset(0.5), th.accent, 1.0);
        c.text("Environments", Rect::new(d.l + 14.0, d.t + 10.0, d.l + 300.0, d.t + 38.0), th.text, TextStyle::new(16.0).bold());
        let close = Rect::new(d.r - 92.0, d.t + 8.0, d.r - 12.0, d.t + 38.0);
        w::button(c, th, close, "Close", false);
        self.push_hot(close, Act::CloseEnv);
        w::rule(c, th, d.l + 8.0, d.r - 8.0, d.t + 48.0);

        let y = d.t + 58.0;
        c.text("Environment:", Rect::new(d.l + 14.0, y, d.l + 116.0, y + 32.0), th.text, w::UI);
        let has = !self.envs.is_empty();
        if has {
            self.paint_picker(c, PickerKind::EnvSelect, Rect::new(d.l + 120.0, y, d.l + 360.0, y + 32.0));
        }
        let new = Rect::new(d.l + 370.0, y, d.l + 450.0, y + 32.0);
        w::button(c, th, new, "+ New", false);
        self.push_hot(new, Act::NewEnv);
        if has {
            let del = Rect::new(d.l + 458.0, y, d.l + 538.0, y + 32.0);
            c.fill_rect(del, th.surface);
            c.text("Delete", del, th.danger, w::UI.align(Align::Center));
            self.push_hot(del, Act::DeleteEnv);
        } else {
            c.text("No environments yet. Click + New to create one.", Rect::new(d.l + 14.0, y + 50.0, d.r - 14.0, y + 80.0), th.text_dim, w::UI);
            return;
        }

        let content = Rect::new(d.l + 14.0, d.t + 104.0, d.r - 14.0, d.b - 10.0);
        self.modal_view_h = content.height();
        c.push_clip(content);
        self.clip = Some(content);
        let top = content.t - self.modal_scroll;
        let mut y = top;
        c.text("Name:", Rect::new(content.l, y, content.l + 56.0, y + ROW_H), th.text, w::UI);
        self.paint_input(c, InputId::EnvName, Rect::new(content.l + 60.0, y, content.r, y + ROW_H));
        y += ROW_STEP;
        c.text("Variables", Rect::new(content.l, y, content.l + 200.0, y + 26.0), th.accent, w::UI.bold());
        y += 30.0;
        y = self.paint_kv(c, th, content, RowKind::EnvVar, y);
        let open = self.modal.as_ref().map_or(false, |m| m.auth_open);
        let tg = Rect::new(content.l, y, content.l + 300.0, y + 30.0);
        c.text(&format!("{} Auth auto-refresh", if open { "▾" } else { "▸" }), tg, th.accent, w::UI);
        self.push_hot(tg, Act::ToggleAuthSection);
        y += 36.0;
        if open {
            y = self.paint_cfg(c, th, content, y);
        }
        self.modal_content_h = y - top;
        self.clip = None;
        c.pop_clip();
    }

    fn cfg_field(&mut self, c: &mut dyn Canvas, th: &Theme, a: Rect, y: f32, label: &str, f: CfgField) -> f32 {
        c.text(label, Rect::new(a.l, y, a.l + 170.0, y + ROW_H), th.text, w::UI);
        self.paint_input(c, InputId::Cfg(f), Rect::new(a.l + 174.0, y, a.r, y + ROW_H));
        y + ROW_STEP
    }

    fn paint_cfg(&mut self, c: &mut dyn Canvas, th: &Theme, a: Rect, mut y: f32) -> f32 {
        let cfg = self.cfg_ref();
        let refresh_mode = cfg.mode == AuthRefreshMode::RefreshToken;
        c.text("Mode:", Rect::new(a.l, y, a.l + 170.0, y + 32.0), th.text, w::UI);
        self.paint_picker(c, PickerKind::AuthMode, Rect::new(a.l + 174.0, y, a.l + 394.0, y + 32.0));
        let cb = Rect::xywh(a.l + 414.0, y + 8.0, 16.0, 16.0);
        w::checkbox(c, th, cb, cfg.auto_refresh_enabled);
        c.text("Enabled", Rect::new(cb.r + 8.0, y, cb.r + 100.0, y + 32.0), th.text, w::UI);
        self.push_hot(Rect::new(cb.l - 5.0, y, cb.r + 100.0, y + 32.0), Act::ToggleAutoRefresh);
        y += ROW_STEP;
        if cfg.mode == AuthRefreshMode::None {
            c.text("Choose a mode to renew the Bearer token automatically.", Rect::new(a.l, y, a.r, y + 28.0), th.text_dim, w::UI);
            return y + 34.0;
        }
        y = self.cfg_field(c, th, a, y, "Login endpoint", CfgField::LoginEndpoint);
        if refresh_mode {
            y = self.cfg_field(c, th, a, y, "Refresh endpoint", CfgField::RefreshEndpoint);
        }
        for (label, mk, fk, ak, show) in [
            ("Login request", PickerKind::LoginMethod, PickerKind::LoginFormat, AreaId::LoginBody, true),
            ("Refresh request", PickerKind::RefreshMethod, PickerKind::RefreshFormat, AreaId::RefreshBody, refresh_mode),
        ] {
            if !show {
                continue;
            }
            c.text(label, Rect::new(a.l, y, a.l + 170.0, y + 32.0), th.text, w::UI);
            self.paint_picker(c, mk, Rect::new(a.l + 174.0, y, a.l + 274.0, y + 32.0));
            self.paint_picker(c, fk, Rect::new(a.l + 284.0, y, a.l + 494.0, y + 32.0));
            y += ROW_STEP;
            c.text("Body template", Rect::new(a.l, y, a.l + 170.0, y + 26.0), th.text, w::UI);
            self.paint_area(c, ak, Rect::new(a.l + 174.0, y, a.r, y + 96.0));
            y += 104.0;
        }
        y = self.cfg_field(c, th, a, y, "Access token var", CfgField::AccessVar);
        if refresh_mode {
            y = self.cfg_field(c, th, a, y, "Refresh token var", CfgField::RefreshVar);
        }
        y = self.cfg_field(c, th, a, y, "Access token path", CfgField::AccessPath);
        if refresh_mode {
            y = self.cfg_field(c, th, a, y, "Refresh token path", CfgField::RefreshPath);
        }
        y = self.cfg_field(c, th, a, y, "Expiry path (optional)", CfgField::ExpiryPath);
        y = self.cfg_field(c, th, a, y, "Default TTL (secs)", CfgField::Ttl);
        c.text("Reference the token as {{access_token}} in a tab's Bearer field.", Rect::new(a.l, y, a.r, y + 26.0), th.text_dim, w::UI);
        y += 28.0;
        let err = self.modal.as_ref().and_then(|m| self.envs.get(m.sel)).and_then(|e| e.auth_last_error.clone());
        if let Some(e) = err {
            c.text(&format!("Last refresh failed: {e}"), Rect::new(a.l, y, a.r, y + 26.0), th.danger, w::UI);
            y += 28.0;
        }
        y
    }

    /// Full-width WebSocket view: big log, counters, compact composer at the bottom.
    fn paint_cookie_modal(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let (wd, ht) = (self.width, self.height);
        c.fill_rect(Rect::new(0.0, 0.0, wd, ht), Color::hex(0x000000).alpha(0.6));
        let dw = (wd - 80.0).clamp(480.0, 900.0);
        let dh = (ht - 80.0).clamp(300.0, 720.0);
        let d = Rect::xywh((wd - dw) / 2.0, (ht - dh) / 2.0, dw, dh);
        self.modal_hot_from = self.hot.len();
        self.push_hot(d, Act::Stub);
        c.fill_rect(d, th.bg);
        c.stroke_rect(d.inset(0.5), th.accent, 1.0);
        c.text("Cookies", Rect::new(d.l + 14.0, d.t + 10.0, d.l + 300.0, d.t + 38.0), th.text, TextStyle::new(16.0).bold());
        let close = Rect::new(d.r - 92.0, d.t + 8.0, d.r - 12.0, d.t + 38.0);
        w::button(c, th, close, "Close", false);
        self.push_hot(close, Act::CloseCookies);
        let clear = Rect::new(d.r - 204.0, d.t + 8.0, d.r - 100.0, d.t + 38.0);
        c.fill_rect(clear, th.surface);
        c.text("Clear all", clear, th.danger, w::UI.align(Align::Center));
        self.push_hot(clear, Act::CookiesClear);
        w::rule(c, th, d.l + 8.0, d.r - 8.0, d.t + 48.0);

        let y = d.t + 58.0;
        c.text("Domain:", Rect::new(d.l + 14.0, y, d.l + 84.0, y + ROW_H), th.text, w::UI);
        self.paint_input(c, InputId::CkDomain, Rect::new(d.l + 88.0, y, d.r - 14.0, y + ROW_H));
        let y2 = y + ROW_STEP;
        c.text("Name:", Rect::new(d.l + 14.0, y2, d.l + 70.0, y2 + ROW_H), th.text, w::UI);
        self.paint_input(c, InputId::CkName, Rect::new(d.l + 74.0, y2, d.l + 300.0, y2 + ROW_H));
        c.text("Value:", Rect::new(d.l + 310.0, y2, d.l + 366.0, y2 + ROW_H), th.text, w::UI);
        let add = Rect::new(d.r - 94.0, y2, d.r - 14.0, y2 + ROW_H);
        self.paint_input(c, InputId::CkValue, Rect::new(d.l + 370.0, y2, add.l - 8.0, y2 + ROW_H));
        w::button(c, th, add, "Add", true);
        self.push_hot(add, Act::CookieAdd);

        let content = Rect::new(d.l + 14.0, y2 + 52.0, d.r - 14.0, d.b - 10.0);
        self.modal_view_h = content.height();
        c.push_clip(content);
        self.clip = Some(content);
        let top = content.t - self.modal_scroll;
        let mut y = top;
        let domains = self.sorted_domains();
        if domains.is_empty() {
            c.text("No cookies yet. Cookies set by responses appear here automatically.", Rect::new(content.l, y, content.r, y + 28.0), th.text_dim, w::UI);
            y += 30.0;
        }
        for (di, dom) in domains.iter().enumerate() {
            c.text(dom, Rect::new(content.l, y, content.r, y + 26.0), th.accent, w::UI.bold());
            y += 28.0;
            let list = self.jar.get(dom).cloned().unwrap_or_default();
            for (i, ck) in list.iter().enumerate() {
                let cb = Rect::xywh(content.l + 2.0, y + 9.0, 16.0, 16.0);
                w::checkbox(c, th, cb, ck.enabled);
                self.push_hot(cb.inset(-5.0), Act::CookieCheck(di, i));
                let nw = (content.width() * 0.3).max(100.0);
                let col = if ck.enabled { th.text } else { th.text_dim };
                c.text(&ck.name, Rect::new(content.l + 26.0, y, content.l + 26.0 + nw, y + ROW_H), col, w::UI.bold());
                c.text(&ck.value, Rect::new(content.l + 34.0 + nw, y, content.r - 34.0, y + ROW_H), col, w::UI);
                let rm = Rect::new(content.r - 28.0, y + 5.0, content.r - 4.0, y + 29.0);
                w::cross(c, rm, th.danger);
                self.push_hot(rm, Act::CookieRemove(di, i));
                y += ROW_H + 2.0;
            }
            y += 8.0;
        }
        self.modal_content_h = y - top;
        self.clip = None;
        c.pop_clip();
    }

    fn paint_ws_view(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let (wd, ht) = (self.width, self.height);
        let bottom = (ht - 10.0).max(300.0);
        let comp = Rect::new(10.0, bottom - 38.0, (wd - 10.0).max(300.0), bottom);
        let stats = Rect::new(10.0, comp.t - 26.0, comp.r, comp.t - 4.0);
        let log = Rect::new(10.0, 142.0, comp.r, stats.t - 4.0);
        self.paint_area(c, AreaId::WsLog, log);
        let (sent, recv) = (self.tab().ws_sent, self.tab().ws_recv);
        c.text(&format!("Sent: {sent} | Received: {recv}"), stats, th.text_dim, TextStyle::new(12.0).align(Align::Center));
        let mut ix = stats.r;
        for (glyph, act) in [("⌫", Act::ClearResp), ("⧉", Act::CopyResp)] {
            let r = Rect::new(ix - 30.0, stats.t - 4.0, ix, stats.b + 2.0);
            c.text(glyph, r, th.text, TextStyle::new(15.0).align(Align::Center));
            self.push_hot(r, act);
            ix -= 34.0;
        }

        let mut x = comp.l;
        self.paint_picker(c, PickerKind::WsType, Rect::new(x, comp.t, x + 96.0, comp.b));
        x += 104.0;
        if self.tab().ws_type == WsMessageType::Binary {
            self.paint_picker(c, PickerKind::WsBin, Rect::new(x, comp.t, x + 130.0, comp.b));
            x += 138.0;
        }
        let send = Rect::new(comp.r - 84.0, comp.t, comp.r, comp.b);
        self.paint_input(c, InputId::WsMsg, Rect::new(x, comp.t, send.l - 8.0, comp.b));
        let ready = self.tab().ws_state == WsState::Connected && !self.tab().ws_msg.edit.is_empty();
        w::button(c, th, send, "Send", ready);
        self.push_hot(send, Act::WsSend);
    }

    fn paint_title(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let wd = self.width;
        c.text("▶", Rect::xywh(10.0, 8.0, 26.0, 30.0), th.accent, TextStyle::new(14.0).align(Align::Center));
        self.push_hot(Rect::xywh(10.0, 8.0, 26.0, 30.0), Act::Stub);
        c.text("CrabiPie HTTP Client", Rect::new(46.0, 8.0, 320.0, 38.0), th.text, TextStyle::new(16.0));

        // right-aligned group (non-functional until environments/themes land)
        let mut x = wd - 10.0;
        for label in ["Save", "Open"] {
            let r = Rect::new(x - 46.0, 10.0, x, 36.0);
            c.text(label, r, th.accent, TextStyle::new(13.0).align(Align::Center));
            self.push_hot(r, Act::Stub);
            x -= 52.0;
        }
        let theme = Rect::new(x - 200.0, 8.0, x, 38.0);
        self.paint_picker(c, PickerKind::AppTheme, theme);
        x -= 208.0;
        c.text("App theme", Rect::new(x - 80.0, 8.0, x, 38.0), th.text, w::UI);
        x -= 92.0;
        let gear = Rect::new(x - 26.0, 8.0, x, 38.0);
        c.text("⚙", gear, th.text_dim, TextStyle::new(16.0).align(Align::Center));
        self.push_hot(gear, Act::OpenEnv);
        x -= 34.0;
        let env = Rect::new(x - 160.0, 8.0, x, 38.0);
        self.paint_picker(c, PickerKind::TabEnv, env);
    }

    fn paint_request_panel(&mut self, c: &mut dyn Canvas, th: &Theme, p: Rect) {
        c.stroke_rect(p.inset(0.5), th.accent.alpha(0.6), 1.0);
        c.text("Request", Rect::new(p.l + 8.0, p.t + 6.0, p.l + 120.0, p.t + 30.0), th.text, w::UI);
        let cookie = Rect::new(p.r - 130.0, p.t + 6.0, p.r - 8.0, p.t + 30.0);
        c.text("Add Cookie", cookie, th.accent, w::UI.align(Align::Right));
        self.push_hot(cookie, Act::OpenCookies);
        w::rule(c, th, p.l + 5.0, p.r - 5.0, p.t + 36.0);

        let seg = Rect::new(p.l + 5.0, p.t + 46.0, p.r - 5.0, p.t + 86.0);
        let kinds = [ReqTabKind::Query, ReqTabKind::Body, ReqTabKind::Headers, ReqTabKind::Auth];
        let active = kinds.iter().position(|k| *k == self.tab().req_tab).unwrap_or(0);
        w::segmented(c, th, seg, &["Query", "Body", "Headers", "Auth"], active);
        for (i, k) in kinds.iter().enumerate() {
            self.push_hot(w::seg_rect(seg, 4, i), Act::ReqTab(*k));
        }

        let content = Rect::new(p.l + 10.0, p.t + 96.0, p.r - 10.0, (p.b - 8.0).max(p.t + 100.0));
        self.req_view_h = content.height();
        c.push_clip(content);
        self.clip = Some(content);
        let end = match self.tab().req_tab {
            ReqTabKind::Query => self.paint_kv(c, th, content, RowKind::Param, content.t - self.req_scroll),
            ReqTabKind::Headers => self.paint_headers(c, th, content),
            ReqTabKind::Body => self.paint_body(c, th, content),
            ReqTabKind::Auth => self.paint_auth(c, th, content),
        };
        self.req_content_h = end - (content.t - self.req_scroll);
        self.clip = None;
        c.pop_clip();
    }

    /// Key/value rows with checkbox, remove button and "+ Add". Returns the bottom y.
    fn paint_kv(&mut self, c: &mut dyn Canvas, th: &Theme, a: Rect, kind: RowKind, mut y: f32) -> f32 {
        let n = self.rows_ref(kind).len();
        let is_form = kind == RowKind::Form;
        for i in 0..n {
            let (enabled, ftype, files) = {
                let r = &self.rows_ref(kind)[i];
                (r.enabled, r.ftype, r.files.clone())
            };
            let cb = Rect::xywh(a.l + 2.0, y + 9.0, 16.0, 16.0);
            w::checkbox(c, th, cb, enabled);
            self.push_hot(cb.inset(-5.0), Act::Check(kind, i));
            let mut x = a.l + 26.0;
            c.text("Key:", Rect::new(x, y, x + 38.0, y + ROW_H), th.text, w::UI);
            x += 40.0;
            let right = a.r - 34.0;
            let fixed = if is_form { 80.0 } else { 0.0 };
            let avail = (right - x - 8.0 - fixed - 54.0).max(80.0);
            let kw = avail * 0.34;
            self.paint_input(c, InputId::Key(kind, i), Rect::new(x, y, x + kw, y + ROW_H));
            x += kw + 8.0;
            if is_form {
                self.paint_picker(c, PickerKind::FieldType(i), Rect::new(x, y, x + 72.0, y + ROW_H));
                x += 80.0;
            }
            let is_file = is_form && ftype == FormFieldType::File;
            c.text(if is_file { "File:" } else { "Value:" }, Rect::new(x, y, x + 52.0, y + ROW_H), th.text, w::UI);
            x += 54.0;
            if is_file {
                let b = Rect::new(x, y, x + 90.0, y + ROW_H);
                w::button(c, th, b, "Choose", false);
                self.push_hot(b, Act::ChooseFile(i));
                let name = match files.as_slice() {
                    [] => "no file".to_string(),
                    [f] => f.rsplit(['\\', '/']).next().unwrap_or(f).to_string(),
                    many => format!("{} files", many.len()),
                };
                c.text(&name, Rect::new(x + 98.0, y, right, y + ROW_H), th.text_dim, w::UI);
            } else {
                self.paint_input(c, InputId::Val(kind, i), Rect::new(x, y, right.max(x + 40.0), y + ROW_H));
            }
            let rm = Rect::new(a.r - 28.0, y + 5.0, a.r - 4.0, y + 29.0);
            w::cross(c, rm, th.danger);
            self.push_hot(rm, Act::Remove(kind, i));
            y += ROW_STEP;
        }
        let add = Rect::xywh(a.l, y + 2.0, 76.0, ROW_H);
        w::button(c, th, add, "+ Add", false);
        self.push_hot(add, Act::Add(kind));
        y + ROW_STEP
    }

    fn paint_headers(&mut self, c: &mut dyn Canvas, th: &Theme, a: Rect) -> f32 {
        c.text("View:", Rect::new(a.l, a.t, a.l + 46.0, a.t + 32.0), th.text, w::UI);
        self.paint_picker(c, PickerKind::HeadersView, Rect::new(a.l + 50.0, a.t, a.l + 190.0, a.t + 32.0));
        let top = a.t + 42.0;
        if self.tab().headers_raw {
            self.paint_area(c, AreaId::RawHeaders, Rect::new(a.l, top, a.r, a.b));
            a.b
        } else {
            self.paint_kv(c, th, a, RowKind::Header, top - self.req_scroll)
        }
    }

    fn paint_body(&mut self, c: &mut dyn Canvas, th: &Theme, a: Rect) -> f32 {
        let t = self.tab();
        if t.request_type == RequestType::WebSocket {
            c.text("WebSocket arrives in a later step.", Rect::new(a.l, a.t, a.r, a.t + 28.0), th.text_dim, w::UI);
            return a.t + 30.0;
        }
        if t.request_type == RequestType::GraphQL {
            c.text("Operation:", Rect::new(a.l, a.t, a.l + 90.0, a.t + 32.0), th.text, w::UI);
            self.paint_input(c, InputId::GqlOp, Rect::new(a.l + 94.0, a.t, a.r, a.t + 32.0));
            let top = a.t + 42.0;
            let qh = ((a.b - top - 62.0) * 0.6).max(60.0);
            c.text("Query", Rect::new(a.l, top, a.l + 200.0, top + 24.0), th.accent, w::UI.bold());
            self.paint_area(c, AreaId::GqlQuery, Rect::new(a.l, top + 28.0, a.r, top + 28.0 + qh));
            let vy = top + 28.0 + qh + 8.0;
            c.text("Variables (JSON)", Rect::new(a.l, vy, a.l + 240.0, vy + 24.0), th.accent, w::UI.bold());
            self.paint_area(c, AreaId::GqlVars, Rect::new(a.l, vy + 28.0, a.r, (a.b).max(vy + 80.0)));
            return a.b;
        }
        if !matches!(t.method, HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH) {
            c.text("Select POST, PUT, or PATCH to edit body.", Rect::new(a.l, a.t, a.r, a.t + 28.0), th.text, w::UI);
            return a.t + 30.0;
        }
        let (ct, form_raw) = (t.content_type, t.form_raw);
        c.text("Type:", Rect::new(a.l, a.t, a.l + 44.0, a.t + 32.0), th.text, w::UI);
        self.paint_picker(c, PickerKind::Content, Rect::new(a.l + 48.0, a.t, a.l + 248.0, a.t + 32.0));
        let btn = Rect::new(a.r - 140.0, a.t, a.r, a.t + 32.0);
        let (label, act) = match (ct, form_raw) {
            (ContentType::Json, _) => ("Prettify", Act::PrettifyBody),
            (_, true) => ("View Formatted", Act::ToggleFormView),
            (_, false) => ("View raw", Act::ToggleFormView),
        };
        c.text(label, btn, th.accent, w::UI.align(Align::Right));
        self.push_hot(btn, act);
        let ed = Rect::new(a.l, a.t + 42.0, a.r, a.b);
        match (ct, form_raw) {
            (ContentType::Json, _) => {
                self.paint_area(c, AreaId::Body, ed);
                a.b
            }
            (_, true) => {
                self.paint_area(c, AreaId::RawForm, ed);
                a.b
            }
            (_, false) => self.paint_kv(c, th, ed, RowKind::Form, ed.t - self.req_scroll),
        }
    }

    fn paint_auth(&mut self, c: &mut dyn Canvas, th: &Theme, a: Rect) -> f32 {
        c.text("Type:", Rect::new(a.l, a.t, a.l + 90.0, a.t + 32.0), th.text, w::UI);
        self.paint_picker(c, PickerKind::AuthType, Rect::new(a.l + 94.0, a.t, a.l + 294.0, a.t + 32.0));
        let mut y = a.t + 46.0;
        let mut field = |s: &mut Self, c: &mut dyn Canvas, label: &str, id: InputId, y: f32| {
            c.text(label, Rect::new(a.l, y, a.l + 90.0, y + ROW_H), th.text, w::UI);
            s.paint_input(c, id, Rect::new(a.l + 94.0, y, a.r, y + ROW_H));
        };
        match self.tab().auth_type {
            AuthType::None => {}
            AuthType::Bearer => {
                field(self, c, "Token:", InputId::Bearer, y);
                y += ROW_STEP;
            }
            AuthType::Basic => {
                field(self, c, "Username:", InputId::User, y);
                field(self, c, "Password:", InputId::Pass, y + ROW_STEP);
                y += 2.0 * ROW_STEP;
            }
            AuthType::ApiKey => {
                field(self, c, "Key:", InputId::ApiName, y);
                field(self, c, "Value:", InputId::ApiVal, y + ROW_STEP);
                c.text("Add to:", Rect::new(a.l, y + 2.0 * ROW_STEP, a.l + 90.0, y + 2.0 * ROW_STEP + ROW_H), th.text, w::UI);
                let r = Rect::new(a.l + 94.0, y + 2.0 * ROW_STEP, a.l + 294.0, y + 2.0 * ROW_STEP + 32.0);
                self.paint_picker(c, PickerKind::ApiPos, r);
                y += 3.0 * ROW_STEP;
            }
        }
        y
    }

    fn paint_response_panel(&mut self, c: &mut dyn Canvas, th: &Theme, p: Rect) {
        c.stroke_rect(p.inset(0.5), th.accent.alpha(0.6), 1.0);
        let hdr_y = p.t + 6.0;
        c.text("Response", Rect::new(p.l + 8.0, hdr_y, p.l + 90.0, hdr_y + 24.0), th.text, w::UI);

        // right-aligned actions: copy / prettify / clear, then the (static) JSON theme picker
        let mut x = p.r - 6.0;
        for (glyph, act) in [("⌫", Act::ClearResp), ("✦", Act::PrettifyResp), ("⧉", Act::CopyResp)] {
            let r = Rect::new(x - 30.0, hdr_y - 2.0, x, hdr_y + 26.0);
            c.text(glyph, r, th.text, TextStyle::new(15.0).align(Align::Center));
            self.push_hot(r, act);
            x -= 34.0;
        }
        let jt = Rect::new(x - 150.0, hdr_y - 2.0, x, hdr_y + 26.0);
        w::picker(c, th, jt, "VS Code Dark", false);
        self.push_hot(jt, Act::Stub);
        c.text("Json Theme:", Rect::new(jt.l - 92.0, hdr_y, jt.l - 6.0, hdr_y + 24.0), th.text, w::UI);
        let status_r = (jt.l - 100.0).max(p.l + 100.0);

        w::rule(c, th, p.l + 5.0, p.r - 5.0, p.t + 36.0);
        let seg = Rect::new(p.l + 5.0, p.t + 46.0, p.r - 5.0, p.t + 86.0);
        let body = Rect::new(p.l + 10.0, p.t + 96.0, p.r - 10.0, (p.b - 8.0).max(p.t + 100.0));

        let Some(rs) = self.tabs[self.active].resp.as_ref() else {
            c.text("Send a request to see the response", body, th.text_dim, w::UI.align(Align::Center));
            return;
        };
        let code = rs.status.split_whitespace().next().and_then(|s| s.parse::<u16>().ok());
        let col = match code {
            Some(200..=299) => th.success,
            Some(300..=399) => th.method_post,
            Some(_) => th.danger,
            None if rs.status == "Error" => th.danger,
            None => th.text_dim,
        };
        let status = rs.status.clone();
        let elapsed = rs.elapsed.map(format_duration);
        let active = if rs.tab == RespTab::Body { 0 } else { 1 };
        c.text(&status, Rect::new(p.l + 92.0, hdr_y, status_r, hdr_y + 24.0), col, w::UI);
        if let Some(e) = elapsed {
            let x0 = p.l + 92.0 + 8.0 * status.chars().count() as f32 + 16.0;
            c.text(&format!("⏱ {e}"), Rect::new(x0, hdr_y, status_r.max(x0 + 100.0), hdr_y + 24.0), th.text, w::UI);
        }
        w::segmented(c, th, seg, &["Body", "Header"], active);
        self.push_hot(w::seg_rect(seg, 2, 0), Act::RespTab(RespTab::Body));
        self.push_hot(w::seg_rect(seg, 2, 1), Act::RespTab(RespTab::Header));
        self.paint_area(c, AreaId::Resp, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testutil::{MemClipboard, MonoCanvas};

    struct H {
        a: App,
        m: MonoCanvas,
        cb: MemClipboard,
    }

    impl H {
        fn new() -> Self {
            let mut a = App::new(vec![], 0, 0);
            a.on_resize(1400.0, 800.0);
            let mut h = H { a, m: MonoCanvas::default(), cb: MemClipboard::default() };
            h.repaint();
            h
        }
        fn repaint(&mut self) {
            self.a.paint(&mut MonoCanvas::default());
        }
        fn rect(&self, act: Act) -> Rect {
            self.a.hot.iter().rev().find(|h| h.act == act).map(|h| h.rect).unwrap_or_else(|| panic!("no region for {act:?}"))
        }
        fn click(&mut self, act: Act) {
            let (x, y) = self.rect(act).center();
            self.a.on_left_down(&mut self.m, &mut self.cb, x, y, false);
            self.repaint();
        }
        fn type_into(&mut self, id: InputId, text: &str) {
            self.click(Act::Input(id));
            self.a.input_mut(id).unwrap().edit.select_all();
            for ch in text.chars() {
                self.a.on_char(ch);
            }
            self.repaint();
        }
        fn pick(&mut self, kind: PickerKind, name: &str) {
            self.click(Act::Picker(kind));
            let p = self.a.popup.as_ref().unwrap();
            let i = p.items.iter().position(|s| s.contains(name)).unwrap_or_else(|| panic!("no item {name}: {:?}", p.items));
            let (x, y) = p.item_rect(i).center();
            self.a.on_left_down(&mut self.m, &mut self.cb, x, y, false);
            self.repaint();
        }
    }

    #[test]
    fn query_table_and_url_stay_in_sync() {
        let mut h = H::new();
        assert_eq!(h.a.tab().url.edit.text(), DEFAULT_URL);
        h.type_into(InputId::Url, "https://x.dev/p?a=1&b=2");
        assert_eq!(h.a.tab().params.len(), 2);
        h.type_into(InputId::Key(RowKind::Param, 0), "z");
        assert_eq!(h.a.tab().url.edit.text(), "https://x.dev/p?z=1&b=2");
        h.click(Act::Check(RowKind::Param, 1)); // disable b
        assert_eq!(h.a.tab().url.edit.text(), "https://x.dev/p?z=1");
        h.click(Act::Remove(RowKind::Param, 0));
        assert_eq!(h.a.tab().url.edit.text(), "https://x.dev/p");
    }

    #[test]
    fn body_needs_a_body_method_and_prettify_formats_json() {
        let mut h = H::new();
        h.click(Act::ReqTab(ReqTabKind::Body));
        assert!(h.a.hot.iter().all(|x| x.act != Act::Area(AreaId::Body)), "GET has no body editor");
        h.pick(PickerKind::Method, "POST");
        h.click(Act::Area(AreaId::Body));
        for ch in r#"{"a":[1,2]}"#.chars() {
            h.a.on_char(ch);
        }
        h.click(Act::PrettifyBody);
        assert_eq!(h.a.tab().body.edit.text(), "{\n  \"a\": [\n    1,\n    2\n  ]\n}");
    }

    #[test]
    fn headers_raw_toggle_and_auth_reach_the_send_job() {
        let mut h = H::new();
        h.click(Act::ReqTab(ReqTabKind::Headers));
        h.type_into(InputId::Key(RowKind::Header, 0), "X-A");
        h.type_into(InputId::Val(RowKind::Header, 0), "1");
        h.pick(PickerKind::HeadersView, "Raw");
        assert_eq!(h.a.tab().headers_area.edit.text(), "X-A: 1");
        h.pick(PickerKind::HeadersView, "Formatted");
        assert_eq!(h.a.tab().headers[0].key.edit.text(), "X-A");
        h.click(Act::ReqTab(ReqTabKind::Auth));
        h.pick(PickerKind::AuthType, "Bearer");
        h.type_into(InputId::Bearer, "tok");
        h.click(Act::Send);
        let job = h.a.take_send().expect("job");
        assert_eq!(job.state.headers.len(), 1);
        assert_eq!((job.state.auth_type, job.state.bearer_token.as_str()), (AuthType::Bearer, "tok"));
    }

    #[test]
    fn form_rows_support_file_fields() {
        let mut h = H::new();
        h.pick(PickerKind::Method, "POST");
        h.click(Act::ReqTab(ReqTabKind::Body));
        h.pick(PickerKind::Content, "Form Data");
        h.click(Act::Add(RowKind::Form));
        assert_eq!(h.a.tab().form.len(), 2);
        h.pick(PickerKind::FieldType(0), "File");
        h.click(Act::ChooseFile(0));
        assert_eq!(h.a.take_file_request(), Some(0));
        h.a.set_form_file(0, "C:\\x\\a.png".into());
        h.type_into(InputId::Key(RowKind::Form, 0), "upload");
        h.click(Act::Send);
        let job = h.a.take_send().unwrap();
        assert_eq!(job.state.form_data.len(), 1);
        assert_eq!(job.state.form_data[0].files, vec!["C:\\x\\a.png".to_string()]);
    }

    #[test]
    fn response_panel_copy_tabs_and_last_tab_reset() {
        let mut h = H::new();
        h.click(Act::Send);
        let id = h.a.take_send().unwrap().tab_id;
        let r = HttpResponse {
            status: "200 OK".into(),
            body: r#"{"k":1}"#.into(),
            headers: "a: 1".into(),
            content_type: "application/json".into(),
            ..Default::default()
        };
        assert!(h.a.on_response(id, ResponseEvent::Response(r)));
        h.repaint();
        h.click(Act::CopyResp);
        assert_eq!(h.cb.0.as_deref(), Some("{\n  \"k\": 1\n}"));
        h.click(Act::RespTab(RespTab::Header));
        assert_eq!(h.a.tab().resp.as_ref().unwrap().headers.line(0), Some("a: 1"));
        // closing the only tab resets it in place
        h.a.on_key(Key::Letter('W'), Mods { ctrl: true, shift: false }, &mut MemClipboard::default());
        assert_eq!((h.a.tabs.len(), h.a.tabs[0].id, h.a.next_id), (1, id, 1));
        assert!(h.a.tab().resp.is_none());
    }

    #[test]
    fn environments_dialog_edits_vars_and_auth_and_reaches_the_send_job() {
        let mut h = H::new();
        h.click(Act::OpenEnv);
        h.click(Act::NewEnv);
        assert_eq!(h.a.envs.len(), 1);
        h.click(Act::Add(RowKind::EnvVar));
        h.type_into(InputId::Key(RowKind::EnvVar, 0), "host");
        h.type_into(InputId::Val(RowKind::EnvVar, 0), "x.dev");
        assert_eq!(h.a.envs[0].variables.len(), 1);
        h.click(Act::ToggleAuthSection);
        h.click(Act::Picker(PickerKind::AuthMode));
        let (x, y) = h.a.popup.as_ref().unwrap().item_rect(1).center();
        h.a.on_left_down(&mut h.m, &mut h.cb, x, y, false);
        h.repaint();
        h.type_into(InputId::Cfg(CfgField::LoginEndpoint), "https://x.dev/login");
        h.click(Act::ToggleAutoRefresh);
        let cfg = h.a.envs[0].auth_config.clone().unwrap();
        assert_eq!((cfg.mode, cfg.login_endpoint.as_str(), cfg.auto_refresh_enabled), (AuthRefreshMode::ALL[1], "https://x.dev/login", true));
        h.click(Act::CloseEnv);
        h.pick(PickerKind::TabEnv, "Environment 1");
        h.click(Act::Send);
        let job = h.a.take_send().unwrap();
        assert_eq!(job.env.as_ref().map(|e| e.variables.len()), Some(1));
        // a refreshed token lands in the env
        let out = AuthRefreshOutcome { access_token: "T".into(), refresh_token: None, expires_at: None };
        assert!(h.a.on_token(job.env.unwrap().id, Ok(out)));
        assert_eq!(h.a.envs[0].get_var("access_token").as_deref(), Some("T"));
    }

    #[test]
    fn tabs_are_named_after_their_url_and_graphql_requests_carry_their_editors() {
        let mut h = H::new();
        assert_eq!(h.a.tab().label(), "jsonplaceholder.typicode.com/posts");
        h.type_into(InputId::Url, ""); // selects all, types nothing
        h.a.on_key(Key::Backspace, Mods::default(), &mut h.cb);
        assert_eq!(h.a.tab().label(), "Request 1");
        h.type_into(InputId::Url, "http://x.dev/graphql");
        assert_eq!(h.a.tab().label(), "x.dev/graphql");

        h.pick(PickerKind::ReqType, "GraphQL");
        h.click(Act::ReqTab(ReqTabKind::Body));
        h.type_into(InputId::GqlOp, "Q1");
        h.click(Act::Area(AreaId::GqlQuery));
        for ch in "{ me }".chars() {
            h.a.on_char(ch);
        }
        h.click(Act::Send);
        let job = h.a.take_send().unwrap();
        assert_eq!(job.state.request_type, RequestType::GraphQL);
        assert_eq!((job.state.graphql_query.as_str(), job.state.graphql_operation.as_str()), ("{ me }", "Q1"));
    }

    #[test]
    fn websocket_types_timestamps_and_counters() {
        use crabipie_core::WsOut;
        let mut h = H::new();
        h.pick(PickerKind::ReqType, "WebSocket");
        h.type_into(InputId::Url, "echo.test/ws");
        h.click(Act::Send);
        assert_eq!(h.a.tab().ws_state, WsState::Connecting);
        let cmds = h.a.take_ws_cmds();
        assert!(matches!(&cmds[..], [WsCmd::Connect { url, .. }] if url == "wss://echo.test/ws"));

        let id = h.a.tab().id;
        let (tx, mut rx) = futures::channel::mpsc::channel::<WsOut>(8);
        assert!(h.a.on_ws_event(id, WsEvent::Connected(WsConnection(tx))));
        h.repaint();

        h.type_into(InputId::WsMsg, "hi");
        h.click(Act::WsSend);
        assert!(matches!(rx.try_next(), Ok(Some(WsOut::Text(s))) if s == "hi"));
        assert!(h.a.tab().ws_msg.edit.is_empty(), "input clears after sending");

        h.pick(PickerKind::WsType, "Json");
        h.type_into(InputId::WsMsg, "{bad");
        h.click(Act::WsSend);
        assert!(rx.try_next().is_err(), "invalid JSON is not sent");

        h.pick(PickerKind::WsType, "Binary");
        h.type_into(InputId::WsMsg, "SGk=");
        h.click(Act::WsSend);
        assert!(matches!(rx.try_next(), Ok(Some(WsOut::Binary(b))) if b == vec![72u8, 105]));

        h.a.on_ws_event(id, WsEvent::MessageReceived(r#"{"a":1}"#.into()));
        h.a.on_ws_event(id, WsEvent::BinaryReceived(vec![1, 2]));
        assert_eq!((h.a.tab().ws_sent, h.a.tab().ws_recv), (2, 2));

        let text = h.a.tab().ws_log.edit.text().to_string();
        let lines: Vec<String> = text
            .lines()
            .map(|l| if l.len() >= 13 && l.as_bytes()[2] == b':' { l[13..].to_string() } else { l.to_string() })
            .collect();
        assert!(lines.iter().any(|l| l == "→ hi"));
        assert!(lines.iter().any(|l| l.starts_with("! Invalid JSON")));
        assert!(lines.iter().any(|l| l == "→ [Binary] 2 bytes: 48 69"));
        assert!(lines.iter().any(|l| l == "← {"), "received JSON is pretty-printed");
        assert!(lines.iter().any(|l| l == "← [binary 2 bytes] 01 02"));

        h.click(Act::Send); // now says Disconnect
        assert_eq!(h.a.tab().ws_state, WsState::Disconnected);
        assert!(matches!(&h.a.take_ws_cmds()[..], [WsCmd::Disconnect { .. }]));
        assert!(!h.a.on_ws_event(id, WsEvent::MessageReceived("late".into())));
    }

    #[test]
    fn cookie_manager_and_theme_picker() {
        let mut h = H::new();
        h.pick(PickerKind::AppTheme, "Nord");
        assert_eq!(h.a.theme.name, "Nord");
        h.click(Act::OpenCookies);
        h.type_into(InputId::CkDomain, "https://api.x.dev/path");
        h.type_into(InputId::CkName, "sid");
        h.type_into(InputId::CkValue, "42");
        h.click(Act::CookieAdd);
        assert_eq!(h.a.jar["api.x.dev"][0].value, "42");
        h.click(Act::CookieCheck(0, 0));
        assert!(!h.a.jar["api.x.dev"][0].enabled);
        h.click(Act::CookieRemove(0, 0));
        assert!(h.a.jar.is_empty());
        h.click(Act::CloseCookies);
        assert!(!h.a.cookie_modal);
    }
}
