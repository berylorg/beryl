use serde::{Deserialize, Serialize};

use crate::{JsonRpcError, ThreadInfo, ThreadSessionMetadata};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadBranchCapabilityProbe {
    ThreadFork,
    ThreadRollback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadBranchCapabilityReport {
    probe_results: Vec<ThreadBranchCapabilityProbeResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadBranchCapabilityProbeResult {
    probe: ThreadBranchCapabilityProbe,
    supported: bool,
    error: Option<JsonRpcError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadBranchCapabilities {
    thread_fork: bool,
    thread_rollback: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadForkOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_turns: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadForkResponse {
    pub thread: ThreadInfo,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRollbackResponse {
    pub thread: ThreadInfo,
}

pub(crate) const THREAD_BRANCH_CAPABILITY_PROBES: &[ThreadBranchCapabilityProbe] = &[
    ThreadBranchCapabilityProbe::ThreadFork,
    ThreadBranchCapabilityProbe::ThreadRollback,
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadForkParams<'a> {
    thread_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exclude_turns: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadRollbackParams<'a> {
    thread_id: &'a str,
    num_turns: u32,
}

impl ThreadBranchCapabilityProbe {
    pub fn method(self) -> &'static str {
        match self {
            Self::ThreadFork => "thread/fork",
            Self::ThreadRollback => "thread/rollback",
        }
    }
}

impl ThreadBranchCapabilityReport {
    pub(crate) fn new(probe_results: Vec<ThreadBranchCapabilityProbeResult>) -> Self {
        Self { probe_results }
    }

    pub fn probe_results(&self) -> &[ThreadBranchCapabilityProbeResult] {
        &self.probe_results
    }

    pub fn capabilities(&self) -> ThreadBranchCapabilities {
        let mut capabilities = ThreadBranchCapabilities::default();

        for result in &self.probe_results {
            match result.probe {
                ThreadBranchCapabilityProbe::ThreadFork => {
                    capabilities.thread_fork = result.supported;
                }
                ThreadBranchCapabilityProbe::ThreadRollback => {
                    capabilities.thread_rollback = result.supported;
                }
            }
        }

        capabilities
    }
}

impl ThreadBranchCapabilityProbeResult {
    pub(crate) fn for_supported_probe(probe: ThreadBranchCapabilityProbe) -> Self {
        Self {
            probe,
            supported: true,
            error: None,
        }
    }

    pub(crate) fn unsupported(probe: ThreadBranchCapabilityProbe, error: JsonRpcError) -> Self {
        Self {
            probe,
            supported: false,
            error: Some(error),
        }
    }

    pub fn probe(&self) -> ThreadBranchCapabilityProbe {
        self.probe
    }

    pub fn supported(&self) -> bool {
        self.supported
    }

    pub fn error(&self) -> Option<&JsonRpcError> {
        self.error.as_ref()
    }
}

impl ThreadBranchCapabilities {
    pub fn new(thread_fork: bool, thread_rollback: bool) -> Self {
        Self {
            thread_fork,
            thread_rollback,
        }
    }

    pub fn thread_fork(&self) -> bool {
        self.thread_fork
    }

    pub fn thread_rollback(&self) -> bool {
        self.thread_rollback
    }

    pub fn thread_branching(&self) -> bool {
        self.thread_fork && self.thread_rollback
    }
}

impl ThreadForkOptions {
    pub fn metadata_only() -> Self {
        Self {
            exclude_turns: Some(true),
        }
    }
}

impl ThreadForkResponse {
    pub fn metadata(&self) -> ThreadSessionMetadata {
        ThreadSessionMetadata {
            model: normalize_optional_string(self.model.clone()),
            model_provider: normalize_optional_string(self.model_provider.clone()),
            reasoning_effort: normalize_optional_string(self.reasoning_effort.clone()),
        }
    }
}

impl<'a> ThreadForkParams<'a> {
    pub(crate) fn new(thread_id: &'a str, options: ThreadForkOptions) -> Self {
        Self {
            thread_id,
            exclude_turns: options.exclude_turns,
        }
    }
}

impl<'a> ThreadRollbackParams<'a> {
    pub(crate) fn new(thread_id: &'a str, num_turns: u32) -> Self {
        Self {
            thread_id,
            num_turns,
        }
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}
