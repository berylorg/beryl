use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendError, ManagedBackendSession, ThreadListOptions,
};
use beryl_model::{
    conversation::ConversationThreadId,
    provenance::{MutationProvenance, MutationSource},
    threaded_decision::{ThreadedDecisionRecord, ThreadedDecisionRecordId, ThreadedDecisionStatus},
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use gpui::{Context, Window};
use tracing::warn;

use crate::threaded_decision_archive_core::{
    archive_operation_id_for_record, record_needs_child_archive,
};

use super::{ShellView, SurfaceNotice, token_usage_snapshot};

#[derive(Clone, Debug)]
pub(super) struct QueuedDecisionArchiveJob {
    workspace_id: BerylWorkspaceId,
    record_id: ThreadedDecisionRecordId,
}

pub(super) struct DecisionArchiveTask {
    record_id: ThreadedDecisionRecordId,
    receiver: Receiver<DecisionArchiveOutcome>,
}

enum DecisionArchiveOutcome {
    Archived {
        record_id: ThreadedDecisionRecordId,
        thread_id: ConversationThreadId,
        provenance: MutationProvenance,
    },
    Failed {
        record_id: ThreadedDecisionRecordId,
        message: String,
        provenance: MutationProvenance,
    },
}

struct DecisionArchiveTarget {
    record_id: ThreadedDecisionRecordId,
    child_thread_id: ConversationThreadId,
    execution_target: WorkspaceId,
}

impl QueuedDecisionArchiveJob {
    fn new(workspace_id: BerylWorkspaceId, record_id: ThreadedDecisionRecordId) -> Self {
        Self {
            workspace_id,
            record_id,
        }
    }
}

impl DecisionArchiveTask {
    fn new(
        record_id: ThreadedDecisionRecordId,
        receiver: Receiver<DecisionArchiveOutcome>,
    ) -> Self {
        Self {
            record_id,
            receiver,
        }
    }

    fn try_recv(&self) -> Result<DecisionArchiveOutcome, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

impl ShellView {
    pub(super) fn begin_next_ready_decision_archive_cleanup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.decision_archive_receiver.is_some() {
            return false;
        }

        self.hydrate_ready_decision_archive_jobs();
        let Some(job) = self.pending_decision_archive_jobs.front().cloned() else {
            return false;
        };
        if !self
            .loaded_workspace()
            .is_some_and(|loaded| loaded.workspace.id() == &job.workspace_id)
        {
            self.pending_decision_archive_jobs.pop_front();
            return true;
        }

        let Some(record) = self.decision_archive_record(&job.record_id) else {
            self.pending_decision_archive_jobs.pop_front();
            return true;
        };
        if !record_needs_child_archive(&record) {
            self.pending_decision_archive_jobs.pop_front();
            return true;
        }
        let Some(target) = self.decision_archive_target(&record) else {
            self.pending_decision_archive_jobs.pop_front();
            self.mark_decision_archive_failed(
                &job.record_id,
                "The resolved decision child thread is no longer registered.",
                workspace_action_provenance("decision_archive_missing_child_thread"),
            );
            return true;
        };
        let Some(connector) =
            self.backend_client_connector_for_execution_target(&target.execution_target)
        else {
            return false;
        };

        let timestamp = token_usage_snapshot::current_unix_millis();
        let Some(operation_id) = archive_operation_id_for_record(&job.record_id, timestamp) else {
            self.pending_decision_archive_jobs.pop_front();
            self.mark_decision_archive_failed(
                &job.record_id,
                "Beryl could not build a valid archive operation id.",
                workspace_action_provenance("decision_archive_invalid_operation"),
            );
            return true;
        };
        let provenance = MutationProvenance::new(
            "beryl",
            timestamp,
            MutationSource::workspace_action("decision_branch_archive_start")
                .expect("workspace action provenance is valid"),
            Some(100),
        )
        .expect("workspace action provenance is valid");
        match self.workspace_shell_state_mut().map(|loaded| {
            loaded.threaded_decision_state.mark_archive_pending(
                &job.record_id,
                operation_id,
                provenance,
            )
        }) {
            Some(Ok(changed)) => {
                if changed {
                    self.persist_current_threaded_decision_state();
                }
            }
            Some(Err(error)) => {
                warn!(error = %error, "failed to mark threaded-decision archive pending");
                self.pending_decision_archive_jobs.pop_front();
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Decision archive unavailable",
                        error.to_string(),
                    ));
                }
                return true;
            }
            None => return false,
        }

        self.pending_decision_archive_jobs.pop_front();
        self.decision_archive_receiver = Some(spawn_decision_archive_worker(
            connector,
            target,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        true
    }

    pub(super) fn poll_decision_archive_updates(&mut self) -> bool {
        let Some(task) = self.decision_archive_receiver.as_ref() else {
            return false;
        };

        let outcome = match task.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => DecisionArchiveOutcome::Failed {
                record_id: task.record_id.clone(),
                message: "Beryl lost the background task that was closing the decision branch."
                    .to_string(),
                provenance: workspace_action_provenance("decision_archive_worker_disconnected"),
            },
        };
        self.decision_archive_receiver = None;

        match outcome {
            DecisionArchiveOutcome::Archived {
                record_id,
                thread_id,
                provenance,
            } => {
                self.mark_decision_archive_closed(&record_id, provenance);
                self.mark_member_thread_inventory_refresh_needed();
                if let Some(surface) = self.conversation_surface_mut()
                    && surface.selected_thread_id() == Some(thread_id.as_str())
                {
                    surface.set_notice(SurfaceNotice::new(
                        "Decision branch closed",
                        "This resolved decision branch is closed and read-only.",
                    ));
                }
            }
            DecisionArchiveOutcome::Failed {
                record_id,
                message,
                provenance,
            } => {
                self.mark_decision_archive_failed(&record_id, message.clone(), provenance);
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new("Decision branch close failed", message));
                }
            }
        }

        true
    }

    pub(in crate::shell) fn queue_decision_archive_job(
        &mut self,
        workspace_id: BerylWorkspaceId,
        record_id: ThreadedDecisionRecordId,
    ) {
        if self
            .pending_decision_archive_jobs
            .iter()
            .any(|job| job.record_id == record_id)
        {
            return;
        }
        if self
            .decision_archive_receiver
            .as_ref()
            .is_some_and(|task| task.record_id == record_id)
        {
            return;
        }
        self.pending_decision_archive_jobs
            .push_back(QueuedDecisionArchiveJob::new(workspace_id, record_id));
    }

    pub(in crate::shell) fn remove_queued_decision_archive_job(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
    ) {
        self.pending_decision_archive_jobs
            .retain(|job| &job.record_id != record_id);
    }

    pub(in crate::shell) fn hydrate_ready_decision_archive_jobs(&mut self) {
        let Some(loaded) = self.loaded_workspace() else {
            return;
        };
        let workspace_id = loaded.workspace.id().clone();
        let record_ids = loaded
            .threaded_decision_state
            .records()
            .iter()
            .filter(|record| {
                record.status() == ThreadedDecisionStatus::ChecklistUpdated
                    || record.status() == ThreadedDecisionStatus::ArchivePending
            })
            .filter(|record| record.child_thread_id().is_some())
            .map(|record| record.record_id().clone())
            .collect::<Vec<_>>();
        for record_id in record_ids {
            self.queue_decision_archive_job(workspace_id.clone(), record_id);
        }
    }

    fn decision_archive_record(
        &self,
        record_id: &ThreadedDecisionRecordId,
    ) -> Option<ThreadedDecisionRecord> {
        self.loaded_workspace()?
            .threaded_decision_state
            .record(record_id)
            .cloned()
    }

    fn decision_archive_target(
        &self,
        record: &ThreadedDecisionRecord,
    ) -> Option<DecisionArchiveTarget> {
        let child_thread_id = record.child_thread_id()?.clone();
        let execution_target = self
            .loaded_workspace()?
            .workspace_state
            .thread_registration(&child_thread_id)?
            .execution_target()
            .clone();
        Some(DecisionArchiveTarget {
            record_id: record.record_id().clone(),
            child_thread_id,
            execution_target,
        })
    }

    fn mark_decision_archive_closed(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        provenance: MutationProvenance,
    ) {
        let changed = self.workspace_shell_state_mut().is_some_and(|loaded| {
            loaded
                .threaded_decision_state
                .mark_closed(record_id, provenance)
                .unwrap_or_else(|error| {
                    warn!(error = %error, "failed to mark threaded-decision archive closed");
                    false
                })
        });
        if changed {
            self.persist_current_threaded_decision_state();
        }
        self.remove_queued_decision_archive_job(record_id);
    }

    fn mark_decision_archive_failed(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        message: impl Into<String>,
        provenance: MutationProvenance,
    ) {
        let changed = self.workspace_shell_state_mut().is_some_and(|loaded| {
            loaded
                .threaded_decision_state
                .mark_archive_failed(record_id, message, provenance)
                .unwrap_or_else(|error| {
                    warn!(error = %error, "failed to mark threaded-decision archive failed");
                    false
                })
        });
        if changed {
            self.persist_current_threaded_decision_state();
        }
        self.remove_queued_decision_archive_job(record_id);
    }
}

fn spawn_decision_archive_worker(
    connector: ManagedBackendClientConnector,
    target: DecisionArchiveTarget,
    timeout: Duration,
) -> DecisionArchiveTask {
    let (sender, receiver) = mpsc::channel();
    let task = DecisionArchiveTask::new(target.record_id.clone(), receiver);
    thread::spawn(move || {
        let outcome = run_decision_archive_worker(connector, target, timeout);
        let _ = sender.send(outcome);
    });
    task
}

fn run_decision_archive_worker(
    connector: ManagedBackendClientConnector,
    target: DecisionArchiveTarget,
    timeout: Duration,
) -> DecisionArchiveOutcome {
    let mut session = match connector.connect_request_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            return archive_failed(
                target.record_id,
                format!("Beryl could not connect to the managed backend: {error}"),
            );
        }
    };

    match session.archive_thread(target.child_thread_id.as_str(), timeout) {
        Ok(()) => archive_succeeded(target.record_id, target.child_thread_id),
        Err(error) => {
            if archived_inventory_contains_thread(
                &mut session,
                &target.execution_target,
                &target.child_thread_id,
                timeout,
            )
            .unwrap_or(false)
            {
                return archive_succeeded(target.record_id, target.child_thread_id);
            }
            archive_failed(
                target.record_id,
                format!(
                    "Beryl could not close decision branch {}: {error}",
                    target.child_thread_id.as_str()
                ),
            )
        }
    }
}

fn archived_inventory_contains_thread(
    session: &mut ManagedBackendSession,
    execution_target: &WorkspaceId,
    thread_id: &ConversationThreadId,
    timeout: Duration,
) -> Result<bool, ManagedBackendError> {
    let threads = session.list_threads_with_options(
        ThreadListOptions::page(100)
            .archived()
            .with_cwd(execution_target.canonical_path().to_path_buf()),
        timeout,
    )?;
    Ok(threads.iter().any(|thread| thread.id == thread_id.as_str()))
}

fn archive_succeeded(
    record_id: ThreadedDecisionRecordId,
    thread_id: ConversationThreadId,
) -> DecisionArchiveOutcome {
    DecisionArchiveOutcome::Archived {
        record_id,
        thread_id,
        provenance: workspace_action_provenance("decision_branch_archive_closed"),
    }
}

fn archive_failed(
    record_id: ThreadedDecisionRecordId,
    message: impl Into<String>,
) -> DecisionArchiveOutcome {
    DecisionArchiveOutcome::Failed {
        record_id,
        message: message.into(),
        provenance: workspace_action_provenance("decision_branch_archive_failed"),
    }
}

fn workspace_action_provenance(action: &str) -> MutationProvenance {
    MutationProvenance::new(
        "beryl",
        token_usage_snapshot::current_unix_millis(),
        MutationSource::workspace_action(action).expect("workspace action provenance is valid"),
        Some(100),
    )
    .expect("workspace action provenance is valid")
}
