mod work;

use super::*;
use crate::composer_host::{ComposerHostRecallCompletion, ComposerHostRecallTicket};
use beryl_home_store::CommandCancellation;
use syndic_storage::{
    ComposerAcceptedInputReferenceV1, DraftComposerBrowseCheckpointV1, DraftSubmissionIntent,
};

pub(super) struct BrowseBookmark {
    checkpoint: DraftComposerBrowseCheckpointV1,
    cursor: usize,
}

pub(super) struct MountedAcceptedInputRecall {
    cancellation: CommandCancellation,
    target: Option<usize>,
    state: State,
}

enum State {
    Requested {
        source: Option<ComposerAcceptedInputReferenceV1>,
        checkpoint: Option<DraftComposerBrowseCheckpointV1>,
        positions: gpui_text_input::MutationPositions,
        capture_checkpoint: bool,
    },
    Running,
    Blocked {
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostRecallTicket,
    },
    Realizing {
        completed: Completion,
        rebind_requested: bool,
        frontier_installed: bool,
    },
}

struct Completion {
    service: Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    initiating_selection: MainWindowComposerSelectionIdentity,
    ticket: ComposerHostRecallTicket,
    value: ComposerHostRecallCompletion,
    acknowledged: bool,
}

impl Completion {
    fn acknowledge(&mut self) -> Result<(), String> {
        self.service
            .slot
            .lock()
            .map_err(|_| "Composer history service is unavailable")?
            .acknowledge_accepted_input_recall(self.selection, self.ticket, &self.value)?;
        self.acknowledged = true;
        Ok(())
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if !self.acknowledged {
            let _ = self.acknowledge();
        }
    }
}

impl MountedAcceptedInputRecall {
    pub(super) fn cancel(&self) {
        self.cancellation.cancel();
    }
}

impl MainWindowConversationComposer {
    pub(super) fn retire_failed_accepted_input_history(&mut self) {
        self.accepted_input_browse = None;
        if let Some(mut recall) = self.accepted_input_recall.take() {
            recall.cancel();
            if let State::Realizing { completed, .. } = &mut recall.state {
                completed.acknowledged = true;
            }
        }
        self.accepted_input_history_feedback = None;
    }
    #[cfg(feature = "test-faults")]
    pub fn test_accepted_input_history_pending(&self) -> bool {
        self.accepted_input_recall.is_some()
    }
    pub(in crate::main_window) fn accepted_input_history_pending(&self) -> bool {
        self.accepted_input_recall.is_some()
    }

    pub(in crate::main_window) fn accepted_input_history_feedback(&self) -> Option<&str> {
        self.accepted_input_history_feedback.as_deref()
    }

    pub(in crate::main_window) fn clear_accepted_input_history_feedback(&mut self) {
        self.accepted_input_history_feedback = None;
    }

    pub(super) fn browse_accepted_input_history(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_live()
            || self.is_pending_target()
            || self.mutation_gated()
            || self.last_error.is_some()
            || self.pending_dispatch.is_some()
            || self.mutation_evidence.is_some()
            || self.quote_insertion.is_some()
            || self.replacement_intent != Some(DraftSubmissionIntent::Ordinary)
            || !self.input.read(cx).is_semantically_quiescent()
        {
            return;
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        let owner = service.accepted_input_history();
        let home = self.selection.binding().home_id();
        let thread = self.selection.claim().thread_id();
        let count = owner.entry_count(home, thread);
        let target = if backward {
            match &self.accepted_input_browse {
                Some(bookmark) => bookmark.cursor.checked_sub(1),
                None => count.checked_sub(1),
            }
            .map(Some)
        } else {
            self.accepted_input_browse.as_ref().map(|bookmark| {
                if bookmark.cursor + 1 < count {
                    Some(bookmark.cursor + 1)
                } else {
                    None
                }
            })
        };
        let Some(target) = target else {
            return;
        };
        let source = match target {
            Some(index) => match owner.entry(home, thread, index) {
                Some(source) => Some(source),
                None => return,
            },
            None => None,
        };
        let checkpoint = if target.is_none() {
            self.accepted_input_browse
                .as_ref()
                .map(|bookmark| bookmark.checkpoint.clone())
        } else {
            None
        };
        let Some(surface) = self.input.read(cx).surface().filter(|surface| {
            surface.binding() == self.selection.binding().range_binding()
                && surface.composition().is_none()
        }) else {
            return;
        };
        let positions = gpui_text_input::MutationPositions::new(
            surface.caret(),
            surface.selection().anchor,
            surface.selection().head,
        );
        owner.select(home, thread);
        self.accepted_input_recall = Some(MountedAcceptedInputRecall {
            cancellation: CommandCancellation::new(),
            target,
            state: State::Requested {
                source,
                checkpoint,
                positions,
                capture_checkpoint: self.accepted_input_browse.is_none(),
            },
        });
        self.accepted_input_history_feedback = None;
        self.sync_mutation_gate(cx);
        self.schedule_pump(window, cx);
        cx.notify();
    }

    pub(super) fn cancel_accepted_input_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(recall) = &self.accepted_input_recall else {
            return;
        };
        recall.cancel();
        if let State::Blocked { selection, ticket } = &recall.state {
            let resume = (*selection, *ticket);
            self.last_error = None;
            if let Err(error) = self.launch_accepted_input_recall(None, Some(resume), window, cx) {
                self.last_error = Some(error);
            } else {
                self.accepted_input_recall.as_mut().unwrap().state = State::Running;
            }
        }
        self.schedule_pump(window, cx);
        cx.notify();
    }

    pub(super) fn end_accepted_input_browse(&mut self) {
        self.accepted_input_browse = None;
        if let Some(recall) = &self.accepted_input_recall {
            recall.cancel();
        }
    }

    pub(super) fn pump_accepted_input_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut recall) = self.accepted_input_recall.take() else {
            return false;
        };
        match &mut recall.state {
            State::Requested {
                source,
                checkpoint,
                positions,
                capture_checkpoint,
            } => {
                if recall.cancellation.is_cancelled() {
                    self.sync_mutation_gate(cx);
                    cx.notify();
                    return false;
                }
                let request = (
                    source.take(),
                    checkpoint.take(),
                    *positions,
                    *capture_checkpoint,
                );
                recall.state = State::Running;
                self.accepted_input_recall = Some(recall);
                if let Err(error) =
                    self.launch_accepted_input_recall(Some(request), None, window, cx)
                {
                    self.accepted_input_recall = None;
                    self.accepted_input_history_feedback = Some(error);
                    self.sync_mutation_gate(cx);
                }
                true
            }
            State::Running | State::Blocked { .. } => {
                self.accepted_input_recall = Some(recall);
                true
            }
            State::Realizing {
                completed,
                rebind_requested,
                frontier_installed,
            } => {
                let result = self.realize_accepted_input_history(
                    completed,
                    rebind_requested,
                    frontier_installed,
                    window,
                    cx,
                );
                match result {
                    Ok(blocked) if !completed.acknowledged => {
                        self.accepted_input_recall = Some(recall);
                        blocked
                    }
                    Ok(_) => {
                        if let Some(refusal) = completed.value.refusal() {
                            use crate::composer_host::ComposerHostRecallRefusal;
                            self.accepted_input_history_feedback = Some(match refusal {
                                ComposerHostRecallRefusal::CapacityUnavailable => "Composer history capacity is unavailable. Your draft is unchanged.",
                                ComposerHostRecallRefusal::SourceUnavailable => "The accepted input or its images are unavailable. Your draft is unchanged.",
                                ComposerHostRecallRefusal::SourceChanged | ComposerHostRecallRefusal::ProofRejected => "The accepted input could not be authenticated. Your draft is unchanged.",
                                ComposerHostRecallRefusal::CommandRefused => "The accepted input could not be recalled. Your draft is unchanged.",
                            }.into());
                        }
                        if completed.value.replaced() && self.is_live() {
                            match recall.target {
                                Some(cursor) => {
                                    if let Some(checkpoint) = completed.value.checkpoint().cloned()
                                    {
                                        self.accepted_input_browse =
                                            Some(BrowseBookmark { checkpoint, cursor });
                                    } else if let Some(bookmark) = &mut self.accepted_input_browse {
                                        bookmark.cursor = cursor;
                                    }
                                }
                                None => self.accepted_input_browse = None,
                            }
                        }
                        self.sync_mutation_gate(cx);
                        cx.notify();
                        false
                    }
                    Err(error) => {
                        self.accepted_input_recall = Some(recall);
                        self.last_error = Some(error);
                        cx.notify();
                        true
                    }
                }
            }
        }
    }

    fn realize_accepted_input_history(
        &mut self,
        completed: &mut Completion,
        rebind_requested: &mut bool,
        frontier_installed: &mut bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self
            .service
            .as_ref()
            .and_then(|service| service.selected_identity())
            != Some(completed.selection)
            || (self.selection != completed.initiating_selection
                && self.selection != completed.selection)
        {
            return Err("Composer history completion lost its original editor".into());
        }
        let binding = completed.value.binding().range_binding();
        if completed.value.replaced() {
            if !*rebind_requested {
                let positions = completed.value.positions();
                self.input
                    .update(cx, |input, cx| input.set_enabled(false, cx));
                match self.input.update(cx, |input, cx| {
                    input.rebind(
                        binding,
                        Some(RangeSourceSelection {
                            anchor: positions.selection_anchor(),
                            head: positions.selection_head(),
                        }),
                        window,
                        cx,
                    )
                }) {
                    Ok(()) | Err(gpui_text_input::RangeTextInputError::Busy) => {
                        *rebind_requested = true
                    }
                    Err(error) => return Err(format!("Composer history rebind failed: {error}")),
                }
                self.admitted_positions = None;
            }
            if !*frontier_installed {
                if self.input.read(cx).history_frontier().binding() != binding {
                    return Ok(false);
                }
                self.input
                    .update(cx, |input, _| {
                        input.set_history_frontier(
                            input.history_frontier(),
                            completed.value.binding().range_history_frontier(),
                        )
                    })
                    .map_err(|error| format!("Composer history frontier failed: {error}"))?;
                *frontier_installed = true;
            }
            self.selection = completed.selection;
            if !self
                .input
                .read(cx)
                .surface()
                .is_some_and(|surface| surface.binding() == binding)
                || !self.input.read(cx).is_surface_current_and_interactive()
            {
                return Ok(false);
            }
        }
        completed.acknowledge()?;
        let previous = completed.initiating_selection;
        self.selection = completed.selection;
        self.image_inspection = None;
        self.image_surfaces.selection_changed(self.selection);
        self.private_clipboard_owner
            .adopted(previous, self.selection, None);
        if previous != self.selection {
            cx.emit(MainWindowConversationComposerEvent::SelectionAdvanced {
                previous,
                current: self.selection,
            });
        }
        Ok(false)
    }
}
