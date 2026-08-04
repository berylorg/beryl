use std::path::{Path, PathBuf};

use beryl_model::CasThreadId;
use serde::Serialize;
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

#[cfg(feature = "lifecycle-test-support")]
pub(crate) fn fresh_idle_thread_for_lifecycle_test(thread_id: CasThreadId) -> FreshIdleThread {
    FreshIdleThread {
        loaded: LoadedThreadSession {
            thread_id,
            status: ThreadStatus::Idle,
            metadata: ThreadSessionMetadata::default(),
        },
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

/// Compact bounded lineage result retained by the incremental response decoder.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq)]
pub struct ThreadLineageResponse {
    thread_id: CasThreadId,
    status: ThreadStatus,
    model: Option<String>,
    model_provider: Option<String>,
    reasoning_effort: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadRollbackParams<'a> {
    thread_id: &'a CasThreadId,
    num_turns: u32,
}

impl ThreadLineageResponse {
    pub(crate) fn try_new(
        thread_id: &str,
        status: ThreadStatus,
        model: &str,
        model_provider: &str,
        reasoning_effort: Option<&str>,
    ) -> Option<Self> {
        Some(Self {
            thread_id: CasThreadId::new(thread_id).ok()?,
            status,
            model: Some(bounded_identity(model)?),
            model_provider: Some(bounded_identity(model_provider)?),
            reasoning_effort: match reasoning_effort {
                Some(reasoning_effort) => Some(bounded_identity(reasoning_effort)?),
                None => None,
            },
        })
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn status(&self) -> &ThreadStatus {
        &self.status
    }

    #[doc(hidden)]
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn model_provider(&self) -> Option<&str> {
        self.model_provider.as_deref()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    pub(crate) fn into_loaded(self) -> LoadedThreadSession {
        LoadedThreadSession {
            thread_id: self.thread_id,
            status: self.status,
            metadata: ThreadSessionMetadata {
                model: self.model,
                model_provider: self.model_provider,
                reasoning_effort: self.reasoning_effort,
            },
        }
    }

    pub(crate) fn into_fresh(self) -> FreshLoadedThreadSession {
        FreshLoadedThreadSession {
            loaded: self.into_loaded(),
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

fn bounded_identity(value: &str) -> Option<String> {
    crate::ProtocolIdentity::try_new(value)
        .ok()
        .map(|identity| identity.as_str().to_owned())
}
