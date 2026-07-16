use beryl_home_store::{
    CurrentDomainCommand, DomainMutation, DomainReader, MutationBuilder, MutationContribution,
};
use beryl_model::{DomainRevision, InputGateRevision, SyndicItemId, SyndicThreadId, SyndicTurnId};

use crate::{
    CasTurnSource, SourceEventPayload, SourceEventRecord, SourceEventSequence, SyndicMutationError,
    SyndicStorage, SyndicTimestamp, TurnItemOrdinal, TurnStateRevision, codec::*,
    domain::SyndicDomain,
};

mod event;
mod finalize;
mod freeze;
mod terminal;

/// Exact revisions and normalized payload for one monotonic live-source event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveSourceEvent {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    expected_state_revision: TurnStateRevision,
    expected_gate_revision: InputGateRevision,
    sequence: SourceEventSequence,
    source: Option<CasTurnSource>,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
}

impl LiveSourceEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        expected_state_revision: TurnStateRevision,
        expected_gate_revision: InputGateRevision,
        sequence: SourceEventSequence,
        source: Option<CasTurnSource>,
        payload: SourceEventPayload,
        observed_at: SyndicTimestamp,
    ) -> Result<Self, crate::SyndicRecordError> {
        SourceEventRecord::new(turn_id, sequence, source.clone(), payload.clone())?;
        Ok(Self {
            thread_id,
            turn_id,
            expected_state_revision,
            expected_gate_revision,
            sequence,
            source,
            payload,
            observed_at,
        })
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    #[must_use]
    pub const fn sequence(&self) -> SourceEventSequence {
        self.sequence
    }

    #[must_use]
    pub const fn payload(&self) -> &SourceEventPayload {
        &self.payload
    }

    #[must_use]
    pub const fn source(&self) -> Option<&CasTurnSource> {
        self.source.as_ref()
    }

    #[must_use]
    pub const fn expected_state_revision(&self) -> TurnStateRevision {
        self.expected_state_revision
    }

    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.observed_at
    }
}

/// Exact result of reconciling one immutable normalized source event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveSourceEventStatus {
    Absent,
    Exact,
    Collision,
}

/// Exact next canonical item whose terminal finalization frontier may advance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizeNextTurnItem {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    expected_state_revision: TurnStateRevision,
    expected_item_ordinal: TurnItemOrdinal,
    expected_item_id: SyndicItemId,
    updated_at: SyndicTimestamp,
}

/// Exact next terminal item whose closed canonical source must become immutable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FreezeNextTurnItem {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    expected_state_revision: TurnStateRevision,
    expected_item_ordinal: TurnItemOrdinal,
    expected_item_id: SyndicItemId,
    updated_at: SyndicTimestamp,
}

impl FreezeNextTurnItem {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        expected_state_revision: TurnStateRevision,
        expected_item_ordinal: TurnItemOrdinal,
        expected_item_id: SyndicItemId,
        updated_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            expected_state_revision,
            expected_item_ordinal,
            expected_item_id,
            updated_at,
        }
    }
}

impl FinalizeNextTurnItem {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        expected_state_revision: TurnStateRevision,
        expected_item_ordinal: TurnItemOrdinal,
        expected_item_id: SyndicItemId,
        updated_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            expected_state_revision,
            expected_item_ordinal,
            expected_item_id,
            updated_at,
        }
    }
}

impl SyndicStorage {
    /// Commits one exact source event after the home writer captures current physical revisions.
    #[must_use]
    pub fn current_admit_live_source_event(&self, event: LiveSourceEvent) -> CurrentDomainCommand {
        self.handle
            .current_command(LiveSourceEventMutation { event })
    }

    /// Commits one exact source event and its canonical/lifecycle effects atomically.
    #[must_use]
    pub fn admit_live_source_event(
        &self,
        expected_domain_revision: DomainRevision,
        event: LiveSourceEvent,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, LiveSourceEventMutation { event })
    }

    /// Finalizes or advances exactly one terminal turn-item frontier entry.
    #[must_use]
    pub fn finalize_next_turn_item(
        &self,
        expected_domain_revision: DomainRevision,
        request: FinalizeNextTurnItem,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            FinalizeNextTurnItemMutation { request },
        )
    }

    #[must_use]
    pub fn current_finalize_next_turn_item(
        &self,
        request: FinalizeNextTurnItem,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(FinalizeNextTurnItemMutation { request })
    }

    /// Freezes one terminal item's canonical source without claiming projection completion.
    #[must_use]
    pub fn freeze_next_turn_item(
        &self,
        expected_domain_revision: DomainRevision,
        request: FreezeNextTurnItem,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            FreezeNextTurnItemMutation { request },
        )
    }

    #[must_use]
    pub fn current_freeze_next_turn_item(
        &self,
        request: FreezeNextTurnItem,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(FreezeNextTurnItemMutation { request })
    }
}

struct LiveSourceEventMutation {
    event: LiveSourceEvent,
}

struct FinalizeNextTurnItemMutation {
    request: FinalizeNextTurnItem,
}

pub(super) struct FreezeNextTurnItemMutation {
    request: FreezeNextTurnItem,
}

impl DomainMutation<SyndicDomain> for LiveSourceEventMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

impl DomainMutation<SyndicDomain> for FinalizeNextTurnItemMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

impl DomainMutation<SyndicDomain> for FreezeNextTurnItemMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}
