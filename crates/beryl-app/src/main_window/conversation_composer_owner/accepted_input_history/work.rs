use super::*;
use crate::composer_host::ComposerHostRecallAdvance;
use crate::main_window::conversation_composer_mount::autosave::fresh_piece_operation_id;

type Request = (
    Option<ComposerAcceptedInputReferenceV1>,
    Option<DraftComposerBrowseCheckpointV1>,
    gpui_text_input::MutationPositions,
    bool,
);

struct WorkFailure {
    ticket: Option<ComposerHostRecallTicket>,
    selection: MainWindowComposerSelectionIdentity,
    error: String,
}
enum WorkOutcome {
    Ready(Completion),
    NotCommitted { cancelled: bool },
}

impl MainWindowConversationComposer {
    pub(in crate::main_window::conversation_composer_owner::accepted_input_history) fn launch_accepted_input_recall(
        &mut self,
        request: Option<Request>,
        resume: Option<(
            MainWindowComposerSelectionIdentity,
            ComposerHostRecallTicket,
        )>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let operation = if resume.is_none() {
            Some(fresh_piece_operation_id()?)
        } else {
            None
        };
        let (flight, service) = self.begin_flight()?;
        let initiating_selection = self.selection;
        let cancellation = self
            .accepted_input_recall
            .as_ref()
            .unwrap()
            .cancellation
            .clone();
        let task = cx.background_executor().spawn(async move {
            #[cfg(feature = "test-faults")]
            if let Some(gate) = service.take_test_accepted_input_recall_gate() {
                gate.await;
            }
            let mut selection = resume.map_or(initiating_selection, |(selection, _)| selection);
            let mut ticket = resume.map(|(_, ticket)| ticket);
            let result = (|| -> Result<WorkOutcome, String> {
                if let Some((source, checkpoint, positions, capture_checkpoint)) = request {
                    ticket = Some(
                        service
                            .slot
                            .lock()
                            .map_err(|_| "Composer history service is unavailable")?
                            .begin_accepted_input_recall(
                                &service.store,
                                selection,
                                source,
                                checkpoint,
                                positions,
                                operation.unwrap(),
                                capture_checkpoint,
                            )?,
                    );
                }
                let ticket = ticket.ok_or("Composer history lost its admitted operation")?;
                loop {
                    let (current, advance) = service
                        .slot
                        .lock()
                        .map_err(|_| "Composer history service is unavailable")?
                        .advance_accepted_input_recall(
                            &service.store,
                            selection,
                            ticket,
                            &cancellation,
                        )?;
                    selection = current;
                    match advance {
                        ComposerHostRecallAdvance::Progress => {}
                        ComposerHostRecallAdvance::NotCommitted => {
                            return Ok(WorkOutcome::NotCommitted {
                                cancelled: cancellation.is_cancelled(),
                            });
                        }
                        ComposerHostRecallAdvance::Ready(value) => {
                            return Ok(WorkOutcome::Ready(Completion {
                                service: service.clone(),
                                selection,
                                initiating_selection,
                                ticket,
                                value,
                                acknowledged: false,
                            }));
                        }
                    }
                }
            })();
            result.map_err(|error| WorkFailure {
                ticket,
                selection,
                error,
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) {
                    return;
                }
                match result {
                    Ok(WorkOutcome::Ready(completed)) => {
                        if this.selection != initiating_selection {
                            return;
                        }
                        if let Some(recall) = this.accepted_input_recall.as_mut() {
                            recall.state = State::Realizing {
                                completed,
                                rebind_requested: false,
                                frontier_installed: false,
                            };
                        }
                    }
                    Ok(WorkOutcome::NotCommitted { cancelled }) => {
                        this.accepted_input_recall = None;
                        if !cancelled { this.accepted_input_history_feedback = Some("The accepted input could not be recalled. Your draft is unchanged.".into()); }
                    }
                    Err(failure) => {
                        if let Some(ticket) = failure.ticket {
                            if let Some(recall) = this.accepted_input_recall.as_mut() {
                                recall.state = State::Blocked {
                                    selection: failure.selection,
                                    ticket,
                                };
                            }
                            this.last_error = Some(failure.error);
                            this.accepted_input_history_feedback = Some(
                                "Composer history has admitted work awaiting recovery.".into(),
                            );
                        } else {
                            this.accepted_input_recall = None;
                            this.accepted_input_history_feedback = Some(
                                "The accepted input is unavailable. Your draft is unchanged."
                                    .into(),
                            );
                        }
                    }
                }
                this.sync_mutation_gate(cx);
                this.schedule_pump(window, cx);
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }
}
