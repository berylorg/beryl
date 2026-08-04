use crate::{BindingState, InputGateState, StopOperationTarget, TurnLifecycle};

use super::{
    DeliveryRecoveryClassificationError, SyndicLiveStopOperation, corruption, facts,
    minimum_timestamp, required,
};
use crate::read::stop::{StopObservation, observation_authenticates_record};

pub(in crate::read) fn classify(
    facts: &facts::RecoveryFacts,
    target: &StopOperationTarget,
    observed: &StopObservation,
) -> Result<SyndicLiveStopOperation, DeliveryRecoveryClassificationError> {
    let gate = required(
        facts.gate.as_ref(),
        "provider-stop recovery input gate is missing",
    )?;
    let record = required(
        facts.stop.as_ref(),
        "provider-stop recovery operation record is missing",
    )?;
    let turn = required(
        facts.turn.as_ref(),
        "provider-stop recovery turn header is missing",
    )?;
    let state = required(
        facts.state.as_ref(),
        "provider-stop recovery turn state is missing",
    )?;
    let summary = required(
        facts.summary.as_ref(),
        "provider-stop recovery history summary is missing",
    )?;
    let binding = required(
        facts.binding.as_ref(),
        "provider-stop recovery current binding is missing",
    )?;
    let InputGateState::Stopping {
        turn_id,
        operation_nonce,
    } = gate.state()
    else {
        return corruption("provider-stop classifier received a non-stopping gate");
    };
    let BindingState::Valid(_) = binding.binding().state() else {
        return corruption("provider-stop recovery does not retain its valid binding");
    };
    if *turn_id != target.turn_id()
        || record.id().nonce() != *operation_nonce
        || record.target() != target
        || !record.state().is_live()
        || !record.admission().is_provider_operation()
        || turn.id() != target.turn_id()
        || state.turn_id() != target.turn_id()
        || !matches!(
            state.lifecycle(),
            TurnLifecycle::Pending | TurnLifecycle::Active
        )
        || summary.thread_id() != target.thread_id()
        || binding.head().thread_id() != target.thread_id()
        || binding.binding().thread_id() != target.thread_id()
        || binding.binding().revision() != target.binding_revision()
        || observed.stop.as_ref() != Some(record)
        || observed.gate.as_ref() != Some(gate)
        || observed.turn.as_ref() != Some(turn)
        || observed.turn_state.as_ref() != Some(state)
        || observed.binding_head.as_ref() != Some(binding.head())
        || observed.binding.as_ref() != Some(binding.binding())
        || !observation_authenticates_record(observed)
    {
        return corruption("provider-stop recovery authority disagrees");
    }
    let snapshot = required(
        observed.snapshot.as_ref(),
        "provider-stop recovery execution snapshot is missing",
    )?;
    let active_turn = required(
        observed.active_turn.as_ref(),
        "provider-stop recovery CAS-turn publication is missing",
    )?;
    let minimum = minimum_timestamp(state, summary)
        .max(turn.submitted_at())
        .max(snapshot.started_at())
        .max(active_turn.published_at());
    Ok(SyndicLiveStopOperation {
        record: record.clone(),
        snapshot: snapshot.clone(),
        current_gate_revision: gate.revision(),
        current_state_revision: state.revision(),
        stopped_route: None,
        minimum_timestamp: minimum,
    })
}
