use std::time::{SystemTime, UNIX_EPOCH};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, CasThreadId, CasTurnId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CasTurnSource, LiveSourceEvent, LiveSourceEventStatus, SourceEventPayload, SourceEventSequence,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnStateRecord,
};
use thiserror::Error;

use super::{ProjectionPublicationFailure, publication};

#[derive(Debug, Error)]
pub(super) enum LiveSourcePublicationError {
    #[error("the live-source publication lost exact verification authority: {0}")]
    Authority(#[source] crate::cas_projection::LiveCommandAdmissionError),
    #[error(transparent)]
    Read(#[from] syndic_storage::SyndicReadError),
    #[error(transparent)]
    Record(#[from] syndic_storage::SyndicRecordError),
    #[error(transparent)]
    Publication(#[from] ProjectionPublicationFailure),
    #[error("the live-source durable publication panicked and the home failed closed")]
    PublicationPanicked,
    #[error("the live-source durable target is unavailable or disagrees with its CAS route")]
    TargetMismatch,
    #[error("the live-source frontier changed during its bounded read")]
    ConcurrentChange,
    #[error("the live-source frontier is already terminal")]
    Terminal,
    #[error("the live-source sequence frontier is exhausted")]
    SequenceExhausted,
    #[error("the system clock precedes the Unix epoch during live-source publication")]
    SystemClockBeforeUnixEpoch(#[source] std::time::SystemTimeError),
    #[error("the system clock milliseconds exceed the durable timestamp range")]
    SystemClockOutOfRange,
}

impl LiveSourcePublicationError {
    pub(super) fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Authority(source) => Some(*source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LiveSourceTarget {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    source: Option<CasTurnSource>,
}

impl LiveSourceTarget {
    pub(super) const fn new_exact(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        source: CasTurnSource,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            source: Some(source),
        }
    }

    pub(super) const fn new_source_less(thread_id: SyndicThreadId, turn_id: SyndicTurnId) -> Self {
        Self {
            thread_id,
            turn_id,
            source: None,
        }
    }

    pub(super) fn resolve(
        store: &HomeStore,
        storage: SyndicStorage,
        expected_thread_id: SyndicThreadId,
        cas_thread_id: &CasThreadId,
        cas_turn_id: &CasTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, LiveSourcePublicationError> {
        let owner =
            storage.cas_turn_owner(store, cas_thread_id.clone(), cas_turn_id.clone(), limit)?;
        let confirmed =
            storage.cas_turn_owner(store, cas_thread_id.clone(), cas_turn_id.clone(), limit)?;
        if confirmed != owner {
            return Err(LiveSourcePublicationError::ConcurrentChange);
        }
        let owner = owner.ok_or(LiveSourcePublicationError::TargetMismatch)?;
        if owner.thread_id() != expected_thread_id
            || owner.cas_thread_id() != cas_thread_id
            || owner.cas_turn_id() != cas_turn_id
        {
            return Err(LiveSourcePublicationError::TargetMismatch);
        }
        Ok(Self {
            thread_id: expected_thread_id,
            turn_id: owner.turn_id(),
            source: Some(CasTurnSource::new(
                cas_thread_id.clone(),
                cas_turn_id.clone(),
            )),
        })
    }

    pub(super) const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    pub(super) const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    pub(super) const fn source(&self) -> &CasTurnSource {
        match &self.source {
            Some(source) => source,
            None => panic!("source-less publication target has no CAS source"),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LiveSourceFrontier {
    state: TurnStateRecord,
    gate_revision: beryl_model::InputGateRevision,
    sequence: SourceEventSequence,
    observed_at: SyndicTimestamp,
}

impl LiveSourceFrontier {
    pub(super) fn read(
        store: &HomeStore,
        storage: SyndicStorage,
        target: &LiveSourceTarget,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, LiveSourcePublicationError> {
        Self::read_at_least(
            store,
            storage,
            target,
            SyndicTimestamp::from_unix_millis(0),
            limit,
        )
    }

    pub(super) fn read_provider_current(
        store: &HomeStore,
        expected_home_id: BerylHomeId,
        expected_home_generation: HomeGeneration,
        storage: SyndicStorage,
        target: &LiveSourceTarget,
        limit: SyndicPointReadLimit,
        command: &crate::cas_projection::LiveCommandPermit,
    ) -> Result<Self, LiveSourcePublicationError> {
        loop {
            let verification = command
                .await_current_or_verification(store, expected_home_id, expected_home_generation)
                .map_err(LiveSourcePublicationError::Authority)?;
            let frontier = Self::read(store, storage, target, limit);
            let settlement = verification
                .settle_after_operation()
                .map_err(LiveSourcePublicationError::Authority)?;
            if settlement.verified_current() {
                continue;
            }
            return frontier;
        }
    }

    pub(super) fn read_at_least(
        store: &HomeStore,
        storage: SyndicStorage,
        target: &LiveSourceTarget,
        minimum_observed_at: SyndicTimestamp,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, LiveSourcePublicationError> {
        let state = storage.turn_state(store, target.turn_id, limit)?;
        let gate = storage.input_gate(store, target.thread_id, limit)?;
        let summary = storage.history_summary(store, target.thread_id, limit)?;
        let confirmed_state = storage.turn_state(store, target.turn_id, limit)?;
        let confirmed_gate = storage.input_gate(store, target.thread_id, limit)?;
        let confirmed_summary = storage.history_summary(store, target.thread_id, limit)?;
        if confirmed_state != state || confirmed_gate != gate || confirmed_summary != summary {
            return Err(LiveSourcePublicationError::ConcurrentChange);
        }
        let state = state.ok_or(LiveSourcePublicationError::TargetMismatch)?;
        let gate = gate.ok_or(LiveSourcePublicationError::TargetMismatch)?;
        let summary = summary.ok_or(LiveSourcePublicationError::TargetMismatch)?;
        if state.turn_id() != target.turn_id
            || summary.thread_id() != target.thread_id
            || summary.committed_tail() != Some(target.turn_id)
        {
            return Err(LiveSourcePublicationError::TargetMismatch);
        }
        if state.lifecycle().is_proven_terminal() {
            return Err(LiveSourcePublicationError::Terminal);
        }
        let sequence = state
            .source_event_count()
            .checked_add(1)
            .and_then(|value| SourceEventSequence::new(value).ok())
            .ok_or(LiveSourcePublicationError::SequenceExhausted)?;
        let minimum = state
            .updated_at()
            .max(summary.last_activity_at())
            .max(minimum_observed_at);
        Ok(Self {
            state,
            gate_revision: gate.revision(),
            sequence,
            observed_at: system_timestamp_at_least(minimum)?,
        })
    }

    pub(super) const fn state(&self) -> &TurnStateRecord {
        &self.state
    }

    pub(super) const fn sequence(&self) -> SourceEventSequence {
        self.sequence
    }

    pub(super) fn event(
        &self,
        target: &LiveSourceTarget,
        source: Option<CasTurnSource>,
        payload: SourceEventPayload,
    ) -> Result<LiveSourceEvent, LiveSourcePublicationError> {
        Ok(LiveSourceEvent::new(
            target.thread_id,
            target.turn_id,
            self.state.revision(),
            self.gate_revision,
            self.sequence,
            source,
            payload,
            self.observed_at,
        )?)
    }
}

pub(super) fn publish_reconciled(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    event: &LiveSourceEvent,
    limit: SyndicPointReadLimit,
) -> Result<(), LiveSourcePublicationError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        publication::admit_live_event_reconciled(
            store,
            expected_home_id,
            expected_home_generation,
            storage,
            event,
            limit,
        )
    }))
    .map_err(|_| LiveSourcePublicationError::PublicationPanicked)??;
    Ok(())
}

pub(super) fn publish_provider_reconciled(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    event: &LiveSourceEvent,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<(), LiveSourcePublicationError> {
    let verification = command
        .await_current_or_verification(store, expected_home_id, expected_home_generation)
        .map_err(LiveSourcePublicationError::Authority)?;
    let dispatch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.execute_current(storage.current_admit_live_source_event(event.clone()))
    }));
    verification
        .settle_after_operation()
        .map_err(LiveSourcePublicationError::Authority)?;
    let dispatch = dispatch.map_err(|_| LiveSourcePublicationError::PublicationPanicked)?;
    match read_provider_event_status(
        store,
        expected_home_id,
        expected_home_generation,
        storage,
        event,
        limit,
        command,
    )? {
        LiveSourceEventStatus::Exact => Ok(()),
        LiveSourceEventStatus::Absent => match dispatch {
            Ok(_) => Err(LiveSourcePublicationError::Publication(
                ProjectionPublicationFailure::Prior,
            )),
            Err(source) => Err(LiveSourcePublicationError::Publication(
                ProjectionPublicationFailure::Command(source),
            )),
        },
        LiveSourceEventStatus::Collision => Err(LiveSourcePublicationError::Publication(
            ProjectionPublicationFailure::Collision,
        )),
    }
}

fn read_provider_event_status(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    event: &LiveSourceEvent,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<LiveSourceEventStatus, LiveSourcePublicationError> {
    loop {
        let verification = command
            .await_current_or_verification(store, expected_home_id, expected_home_generation)
            .map_err(LiveSourcePublicationError::Authority)?;
        let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            storage.live_source_event_status(store, event, limit)
        }));
        let settlement = verification
            .settle_after_operation()
            .map_err(LiveSourcePublicationError::Authority)?;
        if settlement.verified_current() {
            continue;
        }
        return status
            .map_err(|_| LiveSourcePublicationError::PublicationPanicked)?
            .map_err(|source| {
                LiveSourcePublicationError::Publication(
                    ProjectionPublicationFailure::Reconciliation(source),
                )
            });
    }
}

fn system_timestamp_at_least(
    minimum: SyndicTimestamp,
) -> Result<SyndicTimestamp, LiveSourcePublicationError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(LiveSourcePublicationError::SystemClockBeforeUnixEpoch)?;
    let millis = u64::try_from(elapsed.as_millis())
        .map_err(|_| LiveSourcePublicationError::SystemClockOutOfRange)?;
    Ok(SyndicTimestamp::from_unix_millis(millis).max(minimum))
}
