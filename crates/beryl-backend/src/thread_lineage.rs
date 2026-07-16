use std::path::{Path, PathBuf};

use beryl_model::{CasThreadId, CasTurnId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ThreadSessionMetadata, ThreadStatus};

/// Stable approval-policy values accepted by the targeted app-server thread boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ThreadApprovalPolicy {
    #[serde(rename = "untrusted")]
    Untrusted,
    #[serde(rename = "on-request")]
    OnRequest,
    #[serde(rename = "never")]
    Never,
}

/// Stable sandbox modes accepted by the targeted app-server thread boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreadSandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

/// Exact execution-root and optional thread-level overrides used to load native lineage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadLoadOptions {
    cwd: PathBuf,
    model: Option<String>,
    model_provider: Option<String>,
    developer_instructions: Option<String>,
    approval_policy: Option<ThreadApprovalPolicy>,
    sandbox: Option<ThreadSandboxMode>,
}

/// One loaded app-server thread normalized without materializing its historical turns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedThreadSession {
    thread_id: CasThreadId,
    status: ThreadStatus,
    metadata: ThreadSessionMetadata,
}

/// A newly started or forked loaded thread that has not yet been used for injection.
#[derive(Debug, PartialEq, Eq)]
pub struct FreshLoadedThreadSession {
    loaded: LoadedThreadSession,
}

/// A consumed proof that one newly started or forked thread was observed loaded and idle.
#[derive(Debug, PartialEq, Eq)]
pub struct FreshIdleThread {
    loaded: LoadedThreadSession,
}

/// Failure to derive an injection target from a fresh loaded thread.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("fresh loaded CAS thread {thread_id} was not idle: {status:?}")]
pub struct FreshThreadNotIdle {
    thread_id: CasThreadId,
    status: ThreadStatus,
}

impl ThreadLoadOptions {
    /// Creates exact load options for the runtime-native execution root.
    #[must_use]
    pub fn for_root(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            model: None,
            model_provider: None,
            developer_instructions: None,
            approval_policy: None,
            sandbox: None,
        }
    }

    /// Overrides the model established while loading the thread.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = nonempty(model.into());
        self
    }

    /// Overrides the model provider established while loading the thread.
    #[must_use]
    pub fn with_model_provider(mut self, provider: impl Into<String>) -> Self {
        self.model_provider = nonempty(provider.into());
        self
    }

    /// Supplies exact hidden developer instructions at the load boundary.
    #[must_use]
    pub fn with_developer_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.developer_instructions = Some(instructions.into());
        self
    }

    /// Overrides the app-server approval policy for the loaded thread.
    #[must_use]
    pub fn with_approval_policy(mut self, policy: ThreadApprovalPolicy) -> Self {
        self.approval_policy = Some(policy);
        self
    }

    /// Overrides the app-server sandbox mode for the loaded thread.
    #[must_use]
    pub fn with_sandbox(mut self, sandbox: ThreadSandboxMode) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    #[must_use]
    pub fn model_provider(&self) -> Option<&str> {
        self.model_provider.as_deref()
    }

    #[must_use]
    pub fn developer_instructions(&self) -> Option<&str> {
        self.developer_instructions.as_deref()
    }

    #[must_use]
    pub const fn approval_policy(&self) -> Option<ThreadApprovalPolicy> {
        self.approval_policy
    }

    #[must_use]
    pub const fn sandbox(&self) -> Option<ThreadSandboxMode> {
        self.sandbox
    }
}

impl LoadedThreadSession {
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn status(&self) -> &ThreadStatus {
        &self.status
    }

    #[must_use]
    pub const fn metadata(&self) -> &ThreadSessionMetadata {
        &self.metadata
    }
}

impl FreshLoadedThreadSession {
    /// Returns the fresh thread as an ordinary loaded session without authorizing injection.
    #[must_use]
    pub fn into_loaded(self) -> LoadedThreadSession {
        self.loaded
    }

    /// Consumes the fresh response and proves that its exact loaded state was idle.
    pub fn into_idle(self) -> Result<FreshIdleThread, FreshThreadNotIdle> {
        if self.loaded.status == ThreadStatus::Idle {
            return Ok(FreshIdleThread {
                loaded: self.loaded,
            });
        }

        Err(FreshThreadNotIdle {
            thread_id: self.loaded.thread_id,
            status: self.loaded.status,
        })
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        self.loaded.thread_id()
    }

    #[must_use]
    pub const fn status(&self) -> &ThreadStatus {
        self.loaded.status()
    }

    #[must_use]
    pub const fn metadata(&self) -> &ThreadSessionMetadata {
        self.loaded.metadata()
    }
}

impl FreshIdleThread {
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        self.loaded.thread_id()
    }

    pub(crate) fn into_loaded(self) -> LoadedThreadSession {
        self.loaded
    }
}

impl FreshThreadNotIdle {
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn status(&self) -> &ThreadStatus {
        &self.status
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadLineageResponse {
    thread: ThreadLineageWire,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    model_provider: Option<String>,
    #[serde(default)]
    reasoning_effort: Option<String>,
}

#[derive(Deserialize)]
struct ThreadLineageWire {
    id: CasThreadId,
    status: ThreadStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadResumeParams<'a> {
    thread_id: &'a CasThreadId,
    cwd: &'a Path,
    exclude_turns: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_provider: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    developer_instructions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_policy: Option<ThreadApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox: Option<ThreadSandboxMode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadForkParams<'a> {
    thread_id: &'a CasThreadId,
    cwd: &'a Path,
    exclude_turns: bool,
    ephemeral: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_turn_id: Option<&'a CasTurnId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_provider: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    developer_instructions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_policy: Option<ThreadApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox: Option<ThreadSandboxMode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadRollbackParams<'a> {
    thread_id: &'a CasThreadId,
    num_turns: u32,
}

impl ThreadLineageResponse {
    pub(crate) fn into_loaded(self) -> LoadedThreadSession {
        LoadedThreadSession {
            thread_id: self.thread.id,
            status: self.thread.status,
            metadata: ThreadSessionMetadata {
                model: self.model.and_then(nonempty),
                model_provider: self.model_provider.and_then(nonempty),
                reasoning_effort: self.reasoning_effort.and_then(nonempty),
            },
        }
    }

    pub(crate) fn into_fresh(self) -> FreshLoadedThreadSession {
        FreshLoadedThreadSession {
            loaded: self.into_loaded(),
        }
    }
}

impl<'a> ThreadResumeParams<'a> {
    pub(crate) fn new(thread_id: &'a CasThreadId, options: &'a ThreadLoadOptions) -> Self {
        Self {
            thread_id,
            cwd: options.cwd(),
            exclude_turns: true,
            model: options.model(),
            model_provider: options.model_provider(),
            developer_instructions: options.developer_instructions(),
            approval_policy: options.approval_policy(),
            sandbox: options.sandbox(),
        }
    }
}

impl<'a> ThreadForkParams<'a> {
    pub(crate) fn full(thread_id: &'a CasThreadId, options: &'a ThreadLoadOptions) -> Self {
        Self::new(thread_id, options, None)
    }

    pub(crate) fn through_turn(
        thread_id: &'a CasThreadId,
        last_turn_id: &'a CasTurnId,
        options: &'a ThreadLoadOptions,
    ) -> Self {
        Self::new(thread_id, options, Some(last_turn_id))
    }

    fn new(
        thread_id: &'a CasThreadId,
        options: &'a ThreadLoadOptions,
        last_turn_id: Option<&'a CasTurnId>,
    ) -> Self {
        Self {
            thread_id,
            cwd: options.cwd(),
            exclude_turns: true,
            ephemeral: false,
            last_turn_id,
            model: options.model(),
            model_provider: options.model_provider(),
            developer_instructions: options.developer_instructions(),
            approval_policy: options.approval_policy(),
            sandbox: options.sandbox(),
        }
    }
}

impl<'a> ThreadRollbackParams<'a> {
    pub(crate) fn new(thread_id: &'a CasThreadId, num_turns: u32) -> Self {
        Self {
            thread_id,
            num_turns,
        }
    }
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
