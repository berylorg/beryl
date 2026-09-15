use beryl_home_store::{
    HomeCandidateRecoveryAccess, HomeGeneration, HomeStore, PointReadLimit, ReadError,
};
use beryl_model::{BerylHomeId, DomainRevision, InputGateRevision, SyndicThreadId, SyndicTurnId};

use crate::{
    BindingState, InputGateState, SelectedPathProof, SyndicPointReadLimit, SyndicReadError,
    SyndicStorage, TurnLifecycle, TurnStateRevision,
    codec::{ExactCodec, Family, InputGatesFamily, ThreadsFamily, TurnStatesFamily},
    terminal_history::TerminalHistoryReader,
};

use super::access::ReadAccess;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalHistoryEvidence {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    source_revision: DomainRevision,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    selected_path: SelectedPathProof,
    gate_revision: InputGateRevision,
    state_revision: TurnStateRevision,
    lifecycle: TurnLifecycle,
}

impl TerminalHistoryEvidence {
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }
    pub const fn source_revision(self) -> DomainRevision {
        self.source_revision
    }
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }
    pub const fn selected_path(self) -> SelectedPathProof {
        self.selected_path
    }
    pub const fn gate_revision(self) -> InputGateRevision {
        self.gate_revision
    }
    pub const fn state_revision(self) -> TurnStateRevision {
        self.state_revision
    }
    pub const fn lifecycle(self) -> TurnLifecycle {
        self.lifecycle
    }
}

impl SyndicStorage {
    pub fn terminal_history_evidence(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TerminalHistoryEvidence>, SyndicReadError> {
        self.read_terminal_history_evidence_with_access(
            ReadAccess::Ordinary(store),
            thread_id,
            turn_id,
            limit,
            || {},
        )
    }

    pub fn terminal_history_evidence_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TerminalHistoryEvidence>, SyndicReadError> {
        self.read_terminal_history_evidence_with_access(
            ReadAccess::Candidate(store),
            thread_id,
            turn_id,
            limit,
            || {},
        )
    }

    pub(crate) fn read_terminal_history_evidence_with_access(
        &self,
        store: ReadAccess<'_>,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        limit: SyndicPointReadLimit,
        before_confirmation: impl FnOnce(),
    ) -> Result<Option<TerminalHistoryEvidence>, SyndicReadError> {
        let revision = self.revision_with_access(store)?;
        self.with_current_gate_source_with_access(store, thread_id, limit, || {
            let read = || {
                let thread = self
                    .point_with_access::<ThreadsFamily>(store, thread_id, limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "terminal history thread is missing",
                    ))?;
                let gate = self
                    .point_with_access::<InputGatesFamily>(store, thread_id, limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "terminal history gate is missing",
                    ))?;
                if thread.committed_tail() != Some(turn_id)
                    || !matches!(gate.state(), InputGateState::Idle)
                {
                    return Ok(None);
                }
                let state = self
                    .point_with_access::<TurnStatesFamily>(store, turn_id, limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "terminal history turn state is missing",
                    ))?;
                let binding = self
                    .current_binding_with_access(store, thread_id, limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "terminal history binding is missing",
                    ))?;
                if matches!(binding.binding().state(), BindingState::Active(_)) {
                    return Err(SyndicReadError::Invariant(
                        "idle terminal history retains active binding",
                    ));
                }
                let reader = TerminalRead {
                    storage: self,
                    store,
                    limit,
                };
                if !crate::terminal_history::is_complete(&reader, &thread, &state, None)? {
                    return Ok(None);
                }
                Ok(Some(TerminalHistoryEvidence {
                    home_id: store.home_id(),
                    home_generation: self.home_generation,
                    source_revision: revision,
                    thread_id,
                    turn_id,
                    selected_path: SelectedPathProof::new(
                        thread.committed_tail(),
                        thread.revision(),
                        thread.selected_path_digest(),
                    ),
                    gate_revision: gate.revision(),
                    state_revision: state.revision(),
                    lifecycle: state.lifecycle(),
                }))
            };
            let first = read();
            before_confirmation();
            let second = read();
            if self.revision_with_access(store)? != revision {
                return Err(SyndicReadError::ConcurrentChange {
                    operation: "terminal history evidence",
                });
            }
            let first = first?;
            if first != second? {
                return Err(SyndicReadError::ConcurrentChange {
                    operation: "terminal history evidence",
                });
            }
            Ok(first)
        })
    }
}

struct TerminalRead<'a> {
    storage: &'a SyndicStorage,
    store: ReadAccess<'a>,
    limit: SyndicPointReadLimit,
}

impl TerminalHistoryReader for TerminalRead<'_> {
    fn read<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, ReadError> {
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_syndic_point_read();
        self.store
            .read_point::<crate::domain::SyndicDomain, ExactCodec<F>>(
                &self.storage.handle,
                key,
                PointReadLimit::new(self.limit.max_bytes()).expect("nonzero terminal point limit"),
            )
    }
}
