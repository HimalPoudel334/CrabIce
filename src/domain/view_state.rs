//! View selections (which sub-tab is showing) that are stored with a saved request.

use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum FormViewType {
    Raw,
    Formatted,
}

impl std::fmt::Display for FormViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "View {:?}", self)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum HeaderViewType {
    Formatted,
    Raw,
}

impl std::fmt::Display for HeaderViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Edit {:?}", self)
    }
}

impl HeaderViewType {
    pub(crate) const ALL: [HeaderViewType; 2] = [HeaderViewType::Formatted, HeaderViewType::Raw];
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum RequestTab {
    Body,
    Headers,
    Auth,
    Query,
    GraphQL,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum ResponseTab {
    Body,
    Headers,
}
