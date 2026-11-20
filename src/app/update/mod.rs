//! Message routing. Each message group is handled in its own module.

pub(crate) mod collections;
pub(crate) mod cookies;
pub(crate) mod environment;
pub(crate) mod fields;
pub(crate) mod find;
pub(crate) mod general;
pub(crate) mod graphql;
pub(crate) mod media;
pub(crate) mod request;
pub(crate) mod response;
pub(crate) mod tabs;
pub(crate) mod websocket;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::collection;

/// Entry point wired into `iced::application`: routes each message to the
/// handler module that owns it.
pub(crate) fn update(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    if app.sidebar_editing_id.is_some() {
        let is_rename_message = matches!(
            &message,
            Message::CollectionItemRenameInput(_)
                | Message::CollectionItemRenameConfirm(_)
                | Message::CollectionItemRenameCancel
                | Message::EventOccurred(_)
        );
        if !is_rename_message {
            let id = app.sidebar_editing_id.unwrap();
            let new_name = app.sidebar_editing_name.trim().to_string();
            if !new_name.is_empty() {
                collection::rename_item(&mut app.collection.items, id, new_name);
            }
            app.sidebar_editing_id = None;
            app.sidebar_editing_name = String::new();
        }
    }
    match message {
        m @ (
            Message::TabSelected(..)
            | Message::AddNewTab
            | Message::TabBodyLoaded { .. }
            | Message::RequestTabLoad(..)
            | Message::CloseTab(..)
            | Message::TabDuplicate(..)
        ) => tabs::handle(app, m),
        m @ (
            Message::RequestTypeSelected(..)
            | Message::MethodSelected(..)
            | Message::UrlChanged(..)
            | Message::SendRequest
            | Message::SendRequestRetry
            | Message::BodyAction(..)
            | Message::AuthTypeSelected(..)
            | Message::BearerTokenChanged(..)
            | Message::ContentTypeSelected(..)
            | Message::CancelRequest
            | Message::SaveRequest
            | Message::LoadRequest
            | Message::RequestLoaded(..)
            | Message::RequestLoadFailed(..)
            | Message::RequestTabSelected(..)
            | Message::UsernameChanged(..)
            | Message::PasswordChanged(..)
            | Message::ApiKeyPositionChanged(..)
            | Message::ApiKeyNameChanged(..)
            | Message::ApiKeyChanged(..)
            | Message::StreamChunk(..)
            | Message::StreamDone
        ) => request::handle(app, m),
        m @ (
            Message::HeadersViewTypeChanged(..)
            | Message::HeaderRawAction(..)
            | Message::QueryParamAdd
            | Message::QueryParamRemove(..)
            | Message::QueryParamKeyChanged(..)
            | Message::QueryParamValueChanged(..)
            | Message::QueryParamToggled(..)
            | Message::FormFieldKeyChanged(..)
            | Message::FormFieldValueChanged(..)
            | Message::FormFieldTypeSelected(..)
            | Message::FormFieldToggled(..)
            | Message::FormFieldFileSelect(..)
            | Message::FormFieldFilesSelected(..)
            | Message::FormFieldRemove(..)
            | Message::FormFieldAdd
            | Message::ViewRawForm
            | Message::ViewFormattedForm
            | Message::FormRawAction(..)
            | Message::HeaderAdd
            | Message::HeaderRemove(..)
            | Message::HeaderKeyChanged(..)
            | Message::HeaderValueChanged(..)
            | Message::HeaderToggled(..)
        ) => fields::handle(app, m),
        m @ (
            Message::SaveBinaryResponse
            | Message::FileSaved(..)
            | Message::ClearResponseText
            | Message::ResponseReceived(..)
            | Message::ResponseBodyAction(..)
            | Message::ResponseHeadersAction(..)
            | Message::ResponseTabSelected(..)
            | Message::PrettifyJson(..)
            | Message::JsonPrettified(..)
            | Message::CopyToClipboard
            | Message::ResetCopied
        ) => response::handle(app, m),
        m @ (
            Message::GraphqlQueryAction(..)
            | Message::GraphqlVariablesAction(..)
            | Message::GraphqlOperationChanged(..)
            | Message::FetchGraphqlSchema
            | Message::GraphqlSchemaFetched(..)
            | Message::GraphqlFieldToggled(..)
            | Message::GraphqlArgToggled(..)
            | Message::GraphqlTypeToggled(..)
            | Message::GraphqlSearchChanged(..)
            | Message::GraphqlCollapseAll
        ) => graphql::handle(app, m),
        m @ (
            Message::TokenRefreshed(..)
            | Message::EnvironmentAdd
            | Message::EnvironmentSelected(..)
            | Message::EnvAuthPanelToggle(..)
            | Message::EnvAuthModeChanged(..)
            | Message::EnvAuthMethodChanged(..)
            | Message::EnvAuthBodyFormatChanged(..)
            | Message::EnvAuthAutoRefreshToggled(..)
            | Message::EnvAuthFieldChanged(..)
            | Message::EnvAuthTestLogin(..)
            | Message::EnvAuthTestLoginResult(..)
            | Message::EnvAuthApplySuggestion(..)
            | Message::EnvironmentRenamed(..)
            | Message::EnvironmentDeleted(..)
            | Message::EnvVarAdd(..)
            | Message::EnvVarRemove(..)
            | Message::EnvVarKeyChanged(..)
            | Message::EnvVarValueChanged(..)
            | Message::EnvVarToggled(..)
            | Message::EnvPanelOpen
            | Message::EnvPanelClose
        ) => environment::handle(app, m),
        m @ (
            Message::CookieJarOpen
            | Message::CookieJarClose
            | Message::CookieJarAdd(..)
            | Message::CookieJarRemove(..)
            | Message::CookieJarToggled(..)
            | Message::CookieJarNameChanged(..)
            | Message::CookieJarValueChanged(..)
            | Message::CookieJarDomainChanged(..)
            | Message::CookieJarClearDomain(..)
        ) => cookies::handle(app, m),
        m @ (
            Message::WsConnect
            | Message::WsDisconnect
            | Message::WsEvent(..)
            | Message::WsMessageInputChanged(..)
            | Message::WsSendMessage
            | Message::WsClearMessages
            | Message::WsToggleAutoScroll
            | Message::WsMessageEditorAction(..)
            | Message::WsMessageTypeSelected(..)
            | Message::WsBinaryMessageTypeSelected(..)
        ) => websocket::handle(app, m),
        m @ (
            Message::PdfNextPage
            | Message::PdfPreviousPage
            | Message::TogglePause
            | Message::ToggleLoop
            | Message::VideoVolume(..)
            | Message::Seek(..)
            | Message::SeekRelease
            | Message::EndOfStream
            | Message::NewFrame
        ) => media::handle(app, m),
        m @ (
            Message::ToggleFindDialog
            | Message::ToggleFindReplaceDialog
            | Message::CloseFindDialog
            | Message::FindTextChanged(..)
            | Message::ReplaceTextChanged(..)
            | Message::ToggleCaseSensitive
            | Message::ToggleWholeWord
            | Message::FindNext
            | Message::FindPrevious
            | Message::Replace
            | Message::ReplaceAll
        ) => find::handle(app, m),
        m @ (
            Message::CollectionLoaded(..)
            | Message::ToggleSidebar
            | Message::SidebarItemSelected(..)
            | Message::CollectionFolderAdd(..)
            | Message::CollectionItemToggleExpand(..)
            | Message::CollectionRequestOpen(..)
            | Message::CollectionItemRename(..)
            | Message::CollectionItemRenameInput(..)
            | Message::CollectionItemRenameConfirm(..)
            | Message::CollectionItemRenameCancel
            | Message::CollectionItemDelete(..)
            | Message::CollectionItemDuplicate(..)
            | Message::OpenSaveModal
            | Message::SaveModalNameChanged(..)
            | Message::SaveModalFolderSelected(..)
            | Message::SaveModalConfirm
            | Message::SaveModalCancel
            | Message::CollectionSaved
        ) => collections::handle(app, m),
        m @ (
            Message::NoOp
            | Message::Tick
            | Message::JsonThemeChanged(..)
            | Message::AppThemeChanged(..)
            | Message::EventOccurred(..)
            | Message::StateLoaded(..)
            | Message::SaveComplete
        ) => general::handle(app, m),
    }
}
