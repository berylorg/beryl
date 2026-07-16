use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ThreadSummary;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    #[serde(flatten)]
    summary: ThreadSummary,
    pub status: ThreadStatus,
}

impl ThreadInfo {
    #[must_use]
    pub fn summary(&self) -> ThreadSummary {
        self.summary.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSessionResponse {
    pub thread: ThreadInfo,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

impl ThreadSessionResponse {
    #[must_use]
    pub fn metadata(&self) -> ThreadSessionMetadata {
        ThreadSessionMetadata {
            model: non_empty_string(self.model.clone()),
            model_provider: non_empty_string(self.model_provider.clone()),
            reasoning_effort: non_empty_string(self.reasoning_effort.clone()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadSessionMetadata {
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ThreadStatus {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        #[serde(default, rename = "activeFlags")]
        active_flags: Vec<ThreadActiveFlag>,
    },
}

impl ThreadStatus {
    #[must_use]
    pub fn waiting_on_user_input(&self) -> bool {
        matches!(
            self,
            Self::Active { active_flags }
                if active_flags.contains(&ThreadActiveFlag::WaitingOnUserInput)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTokenUsage {
    pub last: TokenUsageBreakdown,
    pub total: TokenUsageBreakdown,
    #[serde(default)]
    pub model_context_window: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageBreakdown {
    #[serde(default)]
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub reasoning_output_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitSnapshot {
    #[serde(default)]
    pub limit_id: Option<String>,
    #[serde(default)]
    pub limit_name: Option<String>,
    #[serde(default)]
    pub primary: Option<RateLimitWindow>,
    #[serde(default)]
    pub secondary: Option<RateLimitWindow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRateLimitsResponse {
    pub rate_limits: RateLimitSnapshot,
    #[serde(default)]
    pub rate_limits_by_limit_id: Option<BTreeMap<String, RateLimitSnapshot>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitWindow {
    pub used_percent: i32,
    #[serde(default)]
    pub window_duration_mins: Option<i64>,
    #[serde(default)]
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUnsubscribeResponse {
    pub status: ThreadUnsubscribeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadUnsubscribeStatus {
    NotLoaded,
    NotSubscribed,
    Unsubscribed,
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}
