//! Application state (`CrabiPie`) and everything that mutates it.

pub(crate) mod collections;
pub(crate) mod environment;
pub(crate) mod find;
pub(crate) mod graphql;
pub(crate) mod media;
pub(crate) mod message;
pub(crate) mod persistence;
pub(crate) mod request;
pub(crate) mod subscription;
pub(crate) mod tab;
pub(crate) mod tabs;
pub(crate) mod update;
pub(crate) mod websocket;

use iced::widget::text_editor;

use crate::app::message::Message;
use crate::app::tab::{TabLoadState, TabState};
use crate::domain::auth::FieldSuggestion;
use crate::domain::collection::{Collection, load_collection};
use crate::domain::cookies::CookieEntry;
use crate::domain::environment::Environment;
use crate::domain::persistence::load_app_state;
use crate::ui::json_highlighter;

pub(crate) struct CrabiPie {
    // Tab management
    pub(crate) tabs: Vec<TabLoadState>,
    pub(crate) active_tab: usize,
    pub(crate) next_tab_id: usize,

    // text_editor contents , same for all tabs,
    pub(crate) request_body_content: text_editor::Content,
    pub(crate) raw_headers_content: text_editor::Content,
    pub(crate) raw_form_content: text_editor::Content,
    pub(crate) graphql_query_content: text_editor::Content,
    pub(crate) graphql_variables_content: text_editor::Content,
    pub(crate) response_body_content: text_editor::Content,
    pub(crate) response_headers_content: text_editor::Content,

    // Global UI state (shared across all tabs)
    pub(crate) json_theme: json_highlighter::JsonThemeWrapper,
    pub(crate) app_theme: iced::Theme,
    pub(crate) svg_rotation: f32,

    // Global environment vars
    pub(crate) environments: Vec<Environment>,
    // Default environment a newly-created tab inherits; each tab can then
    // override its own selection independently (see TabState.active_environment_id).
    pub(crate) active_environment_id: Option<usize>,
    pub(crate) next_environment_id: usize,
    pub(crate) env_panel_open: bool,

    // Auth auto-refresh: which environment's config sub-panel is expanded.
    pub(crate) auth_config_open_env: Option<usize>,
    // Cached "Test Login" results per environment, so suggestions stay on
    // screen until the user applies one or re-tests.
    pub(crate) auth_test_result: std::collections::HashMap<usize, Result<Vec<FieldSuggestion>, String>>,
    pub(crate) auth_test_loading_env: Option<usize>,
    // Tab id waiting to be resent once the in-flight TokenRefreshed
    // completes (set by both the preventive and reactive-401 paths).
    pub(crate) pending_resend_tab: Option<usize>,

    // Global cookie jar
    pub(crate) cookie_jar_open: bool,
    pub(crate) cookie_jar_new_domain: String,
    pub(crate) cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
    pub(crate) cookie_jar_error: Option<String>,

    // Find dialog (global)
    pub(crate) find_dialog_open: bool,
    pub(crate) find_replace_mode: bool,
    pub(crate) find_text: String,
    pub(crate) replace_text: String,
    pub(crate) case_sensitive: bool,
    pub(crate) whole_word: bool,
    pub(crate) current_match: usize,
    pub(crate) current_match_pos: Option<usize>,
    pub(crate) total_matches: usize,

    // For highlighter
    pub(crate) search_match_positions: Vec<(usize, usize)>,
    pub(crate) current_match_line_col: Option<(usize, usize)>,
    pub(crate) search_match_length: usize,

    // collection
    pub(crate) sidebar_open: bool,
    pub(crate) sidebar_selected_id: Option<usize>,
    pub(crate) collection: Collection,
    pub(crate) next_collection_id: usize,
    pub(crate) sidebar_editing_id: Option<usize>,
    pub(crate) sidebar_editing_name: String,
    // Save to collection modal
    pub(crate) save_modal_open: bool,
    pub(crate) save_modal_name: String,
    pub(crate) save_modal_folder_id: Option<usize>,
}

impl CrabiPie {
    pub(crate) fn new() -> (Self, iced::Task<Message>) {
        let app = Self {
            tabs: vec![TabLoadState::Loaded(Box::new(TabState::new(0, None)))],
            active_tab: 0,
            next_tab_id: 1,
            json_theme: json_highlighter::JsonThemeWrapper::Custom(
                json_highlighter::CustomJsonTheme::VSCODE_DARK,
            ),
            app_theme: iced::Theme::CatppuccinMocha,
            svg_rotation: 0.0,
            find_dialog_open: false,
            find_replace_mode: false,
            find_text: String::new(),
            replace_text: String::new(),
            case_sensitive: false,
            whole_word: false,
            current_match: 0,
            current_match_pos: None,
            total_matches: 0,
            search_match_positions: Vec::new(),
            current_match_line_col: None,
            search_match_length: 0,
            sidebar_open: false,
            sidebar_selected_id: None,
            collection: Collection::new(),
            next_collection_id: 1,
            sidebar_editing_id: None,
            sidebar_editing_name: String::new(),
            save_modal_open: false,
            save_modal_name: String::new(),
            save_modal_folder_id: None,
            cookie_jar_open: false,
            cookie_jar_new_domain: String::new(),
            cookie_jar: std::collections::HashMap::new(),
            cookie_jar_error: None,
            raw_headers_content: text_editor::Content::new(),
            raw_form_content: text_editor::Content::new(),
            request_body_content: text_editor::Content::new(),
            graphql_query_content: text_editor::Content::new(),
            graphql_variables_content: text_editor::Content::new(),
            response_headers_content: text_editor::Content::new(),
            response_body_content: text_editor::Content::new(),
            environments: Vec::new(),
            active_environment_id: None,
            next_environment_id: 1,
            env_panel_open: false,
            auth_config_open_env: None,
            auth_test_result: std::collections::HashMap::new(),
            auth_test_loading_env: None,
            pending_resend_tab: None,
        };

        let task = iced::Task::batch([
            iced::Task::perform(load_app_state(), |(session, metadata)| {
                Message::StateLoaded(session, metadata)
            }),
            iced::Task::perform(load_collection(), Message::CollectionLoaded),
        ]);

        (app, task)
    }

    pub(crate) fn title(&self) -> String {
        "CrabiPie".to_string()
    }
}
