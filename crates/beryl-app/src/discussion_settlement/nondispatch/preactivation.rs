use super::*;
use beryl_model::{ExecutionBinding, SyndicThreadId};
use syndic_storage::PendingDispatchEvidence;

#[derive(Clone, Copy)]
pub(crate) enum DiscussionPreparationFailure {
    Runtime,
    Root,
    Cas,
}

impl DiscussionPreparationFailure {
    fn kind(self) -> HandoffFailureKind {
        match self {
            Self::Runtime => HandoffFailureKind::RuntimeUnavailable,
            Self::Root => HandoffFailureKind::RootUnavailable,
            Self::Cas => HandoffFailureKind::CasUnavailable,
        }
    }
}

#[derive(Clone)]
pub(super) struct BeforeActivation {
    pending: PendingDispatchEvidence,
    binding: ExecutionBinding,
}

impl DiscussionParentNondispatch {
    pub(in crate::discussion_settlement) fn before_activation(
        pending: PendingDispatchEvidence,
        binding: ExecutionBinding,
        failure: DiscussionPreparationFailure,
    ) -> Self {
        Self {
            source: Source::BeforeActivation(BeforeActivation { pending, binding }),
            kind: failure.kind(),
        }
    }

    pub(in crate::discussion_settlement) fn thread_id(&self) -> SyndicThreadId {
        match &self.source {
            Source::Activated(request) => request.thread_id(),
            Source::BeforeActivation(proof) => proof.pending.thread_id(),
        }
    }

    pub(in crate::discussion_settlement) fn turn_id(&self) -> SyndicTurnId {
        match &self.source {
            Source::Activated(request) => request.turn_id(),
            Source::BeforeActivation(proof) => proof.pending.turn_id(),
        }
    }
}

impl BeforeActivation {
    pub(super) fn validate(
        &self,
        access: Access<'_>,
        syndic: &SyndicStorage,
    ) -> Result<(), DiscussionSettlementError> {
        let expected = self.pending;
        let limit = SyndicPointReadLimit::new(400_000).expect("bounded pending dispatch evidence");
        let (pending, execution) = match access {
            Access::Ordinary(store) => (
                syndic.pending_dispatch_evidence(store, expected.thread_id(), limit)?,
                syndic.thread_execution(store, expected.thread_id(), limit)?,
            ),
            Access::Candidate(candidate) => (
                syndic.pending_dispatch_evidence_candidate(
                    candidate,
                    expected.thread_id(),
                    limit,
                )?,
                syndic.thread_execution_candidate(candidate, expected.thread_id(), limit)?,
            ),
        };
        let Some(actual) = pending else {
            return Err(DiscussionSettlementError::IdentityMismatch);
        };
        if actual.home_id() != expected.home_id()
            || actual.thread_id() != expected.thread_id()
            || actual.turn_id() != expected.turn_id()
            || actual.binding_revision() != expected.binding_revision()
            || actual.gate_revision() != expected.gate_revision()
            || actual.state_revision() != expected.state_revision()
            || actual.dispatch_provenance() != expected.dispatch_provenance()
            || actual.selected_path() != expected.selected_path()
            || actual.item_id() != expected.item_id()
            || actual.item_revision() != expected.item_revision()
            || actual.input() != expected.input()
            || execution.is_none_or(|execution| {
                execution.thread_id() != expected.thread_id()
                    || execution.execution() != &self.binding
            })
        {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        Ok(())
    }
}
