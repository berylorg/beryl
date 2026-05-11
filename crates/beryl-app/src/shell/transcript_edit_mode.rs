use super::{
    ConversationSurfaceState, ShellState, ShellView, SurfaceNotice,
    composer_draft::AcceptedComposerDraft,
    composer_draft::{composer_image_copy_text, composer_image_marker},
    transcript_edit_commit_worker::{
        TranscriptEditCommitOutcome, TranscriptEditCommitRequest, TranscriptEditCommitUpdate,
        spawn_transcript_edit_commit_worker,
    },
    transcript_edit_menu_state::{TranscriptEditRequest, TranscriptEditTarget},
    transcript_edit_mode_state::{
        TranscriptEditModeSnapshot, TranscriptEditModeState, TranscriptEditSubmitContext,
        TranscriptEditSubmitRoute, cancel_transcript_edit_mode_slot, transcript_edit_submit_route,
    },
};
use crate::text_input::TextInputSelectionAtom;
use beryl_model::conversation::ConversationThreadId;
use gpui::{Context, KeyDownEvent, Window};
use std::sync::mpsc::TryRecvError;

impl ConversationSurfaceState {
    pub(crate) fn transcript_edit_mode(&self) -> Option<&TranscriptEditModeState> {
        self.transcript_edit_mode.as_ref()
    }

    pub(crate) fn transcript_edit_mode_snapshot(&self) -> Option<TranscriptEditModeSnapshot> {
        self.transcript_edit_mode
            .as_ref()
            .map(TranscriptEditModeState::snapshot)
    }

    pub(crate) fn begin_transcript_edit_mode(
        &mut self,
        request: TranscriptEditRequest,
    ) -> AcceptedComposerDraft {
        let edit_mode = TranscriptEditModeState::from_request(request);
        let draft_seed = edit_mode.draft_seed().clone();
        self.transcript_edit_mode = Some(edit_mode);
        self.notices.clear_all();
        draft_seed
    }

    pub(crate) fn cancel_transcript_edit_mode(&mut self) -> bool {
        cancel_transcript_edit_mode_slot(&mut self.transcript_edit_mode)
    }

    pub(crate) fn transcript_edit_mode_start_allowed(&self, target: &TranscriptEditTarget) -> bool {
        self.selected_thread_id() == Some(target.source_thread_id())
            && matches!(
                self.selected_thread_status,
                Some(beryl_backend::ThreadStatus::Idle)
            )
            && self.selected_thread_context_compaction_id().is_none()
            && self.pending_thread_activation.is_none()
            && self.pending_turn_input_queue.is_none()
            && self.pending_active_turn_steering_queue.is_none()
            && self.transcript_edit_target_loaded(target)
    }

    pub(crate) fn reconcile_transcript_edit_mode(&mut self) -> bool {
        let Some(edit_mode) = self.transcript_edit_mode.as_ref() else {
            return false;
        };
        let target = edit_mode.target().clone();
        let remains_valid = edit_mode.remains_valid(
            self.selected_thread_id(),
            matches!(
                self.selected_thread_status,
                Some(beryl_backend::ThreadStatus::Idle)
            ),
            self.selected_thread_context_compaction_id().is_some(),
            self.pending_thread_activation.is_some(),
            self.transcript_edit_target_loaded(&target),
        );
        if remains_valid {
            return false;
        }

        self.transcript_edit_mode = None;
        true
    }

    pub(crate) fn transcript_edit_discarded_turn_ids(
        &self,
        target: &TranscriptEditTarget,
    ) -> Option<Vec<String>> {
        let turns = self.execution_details.turns();
        let source = turns.get(target.source_turn_index())?;
        if source.turn_id.as_deref() != Some(target.source_turn_id())
            || source.thread_id.as_deref() != Some(target.source_thread_id())
        {
            return None;
        }

        let turn_ids = turns[target.source_turn_index()..]
            .iter()
            .filter(|turn| turn.thread_id.as_deref() == Some(target.source_thread_id()))
            .filter_map(|turn| turn.turn_id.clone())
            .collect::<Vec<_>>();
        turn_ids
            .iter()
            .any(|turn_id| turn_id == target.source_turn_id())
            .then_some(turn_ids)
    }
}

impl ShellView {
    pub(crate) fn begin_transcript_edit_mode_from_request(
        &mut self,
        request: TranscriptEditRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_composer_draft_from_input(cx);
        if !self.composer_draft.is_empty() {
            self.set_transcript_branch_notice(
                "Thread edit unavailable",
                "Composer must be empty to edit a message.",
            );
            cx.notify();
            return;
        }

        let target = request.target().clone();
        let start_allowed = self
            .conversation_surface()
            .is_some_and(|surface| surface.transcript_edit_mode_start_allowed(&target));
        if !start_allowed {
            self.set_transcript_branch_notice(
                "Thread edit unavailable",
                "That transcript turn is no longer available for editing.",
            );
            cx.notify();
            return;
        }

        let Some(seed) = self
            .conversation_surface_mut()
            .map(|surface| surface.begin_transcript_edit_mode(request))
        else {
            return;
        };
        if !self.populate_composer_for_transcript_edit(&seed, window, cx) {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.cancel_transcript_edit_mode();
            }
            self.set_transcript_branch_notice(
                "Thread edit unavailable",
                "Beryl could not rebuild this message in the composer without losing attachments.",
            );
            cx.notify();
            return;
        }
        self.notify_transcript_panel(cx);
        cx.notify();
    }

    pub(crate) fn handle_transcript_edit_mode_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape"
            || self.transcript_edit_cancel_deferred_by_popup_or_menu()
        {
            return false;
        }

        let cancelled = self
            .conversation_surface_mut()
            .is_some_and(ConversationSurfaceState::cancel_transcript_edit_mode);
        if cancelled {
            self.notify_transcript_panel(cx);
            cx.notify();
        }
        cancelled
    }

    pub(crate) fn queue_transcript_edit_commit_from_composer(
        &mut self,
        draft: &AcceptedComposerDraft,
        cx: &mut Context<Self>,
    ) -> bool {
        self.reconcile_transcript_edit_mode(cx);
        let route = transcript_edit_submit_route(
            self.conversation_surface()
                .and_then(ConversationSurfaceState::transcript_edit_mode),
            TranscriptEditSubmitContext {
                status_operation_active: self.status_operation_receiver.is_some(),
                active_turn_active: self.turn_receiver.is_some(),
                selected_thread_compaction_active: self.conversation_surface().is_some_and(
                    |surface| surface.selected_thread_context_compaction_id().is_some(),
                ),
            },
        );
        if route != Some(TranscriptEditSubmitRoute::EditCommit) {
            return false;
        }

        let request = match self.transcript_edit_commit_request(draft.clone()) {
            Ok(request) => request,
            Err(message) => {
                self.set_transcript_branch_notice("Thread edit unavailable", message);
                cx.notify();
                return true;
            }
        };

        let Some(connector) = self.backend_client_connector() else {
            self.set_transcript_branch_notice(
                "Thread edit unavailable",
                "Beryl does not have an active managed backend for this workspace.",
            );
            cx.notify();
            return true;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            self.set_transcript_branch_notice(
                "Thread edit unavailable",
                "Beryl could not open the configured workspace persistence root.",
            );
            cx.notify();
            return true;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Editing thread",
                "Beryl is rolling back the selected conversation tail.",
            ));
        }
        self.transcript_edit_commit_receiver = Some(spawn_transcript_edit_commit_worker(
            persistence,
            connector,
            request,
            self.bootstrap.probe_timeout(),
        ));
        cx.notify();
        true
    }

    fn transcript_edit_commit_request(
        &self,
        draft: AcceptedComposerDraft,
    ) -> Result<TranscriptEditCommitRequest, String> {
        if self.workspace_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.composer_image_asset_receiver.is_some()
            || self.composer_image_delivery_receiver.is_some()
        {
            return Err(
                "Beryl cannot edit this message while another thread operation is running."
                    .to_string(),
            );
        }
        if self.status_operation_receiver.is_some() {
            return Err(
                "Beryl cannot edit this message while a status operation is running.".to_string(),
            );
        }
        if self.turn_receiver.is_some() || !self.turn_steering_receivers.is_empty() {
            return Err("Beryl cannot edit this message while a turn is active.".to_string());
        }

        let ShellState::Ready(ready) = &self.state else {
            return Err("Beryl cannot edit this message until the workspace is ready.".to_string());
        };
        let surface = &ready.surface;
        let Some(edit_mode) = surface.transcript_edit_mode() else {
            return Err("Beryl is not currently editing a transcript message.".to_string());
        };
        let target = edit_mode.target().clone();
        if !surface.transcript_edit_mode_start_allowed(&target) {
            return Err("That transcript turn is no longer available for editing.".to_string());
        }
        let Some(discarded_turn_ids) = surface.transcript_edit_discarded_turn_ids(&target) else {
            return Err(
                "Beryl could not verify the loaded conversation tail for this edit.".to_string(),
            );
        };

        let thread_id = ConversationThreadId::new(target.source_thread_id().to_string());
        let automatic_title_generation_allowed = ready
            .loaded_workspace
            .workspace_state
            .thread_automatic_title_generation_eligible(&thread_id);
        let turn_options = ready
            .surface
            .pending_turn_start_options(Some(target.source_thread_id()));

        Ok(TranscriptEditCommitRequest::new(
            ready.loaded_workspace.workspace.id().clone(),
            ready.execution_target.clone(),
            target,
            discarded_turn_ids,
            draft,
            automatic_title_generation_allowed,
            turn_options,
        ))
    }

    pub(super) fn poll_transcript_edit_commit_updates(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(receiver) = self.transcript_edit_commit_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(TranscriptEditCommitUpdate::Finished(outcome)) => {
                self.transcript_edit_commit_receiver = None;
                self.finish_transcript_edit_commit_worker(outcome, cx);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.transcript_edit_commit_receiver = None;
                self.finish_transcript_edit_commit_worker_stopped();
                true
            }
        }
    }

    fn finish_transcript_edit_commit_worker(
        &mut self,
        outcome: TranscriptEditCommitOutcome,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            TranscriptEditCommitOutcome::RolledBack {
                request,
                thread,
                image_resolver,
                replacement_fragment,
            } => self.finish_successful_transcript_edit_rollback(
                request,
                thread,
                image_resolver,
                replacement_fragment,
                cx,
            ),
            TranscriptEditCommitOutcome::PreRollbackFailed { request, message } => {
                if self.transcript_edit_commit_request_is_current(&request) {
                    self.set_transcript_branch_notice("Thread edit unavailable", message);
                }
            }
            TranscriptEditCommitOutcome::RollbackFailed { request, message } => {
                if self.transcript_edit_commit_request_is_current(&request) {
                    self.set_transcript_branch_notice("Thread edit failed", message.clone());
                }
                self.block_if_backend_process_dead(
                    "Managed backend disconnected during thread edit",
                    "The backend process exited before Beryl could roll back the selected conversation tail.",
                    &message,
                );
            }
        }
    }

    fn populate_composer_for_transcript_edit(
        &mut self,
        seed: &AcceptedComposerDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.composer_image_popup = None;
        self.composer_draft.clear();
        let display_text = seed.display_text().to_string();
        let caret = display_text.len();
        if !seed.contains_images() {
            self.conversation_input.update(cx, |input, cx| {
                input.set_text(&display_text, cx);
                input.set_selection(caret..caret, false, cx);
                input.focus(window, cx);
            });
            self.sync_composer_draft_from_input(cx);
            return true;
        }

        let atoms = self
            .composer_draft
            .replace_with_accepted(seed)
            .into_iter()
            .map(|atom| {
                TextInputSelectionAtom::new(
                    atom.atom_id().to_string(),
                    atom.range(),
                    composer_image_marker(atom.label()),
                    composer_image_copy_text(atom.label()),
                )
            })
            .collect::<Vec<_>>();
        let inserted = self.conversation_input.update(cx, |input, cx| {
            input.set_text("", cx);
            match input.replace_selected_text_with_atoms(&display_text, atoms, cx) {
                Ok(inserted) => {
                    input.set_selection(caret..caret, false, cx);
                    input.focus(window, cx);
                    Ok(inserted)
                }
                Err(error) => Err(error),
            }
        });
        match inserted {
            Ok(true) => {
                self.sync_composer_draft_from_input(cx);
                true
            }
            Ok(false) => {
                self.composer_draft.clear();
                false
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "failed to populate composer image atoms for thread edit"
                );
                self.composer_draft.clear();
                self.conversation_input
                    .update(cx, |input, cx| input.set_text("", cx));
                false
            }
        }
    }

    fn transcript_edit_cancel_deferred_by_popup_or_menu(&self) -> bool {
        self.composer_image_popup.is_some()
            || self
                .loaded_workspace()
                .is_some_and(|loaded| loaded.workspace_picker.is_open())
            || self.conversation_surface().is_some_and(|surface| {
                surface.graph_thread_link_menu().is_open()
                    || surface.transcript_branch_menu().is_open()
                    || surface.checklist_thread_start_menu().is_open()
                    || surface.thread_selector().is_open()
                    || surface.status_line_operations().is_open()
            })
    }

    pub(crate) fn reconcile_transcript_edit_mode(&mut self, cx: &mut Context<Self>) {
        let changed = self
            .conversation_surface_mut()
            .is_some_and(ConversationSurfaceState::reconcile_transcript_edit_mode);
        if changed {
            self.notify_transcript_panel(cx);
            cx.notify();
        }
    }
}
