use super::*;
use crate::main_window::MainWindowComposerAutosaveCaptureRequirement;

#[derive(Clone, Copy)]
pub(super) enum WindowCloseWork {
    Capture,
    Advance,
    CaptureDisposal,
    AdvanceDisposal,
}

enum WindowCloseWorkOutcome {
    Capture(ComposerHostFlushCapture),
    Advance(ComposerHostFlushAdvance),
    DisposalCapture(ComposerHostFlushCapture),
    DisposalAdvance(MainWindowComposerDisposalAdvance),
}

impl MainWindowConversationComposerMount {
    #[cfg(feature = "test-faults")]
    pub fn test_cancel_next_window_close_disposal(&mut self) {
        if let Some(close) = self.window_close.as_mut() {
            close.cancel_disposal = true;
        }
    }

    pub(super) fn schedule_window_close_work(
        &mut self,
        close: ActiveWindowClose,
        work: WindowCloseWork,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = self.service.clone();
        let assets = self.submission_assets();
        let seals = self.submission_marker_seals();
        let executor = cx.background_executor().clone();
        let task = executor.spawn(async move {
            let result = run_close_work(&service, close, work, assets, seals);
            let selection = service.selected_identity();
            (result, selection)
        });
        let service = self.service.clone();
        self.window_close_task = Some(cx.spawn_in(window, async move |this, cx| {
            let (result, selection) = task.await;
            let disposal_captured = close.disposal_captured
                || matches!(
                    &result,
                    Ok(WindowCloseWorkOutcome::DisposalCapture(
                        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired,)
                    ))
                );
            let disposing = close.disposing
                && !matches!(
                    &result,
                    Ok(WindowCloseWorkOutcome::DisposalCapture(
                        ComposerHostFlushCapture::Unsatisfied(_)
                    )) | Ok(WindowCloseWorkOutcome::DisposalAdvance(
                        MainWindowComposerDisposalAdvance::Failed
                    ))
                );
            let applied = this.update_in(cx, |this, window, cx| {
                if this.window_close.is_none_or(|current| {
                    current.ticket != close.ticket || current.flush != close.flush
                }) {
                    return;
                }
                this.window_close_task = None;
                if let Err(_) = this.finish_window_close_work(close, result, selection, window, cx)
                {
                    if this.window_close.is_some() {
                        this.record_window_close_state(
                            MainWindowConversationComposerCloseAdvance::Unsatisfied(
                                ComposerHostFlushFailure::Recoverable,
                            ),
                            cx,
                        );
                    }
                }
            });
            if applied.is_err() {
                service.cleanup_unmounted_window_close(
                    close.ticket,
                    close.flush,
                    disposing,
                    disposal_captured,
                    executor,
                );
            }
        }));
    }

    pub(super) fn schedule_window_close_release(
        &mut self,
        close: ActiveWindowClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            service.release_window_close_gate_wait(close.ticket, close.flush)
        });
        self.window_close_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this
                    .window_close
                    .is_none_or(|current| current.ticket != close.ticket)
                {
                    return;
                }
                this.window_close_task = None;
                match result {
                    Ok(true) => {
                        let _ = this.finish_window_close_release(close.ticket, window, cx);
                    }
                    Ok(false) => {
                        this.record_window_close_state(
                            MainWindowConversationComposerCloseAdvance::Stale,
                            cx,
                        );
                    }
                    Err(_) => {
                        this.record_window_close_state(
                            MainWindowConversationComposerCloseAdvance::Unsatisfied(
                                ComposerHostFlushFailure::Recoverable,
                            ),
                            cx,
                        );
                    }
                }
            });
        }));
    }

    fn finish_window_close_work(
        &mut self,
        close: ActiveWindowClose,
        result: Result<WindowCloseWorkOutcome, String>,
        selection: Option<MainWindowComposerSelectionIdentity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.service.window_close_is_current(close.ticket) {
            return Ok(());
        }
        if let Some(selection) = selection {
            if !close.ticket.matches_editor(selection) {
                return Ok(());
            }
            if let Some(contribution) = self.contribution.clone() {
                contribution.update(cx, |composer, cx| {
                    let previous = composer.selection_identity();
                    if previous != selection {
                        composer.synchronize_lifecycle_selection(previous, selection, cx)
                    } else {
                        Ok(())
                    }
                })?;
            }
        }
        let state = match result {
            Ok(WindowCloseWorkOutcome::Capture(capture)) => match capture {
                ComposerHostFlushCapture::Captured(_) => {
                    MainWindowConversationComposerCloseAdvance::Progress(
                        ComposerHostFlushState::PublicationPending,
                    )
                }
                ComposerHostFlushCapture::State(state) => close_progress(state),
                ComposerHostFlushCapture::Unsatisfied(failure) => {
                    MainWindowConversationComposerCloseAdvance::Unsatisfied(failure)
                }
                ComposerHostFlushCapture::Stale => {
                    MainWindowConversationComposerCloseAdvance::Stale
                }
                ComposerHostFlushCapture::Satisfied(_) => {
                    return Err("window close publication lost its resident hold".to_owned());
                }
            },
            Ok(WindowCloseWorkOutcome::Advance(advance)) => match advance {
                ComposerHostFlushAdvance::Progress(state) => close_progress(state),
                ComposerHostFlushAdvance::ReconciliationPending => {
                    MainWindowConversationComposerCloseAdvance::ReconciliationPending
                }
                ComposerHostFlushAdvance::Unsatisfied(failure) => {
                    MainWindowConversationComposerCloseAdvance::Unsatisfied(failure)
                }
                ComposerHostFlushAdvance::Stale => {
                    MainWindowConversationComposerCloseAdvance::Stale
                }
                ComposerHostFlushAdvance::Satisfied(_) => {
                    return Err("window close disposed before final settlement".to_owned());
                }
            },
            Ok(WindowCloseWorkOutcome::DisposalCapture(capture)) => match capture {
                ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired) => {
                    self.window_close.as_mut().unwrap().disposal_captured = true;
                    MainWindowConversationComposerCloseAdvance::Progress(
                        ComposerHostFlushState::DisposalRequired,
                    )
                }
                ComposerHostFlushCapture::Unsatisfied(failure) => {
                    self.restore_window_close_disposal_gate(close, window, cx)?;
                    MainWindowConversationComposerCloseAdvance::Unsatisfied(failure)
                }
                ComposerHostFlushCapture::Stale => {
                    MainWindowConversationComposerCloseAdvance::Stale
                }
                _ => return Err("window close disposal capture changed its exact state".to_owned()),
            },
            Ok(WindowCloseWorkOutcome::DisposalAdvance(advance)) => {
                match self.finish_disposal_advance(advance, window, cx)? {
                    MainWindowConversationComposerMountDisposalAdvance::Disposed => {
                        self.service.finish_window_close_gate(close.ticket);
                        self.record_window_close_state(
                            MainWindowConversationComposerCloseAdvance::Disposed,
                            cx,
                        );
                        cx.notify();
                        return Ok(());
                    }
                    MainWindowConversationComposerMountDisposalAdvance::WidgetReleasePending(_) => {
                        MainWindowConversationComposerCloseAdvance::WidgetReleasePending
                    }
                    MainWindowConversationComposerMountDisposalAdvance::Retained(advance) => {
                        match advance {
                            MainWindowComposerDisposalAdvance::Progress(state) => {
                                MainWindowConversationComposerCloseAdvance::Progress(state)
                            }
                            MainWindowComposerDisposalAdvance::ReconciliationPending => {
                                MainWindowConversationComposerCloseAdvance::ReconciliationPending
                            }
                            MainWindowComposerDisposalAdvance::WidgetReleaseRequired(_) => {
                                MainWindowConversationComposerCloseAdvance::WidgetReleasePending
                            }
                            MainWindowComposerDisposalAdvance::Failed => {
                                self.restore_window_close_disposal_gate(close, window, cx)?;
                                MainWindowConversationComposerCloseAdvance::Unsatisfied(
                                    ComposerHostFlushFailure::Recoverable,
                                )
                            }
                            MainWindowComposerDisposalAdvance::Disposed => unreachable!(),
                        }
                    }
                }
            }
            Err(_) => MainWindowConversationComposerCloseAdvance::Unsatisfied(
                ComposerHostFlushFailure::Recoverable,
            ),
        };
        self.record_window_close_state(state, cx);
        if self
            .window_close
            .is_some_and(|current| current.release_requested && !current.disposing)
        {
            self.release_window_close(close.ticket, window, cx)?;
        }
        Ok(())
    }

    pub(in crate::main_window) fn release_window_close_on_drop(&mut self) {
        let Some(close) = self.window_close.take() else {
            return;
        };
        if close.state == MainWindowConversationComposerCloseAdvance::Disposed {
            return;
        }
        if let Some(task) = self.window_close_task.take() {
            task.detach();
        } else {
            self.service.clone().cleanup_unmounted_window_close(
                close.ticket,
                close.flush,
                close.disposing,
                close.disposal_captured,
                self.submission.executor(),
            );
        }
    }
}

fn run_close_work(
    service: &MainWindowConversationComposerService,
    close: ActiveWindowClose,
    work: WindowCloseWork,
    assets: beryl_state::AssetState,
    seals: DraftMarkerSealService,
) -> Result<WindowCloseWorkOutcome, String> {
    if !service.window_close_is_current(close.ticket) {
        return Ok(WindowCloseWorkOutcome::Advance(
            ComposerHostFlushAdvance::Stale,
        ));
    }
    let selection = service
        .selected_identity()
        .filter(|selection| close.ticket.matches_editor(*selection))
        .ok_or_else(|| "window close editor is stale".to_owned())?;
    let flush = close
        .flush
        .ok_or_else(|| "window close has no flush".to_owned())?;
    match work {
        WindowCloseWork::Capture => {
            let requirement = service.autosave_capture_requirement(selection)?;
            let marker_authority = match requirement {
                MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => {
                    Some(autosave::fresh_marker_authority()?)
                }
                MainWindowComposerAutosaveCaptureRequirement::Clean
                | MainWindowComposerAutosaveCaptureRequirement::UnchangedMarkers => None,
            };
            service
                .capture_flush_publication(
                    selection,
                    flush,
                    assets,
                    &seals,
                    autosave::fresh_piece_operation_id()?,
                    marker_authority,
                    autosave::current_timestamp()?,
                    &CommandCancellation::new(),
                )
                .map(WindowCloseWorkOutcome::Capture)
        }
        WindowCloseWork::Advance => service
            .advance_window_close_flush(close.ticket, flush)
            .map(WindowCloseWorkOutcome::Advance),
        WindowCloseWork::CaptureDisposal => {
            let cancellation = CommandCancellation::new();
            #[cfg(feature = "test-faults")]
            if close.cancel_disposal {
                cancellation.cancel();
            }
            service
                .capture_flush_disposal(
                    selection,
                    flush,
                    autosave::fresh_piece_operation_id()?,
                    &cancellation,
                )
                .map(WindowCloseWorkOutcome::DisposalCapture)
        }
        WindowCloseWork::AdvanceDisposal => service
            .advance_disposal()
            .map(WindowCloseWorkOutcome::DisposalAdvance),
    }
}
