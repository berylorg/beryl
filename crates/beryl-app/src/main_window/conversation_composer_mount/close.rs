use super::*;
use crate::composer_host::{ComposerHostFlushAdvance, ComposerHostFlushFailure};

mod work;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowConversationComposerCloseTicket {
    owner: gpui::EntityId,
    generation: u64,
    selection: MainWindowComposerSelectionIdentity,
}

impl MainWindowConversationComposerCloseTicket {
    pub(in crate::main_window) const fn selection(self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub(in crate::main_window) fn matches_editor(
        self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> bool {
        let previous = self.selection;
        previous.window_id() == selection.window_id()
            && previous.claim() == selection.claim()
            && previous.binding().home_id() == selection.binding().home_id()
            && previous.binding().home_generation() == selection.binding().home_generation()
            && previous.binding().host_generation() == selection.binding().host_generation()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowConversationComposerCloseAdmission {
    pub ticket: MainWindowConversationComposerCloseTicket,
    pub state: MainWindowConversationComposerCloseAdvance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerCloseAdvance {
    Preparing,
    Progress(ComposerHostFlushState),
    ReconciliationPending,
    Ready,
    Unsatisfied(ComposerHostFlushFailure),
    ReleasePending,
    WidgetReleasePending,
    Disposed,
    Stale,
}

#[derive(Clone, Copy)]
pub(super) struct ActiveWindowClose {
    ticket: MainWindowConversationComposerCloseTicket,
    flush: Option<ComposerHostFlushTicket>,
    state: MainWindowConversationComposerCloseAdvance,
    disposing: bool,
    disposal_captured: bool,
    release_requested: bool,
    restore_enabled: Option<bool>,
    #[cfg(feature = "test-faults")]
    cancel_disposal: bool,
}

impl MainWindowConversationComposerMount {
    pub fn begin_window_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerCloseAdmission, String> {
        if let Some(close) = self.window_close {
            return Ok(MainWindowConversationComposerCloseAdmission {
                ticket: close.ticket,
                state: close.state,
            });
        }
        if self.submission.is_active()
            || self.pending_presentation.is_some()
            || self.native_lineage_snapshot.is_some()
            || self.native_lineage_disposal_active
        {
            return Err("conversation composer has an unsettled lifecycle operation".to_owned());
        }
        let contribution = self
            .contribution
            .clone()
            .ok_or_else(|| "conversation composer has no resident editor".to_owned())?;
        let selection = contribution.read(cx).selection_identity();
        let generation = self
            .window_close_generation
            .checked_add(1)
            .ok_or_else(|| "conversation composer close generation exhausted".to_owned())?;
        let ticket = MainWindowConversationComposerCloseTicket {
            owner: cx.entity_id(),
            generation,
            selection,
        };
        self.service.begin_window_close_gate(ticket)?;
        if let Err(error) = contribution.update(cx, |composer, cx| {
            composer.begin_window_close_gate(ticket, window, cx)
        }) {
            self.service.finish_window_close_gate(ticket);
            return Err(error);
        }
        if let Err(error) = self.suspend_autosave() {
            contribution.update(cx, |composer, cx| {
                composer.release_window_close_gate(ticket, window, cx)
            })?;
            self.service.finish_window_close_gate(ticket);
            return Err(error);
        }
        self.window_close_generation = generation;
        self.window_close = Some(ActiveWindowClose {
            ticket,
            flush: None,
            state: MainWindowConversationComposerCloseAdvance::Preparing,
            disposing: false,
            disposal_captured: false,
            release_requested: false,
            restore_enabled: None,
            #[cfg(feature = "test-faults")]
            cancel_disposal: false,
        });
        cx.notify();
        Ok(MainWindowConversationComposerCloseAdmission {
            ticket,
            state: MainWindowConversationComposerCloseAdvance::Preparing,
        })
    }

    pub fn window_close_flush_ticket(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> Option<ComposerHostFlushTicket> {
        self.window_close
            .filter(|close| close.ticket == ticket)
            .and_then(|close| close.flush)
    }

    pub fn advance_window_close(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerCloseAdvance, String> {
        let Some(close) = self
            .window_close
            .filter(|close| close.ticket == ticket && ticket.owner == cx.entity_id())
        else {
            return Ok(MainWindowConversationComposerCloseAdvance::Stale);
        };
        if self.window_close_task.is_some() {
            return Ok(close.state);
        }
        if close.state == MainWindowConversationComposerCloseAdvance::Disposed {
            self.window_close = None;
            return Ok(MainWindowConversationComposerCloseAdvance::Disposed);
        }
        if !self.service.window_close_is_current(ticket) {
            return Ok(MainWindowConversationComposerCloseAdvance::Stale);
        }
        if close.release_requested {
            self.release_window_close(ticket, window, cx)?;
            return Ok(self
                .window_close
                .map_or(MainWindowConversationComposerCloseAdvance::Stale, |close| {
                    close.state
                }));
        }
        if close.disposing {
            self.schedule_window_close_work(
                close,
                if close.disposal_captured {
                    work::WindowCloseWork::AdvanceDisposal
                } else {
                    work::WindowCloseWork::CaptureDisposal
                },
                window,
                cx,
            );
            return Ok(close.state);
        }
        if matches!(
            close.state,
            MainWindowConversationComposerCloseAdvance::Ready
                | MainWindowConversationComposerCloseAdvance::Unsatisfied(_)
        ) {
            return Ok(close.state);
        }
        if close.flush.is_some() {
            self.schedule_window_close_work(
                close,
                if close.state
                    == MainWindowConversationComposerCloseAdvance::Progress(
                        ComposerHostFlushState::CaptureRequired,
                    )
                {
                    work::WindowCloseWork::Capture
                } else {
                    work::WindowCloseWork::Advance
                },
                window,
                cx,
            );
            return Ok(close.state);
        }
        let contribution = self
            .contribution
            .clone()
            .ok_or_else(|| "window close lost its resident editor".to_owned())?;
        match contribution.update(cx, |composer, cx| {
            composer.window_close_flush_ready(ticket, cx)
        }) {
            Ok(false) => return Ok(MainWindowConversationComposerCloseAdvance::Preparing),
            Err(_) => {
                return Ok(self.record_window_close_state(
                    MainWindowConversationComposerCloseAdvance::Unsatisfied(
                        ComposerHostFlushFailure::Recoverable,
                    ),
                    cx,
                ));
            }
            Ok(true) => {}
        }
        let Some(admission) = self.service.begin_window_close_flush(ticket)? else {
            return Ok(MainWindowConversationComposerCloseAdvance::Preparing);
        };
        match admission {
            ComposerHostFlushAdmission::Started {
                ticket: flush,
                state,
            }
            | ComposerHostFlushAdmission::Joined {
                ticket: flush,
                state,
            } => {
                self.window_close.as_mut().unwrap().flush = Some(flush);
                Ok(self.record_window_close_state(close_progress(state), cx))
            }
            ComposerHostFlushAdmission::Satisfied(_) => {
                Err("window close did not retain its flush ticket".to_owned())
            }
        }
    }

    pub fn release_window_close(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let Some(close) = self
            .window_close
            .filter(|close| close.ticket == ticket && ticket.owner == cx.entity_id())
        else {
            return Ok(false);
        };
        if close.disposing {
            return Err("window close disposal is already authorized".to_owned());
        }
        if self.window_close_task.is_some() {
            self.window_close.as_mut().unwrap().release_requested = true;
            self.record_window_close_state(
                MainWindowConversationComposerCloseAdvance::ReleasePending,
                cx,
            );
            return Ok(true);
        }
        if !self.service.window_close_is_current(ticket) {
            return Ok(false);
        }
        match self
            .service
            .release_window_close_gate(ticket, close.flush)?
        {
            Some(false) => return Ok(false),
            None => {
                self.window_close.as_mut().unwrap().release_requested = true;
                self.record_window_close_state(
                    MainWindowConversationComposerCloseAdvance::ReleasePending,
                    cx,
                );
                self.schedule_window_close_release(close, window, cx);
                return Ok(true);
            }
            Some(true) => {}
        }
        self.finish_window_close_release(ticket, window, cx)?;
        Ok(true)
    }

    pub fn authorize_window_close_disposal(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerCloseAdvance, String> {
        let Some(close) = self
            .window_close
            .filter(|close| close.ticket == ticket && ticket.owner == cx.entity_id())
        else {
            return Ok(MainWindowConversationComposerCloseAdvance::Stale);
        };
        if !self.service.window_close_is_current(ticket) {
            return Ok(MainWindowConversationComposerCloseAdvance::Stale);
        }
        if close.disposing {
            return Ok(close.state);
        }
        if self.window_close_task.is_some()
            || close.state != MainWindowConversationComposerCloseAdvance::Ready
        {
            return Err("window close draft is not ready for final disposal".to_owned());
        }
        let contribution = self
            .contribution
            .clone()
            .ok_or_else(|| "window close lost its resident editor".to_owned())?;
        if !contribution.update(cx, |composer, cx| {
            composer.window_close_flush_ready(ticket, cx)
        })? {
            return Ok(MainWindowConversationComposerCloseAdvance::WidgetReleasePending);
        }
        let input = contribution.read(cx).gpui_input();
        let was_enabled = input.read(cx).is_enabled();
        if !contribution.update(cx, |composer, cx| {
            composer.begin_widget_release_fence(window, cx)
        })? {
            return Ok(MainWindowConversationComposerCloseAdvance::WidgetReleasePending);
        }
        let advance = self
            .service
            .authorize_window_close_disposal(ticket, close.flush.unwrap());
        match advance {
            Ok(Some(ComposerHostFlushAdvance::Progress(
                ComposerHostFlushState::DisposalRequired,
            ))) => {
                let active = self.window_close.as_mut().unwrap();
                active.disposing = true;
                active.restore_enabled = Some(was_enabled);
                Ok(self.record_window_close_state(
                    MainWindowConversationComposerCloseAdvance::Progress(
                        ComposerHostFlushState::DisposalRequired,
                    ),
                    cx,
                ))
            }
            result => {
                contribution.update(cx, |composer, cx| {
                    composer.resume_after_widget_release_fence(window, cx)
                })?;
                input.update(cx, |input, cx| input.set_enabled(was_enabled, cx));
                match result {
                    Ok(None) => {
                        Ok(MainWindowConversationComposerCloseAdvance::WidgetReleasePending)
                    }
                    Ok(Some(ComposerHostFlushAdvance::Stale)) => {
                        Ok(MainWindowConversationComposerCloseAdvance::Stale)
                    }
                    Err(error) => Err(error),
                    _ => Err("window close authorization returned an unexpected state".to_owned()),
                }
            }
        }
    }

    fn finish_window_close_release(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .contribution
            .clone()
            .ok_or_else(|| "window close lost its resident editor".to_owned())?;
        if !contribution.update(cx, |composer, cx| {
            composer.release_window_close_gate(ticket, window, cx)
        })? {
            return Err("window close resident gate changed during release".to_owned());
        }
        self.window_close = None;
        self.refresh_autosave(window, cx)?;
        cx.notify();
        Ok(())
    }

    fn restore_window_close_disposal_gate(
        &mut self,
        close: ActiveWindowClose,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .contribution
            .clone()
            .ok_or_else(|| "window close lost its resident editor".to_owned())?;
        if !contribution.read(cx).is_live() {
            contribution.update(cx, |composer, cx| {
                composer.resume_after_widget_release_fence(window, cx)
            })?;
        }
        if let Some(enabled) = close.restore_enabled {
            contribution
                .read(cx)
                .gpui_input()
                .update(cx, |input, cx| input.set_enabled(enabled, cx));
        }
        self.window_close.as_mut().unwrap().disposing = false;
        Ok(())
    }

    fn record_window_close_state(
        &mut self,
        state: MainWindowConversationComposerCloseAdvance,
        cx: &mut Context<Self>,
    ) -> MainWindowConversationComposerCloseAdvance {
        self.window_close.as_mut().unwrap().state = state;
        cx.notify();
        state
    }
}

fn close_progress(state: ComposerHostFlushState) -> MainWindowConversationComposerCloseAdvance {
    if state == ComposerHostFlushState::CloseReady {
        MainWindowConversationComposerCloseAdvance::Ready
    } else {
        MainWindowConversationComposerCloseAdvance::Progress(state)
    }
}
