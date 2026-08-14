use std::{
    fmt,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendSession, ThreadInfo, ThreadRollbackResponse,
    TurnStartOptions,
};
use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
use tracing::warn;

use super::{
    composer_draft::AcceptedComposerDraft,
    composer_image_delivery::{PreparedComposerDraft, prepare_accepted_composer_images},
    composer_submission::prepared_composer_draft_fragment,
    execution_detail::{TranscriptImagePathResolver, UserInputFragment},
    transcript_edit_menu_state::TranscriptEditTarget,
    transcript_image_sources::transcript_image_path_resolver_for_turns,
};
use crate::BerylWorkspacePersistence;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptEditCommitRequest {
    workspace_id: BerylWorkspaceId,
    execution_target: WorkspaceId,
    target: TranscriptEditTarget,
    discarded_turn_ids: Vec<String>,
    accepted_draft: AcceptedComposerDraft,
    automatic_title_generation_allowed: bool,
    turn_options: TurnStartOptions,
}

#[derive(Clone)]
pub(super) enum TranscriptEditCommitOutcome {
    RolledBack {
        request: TranscriptEditCommitRequest,
        thread: ThreadInfo,
        image_resolver: TranscriptImagePathResolver,
        replacement_fragment: UserInputFragment,
    },
    PreRollbackFailed {
        request: TranscriptEditCommitRequest,
        message: String,
    },
    RollbackFailed {
        request: TranscriptEditCommitRequest,
        message: String,
    },
}

pub(super) enum TranscriptEditCommitUpdate {
    Finished(TranscriptEditCommitOutcome),
}

pub(crate) trait TranscriptEditRollbackBackend {
    type Error: fmt::Display;

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error>;
}

impl TranscriptEditRollbackBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        ManagedBackendSession::rollback_thread(self, thread_id, num_turns, timeout)
    }
}

impl TranscriptEditCommitRequest {
    pub(super) fn new(
        workspace_id: BerylWorkspaceId,
        execution_target: WorkspaceId,
        target: TranscriptEditTarget,
        discarded_turn_ids: Vec<String>,
        accepted_draft: AcceptedComposerDraft,
        automatic_title_generation_allowed: bool,
        turn_options: TurnStartOptions,
    ) -> Self {
        Self {
            workspace_id,
            execution_target,
            target,
            discarded_turn_ids,
            accepted_draft,
            automatic_title_generation_allowed,
            turn_options,
        }
    }

    pub(super) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }

    pub(super) fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }

    pub(super) fn source_thread_id(&self) -> &str {
        self.target.source_thread_id()
    }

    pub(super) fn rollback_turn_count(&self) -> u32 {
        self.target.rollback_turn_count()
    }

    pub(super) fn discarded_turn_ids(&self) -> &[String] {
        &self.discarded_turn_ids
    }

    pub(super) fn accepted_draft(&self) -> &AcceptedComposerDraft {
        &self.accepted_draft
    }

    pub(super) fn automatic_title_generation_allowed(&self) -> bool {
        self.automatic_title_generation_allowed
    }

    pub(super) fn turn_options(&self) -> &TurnStartOptions {
        &self.turn_options
    }
}

pub(super) fn spawn_transcript_edit_commit_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    request: TranscriptEditCommitRequest,
    timeout: Duration,
) -> Receiver<TranscriptEditCommitUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let outcome = run_transcript_edit_commit_worker(persistence, connector, request, timeout);
        let _ = sender.send(TranscriptEditCommitUpdate::Finished(outcome));
    });
    receiver
}

fn run_transcript_edit_commit_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    request: TranscriptEditCommitRequest,
    timeout: Duration,
) -> TranscriptEditCommitOutcome {
    let replacement_fragment = match replacement_fragment_for_draft(
        &persistence,
        request.workspace_id(),
        request.execution_target(),
        request.accepted_draft(),
    ) {
        Ok(fragment) => fragment,
        Err(message) => {
            return TranscriptEditCommitOutcome::PreRollbackFailed { request, message };
        }
    };

    let mut session = match connector.connect_request_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            return TranscriptEditCommitOutcome::RollbackFailed {
                request,
                message: format!("Beryl could not connect to the managed backend: {error}"),
            };
        }
    };

    run_transcript_edit_rollback(&mut session, request, replacement_fragment, timeout).map_thread(
        |request, thread, replacement_fragment| {
            let image_resolver = match transcript_image_path_resolver_for_turns(
                &persistence,
                request.workspace_id(),
                request.execution_target().runtime_mode(),
                &thread.turns,
                &mut session,
                timeout,
            ) {
                Ok(resolver) => resolver,
                Err(error) => {
                    warn!(
                        workspace_id = request.workspace_id().as_str(),
                        thread_id = request.source_thread_id(),
                        error = %error,
                        "failed to prepare transcript image source resolver after thread edit rollback"
                    );
                    TranscriptImagePathResolver::default()
                }
            };
            TranscriptEditCommitOutcome::RolledBack {
                request,
                thread,
                image_resolver,
                replacement_fragment,
            }
        },
    )
}

pub(crate) fn run_transcript_edit_rollback<B>(
    backend: &mut B,
    request: TranscriptEditCommitRequest,
    replacement_fragment: UserInputFragment,
    timeout: Duration,
) -> TranscriptEditCommitOutcome
where
    B: TranscriptEditRollbackBackend,
{
    match backend.rollback_thread(
        request.source_thread_id(),
        request.rollback_turn_count(),
        timeout,
    ) {
        Ok(response) => TranscriptEditCommitOutcome::RolledBack {
            request,
            thread: response.thread,
            image_resolver: TranscriptImagePathResolver::default(),
            replacement_fragment,
        },
        Err(error) => TranscriptEditCommitOutcome::RollbackFailed {
            request,
            message: format!("Beryl could not roll back the conversation thread: {error}"),
        },
    }
}

fn replacement_fragment_for_draft(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    execution_target: &WorkspaceId,
    draft: &AcceptedComposerDraft,
) -> Result<UserInputFragment, String> {
    if draft.contains_images() {
        let images = prepare_accepted_composer_images(
            persistence,
            workspace_id,
            draft,
            execution_target.runtime_mode(),
        )
        .map_err(|error| error.to_string())?;
        let prepared = PreparedComposerDraft::new(draft.clone(), images);
        return prepared_composer_draft_fragment(&prepared);
    }

    draft
        .text_only()
        .map(UserInputFragment::text)
        .ok_or_else(|| "Beryl could not submit an empty edited message.".to_string())
}

trait TranscriptEditCommitOutcomeMap {
    fn map_thread(
        self,
        map: impl FnOnce(
            TranscriptEditCommitRequest,
            ThreadInfo,
            UserInputFragment,
        ) -> TranscriptEditCommitOutcome,
    ) -> TranscriptEditCommitOutcome;
}

impl TranscriptEditCommitOutcomeMap for TranscriptEditCommitOutcome {
    fn map_thread(
        self,
        map: impl FnOnce(
            TranscriptEditCommitRequest,
            ThreadInfo,
            UserInputFragment,
        ) -> TranscriptEditCommitOutcome,
    ) -> TranscriptEditCommitOutcome {
        match self {
            TranscriptEditCommitOutcome::RolledBack {
                request,
                thread,
                replacement_fragment,
                ..
            } => map(request, thread, replacement_fragment),
            other => other,
        }
    }
}
