use super::{
    ShellState, ShellView, SurfaceNotice, TranscriptEditReplacementTurnState,
    composer_draft::{AcceptedComposerDraft, composer_image_copy_text, composer_image_marker},
    execution_detail::{TranscriptImagePathResolver, UserInputFragment},
    status_line::ThreadTurnDefaults,
    transcript_edit_commit_worker::TranscriptEditCommitRequest,
    turn_worker::{shell_dynamic_tool_request_channel, spawn_turn_worker},
};
use crate::text_input::TextInputSelectionAtom;
use gpui::{Context, Window};

impl ShellView {
    pub(super) fn finish_successful_transcript_edit_rollback(
        &mut self,
        request: TranscriptEditCommitRequest,
        thread: beryl_backend::ThreadInfo,
        image_resolver: TranscriptImagePathResolver,
        replacement_fragment: UserInputFragment,
        cx: &mut Context<Self>,
    ) {
        if !self.transcript_edit_commit_request_is_current(&request) {
            return;
        }
        let source_thread_id = request.source_thread_id().to_string();
        let turn_context_defaults = self
            .conversation_surface()
            .map(|surface| surface.effective_turn_context_defaults(Some(source_thread_id.as_str())))
            .unwrap_or_default();

        if let Some(surface) = self.conversation_surface_mut() {
            surface.load_thread_history_window(
                &thread,
                super::transcript_history::TranscriptHistoryWindow::default(),
                &image_resolver,
            );
            surface.invalidate_stream_turns(
                request.source_thread_id(),
                request.discarded_turn_ids().iter().cloned(),
            );
        }

        if !self.begin_transcript_edit_replacement_turn(
            request,
            replacement_fragment,
            turn_context_defaults,
            cx,
        ) && let Some(surface) = self.conversation_surface_mut()
        {
            surface.set_notice(SurfaceNotice::new(
                "Thread edit partially applied",
                "Beryl rolled back the selected conversation tail, but could not start the replacement turn.",
            ));
        }
    }

    pub(super) fn transcript_edit_commit_request_is_current(
        &self,
        request: &TranscriptEditCommitRequest,
    ) -> bool {
        match &self.state {
            ShellState::Ready(ready) => {
                ready.loaded_workspace.workspace.id() == request.workspace_id()
                    && &ready.execution_target == request.execution_target()
                    && ready.surface.selected_thread_id() == Some(request.source_thread_id())
            }
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::BackendUnavailable(_)
            | ShellState::Blocked(_) => false,
        }
    }

    fn begin_transcript_edit_replacement_turn(
        &mut self,
        request: TranscriptEditCommitRequest,
        fragment: UserInputFragment,
        turn_context_defaults: ThreadTurnDefaults,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.turn_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || !self.transcript_edit_commit_request_is_current(&request)
        {
            return false;
        }

        let Some(connector) = self.backend_client_connector() else {
            return false;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return false;
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_turn_for_thread(request.source_thread_id(), fragment.clone());
        }
        self.record_accepted_composer_history(request.accepted_draft());
        let composer_cleared =
            self.clear_composer_draft_if_accepted_matches(request.accepted_draft(), cx);
        self.notify_transcript_panel(cx);

        self.transcript_edit_replacement_turn = Some(TranscriptEditReplacementTurnState {
            workspace_id: request.workspace_id().clone(),
            execution_target: request.execution_target().clone(),
            thread_id: request.source_thread_id().to_string(),
            accepted_draft: request.accepted_draft().clone(),
            composer_cleared,
            turn_started: false,
        });
        let turn_options = self.turn_options_with_current_developer_instructions_defaults(
            Some(request.source_thread_id()),
            request.turn_options().clone(),
            turn_context_defaults,
        );
        let (shell_tool_sender, shell_tool_receiver) = shell_dynamic_tool_request_channel();
        self.shell_tool_receiver = Some(shell_tool_receiver);
        self.turn_receiver = Some(spawn_turn_worker(
            persistence,
            connector,
            request.workspace_id().clone(),
            request.execution_target().clone(),
            Some(request.source_thread_id().to_string()),
            request.automatic_title_generation_allowed(),
            vec![fragment],
            turn_options,
            Some(shell_tool_sender),
            self.bootstrap.probe_timeout(),
        ));
        true
    }

    pub(super) fn note_transcript_edit_replacement_turn_started(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(replacement) = self.transcript_edit_replacement_turn.as_ref() else {
            return;
        };
        if replacement.thread_id != thread_id {
            return;
        }
        let composer_cleared = replacement.composer_cleared;
        let accepted_draft = replacement.accepted_draft.clone();
        if let Some(replacement) = self.transcript_edit_replacement_turn.as_mut() {
            replacement.turn_started = true;
        }
        if composer_cleared {
            return;
        }
        let cleared = self.clear_composer_draft_if_accepted_matches(&accepted_draft, cx);
        if let Some(replacement) = self.transcript_edit_replacement_turn.as_mut() {
            replacement.composer_cleared = cleared;
        }
    }

    pub(super) fn finish_transcript_edit_replacement_turn(
        &mut self,
        failed_before_turn_start: bool,
        failure_message: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(replacement) = self.transcript_edit_replacement_turn.take() else {
            return;
        };
        if !failed_before_turn_start || replacement.turn_started {
            return;
        }

        if replacement.composer_cleared {
            self.restore_composer_accepted_draft_if_empty(&replacement.accepted_draft, window, cx);
        }

        let replacement_surface_current = match &self.state {
            ShellState::Ready(ready) => {
                ready.loaded_workspace.workspace.id() == &replacement.workspace_id
                    && ready.execution_target == replacement.execution_target
                    && ready.surface.selected_thread_id() == Some(replacement.thread_id.as_str())
            }
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::BackendUnavailable(_)
            | ShellState::Blocked(_) => false,
        };

        if replacement_surface_current && let Some(surface) = self.conversation_surface_mut() {
            let detail = match failure_message {
                Some(message) if !message.is_empty() => format!(
                    "Beryl rolled back the selected conversation tail, but could not start the replacement turn: {message}"
                ),
                _ => {
                    "Beryl rolled back the selected conversation tail, but could not start the replacement turn."
                        .to_string()
                }
            };
            surface.set_notice(SurfaceNotice::new("Thread edit partially applied", detail));
        }
    }

    fn clear_composer_draft_if_accepted_matches(
        &mut self,
        accepted_draft: &AcceptedComposerDraft,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_composer_draft_from_input(cx);
        if self.composer_draft.accepted().as_ref() != Some(accepted_draft) {
            return false;
        }
        self.clear_composer_draft(cx);
        true
    }

    fn restore_composer_accepted_draft_if_empty(
        &mut self,
        accepted_draft: &AcceptedComposerDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_composer_draft_from_input(cx);
        if !self.composer_draft.is_empty() {
            return false;
        }

        let atoms = self
            .composer_draft
            .replace_with_accepted(accepted_draft)
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
        let display_text = self.composer_draft.display_text().to_string();
        let caret = display_text.len();
        let restored = match self.conversation_input.update(cx, |input, cx| {
            input.set_text("", cx);
            match input.replace_selected_text_with_atoms(&display_text, atoms, cx) {
                Ok(restored) => {
                    input.set_selection(caret..caret, false, cx);
                    input.focus(window, cx);
                    Ok(restored)
                }
                Err(error) => Err(error),
            }
        }) {
            Ok(restored) => restored,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "failed to restore composer image atoms after thread edit"
                );
                self.composer_draft.clear();
                self.conversation_input.update(cx, |input, cx| {
                    input.set_text(accepted_draft.display_text(), cx)
                });
                true
            }
        };
        self.sync_composer_draft_from_input(cx);
        restored
    }

    pub(super) fn finish_transcript_edit_commit_worker_stopped(&mut self) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Thread edit failed",
                "Beryl lost the background task that was editing the thread.",
            ));
        }
        self.block_if_backend_process_dead(
            "Thread edit stopped unexpectedly",
            "Beryl lost the background task that was editing the thread.",
            "Beryl preserved the current workspace surface, but it cannot continue until the managed backend for this workspace is relaunched.",
        );
    }
}
