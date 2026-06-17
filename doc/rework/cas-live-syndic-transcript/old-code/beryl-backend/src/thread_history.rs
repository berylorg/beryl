use serde::{Deserialize, Serialize};

use crate::{
    JsonRpcError, SortDirection, ThreadInfo, ThreadSessionMetadata, ThreadSummary, TurnInfo,
    TurnItemsView,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadHistoryCapabilityProbe {
    ThreadTurnsListItemsView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadHistoryCapabilityReport {
    probe_results: Vec<ThreadHistoryCapabilityProbeResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadHistoryCapabilityProbeResult {
    probe: ThreadHistoryCapabilityProbe,
    supported: bool,
    error: Option<JsonRpcError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadHistoryCapabilities {
    thread_turns_list_items_view: bool,
}

pub(crate) const THREAD_HISTORY_CAPABILITY_PROBES: &[ThreadHistoryCapabilityProbe] =
    &[ThreadHistoryCapabilityProbe::ThreadTurnsListItemsView];

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_view: Option<TurnItemsView>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_view: Option<TurnItemsView>,
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

impl ThreadHistoryCapabilityProbe {
    pub fn method(self) -> &'static str {
        match self {
            Self::ThreadTurnsListItemsView => "thread/turns/list",
        }
    }
}

impl ThreadHistoryCapabilityReport {
    pub(crate) fn new(probe_results: Vec<ThreadHistoryCapabilityProbeResult>) -> Self {
        Self { probe_results }
    }

    pub fn probe_results(&self) -> &[ThreadHistoryCapabilityProbeResult] {
        &self.probe_results
    }

    pub fn capabilities(&self) -> ThreadHistoryCapabilities {
        let mut capabilities = ThreadHistoryCapabilities::default();

        for result in &self.probe_results {
            match result.probe {
                ThreadHistoryCapabilityProbe::ThreadTurnsListItemsView => {
                    capabilities.thread_turns_list_items_view = result.supported;
                }
            }
        }

        capabilities
    }
}

impl ThreadHistoryCapabilityProbeResult {
    pub(crate) fn for_supported_probe(probe: ThreadHistoryCapabilityProbe) -> Self {
        Self {
            probe,
            supported: true,
            error: None,
        }
    }

    pub fn probe(&self) -> ThreadHistoryCapabilityProbe {
        self.probe
    }

    pub fn supported(&self) -> bool {
        self.supported
    }

    pub fn error(&self) -> Option<&JsonRpcError> {
        self.error.as_ref()
    }
}

impl ThreadHistoryCapabilities {
    pub fn new(thread_turns_list_items_view: bool) -> Self {
        Self {
            thread_turns_list_items_view,
        }
    }

    pub fn thread_turns_list_items_view(&self) -> bool {
        self.thread_turns_list_items_view
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

    pub fn with_items_view(mut self, items_view: TurnItemsView) -> Self {
        self.items_view = Some(items_view);
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
            items_view: options.items_view,
        }
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}
