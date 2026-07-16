mod item;
mod terminal;

use std::time::{SystemTime, UNIX_EPOCH};

use beryl_backend::TurnStreamEvent;
use beryl_home_store::HomeStore;
use beryl_model::{CasThreadId, CasTurnId, SyndicItemId};
use syndic_storage::{
    CasTurnSource, ContentReference, LiveSourceEvent, SourceEventPayload, SourceEventSequence,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnEndStatus, TurnIncompleteReason,
    TurnStateRevision,
};

use self::item::PendingDelta;
use super::{OrdinaryDynamicToolContext, OrdinaryDynamicToolHandler, OrdinaryTurnExecutionError};
use crate::cas_projection::{LiveEventTarget, publication};

pub(super) struct LiveCapture {
    context: OrdinaryDynamicToolContext,
    source: CasTurnSource,
    submitted_item_id: SyndicItemId,
    submitted_content: ContentReference,
    state_revision: TurnStateRevision,
    gate_revision: beryl_model::InputGateRevision,
    next_sequence: u64,
    minimum_observed_at: SyndicTimestamp,
    pending_delta: Option<PendingDelta>,
    incomplete_reason: Option<TurnIncompleteReason>,
}

impl LiveCapture {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        context: OrdinaryDynamicToolContext,
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        submitted_item_id: SyndicItemId,
        submitted_content: ContentReference,
        state_revision: TurnStateRevision,
        gate_revision: beryl_model::InputGateRevision,
        minimum_observed_at: SyndicTimestamp,
    ) -> Self {
        Self {
            context,
            source: CasTurnSource::new(cas_thread_id, cas_turn_id),
            submitted_item_id,
            submitted_content,
            state_revision,
            gate_revision,
            next_sequence: 1,
            minimum_observed_at,
            pending_delta: None,
            incomplete_reason: None,
        }
    }

    pub(super) fn activate(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        self.emit(store, storage, limit, SourceEventPayload::TurnActivated)
    }

    pub(super) const fn gate_revision(&self) -> beryl_model::InputGateRevision {
        self.gate_revision
    }

    pub(super) fn cas_turn_id(&self) -> &CasTurnId {
        self.source.turn_id()
    }

    pub(super) const fn minimum_observed_at(&self) -> SyndicTimestamp {
        self.minimum_observed_at
    }

    pub(super) fn flush_for_loss(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        self.flush_delta(store, storage, limit)
    }

    pub(super) fn close_incomplete_after_abandon(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
        reason: TurnIncompleteReason,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        self.gate_revision = next_gate_revision(self.gate_revision)?;
        self.emit_source_less_terminal(store, storage, limit, TurnEndStatus::incomplete(reason))
    }

    pub(super) fn note_incomplete(&mut self, reason: TurnIncompleteReason) {
        self.incomplete_reason.get_or_insert(reason);
    }

    pub(super) const fn incomplete_reason(&self) -> Option<TurnIncompleteReason> {
        self.incomplete_reason
    }

    pub(super) fn handle_event(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        target: &LiveEventTarget,
        event: TurnStreamEvent,
        tools: &mut impl OrdinaryDynamicToolHandler,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnEndStatus>, OrdinaryTurnExecutionError> {
        match event {
            TurnStreamEvent::TurnStarted { .. }
            | TurnStreamEvent::ThreadStarted { .. }
            | TurnStreamEvent::AgentLabelUpdated { .. }
            | TurnStreamEvent::ThreadStatusChanged { .. }
            | TurnStreamEvent::ThreadNameUpdated { .. }
            | TurnStreamEvent::TokenUsageUpdated { .. }
            | TurnStreamEvent::AccountRateLimitsUpdated { .. }
            | TurnStreamEvent::ApprovalRequested(_) => Ok(None),
            TurnStreamEvent::ItemStarted { item, .. } => {
                self.flush_delta(store, storage, limit)?;
                self.start_item(store, storage, &item, limit)?;
                Ok(None)
            }
            TurnStreamEvent::ItemDelta(delta) => {
                self.handle_delta(store, storage, delta, limit)?;
                Ok(None)
            }
            TurnStreamEvent::ItemCompleted { item, .. } => {
                self.flush_delta(store, storage, limit)?;
                self.complete_item(store, storage, &item, limit)?;
                Ok(None)
            }
            TurnStreamEvent::DynamicToolCallRequested(request) => {
                self.flush_delta(store, storage, limit)?;
                let response = tools.respond(self.context, &request);
                target.respond_dynamic_tool_call(&request, &response)?;
                Ok(None)
            }
            TurnStreamEvent::TurnCompleted { turn, .. } => {
                self.flush_delta(store, storage, limit)?;
                let status = self.terminal_status(store, storage, &turn, limit)?;
                self.emit(store, storage, limit, SourceEventPayload::TurnEnded(status))?;
                Ok(Some(status))
            }
            TurnStreamEvent::ThreadClosed { .. } => Err(OrdinaryTurnExecutionError::Invariant(
                "thread closure reached a live target before terminal handoff",
            )),
            TurnStreamEvent::ProtocolError { .. } => Err(OrdinaryTurnExecutionError::Invariant(
                "protocol error reached a live target instead of retiring its connection",
            )),
        }
    }

    fn emit(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
        payload: SourceEventPayload,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        let terminal = matches!(payload, SourceEventPayload::TurnEnded(_));
        let event = self.source_event(Some(self.source.clone()), payload)?;
        let observed_at = event.observed_at();
        publication::admit_live_event(store, storage, &event, limit)?;
        self.advance_after_event(terminal, observed_at)
    }

    fn emit_source_less_terminal(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        limit: SyndicPointReadLimit,
        status: TurnEndStatus,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        let event = self.source_event(None, SourceEventPayload::TurnEnded(status))?;
        let observed_at = event.observed_at();
        publication::admit_live_event(store, storage, &event, limit)?;
        self.advance_after_event(true, observed_at)
    }

    fn source_event(
        &self,
        source: Option<CasTurnSource>,
        payload: SourceEventPayload,
    ) -> Result<LiveSourceEvent, OrdinaryTurnExecutionError> {
        let sequence = SourceEventSequence::new(self.next_sequence).map_err(|_| {
            OrdinaryTurnExecutionError::Invariant("source-event sequence exhausted")
        })?;
        let observed_at = system_timestamp_at_least(self.minimum_observed_at)?;
        Ok(LiveSourceEvent::new(
            self.context.thread_id(),
            self.context.turn_id(),
            self.state_revision,
            self.gate_revision,
            sequence,
            source,
            payload,
            observed_at,
        )?)
    }

    fn advance_after_event(
        &mut self,
        terminal: bool,
        observed_at: SyndicTimestamp,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        self.state_revision = self
            .state_revision
            .checked_next()
            .map_err(|_| OrdinaryTurnExecutionError::Invariant("turn-state revision exhausted"))?;
        if terminal {
            self.gate_revision = next_gate_revision(self.gate_revision)?;
        }
        self.next_sequence =
            self.next_sequence
                .checked_add(1)
                .ok_or(OrdinaryTurnExecutionError::Invariant(
                    "source-event sequence exhausted",
                ))?;
        self.minimum_observed_at = observed_at;
        Ok(())
    }
}

fn next_gate_revision(
    revision: beryl_model::InputGateRevision,
) -> Result<beryl_model::InputGateRevision, OrdinaryTurnExecutionError> {
    revision
        .checked_next()
        .map_err(|_| OrdinaryTurnExecutionError::Invariant("input-gate revision exhausted"))
}

pub(super) fn system_timestamp_at_least(
    minimum: SyndicTimestamp,
) -> Result<SyndicTimestamp, OrdinaryTurnExecutionError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(OrdinaryTurnExecutionError::SystemClockBeforeUnixEpoch)?;
    let millis = u64::try_from(elapsed.as_millis())
        .map_err(|_| OrdinaryTurnExecutionError::SystemClockOutOfRange)?;
    Ok(SyndicTimestamp::from_unix_millis(millis).max(minimum))
}
