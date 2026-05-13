use std::time::Duration;

use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_DIAGNOSTIC_THREAD_LIST_LIMIT: usize = 64;
pub(crate) const MAX_DIAGNOSTIC_THREAD_LIST_LIMIT: usize = 128;
pub(crate) const MAX_DIAGNOSTIC_TURN_TEXT_BYTES: usize = 16 * 1024;
pub(crate) const MAX_DIAGNOSTIC_TURN_ID_BYTES: usize = 512;
pub(crate) const DEFAULT_DIAGNOSTIC_WAIT_TIMEOUT_MS: u64 = 5_000;
pub(crate) const MAX_DIAGNOSTIC_WAIT_TIMEOUT_MS: u64 = 10_000;
pub(crate) const DEFAULT_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS: u64 = 100;
pub(crate) const MIN_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS: u64 = 25;
pub(crate) const MAX_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS: u64 = 1_000;
pub(crate) const MAX_DIAGNOSTIC_WAIT_VISIBLE_ROW_LIMIT: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DiagnosticThreadListArguments {
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DiagnosticStartTurnArguments {
    pub(crate) text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DiagnosticStopTurnArguments {
    pub(crate) expected_thread_id: String,
    pub(crate) expected_turn_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DiagnosticWaitForStateArguments {
    pub(crate) predicate: DiagnosticWaitPredicate,
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) poll_interval_ms: Option<u64>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiagnosticWaitPredicate {
    Ready,
    WorkspaceSelected,
    ThreadSelected,
    PendingNewThread,
    SelectedThreadIdle,
    SelectedThreadActive,
    SelectedThreadCompacting,
    TurnStreamPending,
    NoBackgroundWork,
}

impl DiagnosticThreadListArguments {
    pub(crate) fn normalized_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_DIAGNOSTIC_THREAD_LIST_LIMIT)
            .min(MAX_DIAGNOSTIC_THREAD_LIST_LIMIT)
    }
}

impl DiagnosticStartTurnArguments {
    pub(crate) fn validated_text(self) -> Result<String, String> {
        if self.text.trim().is_empty() {
            return Err("text must not be empty".to_string());
        }
        if self.text.len() > MAX_DIAGNOSTIC_TURN_TEXT_BYTES {
            return Err(format!(
                "text exceeds {MAX_DIAGNOSTIC_TURN_TEXT_BYTES} bytes"
            ));
        }
        Ok(self.text)
    }
}

impl DiagnosticStopTurnArguments {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_required_id("expectedThreadId", &self.expected_thread_id)?;
        validate_required_id("expectedTurnId", &self.expected_turn_id)?;
        Ok(())
    }

    pub(crate) fn matches(&self, thread_id: &str, turn_id: &str) -> bool {
        self.expected_thread_id == thread_id && self.expected_turn_id == turn_id
    }
}

impl DiagnosticWaitForStateArguments {
    pub(crate) fn normalized(mut self) -> Result<Self, String> {
        self.timeout_ms = Some(
            self.timeout_ms
                .unwrap_or(DEFAULT_DIAGNOSTIC_WAIT_TIMEOUT_MS)
                .min(MAX_DIAGNOSTIC_WAIT_TIMEOUT_MS),
        );
        self.poll_interval_ms = Some(
            self.poll_interval_ms
                .unwrap_or(DEFAULT_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS)
                .clamp(
                    MIN_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS,
                    MAX_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS,
                ),
        );
        self.limit = Some(
            self.limit
                .unwrap_or(DEFAULT_DIAGNOSTIC_THREAD_LIST_LIMIT)
                .min(MAX_DIAGNOSTIC_WAIT_VISIBLE_ROW_LIMIT),
        );
        self.validate()?;
        Ok(self)
    }

    pub(crate) fn timeout(&self) -> Duration {
        Duration::from_millis(
            self.timeout_ms
                .unwrap_or(DEFAULT_DIAGNOSTIC_WAIT_TIMEOUT_MS),
        )
    }

    pub(crate) fn poll_interval(&self) -> Duration {
        Duration::from_millis(
            self.poll_interval_ms
                .unwrap_or(DEFAULT_DIAGNOSTIC_WAIT_POLL_INTERVAL_MS),
        )
    }

    pub(crate) fn visible_row_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_DIAGNOSTIC_THREAD_LIST_LIMIT)
            .min(MAX_DIAGNOSTIC_WAIT_VISIBLE_ROW_LIMIT)
    }

    fn validate(&self) -> Result<(), String> {
        if matches!(self.predicate, DiagnosticWaitPredicate::WorkspaceSelected)
            && self
                .workspace_id
                .as_ref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err("workspaceId is required for workspace_selected".to_string());
        }
        if matches!(self.predicate, DiagnosticWaitPredicate::ThreadSelected)
            && self
                .thread_id
                .as_ref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err("threadId is required for thread_selected".to_string());
        }
        if self.turn_id.is_some() && self.thread_id.is_none() {
            return Err("threadId is required when turnId is provided".to_string());
        }
        Ok(())
    }
}

fn validate_required_id(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > MAX_DIAGNOSTIC_TURN_ID_BYTES {
        return Err(format!(
            "{field} exceeds {MAX_DIAGNOSTIC_TURN_ID_BYTES} bytes"
        ));
    }
    Ok(())
}
