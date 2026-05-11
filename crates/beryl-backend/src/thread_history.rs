use serde::{Deserialize, Serialize};

use crate::{SortDirection, ThreadInfo, ThreadSessionMetadata, ThreadSummary, TurnInfo};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadResumeOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_turns: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadReadOptions {
    pub include_turns: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadReadResponse {
    pub thread: ThreadInfo,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadReadMetadata {
    pub thread: ThreadSummary,
    pub session_metadata: ThreadSessionMetadata,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTurnsListOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_direction: Option<SortDirection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTurnsListResponse {
    pub data: Vec<TurnInfo>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub backwards_cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadResumeParams<'a> {
    pub thread_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_turns: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadReadParams<'a> {
    pub thread_id: &'a str,
    pub include_turns: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadTurnsListParams<'a> {
    pub thread_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_direction: Option<SortDirection>,
}

impl ThreadResumeOptions {
    pub fn metadata_only() -> Self {
        Self {
            exclude_turns: Some(true),
        }
    }
}

impl ThreadReadResponse {
    pub fn metadata(&self) -> ThreadSessionMetadata {
        ThreadSessionMetadata {
            model: normalize_optional_string(self.model.clone()),
            model_provider: normalize_optional_string(self.model_provider.clone()),
            reasoning_effort: normalize_optional_string(self.reasoning_effort.clone()),
        }
    }

    pub fn read_metadata(&self) -> ThreadReadMetadata {
        ThreadReadMetadata {
            thread: self.thread.summary(),
            session_metadata: self.metadata(),
        }
    }
}

impl ThreadReadOptions {
    pub fn metadata_only() -> Self {
        Self {
            include_turns: false,
        }
    }

    pub fn include_turns() -> Self {
        Self {
            include_turns: true,
        }
    }
}

impl ThreadTurnsListOptions {
    pub fn page(limit: u32) -> Self {
        Self {
            limit: Some(limit),
            ..Self::default()
        }
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    pub fn with_sort_direction(mut self, direction: SortDirection) -> Self {
        self.sort_direction = Some(direction);
        self
    }
}

impl<'a> ThreadResumeParams<'a> {
    pub(crate) fn new(thread_id: &'a str, options: ThreadResumeOptions) -> Self {
        Self {
            thread_id,
            exclude_turns: options.exclude_turns,
        }
    }
}

impl<'a> ThreadReadParams<'a> {
    pub(crate) fn new(thread_id: &'a str, options: ThreadReadOptions) -> Self {
        Self {
            thread_id,
            include_turns: options.include_turns,
        }
    }
}

impl<'a> ThreadTurnsListParams<'a> {
    pub(crate) fn new(thread_id: &'a str, options: &ThreadTurnsListOptions) -> Self {
        Self {
            thread_id,
            cursor: options.cursor.clone(),
            limit: options.limit,
            sort_direction: options.sort_direction,
        }
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}
