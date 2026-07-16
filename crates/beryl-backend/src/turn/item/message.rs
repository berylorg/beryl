use std::path::PathBuf;

use beryl_model::CasItemId;
use serde::{Deserialize, Deserializer, Serialize};

use crate::ProtocolPhase;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextElement {
    pub byte_range: ByteRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    Auto,
    Low,
    High,
    Original,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UserInput {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        text_elements: Vec<TextElement>,
    },
    Image {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<ImageDetail>,
        url: String,
    },
    LocalImage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<ImageDetail>,
        path: PathBuf,
    },
    Skill {
        name: String,
        path: PathBuf,
    },
    Mention {
        name: String,
        path: String,
    },
}

impl UserInput {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            text_elements: Vec::new(),
        }
    }

    #[must_use]
    pub fn local_image(path: impl Into<PathBuf>) -> Self {
        Self::LocalImage {
            detail: None,
            path: path.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessageItem {
    pub id: CasItemId,
    pub client_id: Option<String>,
    pub content: Vec<UserInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookPromptFragment {
    pub text: String,
    pub hook_run_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookPromptItem {
    pub id: CasItemId,
    pub fragments: Vec<HookPromptFragment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCitationEntry {
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCitation {
    pub entries: Vec<MemoryCitationEntry>,
    pub thread_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageItem {
    pub id: CasItemId,
    pub text: String,
    #[serde(default)]
    pub phase: Option<ProtocolPhase>,
    #[serde(default)]
    pub memory_citation: Option<MemoryCitation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub id: CasItemId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReasoningItem {
    pub id: CasItemId,
    pub summary: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReasoningItemWire {
    id: CasItemId,
    #[serde(default)]
    summary: Vec<String>,
    #[serde(default, rename = "content")]
    _content: Vec<String>,
}

impl<'de> Deserialize<'de> for ReasoningItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ReasoningItemWire::deserialize(deserializer)?;
        Ok(Self {
            id: wire.id,
            summary: wire.summary,
        })
    }
}
