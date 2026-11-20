//! The single `Message` enum every widget and background task speaks.

use iced::Event;
use iced::widget::text_editor;

use crate::domain::auth::{
    ApiKeyPosition,
    AuthBodyFormat,
    AuthHttpMethod,
    AuthRefreshMode,
    AuthRefreshOutcome,
    AuthRequestKind,
    AuthType,
    EnvAuthField,
    FieldSuggestion,
    SuggestionKind,
};
use crate::domain::collection::Collection;
use crate::domain::graphql::GraphqlSchema;
use crate::domain::http::{ContentType, FormFieldType, HttpMethod, HttpResponse, RequestType};
use crate::domain::persistence::{SavedState, SessionState, TabMetadata};
use crate::domain::view_state::{HeaderViewType, RequestTab, ResponseTab};
use crate::domain::websocket::{WsBinaryMessageType, WsEvent, WsMessageType};
use crate::ui::json_highlighter;

//TODOS:
//1 collections

#[derive(Debug, Clone)]
pub(crate) enum Message {
    NoOp,
    //Tabs
    AddNewTab,
    TabSelected(usize),
    CloseTab(usize),
    TabDuplicate(usize),
    TabBodyLoaded {
        id: usize,
        saved: Option<SavedState>,
    },
    RequestTabLoad(usize),

    UrlChanged(String),
    RequestTypeSelected(RequestType),
    MethodSelected(HttpMethod),

    //headers actions
    HeaderAdd,
    HeaderRemove(usize),
    HeaderKeyChanged(usize, String),
    HeaderValueChanged(usize, String),
    HeaderToggled(usize),
    HeadersViewTypeChanged(HeaderViewType),
    HeaderRawAction(text_editor::Action),

    BodyAction(text_editor::Action),
    AuthTypeSelected(AuthType),
    UsernameChanged(String),
    PasswordChanged(String),
    ApiKeyNameChanged(String),
    ApiKeyChanged(String),
    ApiKeyPositionChanged(ApiKeyPosition),
    BearerTokenChanged(String),
    ContentTypeSelected(ContentType),
    SendRequest,
    ResponseReceived(HttpResponse),
    RequestTabSelected(RequestTab),
    ResponseTabSelected(ResponseTab),
    ResponseBodyAction(text_editor::Action),
    ResponseHeadersAction(text_editor::Action),
    PrettifyJson(JsonTarget),
    JsonPrettified(JsonTarget, Result<String, String>),
    CopyToClipboard,
    ResetCopied,
    JsonThemeChanged(json_highlighter::JsonThemeWrapper),
    AppThemeChanged(iced::Theme),
    SaveRequest,
    LoadRequest,
    RequestLoaded(SavedState),
    RequestLoadFailed(String),
    CancelRequest,
    SaveBinaryResponse,
    FileSaved(Result<String, String>),
    ClearResponseText,

    // GraphQL
    GraphqlQueryAction(text_editor::Action),
    GraphqlVariablesAction(text_editor::Action),
    GraphqlOperationChanged(String),
    FetchGraphqlSchema,
    GraphqlSchemaFetched(Result<GraphqlSchema, String>),
    GraphqlFieldToggled(String),
    GraphqlTypeToggled(String),
    GraphqlArgToggled(String),
    GraphqlSearchChanged(String),
    GraphqlCollapseAll,

    // Global Env vars
    EnvironmentAdd,
    EnvironmentSelected(Option<usize>),
    EnvironmentRenamed(usize, String),
    EnvironmentDeleted(usize),
    EnvVarAdd(usize),
    EnvVarRemove(usize, usize),
    EnvVarKeyChanged(usize, usize, String),
    EnvVarValueChanged(usize, usize, String),
    EnvVarToggled(usize, usize),
    EnvPanelOpen,
    EnvPanelClose,

    // Auth auto-refresh (per-environment)
    EnvAuthPanelToggle(usize),
    EnvAuthModeChanged(usize, AuthRefreshMode),
    EnvAuthAutoRefreshToggled(usize),
    EnvAuthFieldChanged(usize, EnvAuthField, String),
    EnvAuthMethodChanged(usize, AuthRequestKind, AuthHttpMethod),
    EnvAuthBodyFormatChanged(usize, AuthRequestKind, AuthBodyFormat),
    EnvAuthTestLogin(usize),
    EnvAuthTestLoginResult(usize, Result<Vec<FieldSuggestion>, String>),
    EnvAuthApplySuggestion(usize, SuggestionKind, String),
    TokenRefreshed(usize, Result<AuthRefreshOutcome, String>),
    SendRequestRetry,

    // Global Cookie auth
    CookieJarOpen,
    CookieJarClose,
    CookieJarAdd(String),
    CookieJarRemove(String, usize),
    CookieJarToggled(String, usize),
    CookieJarNameChanged(String, usize, String),
    CookieJarValueChanged(String, usize, String),
    CookieJarDomainChanged(String),
    CookieJarClearDomain(String),

    // Query params
    QueryParamAdd,
    QueryParamRemove(usize),
    QueryParamKeyChanged(usize, String),
    QueryParamValueChanged(usize, String),
    QueryParamToggled(usize),

    // streaming response
    StreamChunk(String),
    StreamDone,

    // WebSocket messages
    WsConnect,
    WsDisconnect,
    WsEvent(WsEvent),
    WsMessageInputChanged(String),
    WsSendMessage,
    WsClearMessages,
    WsToggleAutoScroll,
    WsMessageEditorAction(text_editor::Action),
    WsMessageTypeSelected(WsMessageType),
    WsBinaryMessageTypeSelected(WsBinaryMessageType),

    //Subscription
    Tick,

    // Pdf controls
    PdfNextPage,
    PdfPreviousPage,

    //Video
    TogglePause,
    VideoVolume(f64),
    ToggleLoop,
    Seek(f64),
    SeekRelease,
    EndOfStream,
    NewFrame,

    // Form data messages
    FormFieldKeyChanged(usize, String),
    FormFieldValueChanged(usize, String),
    FormFieldTypeSelected(usize, FormFieldType),
    FormFieldFileSelect(usize),
    FormFieldFilesSelected(usize, Vec<String>),
    FormFieldRemove(usize),
    FormFieldAdd,
    FormFieldToggled(usize),
    ViewRawForm,
    ViewFormattedForm,
    FormRawAction(text_editor::Action),

    // Find/Replace messages
    ToggleFindDialog,
    ToggleFindReplaceDialog,
    CloseFindDialog,
    FindTextChanged(String),
    ReplaceTextChanged(String),
    ToggleCaseSensitive,
    ToggleWholeWord,
    FindNext,
    FindPrevious,
    Replace,
    ReplaceAll,

    // Saving state
    StateLoaded(Option<SessionState>, Vec<TabMetadata>),
    SaveComplete,

    // Sidebar
    ToggleSidebar,
    SidebarItemSelected(usize),
    CollectionLoaded(Option<Collection>),
    CollectionFolderAdd(Option<usize>),
    CollectionItemToggleExpand(usize),
    CollectionRequestOpen(usize),
    CollectionItemRename(usize),
    CollectionItemRenameInput(String),
    CollectionItemRenameConfirm(usize),
    CollectionItemRenameCancel,
    CollectionItemDelete(usize),
    CollectionItemDuplicate(usize),
    // Save modal
    OpenSaveModal,
    SaveModalNameChanged(String),
    SaveModalFolderSelected(Option<usize>),
    SaveModalConfirm,
    SaveModalCancel,
    CollectionSaved,

    EventOccurred(Event),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum JsonTarget {
    RequestBody,
    ResponseBody,
    ResponseHeaders,
}
