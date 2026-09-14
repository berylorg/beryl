use beryl_model::{SyndicThreadId, SyndicTurnId};

use crate::{
    BindingRecord, BindingState, ExecutionSnapshotKind, ExecutionSnapshotRecord, TurnDispatchAnchor,
};

pub(crate) fn activation_matches(
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    anchor: TurnDispatchAnchor,
    snapshot: &ExecutionSnapshotRecord,
    binding: &BindingRecord,
) -> bool {
    let BindingState::Active(active) = binding.state() else {
        return false;
    };
    binding.thread_id() == thread_id
        && binding.revision() == anchor.binding_revision()
        && binding.selected_path().tail() == Some(turn_id)
        && active.turn_id() == turn_id
        && active.snapshot_id() == anchor.snapshot_id()
        && snapshot.kind() == ExecutionSnapshotKind::OrdinaryConversation
        && snapshot.id() == anchor.snapshot_id()
        && snapshot.thread_id() == thread_id
        && snapshot.binding_revision() == binding.revision()
        && snapshot.active_turn_id() == turn_id
        && snapshot.activation_gate_revision() == active.activation_gate_revision()
        && snapshot.selected_path() == binding.selected_path()
        && snapshot.cas_thread_id() == active.usable().cas_thread_id()
        && snapshot.execution() == active.usable().execution()
        && snapshot.tool_profile() == active.usable().tool_profile()
        && snapshot.lineage() == active.usable().lineage()
        && snapshot.represented_base_prefix() == active.usable().represented_prefix()
        && snapshot.represented_base_native_turn_count() == active.usable().native_turn_count()
        && snapshot.started_at() == active.started_at()
}

pub(crate) fn cancelled_successor_matches(
    active: &BindingRecord,
    successor: &BindingRecord,
) -> bool {
    let (BindingState::Active(active_authority), BindingState::Valid(usable)) =
        (active.state(), successor.state())
    else {
        return false;
    };
    active.thread_id() == successor.thread_id()
        && active.revision().checked_next().ok() == Some(successor.revision())
        && active.selected_path() == successor.selected_path()
        && active_authority.usable() == usable
}
