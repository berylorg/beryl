use super::*;

#[test]
fn prepared_acceptance_is_blocked_by_pending_and_archived_discussion() {
    for archive in [false, true] {
        let home = TestHome::new("discussion-acceptance-gate");
        let (store, storage, request) = seeded(&home, FaultController::new());
        let acceptance = prepare_acceptance(&store, &storage);
        let prepared = storage
            .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
            .unwrap();
        let pending = prepared.intent().new_gate();
        execute(&store, prepared);
        if archive {
            execute(
                &store,
                storage
                    .prepare_discussion_handoff(
                        &store,
                        DiscussionHandoffMutation::ReleaseAndArchive {
                            expected: pending,
                            attributes_revision: ThreadAttributesRevision::FIRST,
                            archived_at: timestamp(205),
                        },
                    )
                    .unwrap(),
            );
        }
        blocked(
            &store,
            storage.first_acceptance(storage.revision(&store).unwrap(), acceptance.clone()),
        );
        assert_eq!(
            storage
                .input_gate(&store, id(36), limit())
                .unwrap()
                .unwrap()
                .live_count(),
            0
        );
        if !archive {
            execute(
                &store,
                storage
                    .prepare_discussion_handoff(
                        &store,
                        DiscussionHandoffMutation::Release { expected: pending },
                    )
                    .unwrap(),
            );
            committed(
                &store,
                storage.first_acceptance(storage.revision(&store).unwrap(), acceptance),
            );
            assert_eq!(
                storage
                    .input_gate(&store, id(36), limit())
                    .unwrap()
                    .unwrap()
                    .live_steering_count(),
                1
            );
        }
        store.close().unwrap();
    }
}

#[test]
fn pending_handoff_allows_terminal_capture_and_preserves_reclassified_steering() {
    let home = TestHome::new("discussion-terminal-steering");
    let (store, storage, mut request) = seeded(&home, FaultController::new());
    accept_next(&store, &storage);
    let gate = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.live_steering_count(), 1);
    request.input_gate_revision = gate.revision();
    request.thread_revision = storage
        .thread(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .revision();
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let pending = prepared.intent().new_gate();
    execute(&store, prepared);
    crate::support::discussion_handoff::complete_resolving_turn(&store, &storage);
    let queued = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(queued.state(), &InputGateState::Idle);
    assert_eq!(queued.live_next_turn_count(), 1);
    assert_eq!(queued.live_steering_count(), 0);
    execute(
        &store,
        storage
            .prepare_discussion_handoff(
                &store,
                DiscussionHandoffMutation::Release { expected: pending },
            )
            .unwrap(),
    );
    assert_eq!(
        storage
            .input_gate(&store, id(36), limit())
            .unwrap()
            .unwrap(),
        queued
    );
    store.close().unwrap();
}

#[test]
fn pending_handoff_preserves_delivery_rejection_and_exact_stop() {
    let home = TestHome::new("discussion-steering-stop");
    let (store, storage, mut request) = seeded(&home, FaultController::new());
    let acceptance = prepare_acceptance(&store, &storage);
    let input = acceptance.accepted_input_id();
    committed(
        &store,
        storage.first_acceptance(storage.revision(&store).unwrap(), acceptance),
    );
    request.input_gate_revision = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .revision();
    request.thread_revision = storage
        .thread(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .revision();
    let target = request.resolving_target.clone();
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    execute(&store, prepared);
    committed(
        &store,
        storage.begin_accepted_input_delivery(
            storage.revision(&store).unwrap(),
            BeginAcceptedInputDelivery::new(
                id(36),
                input,
                beryl_model::AcceptedInputRevision::new(1).unwrap(),
                target.clone(),
            ),
        ),
    );
    committed(
        &store,
        storage.record_steering_rejection(
            storage.revision(&store).unwrap(),
            SteeringRejection::new(
                id(36),
                input,
                beryl_model::AcceptedInputRevision::new(2).unwrap(),
                target,
            ),
        ),
    );
    let gate = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.live_next_turn_count(), 1);
    let StopAdmissionRead::Admissible(candidate) = storage
        .stop_admission_read(&store, id(36), limit())
        .unwrap()
    else {
        panic!("pending handoff must allow exact stop")
    };
    let request = candidate.admission(
        StopOperationNonce::from_bytes([240; 16]),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    assert!(matches!(
        store.execute_current(storage.current_admit_stop_operation(request)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(matches!(
        storage
            .stop_admission_read(&store, id(36), limit())
            .unwrap(),
        StopAdmissionRead::Stopping(_)
    ));
    store.close().unwrap();
}

#[test]
fn pending_discussion_rejects_new_compaction_after_terminal_cleanup() {
    let home = TestHome::new("discussion-compaction-gate");
    let (store, storage, request) = seeded(&home, FaultController::new());
    let generation = storage
        .execution_snapshot(
            &store,
            request.resolving_target.pending().snapshot_id(),
            limit(),
        )
        .unwrap()
        .unwrap()
        .loaded_generation();
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let pending = prepared.intent().new_gate();
    execute(&store, prepared);
    crate::support::discussion_handoff::complete_resolving_turn(&store, &storage);
    let CompactionAdmissionRead::Admissible(candidate) = storage
        .compaction_admission_read(&store, id(36), limit())
        .unwrap()
    else {
        panic!("terminal binding should admit compaction apart from handoff gate")
    };
    let admission = candidate.admission(
        CompactionOperationNonce::from_bytes([242; 16]),
        CompactionAttemptNonce::from_bytes([243; 16]),
        generation,
        timestamp(250),
    );
    blocked(
        &store,
        storage.admit_compaction_operation(storage.revision(&store).unwrap(), admission.clone()),
    );
    execute(
        &store,
        storage
            .prepare_discussion_handoff(
                &store,
                DiscussionHandoffMutation::Release { expected: pending },
            )
            .unwrap(),
    );
    committed(
        &store,
        storage.admit_compaction_operation(storage.revision(&store).unwrap(), admission),
    );
    store.close().unwrap();
}
