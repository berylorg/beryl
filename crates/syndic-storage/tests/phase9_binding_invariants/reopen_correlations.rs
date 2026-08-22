use super::*;

use crate::support::semantic::exercise_seeded_populated_case;

fn populated_active_binding(store: &HomeStore, storage: SyndicStorage) -> BindingRecord {
    let binding = storage
        .current_binding(store, id(40), point_limit())
        .unwrap()
        .expect("populated fixture has an active binding")
        .binding()
        .clone();
    assert!(matches!(binding.state(), BindingState::Active(_)));
    binding
}

fn populated_execution_snapshot(
    store: &HomeStore,
    storage: SyndicStorage,
) -> ExecutionSnapshotRecord {
    let binding = populated_active_binding(store, storage);
    let BindingState::Active(active) = binding.state() else {
        unreachable!();
    };
    storage
        .execution_snapshot(store, active.snapshot_id(), point_limit())
        .unwrap()
        .expect("populated fixture has an execution snapshot")
}

fn populated_active_cas_turn(store: &HomeStore, storage: SyndicStorage) -> ActiveCasTurnRecord {
    let snapshot = populated_execution_snapshot(store, storage);
    let active = storage
        .active_cas_turn(store, snapshot.id(), point_limit())
        .unwrap()
        .expect("populated fixture has an active CAS turn");
    assert_eq!(active.turn_id(), support::populated::active_turn());
    active
}

fn populated_cas_turn_index(store: &HomeStore, storage: SyndicStorage) -> CasTurnIndexRecord {
    let active = populated_active_cas_turn(store, storage);
    storage
        .cas_turn_owner(
            store,
            active.cas_thread_id().clone(),
            active.cas_turn_id().clone(),
            point_limit(),
        )
        .unwrap()
        .expect("populated fixture has a CAS-turn index")
}

fn populated_cas_thread_index(store: &HomeStore, storage: SyndicStorage) -> CasThreadIndexRecord {
    let active = populated_active_cas_turn(store, storage);
    storage
        .cas_thread_owner(store, active.cas_thread_id().clone(), point_limit())
        .unwrap()
        .expect("populated fixture has a CAS-thread index")
}

fn populated_active_turn_state(store: &HomeStore, storage: SyndicStorage) -> TurnStateRecord {
    let active = populated_active_cas_turn(store, storage);
    storage
        .turn_state(store, active.turn_id(), point_limit())
        .unwrap()
        .expect("populated fixture has its active turn state")
}

fn populated_active_transcript_path(
    store: &HomeStore,
    storage: SyndicStorage,
) -> TranscriptPathTurnRecord {
    let active = populated_active_cas_turn(store, storage);
    let head = storage
        .transcript_view_head(store, active.thread_id(), point_limit())
        .unwrap()
        .expect("populated fixture has its active transcript head");
    let paths = storage
        .transcript_path_turns(
            store,
            active.thread_id(),
            head.generation(),
            None,
            CursorReadLimits::new(4, 1_000_000).unwrap(),
        )
        .unwrap();
    paths
        .records()
        .iter()
        .find(|path| path.turn_id() == active.turn_id())
        .copied()
        .expect("populated fixture has its active transcript path")
}

#[test]
fn reopen_rejects_malformed_binding_snapshot_and_cas_turn_correlations() {
    exercise_seeded_populated_case(
        "phase9-binding-snapshot-link",
        "active binding snapshot is missing",
        |store, storage| {
            let binding = populated_active_binding(store, storage);
            let BindingState::Active(active) = binding.state() else {
                unreachable!();
            };
            let corrupt = ActiveCasBinding::new(
                active.usable().clone(),
                SyndicExecutionSnapshotId::from_bytes([99; 16]),
                active.turn_id(),
                active.activation_gate_revision(),
                active.started_at(),
            );
            batch([FixtureRecord::Binding(BindingRecord::new(
                binding.thread_id(),
                binding.revision(),
                binding.selected_path(),
                BindingState::active(corrupt),
            ))])
        },
    );

    exercise_seeded_populated_case(
        "phase9-snapshot-binding-facts",
        "active binding and execution snapshot disagree",
        |store, storage| {
            let snapshot = populated_execution_snapshot(store, storage);
            let later_start = SyndicTimestamp::from_unix_millis(
                snapshot.started_at().unix_millis().checked_add(1).unwrap(),
            );
            batch([FixtureRecord::ExecutionSnapshot(
                ExecutionSnapshotRecord::new(
                    snapshot.id(),
                    snapshot.thread_id(),
                    snapshot.binding_revision(),
                    snapshot.activation_gate_revision(),
                    snapshot.active_turn_id(),
                    snapshot.cas_thread_id().clone(),
                    snapshot.selected_path(),
                    snapshot.represented_base_prefix(),
                    snapshot.represented_base_native_turn_count(),
                    snapshot.tool_profile(),
                    snapshot.lineage(),
                    snapshot.execution().clone(),
                    snapshot.loaded_generation(),
                    later_start,
                ),
            )])
        },
    );

    exercise_seeded_populated_case(
        "phase10-snapshot-native-count",
        "active binding and execution snapshot disagree",
        |store, storage| {
            let snapshot = populated_execution_snapshot(store, storage);
            batch([FixtureRecord::ExecutionSnapshot(
                ExecutionSnapshotRecord::new(
                    snapshot.id(),
                    snapshot.thread_id(),
                    snapshot.binding_revision(),
                    snapshot.activation_gate_revision(),
                    snapshot.active_turn_id(),
                    snapshot.cas_thread_id().clone(),
                    snapshot.selected_path(),
                    snapshot.represented_base_prefix(),
                    beryl_model::CasNativeTurnCount::new(1),
                    snapshot.tool_profile(),
                    snapshot.lineage(),
                    snapshot.execution().clone(),
                    snapshot.loaded_generation(),
                    snapshot.started_at(),
                ),
            )])
        },
    );
}

#[test]
fn reopen_rejects_malformed_tool_profile_and_cas_turn_correlations() {
    exercise_seeded_populated_case(
        "phase10-snapshot-tool-profile",
        "active binding and execution snapshot disagree",
        |store, storage| {
            let snapshot = populated_execution_snapshot(store, storage);
            batch([FixtureRecord::ExecutionSnapshot(
                ExecutionSnapshotRecord::new(
                    snapshot.id(),
                    snapshot.thread_id(),
                    snapshot.binding_revision(),
                    snapshot.activation_gate_revision(),
                    snapshot.active_turn_id(),
                    snapshot.cas_thread_id().clone(),
                    snapshot.selected_path(),
                    snapshot.represented_base_prefix(),
                    snapshot.represented_base_native_turn_count(),
                    beryl_model::CasConversationToolProfile::v1([0x4d; 32]),
                    snapshot.lineage(),
                    snapshot.execution().clone(),
                    snapshot.loaded_generation(),
                    snapshot.started_at(),
                ),
            )])
        },
    );

    exercise_seeded_populated_case(
        "phase9-cas-turn-pre-start",
        "active CAS-turn and immutable snapshot disagree",
        |store, storage| {
            let active = populated_active_cas_turn(store, storage);
            let snapshot = populated_execution_snapshot(store, storage);
            let earlier = SyndicTimestamp::from_unix_millis(
                snapshot.started_at().unix_millis().checked_sub(1).unwrap(),
            );
            batch([FixtureRecord::ActiveCasTurn(ActiveCasTurnRecord::new(
                active.snapshot_id(),
                active.thread_id(),
                active.turn_id(),
                active.binding_revision(),
                active.cas_thread_id().clone(),
                active.cas_turn_id().clone(),
                earlier,
            ))])
        },
    );

    exercise_seeded_populated_case(
        "phase9-cas-turn-reverse-link",
        "active CAS-turn primary and index disagree",
        |store, storage| {
            let index = populated_cas_turn_index(store, storage);
            batch([FixtureRecord::CasTurn(CasTurnIndexRecord::new(
                index.cas_thread_id().clone(),
                index.cas_turn_id().clone(),
                index.thread_id(),
                index.turn_id(),
                BindingRevision::new(1).unwrap(),
                index.snapshot_id(),
                index.post_turn_native_count(),
            ))])
        },
    );
}

#[test]
fn reopen_rejects_malformed_native_count_and_predecessor_correlations() {
    exercise_seeded_populated_case(
        "phase10-cas-turn-native-count",
        "active CAS-turn primary and index disagree",
        |store, storage| {
            let index = populated_cas_turn_index(store, storage);
            batch([FixtureRecord::CasTurn(CasTurnIndexRecord::new(
                index.cas_thread_id().clone(),
                index.cas_turn_id().clone(),
                index.thread_id(),
                index.turn_id(),
                index.binding_revision(),
                index.snapshot_id(),
                beryl_model::CasNativeTurnCount::new(2),
            ))])
        },
    );

    exercise_seeded_populated_case(
        "phase9-active-predecessor-state",
        "active binding does not succeed a valid binding",
        |store, storage| {
            let active = populated_active_binding(store, storage);
            let prior = BindingRevision::new(active.revision().get() - 1).unwrap();
            let index = populated_cas_thread_index(store, storage);
            let mut corruption = FixtureBatch::new();
            corruption
                .put(FixtureRecord::Binding(BindingRecord::new(
                    active.thread_id(),
                    prior,
                    active.selected_path(),
                    BindingState::unbound("corrupt active predecessor").unwrap(),
                )))
                .unwrap();
            corruption
                .delete(FixtureDelete::CasThreadBinding {
                    thread: index.cas_thread_id().clone(),
                    revision: prior,
                })
                .unwrap();
            corruption
                .put(FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
                    index.cas_thread_id().clone(),
                    index.thread_id(),
                    active.revision(),
                    index.latest_binding_revision(),
                )))
                .unwrap();
            corruption
        },
    );

    exercise_seeded_populated_case(
        "phase9-active-predecessor-authority",
        "active binding does not preserve compatible prior valid authority",
        |store, storage| {
            let active = populated_active_binding(store, storage);
            let BindingState::Active(current) = active.state() else {
                unreachable!();
            };
            let prior = BindingRevision::new(active.revision().get() - 1).unwrap();
            let corrupt = UsableCasBinding::new(
                current.usable().execution().clone(),
                current.usable().cas_thread_id().clone(),
                current.usable().represented_prefix(),
                current.usable().native_turn_count(),
                beryl_model::CasConversationToolProfile::v1([0x4e; 32]),
                current.usable().lineage(),
            );
            batch([FixtureRecord::Binding(BindingRecord::new(
                active.thread_id(),
                prior,
                active.selected_path(),
                BindingState::valid(corrupt),
            ))])
        },
    );
}

#[test]
fn reopen_rejects_malformed_membership_and_source_correlations() {
    exercise_seeded_populated_case(
        "phase9-cas-thread-latest-rewind",
        "CAS thread reservation owner or revision range disagrees",
        |store, storage| {
            let index = populated_cas_thread_index(store, storage);
            batch([FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
                index.cas_thread_id().clone(),
                index.thread_id(),
                index.first_binding_revision(),
                index.first_binding_revision(),
            ))])
        },
    );

    exercise_seeded_populated_case(
        "phase9-extra-cas-thread-membership",
        "CAS thread binding membership names a missing binding",
        |store, storage| {
            let index = populated_cas_thread_index(store, storage);
            batch([FixtureRecord::CasThreadBinding(
                CasThreadBindingIndexRecord::new(
                    index.cas_thread_id().clone(),
                    index.thread_id(),
                    BindingRevision::new(index.latest_binding_revision().get() + 1).unwrap(),
                ),
            )])
        },
    );

    exercise_seeded_populated_case(
        "phase9-wrong-source-active-history",
        "active source event lacks exact CAS-turn authority",
        |store, storage| {
            let state = populated_active_turn_state(store, storage);
            let path = populated_active_transcript_path(store, storage);
            let active = populated_active_cas_turn(store, storage);
            let activity_head = storage
                .activity_query_head(store, active.thread_id(), point_limit())
                .unwrap()
                .expect("populated fixture has an activity head");
            let activity_sources = storage
                .activity_query_source_page(
                    store,
                    &activity_head,
                    None,
                    CursorReadLimits::new(4, 1_000_000).unwrap(),
                )
                .unwrap();
            let activity_source = activity_sources
                .records()
                .iter()
                .find(|source| source.source().turn_id() == state.turn_id())
                .cloned()
                .expect("populated fixture has its active activity source");
            let wrong_thread = CasThreadId::new("wrong-history-thread").unwrap();
            let wrong_turn = CasTurnId::new("wrong-history-turn").unwrap();
            batch([
                FixtureRecord::SourceEvent(
                    SourceEventRecord::new(
                        state.turn_id(),
                        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
                        Some(CasTurnSource::new(wrong_thread.clone(), wrong_turn.clone())),
                        SourceEventPayload::TurnActivated,
                    )
                    .unwrap(),
                ),
                FixtureRecord::TurnState(fixture_turn_state_with_capture(
                    state.turn_id(),
                    state.revision(),
                    state.lifecycle(),
                    state.source_event_count() + 1,
                    state.item_count(),
                    state.finalized_item_count(),
                    state.open_item_count(),
                    state.history_blocking_item_count(),
                    state.updated_at(),
                )),
                FixtureRecord::ActivityQueryHead(
                    ActivityQueryHeadRecord::new(
                        active.thread_id(),
                        ActivityWorkPeriod::FIRST,
                        Some(ActivityQuerySource::new(
                            active.thread_id(),
                            state.turn_id(),
                        )),
                        true,
                        state.source_event_count() + 1,
                        activity_head.revision(),
                        activity_head.source_count(),
                        activity_head.logical_row_count(),
                        activity_head.running_row_count(),
                        activity_head.completed_row_count(),
                        activity_head.completed_stored_bytes(),
                        activity_head.completed_retention_cutoff(),
                        activity_head.lifecycle(),
                    )
                    .unwrap(),
                ),
                FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
                    active.thread_id(),
                    ActivityWorkPeriod::FIRST,
                    ActivityQuerySource::new(active.thread_id(), state.turn_id()),
                    activity_source.activity_start(),
                    state.source_event_count() + 1,
                    true,
                    activity_source.child_handoff(),
                )),
                FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
                    path.thread_id(),
                    path.generation(),
                    path.depth(),
                    path.turn_id(),
                    path.turn_path_digest(),
                    path.state_revision(),
                    path.lifecycle(),
                    path.source_event_count() + 1,
                    path.item_count(),
                    path.finalized_item_count(),
                    path.updated_at(),
                )),
                FixtureRecord::CasTurn(CasTurnIndexRecord::new(
                    wrong_thread,
                    wrong_turn,
                    active.thread_id(),
                    active.turn_id(),
                    active.binding_revision(),
                    active.snapshot_id(),
                    beryl_model::CasNativeTurnCount::new(1),
                )),
            ])
        },
    );
}
