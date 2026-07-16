use super::*;

fn populated_active_binding() -> BindingRecord {
    populated_records()
        .into_iter()
        .find_map(|record| match record {
            FixtureRecord::Binding(binding)
                if matches!(binding.state(), BindingState::Active(_)) =>
            {
                Some(binding)
            }
            _ => None,
        })
        .expect("populated fixture has an active binding")
}

fn populated_execution_snapshot() -> ExecutionSnapshotRecord {
    populated_records()
        .into_iter()
        .find_map(|record| match record {
            FixtureRecord::ExecutionSnapshot(snapshot)
                if snapshot.id() == populated_active_snapshot() =>
            {
                Some(snapshot)
            }
            _ => None,
        })
        .expect("populated fixture has an execution snapshot")
}

fn populated_active_cas_turn() -> ActiveCasTurnRecord {
    populated_records()
        .into_iter()
        .find_map(|record| match record {
            FixtureRecord::ActiveCasTurn(active)
                if active.turn_id() == support::populated::active_turn() =>
            {
                Some(active)
            }
            _ => None,
        })
        .expect("populated fixture has an active CAS turn")
}

fn populated_cas_turn_index() -> CasTurnIndexRecord {
    populated_records()
        .into_iter()
        .find_map(|record| match record {
            FixtureRecord::CasTurn(index) => Some(index),
            _ => None,
        })
        .expect("populated fixture has a CAS-turn index")
}

fn populated_cas_thread_index() -> CasThreadIndexRecord {
    populated_records()
        .into_iter()
        .find_map(|record| match record {
            FixtureRecord::CasThread(index) => Some(index),
            _ => None,
        })
        .expect("populated fixture has a CAS-thread index")
}

fn populated_active_turn_state() -> TurnStateRecord {
    populated_records()
        .into_iter()
        .find_map(|record| match record {
            FixtureRecord::TurnState(state)
                if state.turn_id() == support::populated::active_turn() =>
            {
                Some(state)
            }
            _ => None,
        })
        .expect("populated fixture has its active turn state")
}

fn populated_active_transcript_path() -> TranscriptPathTurnRecord {
    populated_records()
        .into_iter()
        .find_map(|record| match record {
            FixtureRecord::TranscriptPathTurn(path)
                if path.turn_id() == support::populated::active_turn() =>
            {
                Some(path)
            }
            _ => None,
        })
        .expect("populated fixture has its active transcript path")
}

#[test]
fn reopen_rejects_malformed_binding_snapshot_and_cas_turn_correlations() {
    exercise_case(
        "phase9-binding-snapshot-link",
        "active binding snapshot is missing",
        || batch(populated_records()),
        || {
            let binding = populated_active_binding();
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

    exercise_case(
        "phase9-snapshot-binding-facts",
        "active binding and execution snapshot disagree",
        || batch(populated_records()),
        || {
            let snapshot = populated_execution_snapshot();
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

    exercise_case(
        "phase10-snapshot-native-count",
        "active binding and execution snapshot disagree",
        || batch(populated_records()),
        || {
            let snapshot = populated_execution_snapshot();
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

    exercise_case(
        "phase10-snapshot-tool-profile",
        "active binding and execution snapshot disagree",
        || batch(populated_records()),
        || {
            let snapshot = populated_execution_snapshot();
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

    exercise_case(
        "phase9-cas-turn-pre-start",
        "active CAS-turn and immutable snapshot disagree",
        || batch(populated_records()),
        || {
            let active = populated_active_cas_turn();
            let snapshot = populated_execution_snapshot();
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

    exercise_case(
        "phase9-cas-turn-reverse-link",
        "active CAS-turn primary and index disagree",
        || batch(populated_records()),
        || {
            let index = populated_cas_turn_index();
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

    exercise_case(
        "phase10-cas-turn-native-count",
        "active CAS-turn primary and index disagree",
        || batch(populated_records()),
        || {
            let index = populated_cas_turn_index();
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

    exercise_case(
        "phase9-active-predecessor-state",
        "active binding does not succeed a valid binding",
        || batch(populated_records()),
        || {
            let active = populated_active_binding();
            let prior = BindingRevision::new(active.revision().get() - 1).unwrap();
            let index = populated_cas_thread_index();
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

    exercise_case(
        "phase9-active-predecessor-authority",
        "active binding does not preserve exact prior valid authority",
        || batch(populated_records()),
        || {
            let active = populated_active_binding();
            let BindingState::Active(current) = active.state() else {
                unreachable!();
            };
            let prior = BindingRevision::new(active.revision().get() - 1).unwrap();
            let different_execution = ExecutionBinding::new(
                RuntimeId::from_bytes([94; 16]),
                RootId::from_bytes([95; 16]),
                RuntimeNativePath::from_admitted(
                    RuntimeMode::host(),
                    PathFlavor::Windows,
                    "C:\\corrupt-active-predecessor",
                )
                .unwrap(),
            );
            let corrupt = UsableCasBinding::new(
                different_execution,
                current.usable().cas_thread_id().clone(),
                current.usable().represented_prefix(),
                current.usable().native_turn_count(),
                current.usable().tool_profile(),
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

    exercise_case(
        "phase9-cas-thread-latest-rewind",
        "CAS thread reservation owner or revision range disagrees",
        || batch(populated_records()),
        || {
            let index = populated_cas_thread_index();
            batch([FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
                index.cas_thread_id().clone(),
                index.thread_id(),
                index.first_binding_revision(),
                index.first_binding_revision(),
            ))])
        },
    );

    exercise_case(
        "phase9-extra-cas-thread-membership",
        "CAS thread binding membership names a missing binding",
        || batch(populated_records()),
        || {
            let index = populated_cas_thread_index();
            batch([FixtureRecord::CasThreadBinding(
                CasThreadBindingIndexRecord::new(
                    index.cas_thread_id().clone(),
                    index.thread_id(),
                    BindingRevision::new(index.latest_binding_revision().get() + 1).unwrap(),
                ),
            )])
        },
    );

    exercise_case(
        "phase9-wrong-source-active-history",
        "active source event lacks exact CAS-turn authority",
        || batch(populated_records()),
        || {
            let state = populated_active_turn_state();
            let path = populated_active_transcript_path();
            let active = populated_active_cas_turn();
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

    assert_eq!(
        populated_execution_snapshot().id(),
        populated_active_snapshot()
    );
}
