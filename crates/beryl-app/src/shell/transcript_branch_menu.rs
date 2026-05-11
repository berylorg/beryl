use beryl_backend::ThreadStatus;
use gpui::{Bounds, Context, KeyDownEvent, MouseDownEvent, Pixels, Point, Window};

use super::{
    ConversationSurfaceState, ShellState, ShellView, SurfaceNotice,
    transcript_branch_menu_state::{
        TranscriptBranchAction, TranscriptBranchMenuOpenGate, TranscriptBranchMenuState,
        TranscriptBranchRequest, TranscriptBranchTarget, TranscriptImageMenuTarget,
        transcript_branch_menu_can_open,
    },
    transcript_branch_worker::spawn_transcript_branch_worker,
    transcript_edit_menu_state::{
        TranscriptEditMenuEntry, TranscriptEditMenuGate, TranscriptEditTarget,
        transcript_edit_menu_entry,
    },
    transcript_image_menu_actions::{copy_transcript_image_to_clipboard, save_transcript_image_as},
};

impl ConversationSurfaceState {
    pub(crate) fn transcript_branch_menu(&self) -> &TranscriptBranchMenuState {
        &self.transcript_branch_menu
    }

    pub(crate) fn transcript_branch_menu_mut(&mut self) -> &mut TranscriptBranchMenuState {
        &mut self.transcript_branch_menu
    }

    pub(crate) fn transcript_branch_menu_open_allowed(
        &self,
        target: &TranscriptBranchTarget,
        transcript_selection_active: bool,
        branch_capability_available: bool,
    ) -> bool {
        transcript_branch_menu_can_open(TranscriptBranchMenuOpenGate {
            transcript_selection_active,
            source_thread_idle: matches!(self.selected_thread_status, Some(ThreadStatus::Idle)),
            selected_thread_matches_target: self.selected_thread_id()
                == Some(target.source_thread_id()),
            selected_thread_compaction_active: self
                .selected_thread_context_compaction_id()
                .is_some(),
            pending_thread_activation: self.pending_thread_activation.is_some(),
            branch_capability_available,
        })
    }

    pub(crate) fn transcript_branch_target_loaded(&self, target: &TranscriptBranchTarget) -> bool {
        (0..self.transcript_presentation.len()).any(|index| {
            self.transcript_presentation
                .turn_at(index)
                .and_then(|row| TranscriptBranchTarget::from_presented_row(&row))
                .is_some_and(|loaded_target| {
                    loaded_target.source_thread_id() == target.source_thread_id()
                        && loaded_target.source_turn_id() == target.source_turn_id()
                })
        })
    }

    pub(crate) fn transcript_edit_menu_entry_for_row(
        &self,
        row_index: usize,
        gate: TranscriptEditMenuGate,
    ) -> Option<TranscriptEditMenuEntry> {
        let row = self.transcript_presentation.turn_at(row_index)?;
        let target = TranscriptEditTarget::resolve_from_presented_row(
            &row,
            self.execution_details.turns(),
            self.transcript_history_window.current_tail_known(),
        )?;
        transcript_edit_menu_entry(target, gate)
    }

    pub(crate) fn transcript_edit_target_loaded(&self, target: &TranscriptEditTarget) -> bool {
        (0..self.transcript_presentation.len()).any(|index| {
            self.transcript_presentation
                .turn_at(index)
                .and_then(|row| {
                    TranscriptEditTarget::from_presented_row(
                        &row,
                        self.execution_details.turns(),
                        self.transcript_history_window.current_tail_known(),
                    )
                })
                .is_some_and(|loaded_target| {
                    loaded_target.source_thread_id() == target.source_thread_id()
                        && loaded_target.source_turn_id() == target.source_turn_id()
                })
        })
    }

    pub(crate) fn transcript_image_menu_target_loaded(
        &self,
        target: &TranscriptImageMenuTarget,
    ) -> bool {
        self.transcript_presentation()
            .row_index_for_identity(target.row_identity())
            .is_some()
    }

    pub(crate) fn reconcile_transcript_branch_menu_target(&mut self) -> bool {
        let image_target = self
            .transcript_branch_menu
            .active()
            .and_then(|open| open.image_target().cloned());
        let mut changed = false;
        if let Some(target) = image_target
            && !self.transcript_image_menu_target_loaded(&target)
        {
            changed |= self.transcript_branch_menu.clear_image_target();
        }

        let Some(open) = self.transcript_branch_menu.active() else {
            return changed;
        };
        let branch_target = open.branch_target().cloned();
        let edit_target = open
            .edit_entry()
            .map(|entry| entry.target_identity().clone());
        let has_image_target = open.image_target().is_some();

        let branch_loaded = branch_target.as_ref().is_some_and(|target| {
            self.selected_thread_id() == Some(target.source_thread_id())
                && self.transcript_branch_target_loaded(target)
        });
        let edit_loaded = edit_target.as_ref().is_some_and(|identity| {
            self.selected_thread_id() == Some(identity.source_thread_id())
                && (0..self.transcript_presentation.len()).any(|index| {
                    self.transcript_presentation
                        .turn_at(index)
                        .and_then(|row| {
                            TranscriptEditTarget::resolve_from_presented_row(
                                &row,
                                self.execution_details.turns(),
                                self.transcript_history_window.current_tail_known(),
                            )
                        })
                        .is_some_and(|loaded| {
                            let loaded_identity = loaded.into_identity();
                            loaded_identity.source_thread_id() == identity.source_thread_id()
                                && loaded_identity.source_turn_id() == identity.source_turn_id()
                        })
                })
        });
        if branch_loaded || edit_loaded || has_image_target {
            return changed;
        }

        changed | self.transcript_branch_menu.close()
    }
}

impl ShellView {
    pub(crate) fn open_transcript_branch_menu_for_row(
        &mut self,
        row_index: usize,
        transcript_selection_active: bool,
        image_target: Option<TranscriptImageMenuTarget>,
        position: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let branch_capability_available = self.transcript_branch_capability_available();
        let edit_rollback_capability_available =
            self.transcript_edit_rollback_capability_available();
        self.sync_composer_draft_from_input(cx);
        let composer_empty = self.composer_draft.is_empty();
        let conflicting_selected_thread_work = self.transcript_edit_menu_conflicting_work_active();

        let branch_target = self.conversation_surface().and_then(|surface| {
            surface
                .transcript_presentation()
                .turn_at(row_index)
                .and_then(|row| TranscriptBranchTarget::from_presented_row(&row))
                .filter(|target| {
                    surface.transcript_branch_menu_open_allowed(
                        target,
                        transcript_selection_active,
                        branch_capability_available,
                    )
                })
        });

        let edit_entry = self.conversation_surface().and_then(|surface| {
            let selected_thread_matches_target = surface
                .transcript_presentation()
                .turn_at(row_index)
                .and_then(|row| row.turn.thread_id.clone())
                .is_some_and(|thread_id| surface.selected_thread_id() == Some(thread_id.as_str()));
            surface.transcript_edit_menu_entry_for_row(
                row_index,
                TranscriptEditMenuGate {
                    transcript_selection_active,
                    source_thread_idle: matches!(
                        surface.selected_thread_status,
                        Some(ThreadStatus::Idle)
                    ),
                    selected_thread_matches_target,
                    selected_thread_compaction_active: surface
                        .selected_thread_context_compaction_id()
                        .is_some(),
                    pending_thread_activation: surface.pending_thread_activation.is_some(),
                    rollback_capability_available: edit_rollback_capability_available,
                    composer_empty,
                    pending_turn_input: surface.pending_turn_input_queue.is_some(),
                    pending_active_turn_steering: surface
                        .pending_active_turn_steering_queue
                        .is_some(),
                    conflicting_selected_thread_work,
                    image_label_readiness: surface.composer_image_paste_readiness(),
                },
            )
        });

        if branch_target.is_none() && edit_entry.is_none() && image_target.is_none() {
            return false;
        }

        let Some(surface) = self.conversation_surface_mut() else {
            return false;
        };

        surface.thread_selector_mut().close();
        surface.graph_thread_link_menu_mut().close();
        surface.checklist_thread_start_menu_mut().close();
        surface.status_line_operations_mut().close();
        surface.transcript_branch_menu_mut().open_menu(
            branch_target,
            edit_entry,
            image_target,
            position,
        );
        cx.stop_propagation();
        cx.notify();
        true
    }

    pub(crate) fn handle_transcript_branch_menu_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_dismiss = self.conversation_surface().is_some_and(|surface| {
            surface
                .transcript_branch_menu()
                .should_dismiss_for_mouse_down(event.position)
        });
        if should_dismiss && let Some(surface) = self.conversation_surface_mut() {
            surface.transcript_branch_menu_mut().close();
            cx.notify();
        }
    }

    pub(crate) fn handle_transcript_branch_menu_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key.as_str() != "escape" {
            return false;
        }
        if let Some(surface) = self.conversation_surface_mut()
            && surface.transcript_branch_menu().is_open()
        {
            surface.transcript_branch_menu_mut().close();
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn record_transcript_branch_menu_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.transcript_branch_menu_mut().set_bounds(bounds);
        }
    }

    pub(crate) fn clear_stale_transcript_image_menu_target(&mut self, cx: &mut Context<Self>) {
        if let Some(surface) = self.conversation_surface_mut()
            && surface.transcript_branch_menu_mut().clear_image_target()
        {
            cx.notify();
        }
    }

    pub(crate) fn branch_transcript_turn_and_switch_to(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.accept_transcript_branch_menu_action(TranscriptBranchAction::SwitchTo, window, cx);
    }

    pub(crate) fn branch_transcript_turn_in_background(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.accept_transcript_branch_menu_action(TranscriptBranchAction::Background, window, cx);
    }

    pub(crate) fn edit_transcript_turn_from_menu(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self
            .conversation_surface_mut()
            .and_then(|surface| surface.transcript_branch_menu_mut().accept_edit())
        else {
            return;
        };
        self.begin_transcript_edit_mode_from_request(request, window, cx);
    }

    pub(crate) fn copy_transcript_image_from_menu(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.transcript_image_menu_action_target_valid(cx) {
            self.clear_stale_transcript_image_menu_target(cx);
            return;
        }

        let Some(target) = self
            .conversation_surface_mut()
            .and_then(|surface| surface.transcript_branch_menu_mut().accept_copy_image())
        else {
            return;
        };

        copy_transcript_image_to_clipboard(&target, cx);
        cx.notify();
    }

    pub(crate) fn save_transcript_image_as_from_menu(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.transcript_image_menu_action_target_valid(cx) {
            self.clear_stale_transcript_image_menu_target(cx);
            return;
        }

        let Some(target) = self
            .conversation_surface_mut()
            .and_then(|surface| surface.transcript_branch_menu_mut().accept_save_image())
        else {
            return;
        };

        save_transcript_image_as(
            target,
            |view: &mut Self, title, detail| {
                view.set_transcript_branch_notice(title, detail);
            },
            cx,
        );
    }

    fn transcript_image_menu_action_target_valid(&self, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.conversation_surface().and_then(|surface| {
            surface
                .transcript_branch_menu()
                .active()
                .and_then(|open| open.image_target())
        }) else {
            return false;
        };

        self.transcript_panel
            .read(cx)
            .transcript_image_menu_target_validated(target)
    }

    fn accept_transcript_branch_menu_action(
        &mut self,
        action: TranscriptBranchAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self
            .conversation_surface_mut()
            .and_then(|surface| surface.transcript_branch_menu_mut().accept(action))
        else {
            return;
        };
        self.dispatch_transcript_branch_request(request, window, cx);
        cx.notify();
    }

    fn dispatch_transcript_branch_request(
        &mut self,
        request: TranscriptBranchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.transcript_branch_dispatch_blocked() {
            self.set_transcript_branch_notice(
                "Thread branch unavailable",
                "Retry after the current thread or workspace operation finishes.",
            );
            return;
        }

        let branch_capability_available = self.transcript_branch_capability_available();
        let target_loaded_and_allowed = self.conversation_surface().is_some_and(|surface| {
            surface.transcript_branch_menu_open_allowed(
                request.target(),
                false,
                branch_capability_available,
            ) && surface.transcript_branch_target_loaded(request.target())
        });
        if !target_loaded_and_allowed {
            self.set_transcript_branch_notice(
                "Thread branch unavailable",
                "That transcript turn is no longer available for branching.",
            );
            return;
        }

        let Some(connector) = self.backend_client_connector() else {
            self.set_transcript_branch_notice(
                "Thread branch unavailable",
                "Beryl does not have an active managed backend for this workspace.",
            );
            return;
        };

        self.transcript_branch_receiver = Some(spawn_transcript_branch_worker(
            connector,
            request,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
    }

    fn transcript_branch_capability_available(&self) -> bool {
        match &self.state {
            ShellState::Ready(ready) => {
                ready.report.thread_branch_capabilities().thread_branching()
            }
            _ => false,
        }
    }

    fn transcript_edit_rollback_capability_available(&self) -> bool {
        match &self.state {
            ShellState::Ready(ready) => ready.report.thread_branch_capabilities().thread_rollback(),
            _ => false,
        }
    }

    fn transcript_edit_menu_conflicting_work_active(&self) -> bool {
        self.workspace_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.composer_image_asset_receiver.is_some()
            || self.composer_image_delivery_receiver.is_some()
    }

    fn transcript_branch_dispatch_blocked(&self) -> bool {
        self.workspace_receiver.is_some()
            || self.graph_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.composer_image_delivery_receiver.is_some()
    }

    pub(crate) fn set_transcript_branch_notice(
        &mut self,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(title, detail));
        }
    }
}
