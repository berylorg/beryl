#![allow(dead_code)]

use std::io;

use beryl_backend::{
    ManagedBackendError, ManagedBackendStartupProgress, ManagedBackendStartupStage,
};
use beryl_model::workspace::WorkspaceId;

#[derive(Clone, Debug)]
pub(super) struct BackendAvailabilityRecord {
    target: WorkspaceId,
    attempt: u32,
    status: BackendAvailabilityStatus,
}

impl BackendAvailabilityRecord {
    pub(super) fn not_tried(target: WorkspaceId) -> Self {
        Self {
            target,
            attempt: 0,
            status: BackendAvailabilityStatus::NotTried,
        }
    }

    pub(super) fn launching(
        target: WorkspaceId,
        attempt: u32,
        progress: &ManagedBackendStartupProgress,
    ) -> Self {
        Self {
            target,
            attempt,
            status: BackendAvailabilityStatus::Launching {
                stage: progress.stage(),
                detail: progress.detail().map(str::to_string),
            },
        }
    }

    pub(super) fn available(target: WorkspaceId, attempt: u32, process_id: Option<u32>) -> Self {
        Self {
            target,
            attempt,
            status: BackendAvailabilityStatus::Available { process_id },
        }
    }

    pub(super) fn unavailable(
        target: WorkspaceId,
        attempt: u32,
        unavailable: BackendUnavailable,
    ) -> Self {
        Self {
            target,
            attempt,
            status: BackendAvailabilityStatus::Unavailable(unavailable),
        }
    }

    pub(super) fn target(&self) -> &WorkspaceId {
        &self.target
    }

    pub(super) fn attempt(&self) -> u32 {
        self.attempt
    }

    pub(super) fn status(&self) -> &BackendAvailabilityStatus {
        &self.status
    }

    pub(super) fn unavailable_reason(&self) -> Option<&BackendUnavailable> {
        match &self.status {
            BackendAvailabilityStatus::Unavailable(unavailable) => Some(unavailable),
            BackendAvailabilityStatus::NotTried
            | BackendAvailabilityStatus::Launching { .. }
            | BackendAvailabilityStatus::Available { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum BackendAvailabilityStatus {
    NotTried,
    Launching {
        stage: ManagedBackendStartupStage,
        detail: Option<String>,
    },
    Available {
        process_id: Option<u32>,
    },
    Unavailable(BackendUnavailable),
}

#[derive(Clone, Debug)]
pub(super) struct BackendUnavailable {
    kind: BackendUnavailableKind,
    stage: Option<ManagedBackendStartupStage>,
    title: &'static str,
    summary: String,
    detail: String,
    next_steps: Vec<String>,
}

impl BackendUnavailable {
    pub(super) fn new(
        kind: BackendUnavailableKind,
        stage: Option<ManagedBackendStartupStage>,
        title: &'static str,
        summary: String,
        detail: String,
        next_steps: Vec<String>,
    ) -> Self {
        Self {
            kind,
            stage,
            title,
            summary,
            detail,
            next_steps,
        }
    }

    pub(super) fn kind(&self) -> BackendUnavailableKind {
        self.kind
    }

    pub(super) fn stage(&self) -> Option<ManagedBackendStartupStage> {
        self.stage
    }

    pub(super) fn title(&self) -> &'static str {
        self.title
    }

    pub(super) fn summary(&self) -> &str {
        &self.summary
    }

    pub(super) fn detail(&self) -> &str {
        &self.detail
    }

    pub(super) fn next_steps(&self) -> &[String] {
        &self.next_steps
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BackendUnavailableKind {
    MissingExecutable,
    SpawnFailed,
    ProbeFailed,
    Incompatible,
}

impl BackendUnavailableKind {
    pub(super) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::MissingExecutable => "missing_executable",
            Self::SpawnFailed => "spawn_failed",
            Self::ProbeFailed => "probe_failed",
            Self::Incompatible => "incompatible",
        }
    }

    pub(super) fn from_backend_error(error: &ManagedBackendError) -> Self {
        match error {
            ManagedBackendError::Compatibility(_) => Self::Incompatible,
            ManagedBackendError::Spawn { source, .. }
                if source.kind() == io::ErrorKind::NotFound =>
            {
                Self::MissingExecutable
            }
            ManagedBackendError::Spawn { .. } => Self::SpawnFailed,
            ManagedBackendError::BuildCommandLine { .. }
            | ManagedBackendError::MissingPipe { .. }
            | ManagedBackendError::WriteRequest { .. }
            | ManagedBackendError::ReadTransport { .. }
            | ManagedBackendError::InvalidJsonLine { .. }
            | ManagedBackendError::DeserializeResponse { .. }
            | ManagedBackendError::SanitizeResponse { .. }
            | ManagedBackendError::DecodeBase64Response { .. }
            | ManagedBackendError::SerializeRequest { .. }
            | ManagedBackendError::RequestTimeout { .. }
            | ManagedBackendError::ProcessExited { .. }
            | ManagedBackendError::QueryProcessStatus { .. }
            | ManagedBackendError::TerminateProcess { .. }
            | ManagedBackendError::ShutdownTimeout { .. }
            | ManagedBackendError::CreateProcessJob { .. }
            | ManagedBackendError::ConfigureProcessJob { .. }
            | ManagedBackendError::AssignProcessToJob { .. }
            | ManagedBackendError::TerminateProcessJob { .. }
            | ManagedBackendError::SpawnWslProcessGroupCleanup { .. }
            | ManagedBackendError::QueryWslProcessGroupCleanupStatus { .. }
            | ManagedBackendError::TerminateWslProcessGroupCleanup { .. }
            | ManagedBackendError::WslProcessGroupCleanupTimeout { .. }
            | ManagedBackendError::WslProcessGroupCleanupFailed { .. }
            | ManagedBackendError::TransportClosed { .. }
            | ManagedBackendError::SelectWebSocketPort { .. }
            | ManagedBackendError::GenerateWebSocketToken { .. }
            | ManagedBackendError::CreateWebSocketTokenFile { .. }
            | ManagedBackendError::WriteWebSocketTokenFile { .. }
            | ManagedBackendError::CleanUpWebSocketTokenFile { .. }
            | ManagedBackendError::ConnectWebSocket { .. }
            | ManagedBackendError::WebSocketTransport { .. }
            | ManagedBackendError::RequestFailed { .. }
            | ManagedBackendError::UnexpectedMessageShape
            | ManagedBackendError::BoundedResourceExceeded { .. }
            | ManagedBackendError::DeserializeNotification { .. }
            | ManagedBackendError::DeserializeServerRequest { .. } => Self::ProbeFailed,
        }
    }
}
