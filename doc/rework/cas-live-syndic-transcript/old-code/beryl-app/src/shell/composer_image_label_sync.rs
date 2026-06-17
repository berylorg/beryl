use std::sync::mpsc::{Receiver, TryRecvError};

use beryl_model::workspace::BerylWorkspaceId;
use gpui::{Context, Window};

use super::{
    ConversationSurfaceState, PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX, ShellView,
    SurfaceNotice,
    composer_image_label_scan::{
        ComposerImageLabelFrontierValidationOutcome, ComposerImageLabelScanOutcome,
        ComposerImageLabelScanPlan, ComposerImageLabelScanUpdate,
        ComposerImageLabelValidationUpdate, ComposerImageLabelValidationWorkerOutcome,
        spawn_composer_image_label_scan_worker_for_plan,
        spawn_composer_image_label_validation_worker,
    },
    composer_image_labels::{
        ComposerImageLabelHistoryFrontier, ComposerImageLabelHistorySyncRequest,
        ComposerImageLabelObservations, ComposerImagePasteReadiness,
    },
    execution_detail::UserInputFragment,
};

pub(super) struct ComposerImageLabelScanTask {
    workspace_id: BerylWorkspaceId,
    thread_id: String,
    receiver: Receiver<ComposerImageLabelScanUpdate>,
}

pub(super) struct ComposerImageLabelValidationTask {
    workspace_id: BerylWorkspaceId,
    thread_id: String,
    receiver: Receiver<ComposerImageLabelValidationUpdate>,
}

impl ConversationSurfaceState {
    pub(super) fn try_allocate_composer_image_label(
        &mut self,
        reserved_labels: &[String],
    ) -> Result<String, ComposerImagePasteReadiness> {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.composer_image_labels
            .try_allocate(selected_thread_id.as_deref(), reserved_labels)
    }

    pub(super) fn composer_image_paste_readiness(&self) -> ComposerImagePasteReadiness {
        self.composer_image_labels
            .paste_readiness(self.selected_thread_id())
    }

    pub(super) fn selected_thread_needing_composer_image_label_sync(
        &self,
    ) -> Option<ComposerImageLabelHistorySyncRequest> {
        self.composer_image_labels
            .selected_thread_needing_history_sync(self.selected_thread_id())
    }

    pub(super) fn begin_composer_image_label_scan(&mut self, thread_id: &str) -> bool {
        self.composer_image_labels
            .begin_thread_history_scan(thread_id)
    }

    pub(super) fn begin_composer_image_label_validation(&mut self, thread_id: &str) -> bool {
        self.composer_image_labels
            .begin_thread_history_validation(thread_id)
    }

    pub(super) fn finish_composer_image_label_validation(
        &mut self,
        thread_id: &str,
        frontier: ComposerImageLabelHistoryFrontier,
    ) -> bool {
        self.composer_image_labels
            .finish_thread_history_validation(thread_id, frontier)
    }

    pub(super) fn begin_composer_image_label_scan_after_validation(
        &mut self,
        thread_id: &str,
        frontier: ComposerImageLabelHistoryFrontier,
    ) -> bool {
        self.composer_image_labels
            .begin_thread_history_scan_after_validation(thread_id, frontier)
    }

    pub(super) fn observe_composer_image_labels_in_fragment(
        &mut self,
        fragment: &UserInputFragment,
    ) {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.composer_image_labels
            .observe_backend_input(selected_thread_id.as_deref(), fragment.backend_input());
    }

    pub(super) fn observe_composer_image_labels_in_thread_fragment(
        &mut self,
        thread_id: &str,
        fragment: &UserInputFragment,
    ) {
        self.composer_image_labels
            .observe_thread_backend_input(thread_id, fragment.backend_input());
    }

    pub(super) fn bind_pending_new_thread_image_labels_to_thread(&mut self, thread_id: &str) {
        self.composer_image_labels
            .bind_pending_new_thread_to_thread(thread_id);
        self.pending_new_thread_label_scope_bindings.insert(
            self.pending_new_thread_label_scope_id,
            thread_id.to_string(),
        );
        self.prune_pending_new_thread_label_scope_bindings();
        self.composer_history.bind_pending_new_thread_to_thread(
            self.pending_new_thread_label_scope_id,
            thread_id.to_string(),
        );
    }

    fn prune_pending_new_thread_label_scope_bindings(&mut self) {
        if self.pending_new_thread_label_scope_bindings.len()
            <= PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX
        {
            return;
        }

        let current_scope = self.pending_new_thread_label_scope_id;
        let mut removable_scopes = self
            .pending_new_thread_label_scope_bindings
            .keys()
            .copied()
            .filter(|scope_id| *scope_id != current_scope)
            .collect::<Vec<_>>();
        removable_scopes.sort_unstable();
        for scope_id in removable_scopes {
            if self.pending_new_thread_label_scope_bindings.len()
                <= PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX
            {
                break;
            }
            self.pending_new_thread_label_scope_bindings
                .remove(&scope_id);
        }
    }

    pub(super) fn finish_composer_image_label_scan(
        &mut self,
        thread_id: &str,
        observations: ComposerImageLabelObservations,
        frontier: ComposerImageLabelHistoryFrontier,
    ) -> bool {
        self.composer_image_labels
            .finish_in_flight_thread_history_scan_with_frontier(
                thread_id,
                observations,
                Some(frontier),
            )
    }

    pub(super) fn fail_composer_image_label_scan(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) {
        self.composer_image_labels
            .fail_thread_history_scan(thread_id, message);
    }

    pub(super) fn fail_composer_image_label_validation(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) {
        self.composer_image_labels
            .fail_thread_history_validation(thread_id, message);
    }

    pub(super) fn fail_in_flight_composer_image_label_scan(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) -> bool {
        self.composer_image_labels
            .fail_in_flight_thread_history_scan(thread_id, message)
    }

    pub(super) fn fail_in_flight_composer_image_label_validation(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) -> bool {
        self.composer_image_labels
            .fail_in_flight_thread_history_validation(thread_id, message)
    }

    pub(super) fn mark_selected_thread_image_labels_need_validation_if_updated(
        &mut self,
        thread_id: &str,
        updated_at: i64,
    ) -> bool {
        let Some(index) = self.selected_thread else {
            return false;
        };
        let Some(thread) = self.known_threads.get_mut(index) else {
            return false;
        };
        if thread.id != thread_id || thread.updated_at == updated_at {
            return false;
        }

        thread.updated_at = updated_at;
        self.composer_image_labels
            .mark_thread_history_needs_validation(thread_id);
        true
    }
}

impl ShellView {
    fn composer_image_label_task_matches_selected(
        &self,
        workspace_id: &BerylWorkspaceId,
        thread_id: &str,
    ) -> bool {
        self.loaded_workspace()
            .is_some_and(|loaded| loaded.workspace.id() == workspace_id)
            && self
                .conversation_surface()
                .is_some_and(|surface| surface.selected_thread_id() == Some(thread_id))
    }

    pub(super) fn poll_composer_image_label_validation_updates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(receiver) = self
            .composer_image_label_validation_receiver
            .as_ref()
            .map(|task| &task.receiver)
        else {
            return false;
        };

        match receiver.try_recv() {
            Ok(ComposerImageLabelValidationUpdate::Finished(outcome)) => {
                let Some(task) = self.composer_image_label_validation_receiver.take() else {
                    return false;
                };
                self.finish_composer_image_label_validation_worker(task, outcome, window, cx);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                let Some(task) = self.composer_image_label_validation_receiver.take() else {
                    return false;
                };
                if self
                    .composer_image_label_task_matches_selected(&task.workspace_id, &task.thread_id)
                    && let Some(surface) = self.conversation_surface_mut()
                    && surface.fail_in_flight_composer_image_label_validation(
                        &task.thread_id,
                        "Beryl lost the background task that was validating image labels.",
                    )
                {
                    surface.set_notice(SurfaceNotice::new(
                        "Image label validation failed",
                        "Beryl lost the background task that was validating this thread's image-label cache.",
                    ));
                }
                true
            }
        }
    }

    fn finish_composer_image_label_validation_worker(
        &mut self,
        task: ComposerImageLabelValidationTask,
        outcome: ComposerImageLabelValidationWorkerOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.composer_image_label_task_matches_selected(&task.workspace_id, &task.thread_id) {
            return;
        }

        match outcome {
            ComposerImageLabelValidationWorkerOutcome::Completed {
                thread_id,
                validation,
            } => {
                if thread_id != task.thread_id {
                    return;
                }
                match validation.outcome {
                    ComposerImageLabelFrontierValidationOutcome::CacheValid { frontier } => {
                        if let Some(surface) = self.conversation_surface_mut()
                            && surface.finish_composer_image_label_validation(&thread_id, frontier)
                        {
                            surface.clear_notice_with_title("Image input unavailable");
                        }
                    }
                    ComposerImageLabelFrontierValidationOutcome::AppendOnly {
                        appended_turn_count,
                        previous_newest_turn_id,
                        frontier,
                    } => {
                        let plan = ComposerImageLabelScanPlan::AppendOnlySuffix {
                            expected_appended_turn_count: appended_turn_count,
                            previous_newest_turn_id,
                            frontier: frontier.clone(),
                        };
                        self.begin_composer_image_label_scan_after_validation(
                            task.workspace_id,
                            thread_id,
                            frontier,
                            plan,
                            window,
                            cx,
                        );
                    }
                    ComposerImageLabelFrontierValidationOutcome::UnknownMutation { frontier } => {
                        self.begin_composer_image_label_scan_after_validation(
                            task.workspace_id,
                            thread_id,
                            frontier,
                            ComposerImageLabelScanPlan::FullCurrentHistory,
                            window,
                            cx,
                        );
                    }
                }
            }
            ComposerImageLabelValidationWorkerOutcome::Failed { thread_id, message } => {
                if thread_id != task.thread_id {
                    return;
                }
                if let Some(surface) = self.conversation_surface_mut() {
                    if surface
                        .fail_in_flight_composer_image_label_validation(&thread_id, message.clone())
                    {
                        surface.set_notice(SurfaceNotice::new(
                            "Image label validation failed",
                            message.clone(),
                        ));
                    }
                }
                self.block_if_backend_process_dead(
                    "Managed backend disconnected during image label validation",
                    "The backend process for the selected workspace exited before Beryl could validate earlier image labels.",
                    &message,
                );
            }
        }
    }

    fn begin_composer_image_label_scan_after_validation(
        &mut self,
        workspace_id: BerylWorkspaceId,
        thread_id: String,
        frontier: ComposerImageLabelHistoryFrontier,
        plan: ComposerImageLabelScanPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.composer_image_label_scan_receiver.is_some() {
            return false;
        }

        let Some(surface) = self.conversation_surface_mut() else {
            return false;
        };
        if !surface.begin_composer_image_label_scan_after_validation(&thread_id, frontier) {
            return false;
        }

        let Some(connector) = self.backend_client_connector() else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.fail_composer_image_label_scan(
                    &thread_id,
                    "Beryl does not have an active managed backend for image-label scanning.",
                );
                surface.set_notice(SurfaceNotice::new(
                    "Image label scan failed",
                    "Beryl does not have an active managed backend for image-label scanning.",
                ));
            }
            return true;
        };

        self.composer_image_label_scan_receiver = Some(ComposerImageLabelScanTask {
            workspace_id,
            thread_id: thread_id.clone(),
            receiver: spawn_composer_image_label_scan_worker_for_plan(
                connector,
                thread_id,
                plan,
                self.bootstrap.probe_timeout(),
            ),
        });
        self.schedule_poll_if_needed(window, cx);
        true
    }

    pub(super) fn poll_composer_image_label_scan_updates(&mut self) -> bool {
        let Some(receiver) = self
            .composer_image_label_scan_receiver
            .as_ref()
            .map(|task| &task.receiver)
        else {
            return false;
        };

        match receiver.try_recv() {
            Ok(ComposerImageLabelScanUpdate::Finished(outcome)) => {
                let Some(task) = self.composer_image_label_scan_receiver.take() else {
                    return false;
                };
                self.finish_composer_image_label_scan_worker(task, outcome);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                let Some(task) = self.composer_image_label_scan_receiver.take() else {
                    return false;
                };
                if self
                    .composer_image_label_task_matches_selected(&task.workspace_id, &task.thread_id)
                    && let Some(surface) = self.conversation_surface_mut()
                    && surface.fail_in_flight_composer_image_label_scan(
                        &task.thread_id,
                        "Beryl lost the background task that was scanning image labels.",
                    )
                {
                    surface.set_notice(SurfaceNotice::new(
                        "Image label scan failed",
                        "Beryl lost the background task that was scanning this thread's earlier image labels.",
                    ));
                }
                true
            }
        }
    }

    fn finish_composer_image_label_scan_worker(
        &mut self,
        task: ComposerImageLabelScanTask,
        outcome: ComposerImageLabelScanOutcome,
    ) {
        if !self.composer_image_label_task_matches_selected(&task.workspace_id, &task.thread_id) {
            return;
        }

        match outcome {
            ComposerImageLabelScanOutcome::Completed {
                thread_id,
                observations,
                frontier,
            } => {
                if thread_id != task.thread_id {
                    return;
                }
                if let Some(surface) = self.conversation_surface_mut() {
                    if surface.finish_composer_image_label_scan(&thread_id, observations, frontier)
                    {
                        surface.clear_notice_with_title("Image input unavailable");
                    }
                }
            }
            ComposerImageLabelScanOutcome::Failed { thread_id, message } => {
                if thread_id != task.thread_id {
                    return;
                }
                if let Some(surface) = self.conversation_surface_mut() {
                    if surface.fail_in_flight_composer_image_label_scan(&thread_id, message.clone())
                    {
                        surface.set_notice(SurfaceNotice::new(
                            "Image label scan failed",
                            message.clone(),
                        ));
                    }
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected during image label scanning",
                    "The backend process for the selected workspace exited before Beryl could scan earlier image labels.",
                    &message,
                );
            }
        }
    }

    pub(super) fn begin_composer_image_label_sync_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.composer_image_label_validation_receiver.is_some()
            || self.composer_image_label_scan_receiver.is_some()
            || self.workspace_receiver.is_some()
            || self.selected_thread_activation_pending()
        {
            return false;
        }

        let Some(workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return false;
        };
        let Some(request) = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_needing_composer_image_label_sync)
        else {
            return false;
        };

        let Some(connector) = self.backend_client_connector() else {
            if let Some(surface) = self.conversation_surface_mut() {
                match &request {
                    ComposerImageLabelHistorySyncRequest::Validate { thread_id, .. } => {
                        surface.fail_composer_image_label_validation(
                            thread_id,
                            "Beryl does not have an active managed backend for image-label validation.",
                        );
                    }
                    ComposerImageLabelHistorySyncRequest::Scan { thread_id } => {
                        surface.fail_composer_image_label_scan(
                            thread_id,
                            "Beryl does not have an active managed backend for image-label scanning.",
                        );
                    }
                }
            }
            return true;
        };

        match request {
            ComposerImageLabelHistorySyncRequest::Validate {
                thread_id,
                frontier,
            } => {
                let Some(surface) = self.conversation_surface_mut() else {
                    return false;
                };
                if !surface.begin_composer_image_label_validation(&thread_id) {
                    return false;
                }
                self.composer_image_label_validation_receiver =
                    Some(ComposerImageLabelValidationTask {
                        workspace_id,
                        thread_id: thread_id.clone(),
                        receiver: spawn_composer_image_label_validation_worker(
                            connector,
                            thread_id,
                            frontier,
                            self.bootstrap.probe_timeout(),
                        ),
                    });
            }
            ComposerImageLabelHistorySyncRequest::Scan { thread_id } => {
                let Some(surface) = self.conversation_surface_mut() else {
                    return false;
                };
                if !surface.begin_composer_image_label_scan(&thread_id) {
                    return false;
                }
                self.composer_image_label_scan_receiver = Some(ComposerImageLabelScanTask {
                    workspace_id,
                    thread_id: thread_id.clone(),
                    receiver: spawn_composer_image_label_scan_worker_for_plan(
                        connector,
                        thread_id,
                        ComposerImageLabelScanPlan::FullCurrentHistory,
                        self.bootstrap.probe_timeout(),
                    ),
                });
            }
        }
        self.schedule_poll_if_needed(window, cx);
        true
    }
}
