//! Content-free capture independent of GUI update delivery.
use super::*;
use crate::compaction_diagnostics::{
    CompactionDiagnosticAcceptance as Acceptance, CompactionDiagnosticCategory as Category,
    CompactionDiagnosticEventData, CompactionDiagnosticHandle,
    CompactionDiagnosticOutcome as DiagnosticOutcome, CompactionDiagnosticStage as Stage,
};
use std::cell::Cell;

pub(super) struct Capture {
    handle: Option<CompactionDiagnosticHandle>,
    elapsed: Cell<Duration>,
    last_event: Cell<Option<Duration>>,
    outcome: Cell<Option<DiagnosticOutcome>>,
}

impl Capture {
    pub(super) fn new(handle: Option<CompactionDiagnosticHandle>) -> Self {
        Self {
            handle,
            elapsed: Cell::new(Duration::ZERO),
            last_event: Cell::new(None),
            outcome: Cell::new(None),
        }
    }

    pub(super) fn at(&self, elapsed: Duration) {
        self.elapsed.set(elapsed);
    }

    fn data<'a>(&self, turn: Option<&'a str>) -> CompactionDiagnosticEventData<'a> {
        CompactionDiagnosticEventData {
            turn_identity: turn,
            elapsed: self.elapsed.get(),
            last_event_age: self
                .last_event
                .get()
                .map(|last| self.elapsed.get().saturating_sub(last)),
        }
    }

    pub(super) fn record(&self, stage: Stage, category: Category, turn: Option<&str>) {
        if let Some(handle) = &self.handle {
            handle.record(stage, category, self.data(turn));
        }
    }

    pub(super) fn bind_operation(&self, operation: &OperationIdentity) {
        if let Some(handle) = &self.handle {
            handle.bind_thread(Some(&operation.thread_id));
            handle.bind_operation(
                Some(&operation.operation_id),
                Some(&operation.observation_session_id),
            );
            self.record(Stage::Start, Category::IdentityKnown, None);
        }
    }

    pub(super) fn acceptance(&self, acceptance: Acceptance) {
        if let Some(handle) = &self.handle {
            handle.set_acceptance(acceptance);
        }
    }

    pub(super) fn lifecycle(&self, category: Category, turn: Option<&str>) {
        self.last_event.set(Some(self.elapsed.get()));
        self.record(Stage::Lifecycle, category, turn);
    }

    pub(super) fn update(&self, kind: &UpdateKind, turn: Option<&str>) {
        match kind {
            UpdateKind::TurnKnown { turn_id } => {
                if let Some(handle) = &self.handle {
                    handle.bind_turn(Some(turn_id));
                }
                self.record(Stage::Start, Category::IdentityKnown, Some(turn_id));
            }
            UpdateKind::Warning => self.record(Stage::Warning, Category::ThresholdReached, turn),
            UpdateKind::Unconfirmed(reason) => {
                let category = match reason {
                    UnconfirmedReason::Unavailable => Category::Unavailable,
                    UnconfirmedReason::InvalidEvidence => Category::Invalid,
                    UnconfirmedReason::Receipt(CompactionUnknownReason::RuntimeChange) => {
                        Category::RuntimeChanged
                    }
                    UnconfirmedReason::Receipt(_) => Category::Unknown,
                };
                self.record(Stage::Reconcile, category, turn);
                if let Some(handle) = &self.handle {
                    handle.set_outcome(DiagnosticOutcome::Unconfirmed);
                }
            }
            UpdateKind::Activity(activity) => self.lifecycle(
                match activity {
                    Activity::Started => Category::Started,
                    Activity::CompactionItemStarted => Category::ItemStarted,
                    Activity::CompactionItemCompleted => Category::ItemCompleted,
                    Activity::Retrying => Category::Retrying,
                    Activity::CompletedAwaitingIdle => Category::CompletedAwaitingIdle,
                },
                turn,
            ),
            UpdateKind::Finished(outcome) => {
                let (outcome, category) = match outcome {
                    Outcome::Succeeded => (DiagnosticOutcome::Completed, Category::Succeeded),
                    Outcome::Failed { .. } => (DiagnosticOutcome::Failed, Category::Failed),
                    Outcome::Interrupted => (DiagnosticOutcome::Interrupted, Category::Interrupted),
                    Outcome::Rejected { .. } => {
                        self.acceptance(Acceptance::Rejected);
                        (DiagnosticOutcome::Rejected, Category::Rejected)
                    }
                };
                self.outcome.set(Some(outcome));
                if let Some(handle) = &self.handle {
                    handle.set_outcome(outcome);
                }
                self.record(Stage::Lifecycle, category, turn);
            }
            UpdateKind::TokenUsage { .. } | UpdateKind::Prepared => {}
        }
    }

    pub(super) fn finish(&self, turn: Option<&str>) {
        if let Some(handle) = &self.handle {
            let outcome = self.outcome.get().unwrap_or(DiagnosticOutcome::Cancelled);
            handle.finish(outcome, self.data(turn));
        }
    }
}
