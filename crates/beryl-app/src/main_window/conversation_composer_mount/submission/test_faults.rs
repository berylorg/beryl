use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll, Waker},
};

use beryl_home_store::CommandCancellation;
use beryl_state::AssetState;
use gpui::{Context, Window};

use super::*;

#[derive(Clone)]
pub struct MainWindowComposerSubmissionAdvanceTestRelease(Arc<Mutex<TestGateState>>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerSubmissionAdvanceTestToken {
    generation: u64,
    ticket: ComposerHostSubmissionTicket,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerSubmissionTestAdvance {
    ReconciliationPending,
    Collision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowConversationComposerSubmissionTestDiagnostics {
    status: MainWindowConversationComposerSubmissionStatus,
    active_ticket: bool,
    active_task: bool,
    successor: bool,
}

pub(super) struct SubmissionAdvanceTestGate(Arc<Mutex<TestGateState>>);

struct TestGateState {
    entered: bool,
    released: bool,
    waker: Option<Waker>,
}

impl MainWindowComposerSubmissionAdvanceTestRelease {
    pub fn is_blocked(&self) -> bool {
        self.0.lock().unwrap().entered
    }

    pub fn release(self) {
        let mut state = self.0.lock().unwrap();
        state.released = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

impl MainWindowConversationComposerSubmissionTestDiagnostics {
    pub const fn status(self) -> MainWindowConversationComposerSubmissionStatus {
        self.status
    }

    pub const fn active_ticket(self) -> bool {
        self.active_ticket
    }

    pub const fn active_task(self) -> bool {
        self.active_task
    }

    pub const fn successor(self) -> bool {
        self.successor
    }
}

impl Future for SubmissionAdvanceTestGate {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        let mut state = self.0.lock().unwrap();
        state.entered = true;
        if state.released {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl MainWindowConversationComposerMount {
    pub fn test_fail_submission_successor_after_readiness_once(&mut self) {
        assert!(!self.submission.test_fail_successor_after_readiness);
        self.submission.test_fail_successor_after_readiness = true;
    }

    pub fn test_block_next_submission_advance(
        &mut self,
    ) -> MainWindowComposerSubmissionAdvanceTestRelease {
        let state = Arc::new(Mutex::new(TestGateState {
            entered: false,
            released: false,
            waker: None,
        }));
        assert!(
            self.submission
                .test_advance_gate
                .replace(SubmissionAdvanceTestGate(state.clone()))
                .is_none()
        );
        MainWindowComposerSubmissionAdvanceTestRelease(state)
    }

    pub fn test_inject_next_submission_advance(
        &mut self,
        advance: MainWindowComposerSubmissionTestAdvance,
    ) {
        assert!(self.submission.test_advance.replace(advance).is_none());
    }

    pub fn test_submission_diagnostics(
        &self,
    ) -> MainWindowConversationComposerSubmissionTestDiagnostics {
        MainWindowConversationComposerSubmissionTestDiagnostics {
            status: self.submission.status,
            active_ticket: self
                .submission
                .active
                .as_ref()
                .is_some_and(|active| active.ticket.is_some()),
            active_task: self.submission.task.is_some(),
            successor: self
                .submission
                .active
                .as_ref()
                .is_some_and(|active| active.successor.is_some()),
        }
    }

    pub fn test_submission_advance_token(
        &self,
    ) -> Option<MainWindowComposerSubmissionAdvanceTestToken> {
        let active = self.submission.active.as_ref()?;
        Some(MainWindowComposerSubmissionAdvanceTestToken {
            generation: self.submission.generation,
            ticket: active.ticket?,
        })
    }

    pub fn test_resume_submission_after_reconciliation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.submission.status != MainWindowConversationComposerSubmissionStatus::Reconciling {
            return Err("mounted submission is not reconciling".to_owned());
        }
        let ticket = self
            .submission
            .active
            .as_ref()
            .and_then(|active| active.ticket)
            .ok_or_else(|| "reconciling mounted submission has no ticket".to_owned())?;
        self.schedule_submission_advance(self.submission.generation, ticket, window, cx)
    }

    pub fn test_apply_late_submission_advance(
        &mut self,
        token: MainWindowComposerSubmissionAdvanceTestToken,
        advance: MainWindowComposerSubmissionTestAdvance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.finish_submission_advance(
            token.generation,
            token.ticket,
            Ok(match advance {
                MainWindowComposerSubmissionTestAdvance::ReconciliationPending => {
                    MainWindowComposerSubmissionAdvance::ReconciliationPending
                }
                MainWindowComposerSubmissionTestAdvance::Collision => {
                    MainWindowComposerSubmissionAdvance::Collision
                }
            }),
            window,
            cx,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_test_advance(
    advance: MainWindowComposerSubmissionTestAdvance,
    actual: Result<MainWindowComposerSubmissionAdvance, String>,
    service: &crate::main_window::MainWindowConversationComposerService,
    selection: MainWindowComposerSelectionIdentity,
    ticket: ComposerHostSubmissionTicket,
    assets: AssetState,
    marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
    prepared: &PreparedMountedSubmission,
    cancellation: &CommandCancellation,
) -> Result<MainWindowComposerSubmissionAdvance, String> {
    if !matches!(actual, Ok(MainWindowComposerSubmissionAdvance::Progress(_))) {
        return actual;
    }
    match advance {
        MainWindowComposerSubmissionTestAdvance::ReconciliationPending => {
            Ok(MainWindowComposerSubmissionAdvance::TestReconciliationPending)
        }
        MainWindowComposerSubmissionTestAdvance::Collision => {
            cancellation.cancel();
            let current = service.selected_identity().ok_or_else(|| {
                "test mounted submission selection disappeared during collision settlement"
                    .to_owned()
            })?;
            if current.window_id() != selection.window_id()
                || current.claim() != selection.claim()
                || current.binding().presentation_generation()
                    != selection.binding().presentation_generation()
            {
                return Err(
                    "test mounted submission selection drifted during collision settlement"
                        .to_owned(),
                );
            }
            let settled = service.advance_submission(
                current,
                ticket,
                assets,
                marker_seals,
                prepared.publication_operation_id,
                prepared.marker_authority,
                prepared.published_at,
                &prepared.successor_request,
                prepared.successor_retirement_operation_id,
                prepared.next_draft_id,
                cancellation,
            );
            match settled {
                Ok(MainWindowComposerSubmissionAdvance::Cancelled)
                | Ok(MainWindowComposerSubmissionAdvance::NotCommitted)
                | Ok(MainWindowComposerSubmissionAdvance::Stale) => {
                    Ok(MainWindowComposerSubmissionAdvance::Collision)
                }
                other => other,
            }
        }
    }
}
