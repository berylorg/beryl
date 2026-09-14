use super::*;
use crate::{
    CompactionOperationState, ConversationParent, ExecutionSnapshotKind, ProviderOperationKind,
    TurnKind,
};

pub(super) fn validate(
    facts: &facts::RecoveryFacts,
    gate: &InputGateRecord,
    turn_id: SyndicTurnId,
) -> Result<(), DeliveryRecoveryClassificationError> {
    let Some(operation) = facts
        .compaction
        .as_ref()
        .filter(|operation| operation.state() == &CompactionOperationState::Finalizing)
    else {
        validate_blocking_turn(facts, turn_id)?;
        return Ok(());
    };
    let target = operation.target();
    let turn = required(
        facts.turn.as_ref(),
        "finalizing compaction provider turn is missing",
    )?;
    let state = required(
        facts.state.as_ref(),
        "finalizing compaction turn state is missing",
    )?;
    let snapshot = required(
        facts.snapshot.as_ref(),
        "finalizing compaction snapshot is missing",
    )?;
    let binding = required(
        facts.binding.as_ref(),
        "finalizing compaction binding is missing",
    )?;
    let active = required(
        facts.active_turn.as_ref(),
        "finalizing compaction CAS turn is missing",
    )?;
    let terminal = operation.terminal().ok_or_else(|| {
        DeliveryRecoveryClassificationError::Corruption(
            "finalizing compaction terminal is missing".into(),
        )
    })?;
    let observed = operation.cas_turn().ok_or_else(|| {
        DeliveryRecoveryClassificationError::Corruption(
            "finalizing compaction CAS observation is missing".into(),
        )
    })?;
    let BindingState::Valid(usable) = binding.binding().state() else {
        return corruption("finalizing compaction binding is not valid");
    };
    if operation.home_id() != facts.home_id
        || gate.live_steering_count() != 0
        || operation.id().thread_id() != gate.thread_id()
        || operation.id().provider_turn_id() != turn_id
        || gate.state().compaction_operation_nonce() != Some(operation.id().nonce())
        || target.thread_id() != gate.thread_id()
        || target.turn_id() != turn_id
        || turn.id() != turn_id
        || turn.origin_thread_id() != target.thread_id()
        || turn.parent() != ConversationParent::Root
        || turn.kind() != TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction)
        || state.turn_id() != turn_id
        || state.revision() != terminal.turn_state_revision()
        || state.end_status() != Some(terminal.status())
        || state.lifecycle() != terminal.status().lifecycle()
        || !state.lifecycle().is_proven_terminal()
        || snapshot.id() != target.snapshot_id()
        || snapshot.thread_id() != target.thread_id()
        || snapshot.active_turn_id() != turn_id
        || snapshot.binding_revision() != target.binding_revision()
        || snapshot.kind()
            != ExecutionSnapshotKind::ProviderOperation(ProviderOperationKind::ContextCompaction)
        || snapshot.cas_thread_id() != target.cas_thread_id()
        || snapshot.execution().runtime_id() != target.runtime_id()
        || snapshot.loaded_generation() != target.loaded_generation()
        || binding.binding().revision() != target.binding_revision()
        || binding.binding().selected_path() != snapshot.selected_path()
        || usable.cas_thread_id() != target.cas_thread_id()
        || usable.execution() != snapshot.execution()
        || usable.represented_prefix() != snapshot.represented_base_prefix()
        || usable.native_turn_count() != snapshot.represented_base_native_turn_count()
        || usable.tool_profile() != snapshot.tool_profile()
        || usable.lineage() != snapshot.lineage()
        || active.snapshot_id() != target.snapshot_id()
        || active.thread_id() != target.thread_id()
        || active.turn_id() != turn_id
        || active.binding_revision() != target.binding_revision()
        || active.cas_thread_id() != target.cas_thread_id()
        || active.cas_turn_id() != observed.cas_turn_id()
    {
        return corruption("finalizing compaction authority disagrees");
    }
    Ok(())
}
