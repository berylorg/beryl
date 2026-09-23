use std::num::NonZeroU64;

use beryl_home_store::{HomeCandidateRecoveryAccess, HomeStore};
use beryl_model::{JobId, ResolutionIntentId, SyndicThreadId, SyndicTurnId};

use crate::{
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, SyndicValueError,
    codec::DiscussionHandoffGatesFamily, read::access::ReadAccess,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiscussionHandoffGateRevision(NonZeroU64);

impl DiscussionHandoffGateRevision {
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    pub fn new(value: u64) -> Result<Self, SyndicValueError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(SyndicValueError::ZeroOrdinal {
                kind: "discussion handoff gate revision",
            })
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub fn checked_next(self) -> Result<Self, SyndicValueError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(SyndicValueError::OrdinalExhausted {
                kind: "discussion handoff gate revision",
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionHandoffGateState {
    Open,
    Pending {
        intent_id: ResolutionIntentId,
        job_id: JobId,
        resolving_turn_id: SyndicTurnId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscussionHandoffGateRecord {
    thread_id: SyndicThreadId,
    revision: DiscussionHandoffGateRevision,
    state: DiscussionHandoffGateState,
}

impl DiscussionHandoffGateRecord {
    pub const fn new(
        thread_id: SyndicThreadId,
        revision: DiscussionHandoffGateRevision,
        state: DiscussionHandoffGateState,
    ) -> Self {
        Self {
            thread_id,
            revision,
            state,
        }
    }

    pub const fn open(thread_id: SyndicThreadId) -> Self {
        Self::new(
            thread_id,
            DiscussionHandoffGateRevision::FIRST,
            DiscussionHandoffGateState::Open,
        )
    }

    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn revision(self) -> DiscussionHandoffGateRevision {
        self.revision
    }
    pub const fn state(self) -> DiscussionHandoffGateState {
        self.state
    }
}

impl SyndicStorage {
    pub fn discussion_handoff_gate(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<DiscussionHandoffGateRecord>, SyndicReadError> {
        checked_gate(
            thread,
            self.point_with_access::<DiscussionHandoffGatesFamily>(
                ReadAccess::Ordinary(store),
                thread,
                limit,
            )?,
        )
    }

    pub fn discussion_handoff_gate_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        thread: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<DiscussionHandoffGateRecord>, SyndicReadError> {
        checked_gate(
            thread,
            self.point_with_access::<DiscussionHandoffGatesFamily>(
                ReadAccess::Candidate(store),
                thread,
                limit,
            )?,
        )
    }
}

fn checked_gate(
    thread: SyndicThreadId,
    gate: Option<DiscussionHandoffGateRecord>,
) -> Result<Option<DiscussionHandoffGateRecord>, SyndicReadError> {
    if gate.is_some_and(|gate| gate.thread_id() != thread) {
        return Err(SyndicReadError::Invariant(
            "discussion gate key and value disagree",
        ));
    }
    Ok(gate)
}
