use std::path::PathBuf;

use beryl_model::CasItemId;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebSearchAction {
    Search {
        query: Option<String>,
        queries: Option<Vec<String>>,
    },
    OpenPage {
        url: Option<String>,
    },
    FindInPage {
        url: Option<String>,
        pattern: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchItem {
    pub id: CasItemId,
    pub query: String,
    pub action: Option<WebSearchAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageViewItem {
    pub id: CasItemId,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepItem {
    pub id: CasItemId,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationItem {
    pub id: CasItemId,
    pub status: String,
    pub revised_prompt: Option<String>,
    #[serde(default)]
    pub saved_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnteredReviewModeItem {
    pub id: CasItemId,
    pub review: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitedReviewModeItem {
    pub id: CasItemId,
    pub review: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompactionItem {
    pub id: CasItemId,
}
