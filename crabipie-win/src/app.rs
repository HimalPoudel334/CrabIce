//! Application state, layout and input handling. Toolkit-free: the window
//! layer feeds it DIP-space events and hands it a `Canvas` to paint on.
//!
//! Painting registers clickable regions (`hot`) as it goes; mouse events are
//! resolved against that list, so layout lives in exactly one place.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::ui::icons::{self, Icon};
use crate::ui::canvas::{Align, Canvas, TextMeasure, TextStyle};
use crate::ui::geom::{Color, Rect};
use crate::ui::input::{Clipboard, Key, Mods};
use crate::ui::gql_tree::{self, TreeRow};
use crate::ui::json_fmt;
use crate::ui::tab_bar::{Hit, TabBar, TabItem};
use crate::ui::text_area::TextArea;
use crate::ui::text_input::TextInput;
use crate::ui::theme::Theme;
use crate::ui::widgets as w;
use crabipie_core::{
    DropAt, apply_auth_refresh_outcome, extract_domain, form_data_to_raw, format_duration, headers_to_raw, merge_set_cookies, parse_url_query, raw_to_form_data,
    raw_to_headers, rebuild_url, ApiKeyPosition, AuthType, ContentType, CookieEntry, FormField, FormFieldType,
    AuthBodyFormat, AuthHttpMethod, AuthRefreshConfig, AuthRefreshMode, AuthRefreshOutcome, EnvVar, Environment,
    HttpMethod, HttpResponse, QueryParam, WsBinaryMessageType, WsConnection, WsEvent, WsMessageType, WsPayload,
    build_graphql_document, encode_ws_payload, hex_dump, variables_template, GraphqlSchema, RequestHeaders, RequestType, ResponseEvent, SavedState,
    RAW_FORM_PLACEHOLDER, Collection, CollectionFolder, CollectionItem, CollectionRequest, FormViewType, HeaderViewType, RequestTab, ResponseTab,
};

pub const DEFAULT_URL: &str = "https://jsonplaceholder.typicode.com/posts";
const ROW_H: f32 = 34.0;
const ROW_STEP: f32 = 42.0;
/// Collections sidebar width and row height (DIPs).
pub const SB_W: f32 = 240.0; // default; the user can drag it
const SB_ROW: f32 = 28.0;

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
pub enum InputId { GqlSearch, CkDomain, CkName, CkValue, WsMsg, GqlOp, EnvName, Cfg(CfgField), Url, Key(RowKind, usize), Val(RowKind, usize), Bearer, User, Pass, ApiName, ApiVal, SbRename, SaveName }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaId { WsLog, GqlQuery, GqlVars, Body, RawHeaders, RawForm, Resp, LoginBody, RefreshBody }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerKind { AppTheme, WsType, WsBin, TabEnv, EnvSelect, AuthMode, LoginMethod, LoginFormat, RefreshMethod, RefreshFormat, ReqType, Method, Content, AuthType, ApiPos, HeadersView, FieldType(usize), SaveFolder, JsonTheme }
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
    GqlFetch,
    GqlCollapse,
    GqlClearSel,
    GqlExpand(usize),
    GqlCheck(usize),
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
    ToggleSidebar,
    SbCollapseAll,
    ExportTab,
    ImportTab,
    SbNewFolder,
    SbSubfolder(usize),
    SbToggle(usize),
    SbSelect(usize),
    SbOpen(usize),
    SbRename(usize),
    SbDuplicate(usize),
    SbDelete(usize),
    OpenSave,
    SaveConfirm,
    SaveCancel,
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

/// Mouse drags in progress: panel splitters and sidebar item moves.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag { Sidebar, Split, GqlTree, GqlHeight, Item { id: usize, sx: f32, sy: f32, active: bool } }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind { Arrow, IBeam, ResizeWE, ResizeNS }

/// Right-click menu on a sidebar row (window coordinates).
#[derive(Clone, Debug)]
struct Ctx { id: usize, folder: bool, x: f32, y: f32 }

struct SbRow { id: usize, depth: usize, folder: bool, expanded: bool, name: String, method: HttpMethod }

fn sb_rows(items: &[CollectionItem], depth: usize, out: &mut Vec<SbRow>) {
    for item in items {
        match item {
            CollectionItem::Folder(f) => {
                out.push(SbRow { id: f.id, depth, folder: true, expanded: f.expanded, name: f.name.clone(), method: HttpMethod::GET });
                if f.expanded {
                    sb_rows(&f.children, depth + 1, out);
                }
            }
            CollectionItem::Request(r) => {
                out.push(SbRow { id: r.id, depth, folder: false, expanded: false, name: r.name.clone(), method: r.method });
            }
        }
    }
}

/// Wraps a canvas and moves everything `dx` to the right, so the main area
/// can be laid out from x = 0 whether or not the sidebar is open.
struct Shift<'a, 'b> { inner: &'a mut (dyn Canvas + 'b), dx: f32 }

fn shifted(r: Rect, dx: f32) -> Rect {
    Rect::new(r.l + dx, r.t, r.r + dx, r.b)
}

impl TextMeasure for Shift<'_, '_> {
    fn text_width(&mut self, s: &str, style: TextStyle) -> f32 {
        self.inner.text_width(s, style)
    }
    fn text_index_at(&mut self, s: &str, style: TextStyle, x: f32) -> usize {
        self.inner.text_index_at(s, style, x)
    }
}

impl Canvas for Shift<'_, '_> {
    fn fill_rect(&mut self, r: Rect, c: Color) {
        self.inner.fill_rect(shifted(r, self.dx), c);
    }
    fn stroke_rect(&mut self, r: Rect, c: Color, width: f32) {
        self.inner.stroke_rect(shifted(r, self.dx), c, width);
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: Color, width: f32) {
        self.inner.line((a.0 + self.dx, a.1), (b.0 + self.dx, b.1), c, width);
    }
    fn text(&mut self, s: &str, r: Rect, c: Color, style: TextStyle) {
        self.inner.text(s, shifted(r, self.dx), c, style);
    }
    fn push_clip(&mut self, r: Rect) {
        self.inner.push_clip(shifted(r, self.dx));
    }
    fn pop_clip(&mut self) {
        self.inner.pop_clip();
    }
}

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
    pub gql_schema: Option<GraphqlSchema>,
    pub gql_loading: bool,
    pub gql_error: Option<String>,
    pub gql_expanded: HashSet<String>,
    pub gql_selected: HashSet<String>,
    pub gql_manual: HashSet<String>,
    pub gql_search: TextInput,
    pub gql_scroll: f32,
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
    /// The state this tab was loaded from: fields the native UI does not own
    /// (responses, json theme, ...) are written back untouched.
    base: Option<SavedState>,
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
            gql_op: input("optional"),
            gql_schema: None,
            gql_loading: false,
            gql_error: None,
            gql_expanded: HashSet::new(),
            gql_selected: HashSet::new(),
            gql_manual: HashSet::new(),
            gql_search: input("Search fields"),
            gql_scroll: 0.0,
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
            base: None,
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

    /// Snapshot for the collection. Responses are not kept.
    pub fn to_saved(&self, app_theme: &str) -> SavedState {
        let headers = self.header_list();
        let raw_headers = if self.headers_raw { self.headers_area.edit.text().to_string() } else { headers_to_raw(&headers) };
        let form = self.form_list();
        let raw_form = if self.form_raw { self.form_area.edit.text().to_string() } else { form_data_to_raw(&form) };
        let gql = self.request_type == RequestType::GraphQL && self.req_tab == ReqTabKind::Body;
        let base = self.base.clone().unwrap_or_default();
        SavedState {
            id: self.id,
            title: self.title.clone(),
            url: self.url.edit.text().to_string(),
            request_type: self.request_type.clone(),
            method: self.method.clone(),
            headers,
            header_view_type: if self.headers_raw { HeaderViewType::Raw } else { HeaderViewType::Formatted },
            raw_headers_content: raw_headers,
            body: self.body.edit.text().to_string(),
            form_view_type: if self.form_raw { FormViewType::Raw } else { FormViewType::Formatted },
            form_data: form,
            raw_form_content: raw_form,
            auth_type: self.auth_type.clone(),
            username: self.username.edit.text().to_string(),
            password: self.password.edit.text().to_string(),
            bearer_token: self.bearer.edit.text().to_string(),
            api_key_name: self.api_name.edit.text().to_string(),
            api_key: self.api_val.edit.text().to_string(),
            api_key_position: self.api_pos.clone(),
            content_type: self.content_type.clone(),
            query_params: self
                .params
                .iter()
                .filter(|r| !r.blank())
                .map(|r| QueryParam { key: r.key.edit.text().into(), value: r.value.edit.text().into(), enabled: r.enabled })
                .collect(),
            app_theme: app_theme.to_string(),
            ws_message_type: self.ws_type.clone(),
            ws_binary_message_type: self.ws_bin.clone(),
            graphql_query: self.gql_query.edit.text().to_string(),
            graphql_variables: self.gql_vars.edit.text().to_string(),
            graphql_operation: self.gql_op.edit.text().to_string(),
            graphql_schema: self.gql_schema.clone(),
            graphql_schema_error: self.gql_error.clone(),
            graphql_expanded_types: self.gql_expanded.clone(),
            graphql_selected_paths: self.gql_selected.clone(),
            manually_selected_paths: self.gql_manual.clone(),
            active_request_tab: match self.req_tab {
                _ if gql => RequestTab::GraphQL,
                ReqTabKind::Query => RequestTab::Query,
                ReqTabKind::Body => RequestTab::Body,
                ReqTabKind::Headers => RequestTab::Headers,
                ReqTabKind::Auth => RequestTab::Auth,
            },
            active_response_tab: match self.resp.as_ref().map(|r| r.tab) {
                Some(RespTab::Header) => ResponseTab::Headers,
                _ => ResponseTab::Body,
            },
            active_environment_id: self.env_id,
            ..base
        }
    }

    /// A fresh tab (id `id`) holding a saved request.
    pub fn from_saved(id: usize, s: &SavedState) -> Tab {
        let mut t = Tab::new(id);
        t.base = Some(s.clone());
        t.title = s.title.clone();
        t.request_type = s.request_type.clone();
        t.method = s.method.clone();
        t.env_id = s.active_environment_id;
        t.url.edit.set_text(&s.url);
        t.params = s.query_params.iter().map(|p| KvRow::new(&p.key, &p.value, p.enabled)).collect();
        if t.params.is_empty() {
            t.params.push(KvRow::empty());
        }
        t.headers = s.headers.iter().map(|h| KvRow::new(&h.key, &h.value, h.enabled)).collect();
        if t.headers.is_empty() {
            t.headers.push(KvRow::empty());
        }
        t.headers_raw = s.header_view_type == HeaderViewType::Raw;
        if t.headers_raw {
            t.headers_area.set_text(&s.raw_headers_content);
        }
        t.content_type = s.content_type.clone();
        t.body.set_text(&s.body);
        t.form = s
            .form_data
            .iter()
            .map(|f| {
                let mut r = KvRow::new(&f.key, &f.value, f.enabled);
                r.ftype = f.field_type.clone();
                r.files = f.files.clone();
                r
            })
            .collect();
        if t.form.is_empty() {
            t.form.push(KvRow::empty());
        }
        t.form_raw = s.form_view_type == FormViewType::Raw;
        if t.form_raw {
            t.form_area.set_text(&s.raw_form_content);
        }
        t.auth_type = s.auth_type.clone();
        t.bearer.edit.set_text(&s.bearer_token);
        t.username.edit.set_text(&s.username);
        t.password.edit.set_text(&s.password);
        t.api_name.edit.set_text(&s.api_key_name);
        t.api_val.edit.set_text(&s.api_key);
        t.api_pos = s.api_key_position.clone();
        t.ws_type = s.ws_message_type.clone();
        t.ws_bin = s.ws_binary_message_type.clone();
        t.gql_query.set_text(&s.graphql_query);
        t.gql_vars.set_text(&s.graphql_variables);
        t.gql_op.edit.set_text(&s.graphql_operation);
        t.gql_schema = s.graphql_schema.clone();
        t.gql_error = s.graphql_schema_error.clone();
        t.gql_expanded = s.graphql_expanded_types.clone();
        t.gql_selected = s.graphql_selected_paths.clone();
        t.gql_manual = s.manually_selected_paths.clone();
        t.req_tab = match s.active_request_tab {
            RequestTab::Query => ReqTabKind::Query,
            RequestTab::Headers => ReqTabKind::Headers,
            RequestTab::Auth => ReqTabKind::Auth,
            RequestTab::Body | RequestTab::GraphQL => ReqTabKind::Body,
        };
        t
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

/// Introspection request for the schema browser (run by the window layer).
pub struct GqlJob {
    pub tab_id: usize,
    pub state: SavedState,
    pub env: Option<Environment>,
    pub jar: HashMap<String, Vec<CookieEntry>>,
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
    gql_rows: Vec<TreeRow>,
    gql_tree_rect: Rect,
    gql_jobs: Vec<GqlJob>,
    cookie_modal: bool,
    ck_domain: TextInput,
    ck_name: TextInput,
    ck_value: TextInput,
    clock: fn() -> (u8, u8, u8, u16),
    // collections sidebar
    win_w: f32,
    pub sidebar_open: bool,
    pub collection: Collection,
    next_cid: usize,
    sb_selected: Option<usize>,
    sb_hover: Option<usize>,
    sb_editing: Option<usize>,
    sb_rename: TextInput,
    sb_scroll: f32,
    sb_hot: Vec<Hot>,
    mouse_win: (f32, f32),
    ctx: Option<Ctx>,
    save_modal: bool,
    save_name: TextInput,
    save_folder: Option<usize>,
    // resizable panels
    sb_w: f32,
    split: f32,
    gql_tree_w: f32,
    gql_qfrac: f32,
    drag: Option<Drag>,
    gql_a: Rect,
    gql_vsplit: Rect,
    gql_hsplit: Rect,
    gql_hs: (f32, f32),
    /// Icon button under the mouse (hover highlight + tooltip).
    hover_act: Option<Act>,
    // persistence
    dirty_at: Option<std::time::Instant>,
    coll_dirty: bool,
    session_json_theme: String,
    session_env_id: Option<usize>,
    export_req: bool,
    import_req: bool,
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
            gql_rows: Vec::new(),
            gql_tree_rect: Rect::default(),
            gql_jobs: Vec::new(),
            cookie_modal: false,
            ck_domain: input("example.com"),
            ck_name: input("name"),
            ck_value: input("value"),
            clock: utc_clock,
            win_w: 0.0,
            sidebar_open: false,
            collection: Collection::new(),
            next_cid: 1,
            sb_selected: None,
            sb_hover: None,
            sb_editing: None,
            sb_rename: input("Name"),
            sb_scroll: 0.0,
            sb_hot: Vec::new(),
            mouse_win: (0.0, 0.0),
            ctx: None,
            save_modal: false,
            save_name: input("Request name"),
            save_folder: None,
            sb_w: SB_W,
            split: 0.5,
            gql_tree_w: 250.0,
            gql_qfrac: 0.6,
            drag: None,
            gql_a: Rect::default(),
            gql_vsplit: Rect::default(),
            gql_hsplit: Rect::default(),
            gql_hs: (0.0, 1.0),
            hover_act: None,
            dirty_at: None,
            coll_dirty: false,
            session_json_theme: String::new(),
            session_env_id: None,
            export_req: false,
            import_req: false,
        }
    }

    /// Loads the ~/.crabipie session the iced app uses: tabs (full state), theme,
    /// environments, cookies and the collection, plus this shell's panel layout.
    pub fn from_disk(rt: &tokio::runtime::Runtime) -> Self {
        let (session, meta) = rt.block_on(crabipie_core::load_app_state());
        let tabs = meta
            .into_iter()
            .map(|m| match rt.block_on(SavedState::load(m.id)) {
                Some(s) => Tab::from_saved(m.id, &s),
                None => {
                    let mut t = Tab::new(m.id);
                    t.title = m.title;
                    t.method = m.method;
                    t
                }
            })
            .collect();
        let (active, next_id) = session.as_ref().map(|s| (s.active_tab, s.next_tab_id)).unwrap_or((0, 0));
        let mut app = Self::new(tabs, active, next_id);
        if let Some(s) = session {
            app.theme = Theme::named(&s.app_theme);
            crate::ui::json_color::set_theme(crate::ui::json_color::index_of(&s.json_theme));
            app.session_json_theme = s.json_theme;
            app.session_env_id = s.active_environment_id;
            app.jar = s.cookie_jar;
            app.next_env_id = s.next_environment_id.max(s.environments.iter().map(|e| e.id + 1).max().unwrap_or(0));
            app.envs = s.environments;
        }
        if let Some(c) = rt.block_on(crabipie_core::load_collection()) {
            app.next_cid = crabipie_core::next_free_id(&c.items);
            app.collection = c;
        }
        if let Some(u) = crabipie_core::UiLayout::load() {
            app.sidebar_open = u.sidebar_open;
            app.sb_w = u.sidebar_width.clamp(180.0, 700.0);
            app.split = u.split.clamp(0.2, 0.8);
            app.gql_tree_w = u.gql_tree_width.clamp(200.0, 600.0);
            app.gql_qfrac = u.gql_query_frac.clamp(0.15, 0.85);
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
            InputId::SbRename => return Some(&mut self.sb_rename),
            InputId::SaveName => return Some(&mut self.save_name),
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
            InputId::GqlSearch => Some(&mut t.gql_search),
            InputId::WsMsg => Some(&mut t.ws_msg),
            InputId::EnvName | InputId::Cfg(_) | InputId::CkDomain | InputId::CkName | InputId::CkValue | InputId::SbRename | InputId::SaveName => None, // handled above
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
        self.win_w = w;
        self.height = h;
        self.apply_width();
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

    // ── graphql schema browser ──────────────────────────────────────

    fn gql_fetch(&mut self) {
        let ix = self.active;
        if self.tabs[ix].gql_loading || self.tabs[ix].url.edit.text().trim().is_empty() {
            return;
        }
        let env = self.tabs[ix].env_id.and_then(|id| self.envs.iter().find(|e| e.id == id).cloned());
        let jar = self.jar.clone();
        let t = &mut self.tabs[ix];
        let state = SavedState {
            url: t.url.edit.text().trim().to_string(),
            method: HttpMethod::POST,
            request_type: RequestType::GraphQL,
            headers: t.header_list(),
            auth_type: t.auth_type,
            username: t.username.edit.text().to_string(),
            password: t.password.edit.text().to_string(),
            bearer_token: t.bearer.edit.text().to_string(),
            api_key_name: t.api_name.edit.text().to_string(),
            api_key: t.api_val.edit.text().to_string(),
            api_key_position: t.api_pos,
            ..SavedState::default()
        };
        t.gql_loading = true;
        t.gql_error = None;
        let tab_id = t.id;
        self.gql_jobs.push(GqlJob { tab_id, state, env, jar });
    }

    pub fn take_gql_jobs(&mut self) -> Vec<GqlJob> {
        std::mem::take(&mut self.gql_jobs)
    }

    /// Introspection finished. The root type starts expanded.
    pub fn on_schema(&mut self, tab_id: usize, res: Result<GraphqlSchema, String>) -> bool {
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab_id) else { return false };
        t.gql_loading = false;
        match res {
            Ok(schema) => {
                t.gql_expanded.clear();
                t.gql_expanded.insert(schema.query_type.name.clone());
                t.gql_selected.clear();
                t.gql_manual.clear();
                t.gql_scroll = 0.0;
                t.gql_schema = Some(schema);
                t.gql_error = None;
            }
            Err(e) => t.gql_error = Some(e),
        }
        true
    }

    /// A tree checkbox was clicked: update the selection and rewrite the query text.
    fn gql_check(&mut self, i: usize) {
        let Some(path) = self.gql_rows.get(i).map(|r| r.path.clone()) else { return };
        let t = self.tab_mut();
        gql_tree::toggle(&mut t.gql_selected, &mut t.gql_manual, &path);
        self.gql_rewrite_query();
    }

    /// Rewrites the Query (and seeds Variables) from the tree selection.
    fn gql_rewrite_query(&mut self) {
        let t = self.tab_mut();
        let Some(schema) = t.gql_schema.as_ref() else { return };
        let doc = build_graphql_document(schema, &t.gql_selected);
        let vars = variables_template(schema, &t.gql_selected);
        t.gql_query.edit.select_all();
        t.gql_query.edit.insert(&doc);
        // seed the Variables editor only when it is empty (never overwrite the user's values)
        if let Some(v) = vars {
            if t.gql_vars.edit.text().trim().is_empty() {
                t.gql_vars.set_text(&v);
            }
        }
    }

    // ── cookie manager ──────────────────────────────────────────────

    fn any_modal(&self) -> bool {
        self.modal.is_some() || self.cookie_modal || self.save_modal
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
        let entry = CookieEntry::new(&name, self.ck_value.edit.text(), &domain);
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
            PickerKind::JsonTheme => (crate::ui::json_color::theme_names(), crate::ui::json_color::theme_index()),
            PickerKind::SaveFolder => {
                let opts = crabipie_core::folder_options(&self.collection.items);
                let cur = opts.iter().position(|(id, _)| *id == self.save_folder).unwrap_or(0);
                (opts.into_iter().map(|(_, l)| l).collect(), cur)
            }
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
            PickerKind::JsonTheme => {
                crate::ui::json_color::set_theme(idx);
                self.session_json_theme = crate::ui::json_color::theme_names()[idx].clone();
                self.touch();
                return;
            }
            PickerKind::SaveFolder => {
                if let Some((id, _)) = crabipie_core::folder_options(&self.collection.items).get(idx) {
                    self.save_folder = *id;
                }
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
            Act::GqlFetch => self.gql_fetch(),
            Act::GqlCollapse => {
                let t = self.tab_mut();
                t.gql_expanded.clear();
                t.gql_scroll = 0.0;
            }
            Act::GqlClearSel => {
                let t = self.tab_mut();
                t.gql_selected.clear();
                t.gql_manual.clear();
                self.gql_rewrite_query();
            }
            Act::GqlExpand(i) => {
                let path = if i == usize::MAX {
                    self.tab().gql_schema.as_ref().map(|s| s.query_type.name.clone())
                } else {
                    self.gql_rows.get(i).map(|r| r.path.clone())
                };
                if let Some(path) = path {
                    let t = self.tab_mut();
                    if !t.gql_expanded.remove(&path) {
                        t.gql_expanded.insert(path);
                    }
                }
            }
            Act::GqlCheck(i) => self.gql_check(i),
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
            Act::ToggleSidebar => self.toggle_sidebar(),
            Act::SbCollapseAll => {
                crabipie_core::collapse_all(&mut self.collection.items);
                self.coll_dirty = true;
            }
            Act::ExportTab => self.export_req = true,
            Act::ImportTab => self.import_req = true,
            Act::SbNewFolder => self.sb_add_folder(None),
            Act::SbSubfolder(id) => self.sb_add_folder(Some(id)),
            Act::SbToggle(id) => {
                self.sb_selected = Some(id);
                crabipie_core::toggle_expand(&mut self.collection.items, id);
                self.coll_dirty = true;
            }
            Act::SbSelect(id) => self.sb_selected = Some(id),
            Act::SbOpen(id) => self.sb_open(id),
            Act::SbRename(id) => self.sb_start_rename(id),
            Act::SbDuplicate(id) => {
                let mut next = self.next_cid;
                crabipie_core::duplicate_item_fresh(&mut self.collection.items, id, &mut next);
                self.next_cid = next;
                self.coll_dirty = true;
            }
            Act::SbDelete(id) => {
                crabipie_core::remove_item(&mut self.collection.items, id);
                self.coll_dirty = true;
                if self.sb_selected == Some(id) {
                    self.sb_selected = None;
                }
                if self.sb_editing == Some(id) {
                    self.sb_cancel_rename();
                }
            }
            Act::OpenSave => self.open_save_modal(),
            Act::SaveConfirm => self.save_confirm(),
            Act::SaveCancel => {
                self.save_modal = false;
                self.set_focus(Focus::None);
            }
        }
    }

    // ── mouse (coordinates in DIPs) ─────────────────────────────────

    fn hot_at(&self, x: f32, y: f32) -> Option<(Act, Rect)> {
        let from = if self.any_modal() { self.modal_hot_from.min(self.hot.len()) } else { 0 };
        self.hot[from..].iter().rev().find(|h| h.rect.contains(x, y)).map(|h| (h.act, h.rect))
    }

    /// Returns true when a repaint is needed.
    pub fn on_mouse_move(&mut self, m: &mut dyn TextMeasure, wx: f32, y: f32) -> bool {
        self.mouse_win = (wx, y);
        let x = wx - self.ox();
        self.mouse = (x, y);
        if let Some(d) = self.drag {
            self.drag_to(d, wx, y);
            return true;
        }
        let hover = self.sb_row_at(wx, y);
        let mut changed = hover != self.sb_hover || self.ctx.is_some();
        self.sb_hover = hover;
        let ha = self.hover_at(wx, y).filter(|a| tip(*a).is_some());
        changed |= ha != self.hover_act;
        self.hover_act = ha;
        changed |= self.bar.set_hover(TabBar::hit_test(x, y, self.tabs.len(), self.width));
        if let Some(p) = self.popup.as_mut() {
            let h = p.hit(x, y);
            if h != p.hover {
                p.hover = h;
                changed = true;
            }
        }
        match self.focus {
            Focus::Input(id) => {
                let mx = if id == InputId::SbRename { wx } else { x };
                if let Some(i) = self.input_mut(id) {
                    changed |= i.mouse_move(m, mx);
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
        let had = self.sb_hover.take().is_some() | self.hover_act.take().is_some();
        self.bar.set_hover(Hit::None) | had
    }

    /// `wx` is in window coordinates; the main area is shifted by the sidebar.
    pub fn on_left_down(&mut self, m: &mut dyn TextMeasure, cb: &mut dyn Clipboard, wx: f32, y: f32, shift: bool) -> bool {
        self.touch();
        self.hover_act = None;
        let ox = self.ox();
        // An open popup owns the click, wherever it lands.
        if let Some(p) = self.popup.take() {
            if let Some(i) = p.hit(wx - ox, y) {
                self.apply_picker(p.kind, i);
            }
            return true;
        }
        if let Some(cx) = self.ctx.take() {
            if let Some(act) = self.ctx_hit(&cx, wx, y) {
                self.run(act, Rect::default(), cb);
            }
            return true;
        }
        if self.sb_editing.is_some() && !self.sb_input_hit(wx, y) {
            self.sb_commit_rename();
        }
        if let Some(d) = self.splitter_at(wx, y) {
            self.set_focus(Focus::None);
            self.drag = Some(d);
            return true;
        }
        if wx < ox {
            if self.any_modal() {
                return false;
            }
            return self.sb_left_down(m, cb, wx, y, shift);
        }
        self.left_down_main(m, cb, wx - ox, y, shift)
    }

    fn left_down_main(&mut self, m: &mut dyn TextMeasure, cb: &mut dyn Clipboard, x: f32, y: f32, shift: bool) -> bool {
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

    pub fn on_double_click(&mut self, m: &mut dyn TextMeasure, wx: f32, y: f32) -> bool {
        self.touch();
        let ox = self.ox();
        if wx < ox {
            if self.any_modal() {
                return false;
            }
            return match self.sb_hot_at(wx, y) {
                Some((Act::SbSelect(id), _)) => {
                    self.sb_open(id);
                    true
                }
                Some((Act::Input(id), _)) => {
                    self.set_focus(Focus::Input(id));
                    if let Some(i) = self.input_mut(id) {
                        i.double_click(m, wx);
                    }
                    true
                }
                _ => false,
            };
        }
        self.double_click_main(m, wx - ox, y)
    }

    fn double_click_main(&mut self, m: &mut dyn TextMeasure, x: f32, y: f32) -> bool {
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

    pub fn on_left_up(&mut self) -> bool {
        self.touch();
        if let Some(d) = self.drag.take() {
            if let Drag::Item { id, active, .. } = d {
                if active {
                    self.drop_item(id);
                } else if crabipie_core::find_request(&self.collection.items, id).is_none() {
                    crabipie_core::toggle_expand(&mut self.collection.items, id);
                    self.coll_dirty = true;
                }
            }
            return true;
        }
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
        false
    }

    pub fn on_middle_click(&mut self, wx: f32, y: f32) -> bool {
        self.touch();
        let x = wx - self.ox();
        if x < 0.0 {
            return false;
        }
        match TabBar::hit_test(x, y, self.tabs.len(), self.width) {
            Hit::Tab(i) | Hit::Close(i) => {
                self.close_tab(i);
                true
            }
            _ => false,
        }
    }

    pub fn on_wheel(&mut self, notches: f32) -> bool {
        if self.mouse_win.0 < self.ox() && !self.any_modal() {
            let before = self.sb_scroll;
            self.sb_scroll = (self.sb_scroll - notches * 56.0).max(0.0); // upper bound is clamped when painting
            return before != self.sb_scroll;
        }
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
        if self.gql_tree_rect.contains(x, y) {
            let t = self.tab_mut();
            let before = t.gql_scroll;
            t.gql_scroll = (t.gql_scroll - notches * 66.0).max(0.0); // upper bound is clamped when painting
            return before != t.gql_scroll;
        }
        if self.req_panel.contains(x, y) {
            let max = (self.req_content_h - self.req_view_h).max(0.0);
            let before = self.req_scroll;
            self.req_scroll = (self.req_scroll - notches * 54.0).clamp(0.0, max);
            return before != self.req_scroll;
        }
        false
    }

    pub fn cursor_kind(&self) -> CursorKind {
        let d = self.drag.or_else(|| self.splitter_at(self.mouse_win.0, self.mouse_win.1));
        match d {
            Some(Drag::Sidebar | Drag::Split | Drag::GqlTree) => CursorKind::ResizeWE,
            Some(Drag::GqlHeight) => CursorKind::ResizeNS,
            _ if self.text_cursor_active() => CursorKind::IBeam,
            _ => CursorKind::Arrow,
        }
    }

    /// True when the cursor should be an I-beam.
    pub fn text_cursor_active(&self) -> bool {
        if self.popup.is_some() {
            return false;
        }
        if self.mouse_win.0 < self.ox() {
            return matches!(self.sb_hot_at(self.mouse_win.0, self.mouse_win.1), Some((Act::Input(_), _)));
        }
        matches!(self.hot_at(self.mouse.0, self.mouse.1), Some((Act::Input(_) | Act::Area(_), _)))
    }

    // ── keyboard ────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: Key, mods: Mods, cb: &mut dyn Clipboard) -> bool {
        self.touch();
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
                Key::Letter('B') => {
                    self.toggle_sidebar();
                    return true;
                }
                Key::Letter('S') => {
                    self.open_save_modal();
                    return true;
                }
                _ => {}
            }
        }
        if key == Key::Escape {
            if self.popup.take().is_some() {
                return true;
            }
            if self.ctx.take().is_some() {
                return true;
            }
            if self.sb_editing.is_some() {
                self.sb_cancel_rename();
                return true;
            }
            if self.save_modal {
                self.save_modal = false;
                self.set_focus(Focus::None);
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
        if key == Key::Enter && self.focus == Focus::Input(InputId::SbRename) {
            self.sb_commit_rename();
            return true;
        }
        if key == Key::Enter && self.focus == Focus::Input(InputId::SaveName) {
            self.save_confirm();
            return true;
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
        self.touch();
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
        let th = self.theme.clone();
        let ox = self.ox();
        c.fill_rect(Rect::new(0.0, 0.0, self.win_w.max(self.width + ox), self.height), th.bg);
        self.sb_hot.clear();
        if ox > 0.0 {
            self.paint_sidebar(c, &th);
        }
        {
            let mut s = Shift { inner: &mut *c, dx: ox };
            self.paint_main(&mut s);
        }
        self.paint_ctx(c, &th);
        self.paint_tip(c, &th);
        self.paint_item_drag(c, &th);
    }

    fn paint_main(&mut self, c: &mut dyn Canvas) {
        self.hot.clear();
        self.clip = None;
        self.gql_tree_rect = Rect::default();
        self.gql_vsplit = Rect::default();
        self.gql_hsplit = Rect::default();
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
        let (top, bottom, mid) = (142.0, (ht - 10.0).max(200.0), wd * self.split);
        self.req_panel = Rect::new(10.0, top, (mid - 5.0).max(200.0), bottom);
        self.resp_panel = Rect::new((mid + 5.0).max(210.0), top, (wd - 10.0).max(400.0), bottom);
        if ws {
            self.paint_ws_view(c, &th);
        } else {
            let (rp, sp) = (self.req_panel, self.resp_panel);
            self.paint_request_panel(c, &th, rp);
            self.paint_response_panel(c, &th, sp);
        }

        if self.drag == Some(Drag::Split) && !ws {
            c.fill_rect(Rect::new(self.req_panel.r + 2.0, top, self.resp_panel.l - 2.0, bottom), th.accent.alpha(0.6));
        }

        if self.modal.is_some() {
            self.paint_modal(c, &th);
        } else if self.cookie_modal {
            self.paint_cookie_modal(c, &th);
        } else if self.save_modal {
            self.paint_save_modal(c, &th);
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
    /// GraphQL body: schema tree on the left, Operation / Query / Variables on the right.
    fn paint_gql(&mut self, c: &mut dyn Canvas, th: &Theme, a: Rect) {
        self.gql_a = a;
        let tw = self.gql_tree_w.clamp(200.0, (a.width() - 220.0).max(200.0));
        let left_r = a.l + tw;
        self.paint_input(c, InputId::GqlSearch, Rect::new(a.l, a.t, left_r - 108.0, a.t + 32.0));
        let mut x = left_r - 104.0;
        for (icon, act) in [(Icon::Clear, Act::GqlClearSel), (Icon::Collapse, Act::GqlCollapse), (Icon::Refresh, Act::GqlFetch)] {
            let r = Rect::new(x, a.t, x + 30.0, a.t + 32.0);
            self.icon_button(c, th, r, icon, act);
            self.push_hot(r, act);
            x += 34.0;
        }
        let tree = Rect::new(a.l, a.t + 40.0, left_r, a.b);
        self.gql_tree_rect = tree;
        self.paint_gql_tree(c, th, tree);
        self.gql_vsplit = Rect::new(left_r + 1.0, a.t, left_r + 10.0, a.b);
        let v_on = self.drag == Some(Drag::GqlTree);
        c.line((left_r + 5.0, a.t), (left_r + 5.0, a.b), if v_on { th.accent } else { th.border }, if v_on { 2.0 } else { 1.0 });

        let r = Rect::new(left_r + 12.0, a.t, a.r, a.b);
        c.text("Operation:", Rect::new(r.l, r.t, r.l + 84.0, r.t + 32.0), th.text, w::UI);
        self.paint_input(c, InputId::GqlOp, Rect::new(r.l + 88.0, r.t, r.r, r.t + 32.0));
        let top = r.t + 40.0;
        let avail = (r.b - top - 60.0).max(1.0);
        let qh = (avail * self.gql_qfrac).max(60.0);
        self.gql_hs = (top + 26.0, avail);
        self.gql_hsplit = Rect::new(r.l, top + 26.0 + qh - 1.0, r.r, top + 26.0 + qh + 9.0);
        if self.drag == Some(Drag::GqlHeight) {
            c.fill_rect(Rect::new(r.l, top + 26.0 + qh + 2.0, r.r, top + 26.0 + qh + 6.0), th.accent.alpha(0.6));
        }
        c.text("Query:", Rect::new(r.l, top, r.l + 200.0, top + 24.0), th.text, TextStyle::new(13.0));
        self.paint_area(c, AreaId::GqlQuery, Rect::new(r.l, top + 26.0, r.r, top + 26.0 + qh));
        let vy = top + 26.0 + qh + 8.0;
        c.text("Variables (JSON):", Rect::new(r.l, vy, r.l + 240.0, vy + 24.0), th.text, TextStyle::new(13.0));
        self.paint_area(c, AreaId::GqlVars, Rect::new(r.l, vy + 26.0, r.r, r.b.max(vy + 80.0)));
    }

    fn paint_gql_tree(&mut self, c: &mut dyn Canvas, th: &Theme, tree: Rect) {
        const RH: f32 = 22.0;
        let orange = Color::hex(0xe68019);
        if self.tabs[self.active].gql_schema.is_none() {
            self.gql_rows.clear();
            let t = &self.tabs[self.active];
            let (line1, line2, col) = if t.gql_loading {
                ("Fetching schema…".to_string(), String::new(), th.text_dim)
            } else if let Some(e) = &t.gql_error {
                ("Schema fetch failed".to_string(), e.clone(), th.danger)
            } else {
                ("Press ↻ to fetch the schema".to_string(), String::new(), th.text_dim)
            };
            c.text(&line1, Rect::new(tree.l + 4.0, tree.t, tree.r, tree.t + 26.0), col, w::UI);
            if !line2.is_empty() {
                c.text(&line2, Rect::new(tree.l + 4.0, tree.t + 26.0, tree.r, tree.t + 50.0), th.text_dim, TextStyle::new(12.0));
            }
            return;
        }
        // build the visible rows with an immutable borrow, then paint with a mutable one
        let (root_name, root_open, rows) = {
            let t = &self.tabs[self.active];
            let schema = t.gql_schema.as_ref().unwrap();
            let root_name = schema.query_type.name.clone();
            let open = t.gql_expanded.contains(&root_name);
            let mut rows = Vec::new();
            if open {
                if let Some(rt) = schema.types.iter().find(|x| x.name.as_deref() == Some(root_name.as_str())) {
                    gql_tree::flatten(&schema.types, rt, &t.gql_expanded, &t.gql_selected, t.gql_search.edit.text(), 1, &root_name, &mut rows);
                }
            }
            (root_name, open, rows)
        };
        self.gql_rows = rows;
        let total = (self.gql_rows.len() as f32 + 1.0) * RH;
        let max = (total - tree.height()).max(0.0);
        let scroll = self.tabs[self.active].gql_scroll.clamp(0.0, max);
        self.tabs[self.active].gql_scroll = scroll;

        c.push_clip(tree);
        let saved = self.clip;
        self.clip = Some(tree);
        let y0 = tree.t - scroll;
        let root_row = Rect::new(tree.l, y0, tree.r, y0 + RH);
        c.text(if root_open { "▼" } else { "▶" }, Rect::new(tree.l + 2.0, y0, tree.l + 18.0, y0 + RH), th.text, TextStyle::new(10.0).align(Align::Center));
        c.text(&root_name, Rect::new(tree.l + 22.0, y0, tree.r, y0 + RH), th.text, w::UI);
        self.push_hot(root_row, Act::GqlExpand(usize::MAX));

        let n = self.gql_rows.len();
        let start = ((scroll / RH) as usize).saturating_sub(1).min(n);
        let end = (start + (tree.height() / RH) as usize + 3).min(n);
        for i in start..end {
            let row = self.gql_rows[i].clone();
            let ry = y0 + (i as f32 + 1.0) * RH;
            let x0 = tree.l + 14.0 * row.depth as f32;
            if row.expandable {
                let ar = Rect::new(x0, ry, x0 + 18.0, ry + RH);
                c.text(if row.expanded { "▼" } else { "▶" }, ar, th.text_dim, TextStyle::new(10.0).align(Align::Center));
                self.push_hot(ar, Act::GqlExpand(i));
            }
            let cb = Rect::xywh(x0 + 20.0, ry + 4.0, 14.0, 14.0);
            w::checkbox(c, th, cb, row.selected);
            self.push_hot(cb.inset(-4.0), Act::GqlCheck(i));
            let nx = x0 + 40.0;
            let name_col = if row.is_arg { orange } else { th.text };
            let nw = c.text_width(&row.name, w::UI);
            c.text(&row.name, Rect::new(nx, ry, tree.r, ry + RH), name_col, w::UI);
            c.text(&row.ty, Rect::new(nx + nw + 6.0, ry, tree.r, ry + RH), th.text_dim, TextStyle::new(12.0));
        }
        if max > 0.0 {
            let h = (tree.height() * tree.height() / total).max(24.0);
            let ty = tree.t + (tree.height() - h) * (scroll / max);
            c.fill_rect(Rect::new(tree.r - 6.0, ty, tree.r - 2.0, ty + h), th.text_dim.alpha(0.5));
        }
        self.clip = saved;
        c.pop_clip();
    }

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
        let tg = Rect::xywh(10.0, 8.0, 26.0, 30.0);
        let tg_col = if self.sidebar_open { th.accent } else { th.text };
        for k in 0..3 {
            let y = tg.t + 9.0 + k as f32 * 6.0;
            c.line((tg.l + 5.0, y), (tg.r - 5.0, y), tg_col, 1.6);
        }
        self.push_hot(tg, Act::ToggleSidebar);
        c.text("CrabiPie HTTP Client", Rect::new(46.0, 8.0, 320.0, 38.0), th.text, TextStyle::new(16.0));

        // right-aligned group: Save/Open export and import the current request as a .json file
        let mut x = wd - 10.0;
        for (icon, act) in [(Icon::Save, Act::ExportTab), (Icon::Open, Act::ImportTab)] {
            let r = Rect::new(x - 30.0, 10.0, x, 36.0);
            self.icon_button(c, th, r, icon, act);
            self.push_hot(r, act);
            x -= 34.0;
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
            self.paint_gql(c, th, a);
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
        let field = |s: &mut Self, c: &mut dyn Canvas, label: &str, id: InputId, y: f32| {
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

        // right-aligned actions: copy / prettify / clear, then the JSON theme picker
        let mut x = p.r - 6.0;
        for (glyph, act) in [("⌫", Act::ClearResp), ("✦", Act::PrettifyResp), ("⧉", Act::CopyResp)] {
            let r = Rect::new(x - 30.0, hdr_y - 2.0, x, hdr_y + 26.0);
            c.text(glyph, r, th.text, TextStyle::new(15.0).align(Align::Center));
            self.push_hot(r, act);
            x -= 34.0;
        }
        let jt = Rect::new(x - 150.0, hdr_y - 2.0, x, hdr_y + 26.0);
        self.paint_picker(c, PickerKind::JsonTheme, jt);
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

// ───────────────────────── collections sidebar ─────────────────────────

impl App {
    fn ox(&self) -> f32 {
        if self.sidebar_open { self.sb_w } else { 0.0 }
    }

    /// `width` is the main area's width: the window minus the sidebar.
    fn apply_width(&mut self) {
        self.width = (self.win_w - self.ox()).max(0.0);
    }

    pub fn toggle_sidebar(&mut self) {
        if self.sb_editing.is_some() {
            self.sb_cancel_rename();
        }
        self.ctx = None;
        self.sidebar_open = !self.sidebar_open;
        self.apply_width();
    }

    fn sb_hot_at(&self, x: f32, y: f32) -> Option<(Act, Rect)> {
        self.sb_hot.iter().rev().find(|h| h.rect.contains(x, y)).map(|h| (h.act, h.rect))
    }

    fn sb_row_at(&self, x: f32, y: f32) -> Option<usize> {
        if x >= self.ox() {
            return None;
        }
        match self.sb_hot_at(x, y) {
            Some((Act::SbToggle(id) | Act::SbSelect(id), _)) => Some(id),
            _ => None,
        }
    }

    fn sb_input_hit(&self, x: f32, y: f32) -> bool {
        matches!(self.sb_hot_at(x, y), Some((Act::Input(InputId::SbRename), _)))
    }

    fn sb_push(&mut self, rect: Rect, act: Act, clip: Option<Rect>) {
        if let Some(cl) = clip {
            let (cx, cy) = rect.center();
            if !cl.contains(cx, cy) {
                return;
            }
        }
        self.sb_hot.push(Hot { rect, act });
    }

    fn sb_left_down(&mut self, m: &mut dyn TextMeasure, cb: &mut dyn Clipboard, x: f32, y: f32, shift: bool) -> bool {
        match self.sb_hot_at(x, y) {
            Some((Act::Input(id), _)) => {
                self.set_focus(Focus::Input(id));
                if let Some(i) = self.input_mut(id) {
                    i.mouse_down(m, x, shift); // the rename box lives in window coordinates
                }
                true
            }
            Some((Act::SbToggle(id) | Act::SbSelect(id), _)) => {
                // selecting happens now; a folder toggles on release (unless it was dragged)
                self.set_focus(Focus::None);
                self.sb_selected = Some(id);
                self.drag = Some(Drag::Item { id, sx: x, sy: y, active: false });
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

    pub fn on_right_click(&mut self, x: f32, y: f32) -> bool {
        self.touch();
        let had = self.ctx.take().is_some();
        if self.any_modal() || self.popup.is_some() || x >= self.ox() {
            return had;
        }
        match self.sb_hot_at(x, y) {
            Some((Act::SbToggle(id), _)) => {
                self.sb_selected = Some(id);
                self.ctx = Some(Ctx { id, folder: true, x, y });
                true
            }
            Some((Act::SbSelect(id), _)) => {
                self.sb_selected = Some(id);
                self.ctx = Some(Ctx { id, folder: false, x, y });
                true
            }
            _ => had,
        }
    }

    fn ctx_items(c: &Ctx) -> Vec<(&'static str, Act)> {
        let first = if c.folder { ("Add Subfolder", Act::SbSubfolder(c.id)) } else { ("Open in new tab", Act::SbOpen(c.id)) };
        vec![first, ("Rename", Act::SbRename(c.id)), ("Duplicate", Act::SbDuplicate(c.id)), ("Delete", Act::SbDelete(c.id))]
    }

    fn ctx_rect(&self, c: &Ctx, i: usize) -> Rect {
        let (w, n) = (170.0, 4.0);
        let x = c.x.min(self.win_w - w - 4.0).max(0.0);
        let y = c.y.min(self.height - n * SB_ROW - 4.0).max(0.0);
        Rect::new(x, y + i as f32 * SB_ROW, x + w, y + (i as f32 + 1.0) * SB_ROW)
    }

    fn ctx_hit(&self, c: &Ctx, x: f32, y: f32) -> Option<Act> {
        for (i, (_, act)) in Self::ctx_items(c).into_iter().enumerate() {
            if self.ctx_rect(c, i).contains(x, y) {
                return Some(act);
            }
        }
        None
    }

    fn sb_add_folder(&mut self, parent: Option<usize>) {
        let id = self.next_cid;
        self.next_cid += 1;
        let folder = CollectionItem::Folder(CollectionFolder { id, name: "New Folder".into(), expanded: true, children: Vec::new() });
        if let Some(p) = parent {
            crabipie_core::expand_item(&mut self.collection.items, p);
        }
        if !crabipie_core::insert_into(&mut self.collection.items, parent, folder.clone()) {
            self.collection.items.push(folder);
        }
        self.coll_dirty = true;
        self.sb_selected = Some(id);
        self.sb_start_rename(id);
    }

    fn sb_start_rename(&mut self, id: usize) {
        if self.sb_editing.is_some() {
            self.sb_commit_rename();
        }
        if let Some(name) = crabipie_core::find_name(&self.collection.items, id) {
            self.sb_editing = Some(id);
            self.sb_rename.edit.set_text(&name);
            self.sb_rename.edit.select_all();
            self.set_focus(Focus::Input(InputId::SbRename));
        }
    }

    fn sb_commit_rename(&mut self) {
        if let Some(id) = self.sb_editing.take() {
            let name = self.sb_rename.edit.text().trim().to_string();
            if !name.is_empty() {
                crabipie_core::rename_item(&mut self.collection.items, id, name);
                self.coll_dirty = true;
            }
            self.set_focus(Focus::None);
        }
    }

    fn sb_cancel_rename(&mut self) {
        self.sb_editing = None;
        self.set_focus(Focus::None);
    }

    /// Opens a saved request in a new tab.
    pub fn sb_open(&mut self, id: usize) {
        let Some(saved) = crabipie_core::find_request(&self.collection.items, id).map(|r| r.saved_state.clone()) else { return };
        self.set_focus(Focus::None);
        let tab = Tab::from_saved(self.next_id, &saved);
        self.next_id += 1;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.req_scroll = 0.0;
        self.sb_selected = Some(id);
    }

    fn open_save_modal(&mut self) {
        if self.modal.is_some() || self.cookie_modal {
            return;
        }
        if self.sb_editing.is_some() {
            self.sb_commit_rename();
        }
        self.popup = None;
        self.ctx = None;
        let name = self.tab().label();
        self.save_name.edit.set_text(&name);
        self.save_name.edit.select_all();
        self.save_folder = None;
        self.save_modal = true;
        self.set_focus(Focus::Input(InputId::SaveName));
    }

    fn save_confirm(&mut self) {
        let mut name = self.save_name.edit.text().trim().to_string();
        if name.is_empty() {
            name = self.tab().label();
        }
        let id = self.next_cid;
        self.next_cid += 1;
        let mut saved = self.tab().to_saved(self.theme.name);
        saved.title = name.clone();
        let req = CollectionItem::Request(CollectionRequest { id, name, method: self.tab().method, saved_state: saved });
        let folder = self.save_folder.filter(|f| crabipie_core::find_name(&self.collection.items, *f).is_some());
        if let Some(f) = folder {
            crabipie_core::expand_item(&mut self.collection.items, f);
        }
        if !crabipie_core::insert_into(&mut self.collection.items, folder, req.clone()) {
            self.collection.items.push(req);
        }
        self.coll_dirty = true;
        self.sb_selected = Some(id);
        self.sidebar_open = true;
        self.apply_width();
        self.save_modal = false;
        self.set_focus(Focus::None);
    }

    fn paint_sidebar(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let h = self.height;
        let sw = self.sb_w;
        c.fill_rect(Rect::new(0.0, 0.0, sw, h), th.bg);
        let edge = self.drag == Some(Drag::Sidebar);
        c.line((sw - 0.5, 0.0), (sw - 0.5, h), if edge { th.accent } else { th.border }, if edge { 2.0 } else { 1.0 });
        for (i, (icon, act)) in [(Icon::NewFolder, Act::SbNewFolder), (Icon::Collapse, Act::SbCollapseAll), (Icon::Bookmark, Act::OpenSave)].into_iter().enumerate() {
            let r = Rect::new(8.0 + i as f32 * 34.0, 10.0, 38.0 + i as f32 * 34.0, 36.0);
            self.icon_button(c, th, r, icon, act);
            self.sb_push(r, act, None);
        }
        w::rule(c, th, 8.0, sw - 8.0, 44.0);

        let mut rows = Vec::new();
        sb_rows(&self.collection.items, 0, &mut rows);
        if rows.is_empty() {
            let note = TextStyle::new(12.0);
            c.text("No saved requests.", Rect::new(12.0, 54.0, sw - 12.0, 76.0), th.text_dim, note);
            c.text("Click Save to keep the current tab.", Rect::new(12.0, 76.0, sw - 8.0, 98.0), th.text_dim, note);
            return;
        }
        let area = Rect::new(0.0, 48.0, sw - 1.0, (h - 6.0).max(120.0));
        let content_h = rows.len() as f32 * SB_ROW + 8.0;
        self.sb_scroll = self.sb_scroll.clamp(0.0, (content_h - area.height()).max(0.0));
        c.push_clip(area);
        let mut y = area.t - self.sb_scroll;
        for r in &rows {
            let row = Rect::new(0.0, y, sw - 1.0, y + SB_ROW);
            y += SB_ROW;
            if row.b <= area.t || row.t >= area.b {
                continue;
            }
            if self.sb_selected == Some(r.id) {
                c.fill_rect(row, th.accent.alpha(0.25));
            } else if self.sb_hover == Some(r.id) {
                c.fill_rect(row, th.tab_hover);
            }
            let x0 = 10.0 + r.depth as f32 * 14.0;
            let name_x;
            if r.folder {
                let arrow = if r.expanded { "▾" } else { "▸" };
                c.text(arrow, Rect::new(x0, row.t, x0 + 16.0, row.b), th.text_dim, TextStyle::new(13.0).align(Align::Center));
                name_x = x0 + 20.0;
            } else {
                let col = TabBar::method_color(th, r.method);
                c.text(&r.method.to_string(), Rect::new(x0, row.t, x0 + 46.0, row.b), col, TextStyle::new(10.0).bold());
                name_x = x0 + 50.0;
            }
            let act = if r.folder { Act::SbToggle(r.id) } else { Act::SbSelect(r.id) };
            self.sb_push(row, act, Some(area));
            if self.sb_editing == Some(r.id) {
                let ir = Rect::new(name_x - 4.0, row.t + 2.0, sw - 8.0, row.b - 2.0);
                self.sb_rename.paint(c, th, ir);
                self.sb_push(ir, Act::Input(InputId::SbRename), Some(area));
            } else {
                c.text(&r.name, Rect::new(name_x, row.t, sw - 8.0, row.b), th.text, TextStyle::new(13.0));
            }
        }
        c.pop_clip();
    }

    fn paint_ctx(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let Some(cx) = self.ctx.clone() else { return };
        let items = Self::ctx_items(&cx);
        let all = Rect::new(self.ctx_rect(&cx, 0).l, self.ctx_rect(&cx, 0).t, self.ctx_rect(&cx, 0).r, self.ctx_rect(&cx, items.len() - 1).b);
        c.fill_rect(all, th.surface);
        for (i, (label, act)) in items.iter().enumerate() {
            let r = self.ctx_rect(&cx, i);
            if r.contains(self.mouse_win.0, self.mouse_win.1) {
                c.fill_rect(r, th.tab_hover);
            }
            let col = if matches!(act, Act::SbDelete(_)) { th.danger } else { th.text };
            c.text(label, Rect::new(r.l + 12.0, r.t, r.r, r.b), col, TextStyle::new(13.0));
        }
        c.stroke_rect(all.inset(0.5), th.border, 1.0);
    }

    fn paint_save_modal(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let (wd, ht) = (self.width, self.height);
        c.fill_rect(Rect::new(0.0, 0.0, wd, ht), Color::hex(0x000000).alpha(0.6));
        let dw = (wd - 60.0).clamp(300.0, 460.0);
        let dh = 250.0;
        let d = Rect::xywh((wd - dw) / 2.0, ((ht - dh) / 2.0).max(10.0), dw, dh);
        self.modal_hot_from = self.hot.len();
        self.push_hot(d, Act::Stub);
        c.fill_rect(d, th.bg);
        c.stroke_rect(d.inset(0.5), th.accent, 1.0);
        c.text("Save to Collection", Rect::new(d.l + 14.0, d.t + 10.0, d.r - 14.0, d.t + 38.0), th.text, TextStyle::new(16.0).bold());
        w::rule(c, th, d.l + 8.0, d.r - 8.0, d.t + 48.0);
        let small = TextStyle::new(12.0);
        c.text("Name", Rect::new(d.l + 14.0, d.t + 58.0, d.r - 14.0, d.t + 80.0), th.text_dim, small);
        self.paint_input(c, InputId::SaveName, Rect::new(d.l + 14.0, d.t + 82.0, d.r - 14.0, d.t + 116.0));
        c.text("Save into folder", Rect::new(d.l + 14.0, d.t + 124.0, d.r - 14.0, d.t + 146.0), th.text_dim, small);
        self.paint_picker(c, PickerKind::SaveFolder, Rect::new(d.l + 14.0, d.t + 148.0, d.r - 14.0, d.t + 182.0));
        let cancel = Rect::new(d.r - 190.0, d.b - 48.0, d.r - 102.0, d.b - 14.0);
        w::button(c, th, cancel, "Cancel", false);
        self.push_hot(cancel, Act::SaveCancel);
        let save = Rect::new(d.r - 94.0, d.b - 48.0, d.r - 14.0, d.b - 14.0);
        w::button(c, th, save, "Save", true);
        self.push_hot(save, Act::SaveConfirm);
    }
}

impl App {
    /// The splitter under a window-coordinate point, if any.
    fn splitter_at(&self, wx: f32, y: f32) -> Option<Drag> {
        if self.any_modal() || self.popup.is_some() || self.ctx.is_some() {
            return None;
        }
        let ox = self.ox();
        if self.sidebar_open && (wx - ox).abs() <= 3.0 {
            return Some(Drag::Sidebar);
        }
        let x = wx - ox;
        if self.tab().request_type == RequestType::WebSocket {
            return None;
        }
        let (a, b) = (self.req_panel, self.resp_panel);
        if Rect::new(a.r - 1.0, a.t, b.l + 1.0, a.b).contains(x, y) {
            return Some(Drag::Split);
        }
        if a.contains(x, y) {
            if self.gql_vsplit.contains(x, y) {
                return Some(Drag::GqlTree);
            }
            if self.gql_hsplit.contains(x, y) {
                return Some(Drag::GqlHeight);
            }
        }
        None
    }

    fn drag_to(&mut self, d: Drag, wx: f32, y: f32) {
        let x = wx - self.ox();
        match d {
            Drag::Sidebar => {
                self.sb_w = wx.clamp(180.0, (self.win_w - 420.0).max(180.0));
                self.apply_width();
            }
            Drag::Split => {
                let w = self.width.max(1.0);
                let lo = (210.0 / w).min(0.5);
                self.split = (x / w).clamp(lo, 1.0 - lo);
            }
            Drag::GqlTree => {
                self.gql_tree_w = (x - 5.0 - self.gql_a.l).clamp(200.0, (self.gql_a.width() - 220.0).max(200.0));
            }
            Drag::GqlHeight => {
                self.gql_qfrac = ((y - self.gql_hs.0 - 4.0) / self.gql_hs.1).clamp(0.15, 0.85);
            }
            Drag::Item { id, sx, sy, active } => {
                if !active && ((wx - sx).powi(2) + (y - sy).powi(2)).sqrt() > 5.0 {
                    self.drag = Some(Drag::Item { id, sx, sy, active: true });
                }
            }
        }
    }

    /// Where the dragged item would land if released at the mouse, and the row it points at.
    fn drop_target(&self) -> Option<(DropAt, Rect)> {
        let (x, y) = self.mouse_win;
        if x >= self.ox() {
            return None;
        }
        for h in self.sb_hot.iter().rev() {
            let (tid, folder) = match h.act {
                Act::SbToggle(t) => (t, true),
                Act::SbSelect(t) => (t, false),
                _ => continue,
            };
            if !h.rect.contains(x, y) {
                continue;
            }
            let f = (y - h.rect.t) / h.rect.height();
            let at = if folder {
                if f < 0.25 {
                    DropAt::Before(tid)
                } else if f > 0.75 {
                    DropAt::After(tid)
                } else {
                    DropAt::Into(Some(tid))
                }
            } else if f < 0.5 {
                DropAt::Before(tid)
            } else {
                DropAt::After(tid)
            };
            return Some((at, h.rect));
        }
        // below the last row: end of the root list
        if y >= 48.0 {
            return Some((DropAt::Into(None), Rect::new(0.0, y, self.sb_w, y)));
        }
        None
    }

    fn drop_item(&mut self, id: usize) {
        if let Some((at, _)) = self.drop_target() {
            if crabipie_core::move_item(&mut self.collection.items, id, at) {
                self.coll_dirty = true;
                self.sb_selected = Some(id);
            }
        }
    }

    fn paint_item_drag(&mut self, c: &mut dyn Canvas, th: &Theme) {
        let Some(Drag::Item { id, active: true, .. }) = self.drag else { return };
        if let Some((at, row)) = self.drop_target() {
            let sw = self.sb_w;
            match at {
                DropAt::Before(_) => c.line((row.l + 6.0, row.t), (sw - 6.0, row.t), th.accent, 2.0),
                DropAt::After(_) => c.line((row.l + 6.0, row.b), (sw - 6.0, row.b), th.accent, 2.0),
                DropAt::Into(Some(_)) => c.stroke_rect(row.inset(1.0), th.accent, 2.0),
                DropAt::Into(None) => {}
            }
        }
        if let Some(name) = crabipie_core::find_name(&self.collection.items, id) {
            let (x, y) = self.mouse_win;
            let r = Rect::new(x + 12.0, y + 4.0, x + 212.0, y + 4.0 + SB_ROW);
            c.fill_rect(r, th.surface);
            c.stroke_rect(r.inset(0.5), th.accent, 1.0);
            c.text(&name, Rect::new(r.l + 8.0, r.t, r.r - 4.0, r.b), th.text, TextStyle::new(13.0));
        }
    }
}

// ───────────────────────── persistence ─────────────────────────

/// Everything the background writer needs; built on the UI thread, written elsewhere.
pub struct PersistJob {
    pub tabs: Vec<SavedState>,
    pub meta: Vec<crabipie_core::TabMetadata>,
    pub session: crabipie_core::SessionState,
    /// Only present when the collection changed.
    pub collection: Option<Collection>,
    pub ui: crabipie_core::UiLayout,
}

impl PersistJob {
    pub async fn write(self) {
        tokio::fs::create_dir_all(crabipie_core::state_dir().join("tabs")).await.ok();
        for t in &self.tabs {
            t.save().await;
        }
        crabipie_core::TabMetadata::save_all(&self.meta).await;
        self.session.save().await;
        if let Some(c) = self.collection {
            crabipie_core::save_collection(c).await;
        }
        self.ui.save();
    }
}

impl App {
    /// Marks the state as changed; `take_save` writes it once input goes quiet.
    pub fn touch(&mut self) {
        self.dirty_at = Some(std::time::Instant::now());
    }

    /// A snapshot to write, if something changed. Unless `force`, waits until
    /// the last change is at least 800 ms old so typing doesn't write per key.
    pub fn take_save(&mut self, force: bool) -> Option<PersistJob> {
        let at = self.dirty_at?;
        if !force && at.elapsed() < std::time::Duration::from_millis(800) {
            return None;
        }
        self.dirty_at = None;
        let theme = self.theme.name;
        let tabs = self.tabs.iter().map(|t| t.to_saved(theme)).collect();
        let meta = self
            .tabs
            .iter()
            .map(|t| crabipie_core::TabMetadata { id: t.id, title: t.title.clone(), url: t.url.edit.text().to_string(), method: t.method, request_type: t.request_type.clone() })
            .collect();
        let session = crabipie_core::SessionState {
            active_tab: self.active,
            json_theme: self.session_json_theme.clone(),
            app_theme: theme.to_string(),
            next_tab_id: self.next_id,
            cookie_jar: self.jar.clone(),
            environments: self.envs.clone(),
            active_environment_id: self.session_env_id,
            next_environment_id: self.next_env_id,
        };
        let collection = if std::mem::take(&mut self.coll_dirty) { Some(self.collection.clone()) } else { None };
        let ui = crabipie_core::UiLayout {
            sidebar_open: self.sidebar_open,
            sidebar_width: self.sb_w,
            split: self.split,
            gql_tree_width: self.gql_tree_w,
            gql_query_frac: self.gql_qfrac,
        };
        Some(PersistJob { tabs, meta, session, collection, ui })
    }

    /// (suggested file name, JSON) when the user pressed Save in the title row.
    pub fn take_export(&mut self) -> Option<(String, String)> {
        if !std::mem::take(&mut self.export_req) {
            return None;
        }
        let tab = self.tab();
        let json = serde_json::to_string_pretty(&tab.to_saved(self.theme.name)).ok()?;
        let stem: String = tab.label().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let stem = if stem.trim_matches('_').is_empty() { "request".to_string() } else { stem };
        Some((format!("{stem}.json"), json))
    }

    pub fn take_import_request(&mut self) -> bool {
        std::mem::take(&mut self.import_req)
    }

    /// Replaces the active tab with a request exported by `take_export` (or by the iced app).
    pub fn import_tab(&mut self, json: &str) -> Result<(), String> {
        let s: SavedState = serde_json::from_str(json).map_err(|e| format!("Not a CrabiPie request file: {e}"))?;
        let i = self.active;
        self.set_focus(Focus::None);
        self.tabs[i].cancel.store(true, Ordering::Relaxed);
        self.ws_disconnect_ix(i);
        let id = self.tabs[i].id;
        self.tabs[i] = Tab::from_saved(id, &s);
        self.req_scroll = 0.0;
        self.touch();
        Ok(())
    }
}

// ───────────────────────── icon buttons & tooltips ─────────────────────────

/// Tooltip text for icon-only buttons.
fn tip(act: Act) -> Option<&'static str> {
    Some(match act {
        Act::ToggleSidebar => "Toggle collections (Ctrl+B)",
        Act::ExportTab => "Save request to a file",
        Act::ImportTab => "Open request from a file",
        Act::SbNewFolder => "New folder",
        Act::SbCollapseAll => "Collapse all folders",
        Act::OpenSave => "Save to collection (Ctrl+S)",
        Act::GqlClearSel => "Clear selection",
        Act::GqlCollapse => "Collapse all types",
        Act::GqlFetch => "Fetch schema",
        _ => return None,
    })
}

impl App {
    fn icon_button(&self, c: &mut dyn Canvas, th: &Theme, r: Rect, icon: Icon, act: Act) {
        let hot = self.hover_act == Some(act);
        if hot {
            c.fill_rect(r, th.tab_hover);
        }
        icons::draw(c, icon, r, if hot { th.text } else { th.accent });
    }

    /// The clickable region under a window-coordinate point (as of the last paint).
    fn hover_at(&self, wx: f32, y: f32) -> Option<Act> {
        if self.drag.is_some() || self.popup.is_some() || self.ctx.is_some() {
            return None;
        }
        let ox = self.ox();
        if wx < ox {
            if self.any_modal() {
                return None;
            }
            return self.sb_hot_at(wx, y).map(|(a, _)| a);
        }
        self.hot_at(wx - ox, y).map(|(a, _)| a)
    }

    fn paint_tip(&mut self, c: &mut dyn Canvas, th: &Theme) {
        if self.drag.is_some() || self.ctx.is_some() || self.popup.is_some() {
            return;
        }
        let Some(text) = self.hover_act.and_then(tip) else { return };
        let style = TextStyle::new(12.0);
        let w = c.text_width(text, style) + 16.0;
        let (mx, my) = self.mouse_win;
        let x = (mx + 10.0).min(self.win_w - w - 4.0).max(2.0);
        let y = (my + 22.0).min(self.height - 26.0).max(2.0);
        let r = Rect::new(x, y, x + w, y + 22.0);
        c.fill_rect(r, th.surface);
        c.stroke_rect(r.inset(0.5), th.border, 1.0);
        c.text(text, Rect::new(r.l + 8.0, r.t, r.r, r.b), th.text, style);
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
            let x = x + self.a.ox(); // hot regions are in main-area coordinates
            self.a.on_left_down(&mut self.m, &mut self.cb, x, y, false);
            self.repaint();
        }
        fn sb_click(&mut self, act: Act) {
            let (x, y) = self
                .a
                .sb_hot
                .iter()
                .rev()
                .find(|h| h.act == act)
                .map(|h| h.rect.center())
                .unwrap_or_else(|| panic!("no sidebar region for {act:?}"));
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
            let ox = self.a.ox();
            let p = self.a.popup.as_ref().unwrap();
            let i = p.items.iter().position(|s| s.contains(name)).unwrap_or_else(|| panic!("no item {name}: {:?}", p.items));
            let (x, y) = p.item_rect(i).center();
            self.a.on_left_down(&mut self.m, &mut self.cb, x + ox, y, false);
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

    #[test]
    fn graphql_schema_tree_checkboxes_write_the_query() {
        let mut h = H::new();
        h.pick(PickerKind::ReqType, "GraphQL");
        h.click(Act::ReqTab(ReqTabKind::Body));
        h.click(Act::GqlFetch);
        let jobs = h.a.take_gql_jobs();
        assert_eq!(jobs.len(), 1);
        let schema = crabipie_core::parse_graphql_schema(crate::ui::gql_tree::TEST_SCHEMA).unwrap();
        assert!(h.a.on_schema(jobs[0].tab_id, Ok(schema)));
        h.repaint();
        h.click(Act::GqlExpand(0)); // users: reveals its argument and fields
        h.click(Act::GqlCheck(2)); // rows: users(0), arg: first(1), id(2), name(3)
        let q = h.a.tab().gql_query.edit.text().to_string();
        assert_eq!(q, "query {\n  users {\n    id\n  }\n}");
        h.click(Act::GqlCheck(1)); // the `first` argument
        let q = h.a.tab().gql_query.edit.text().to_string();
        assert!(q.starts_with("query ($first: Int) {") && q.contains("users(first: $first) {"), "{q}");
        assert!(h.a.tab().gql_vars.edit.text().contains("\"first\""));
    }

    #[test]
    fn sidebar_saves_opens_and_edits_the_collection() {
        let mut h = H::new();
        h.click(Act::ToggleSidebar);
        assert_eq!(h.a.width, 1400.0 - h.a.sb_w);

        // a folder, named inline ("New Folder" is selected, so typing replaces it)
        h.sb_click(Act::SbNewFolder);
        let fid = h.a.collection.items[0].id();
        assert_eq!(h.a.sb_editing, Some(fid));
        for ch in "Auth".chars() {
            h.a.on_char(ch);
        }
        h.a.on_key(Key::Enter, Mods::default(), &mut h.cb);
        assert_eq!(h.a.collection.items[0].name(), "Auth");
        assert_eq!(h.a.sb_editing, None);

        // save the current tab into it
        h.type_into(InputId::Url, "https://x.dev/login?a=1");
        h.pick(PickerKind::Method, "POST");
        h.a.tab_mut().auth_type = AuthType::Bearer;
        h.a.tab_mut().bearer.edit.set_text("tok");
        h.sb_click(Act::OpenSave);
        assert!(h.a.save_modal);
        h.type_into(InputId::SaveName, "Login");
        h.pick(PickerKind::SaveFolder, "Auth");
        h.click(Act::SaveConfirm);
        assert!(!h.a.save_modal);
        let CollectionItem::Folder(f) = &h.a.collection.items[0] else { panic!("folder expected") };
        assert_eq!(f.children.len(), 1);
        let rid = f.children[0].id();
        assert_eq!(f.children[0].name(), "Login");

        // open it: the request comes back whole, in a new tab
        h.a.sb_open(rid);
        assert_eq!(h.a.tabs.len(), 2);
        let t = h.a.tab();
        assert_eq!(t.url.edit.text(), "https://x.dev/login?a=1");
        assert_eq!(t.method, HttpMethod::POST);
        assert!(matches!(t.auth_type, AuthType::Bearer));
        assert_eq!(t.bearer.edit.text(), "tok");
        assert_eq!(t.params.len(), 1);

        // right-click -> Duplicate gives the copy a fresh id
        h.repaint();
        let (x, y) = h.a.sb_hot.iter().rev().find(|h| h.act == Act::SbSelect(rid)).unwrap().rect.center();
        assert!(h.a.on_right_click(x, y));
        let cx = h.a.ctx.clone().unwrap();
        let (mx, my) = h.a.ctx_rect(&cx, 2).center(); // Open, Rename, Duplicate, Delete
        h.a.on_left_down(&mut h.m, &mut h.cb, mx, my, false);
        let CollectionItem::Folder(f) = &h.a.collection.items[0] else { panic!("folder expected") };
        assert_eq!(f.children.len(), 2);
        assert_eq!(f.children[1].name(), "Login (copy)");
        assert_ne!(f.children[0].id(), f.children[1].id());

        // delete, then close the sidebar: the main area takes the full width again
        h.a.run(Act::SbDelete(rid), Rect::default(), &mut h.cb);
        let CollectionItem::Folder(f) = &h.a.collection.items[0] else { panic!("folder expected") };
        assert_eq!(f.children.len(), 1);
        h.click(Act::ToggleSidebar);
        assert_eq!(h.a.width, 1400.0);
    }

    #[test]
    fn splitters_resize_panels_and_drag_moves_items() {
        let mut h = H::new();
        // request | response splitter
        let (x, y) = (h.a.req_panel.r + 2.0, h.a.req_panel.t + 50.0);
        h.a.on_left_down(&mut h.m, &mut h.cb, x, y, false);
        assert_eq!(h.a.drag, Some(Drag::Split));
        assert_eq!(h.a.cursor_kind(), CursorKind::ResizeWE);
        h.a.on_mouse_move(&mut h.m, 1000.0, y);
        assert!(h.a.on_left_up());
        h.repaint();
        assert!((h.a.split - 1000.0 / 1400.0).abs() < 0.01);
        assert!(h.a.req_panel.r > 900.0);

        // sidebar edge
        h.click(Act::ToggleSidebar);
        let ox = h.a.ox();
        h.a.on_left_down(&mut h.m, &mut h.cb, ox, 300.0, false);
        assert_eq!(h.a.drag, Some(Drag::Sidebar));
        h.a.on_mouse_move(&mut h.m, 300.0, 300.0);
        h.a.on_left_up();
        assert_eq!(h.a.sb_w, 300.0);
        assert_eq!(h.a.width, 1400.0 - 300.0);

        // drag a request onto a folder
        let req = CollectionItem::Request(CollectionRequest { id: 2, name: "R".into(), method: HttpMethod::GET, saved_state: SavedState::default() });
        let folder = CollectionItem::Folder(CollectionFolder { id: 1, name: "A".into(), expanded: true, children: vec![] });
        h.a.collection.items = vec![folder, req];
        h.repaint();
        let find = |h: &H, act: Act| h.a.sb_hot.iter().find(|x| x.act == act).unwrap().rect.center();
        let (rx, ry) = find(&h, Act::SbSelect(2));
        let (fx, fy) = find(&h, Act::SbToggle(1));
        h.a.on_left_down(&mut h.m, &mut h.cb, rx, ry, false);
        h.a.on_mouse_move(&mut h.m, fx, fy);
        h.a.on_left_up();
        assert_eq!(h.a.collection.items.len(), 1);
        let CollectionItem::Folder(f) = &h.a.collection.items[0] else { panic!("folder expected") };
        assert_eq!(f.children.len(), 1);

        // a plain click on a folder toggles it; Collapse closes everything
        h.repaint();
        let (fx, fy) = find(&h, Act::SbToggle(1));
        h.a.on_left_down(&mut h.m, &mut h.cb, fx, fy, false);
        h.a.on_left_up();
        let CollectionItem::Folder(f) = &h.a.collection.items[0] else { panic!("folder expected") };
        assert!(!f.expanded);
        h.a.run(Act::SbCollapseAll, Rect::default(), &mut h.cb);
    }

    #[test]
    fn persistence_keeps_unowned_fields_debounces_and_round_trips_files() {
        let mut h = H::new();
        let mut base = SavedState::default();
        base.json_theme = "Solarized".into();
        base.response_body = Some("old".into());
        h.a.tab_mut().base = Some(base);
        h.type_into(InputId::Url, "https://x.dev/a");
        assert!(h.a.take_save(false).is_none(), "still inside the debounce window");
        let job = h.a.take_save(true).expect("dirty");
        assert_eq!(job.tabs[0].url, "https://x.dev/a");
        assert_eq!(job.tabs[0].json_theme, "Solarized", "fields the UI doesn't own survive");
        assert_eq!(job.tabs[0].response_body.as_deref(), Some("old"));
        assert_eq!(job.meta[0].url, "https://x.dev/a");
        assert_eq!(job.session.next_tab_id, h.a.next_id);
        assert!(job.collection.is_none());
        assert!(h.a.take_save(true).is_none(), "clean after a snapshot");

        h.a.run(Act::SbCollapseAll, Rect::default(), &mut h.cb);
        h.a.touch();
        assert!(h.a.take_save(true).unwrap().collection.is_some());

        // export, edit, import: the request comes back
        h.a.run(Act::ExportTab, Rect::default(), &mut h.cb);
        let (name, json) = h.a.take_export().unwrap();
        assert!(name.ends_with(".json"));
        h.type_into(InputId::Url, "https://x.dev/b");
        h.a.import_tab(&json).unwrap();
        assert_eq!(h.a.tab().url.edit.text(), "https://x.dev/a");
        assert!(h.a.import_tab("not json").is_err());
    }

    #[test]
    fn icon_buttons_show_tooltips_and_graphql_selection_can_be_cleared() {
        let mut h = H::new();
        let (x, y) = h.rect(Act::ExportTab).center();
        assert!(h.a.on_mouse_move(&mut h.m, x, y));
        assert_eq!(h.a.hover_act, Some(Act::ExportTab));
        h.repaint(); // tooltip + hover highlight paint without trouble
        let (x, y) = h.rect(Act::ImportTab).center();
        h.a.on_mouse_move(&mut h.m, x, y);
        assert_eq!(h.a.hover_act, Some(Act::ImportTab));
        h.a.on_mouse_leave();
        assert_eq!(h.a.hover_act, None);

        h.pick(PickerKind::ReqType, "GraphQL");
        h.click(Act::ReqTab(ReqTabKind::Body));
        h.click(Act::GqlFetch);
        let jobs = h.a.take_gql_jobs();
        let schema = crabipie_core::parse_graphql_schema(crate::ui::gql_tree::TEST_SCHEMA).unwrap();
        h.a.on_schema(jobs[0].tab_id, Ok(schema));
        h.repaint();
        h.click(Act::GqlExpand(0));
        h.click(Act::GqlCheck(2));
        assert!(!h.a.tab().gql_selected.is_empty());
        assert!(h.a.tab().gql_query.edit.text().contains("users"));
        h.click(Act::GqlClearSel);
        assert!(h.a.tab().gql_selected.is_empty() && h.a.tab().gql_manual.is_empty());
        assert!(!h.a.tab().gql_query.edit.text().contains("users"));
    }
}
