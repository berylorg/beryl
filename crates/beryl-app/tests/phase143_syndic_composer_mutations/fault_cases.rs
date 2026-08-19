use super::*;

#[cfg(feature = "test-faults")]
#[test]
fn full_widget_proposal_and_successor_facts_participate_in_collision_identity() {
    let (_home, store, storage, thread) = fixture("phase143-full-identity", 84);
    let (mut host, binding) = activated(storage, &store, thread, 85, 86);

    let line_break_request = text_request(binding, 87, 0, 0, &["x"], 1);
    host.begin_mutation(&store, line_break_request.clone())
        .unwrap();
    host.test_set_mutation_transition_limit(0);
    let line_break_admission = host.execute_mutation(&store, &CommandCancellation::new());
    assert!(
        matches!(
            line_break_admission,
            Err(ComposerHostError::MutationWorkPending)
        ),
        "unexpected bounded admission: {line_break_admission:?}"
    );
    host.test_set_mutation_transition_limit(4096);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(&store, &cancellation).unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
    let line_break_collision = ComposerHostMutationRequest::new(
        binding,
        MutationProposal::new(
            line_break_request.proposal().key(),
            MutationKind::Edit,
            line_break_request.proposal().replacement(),
            1,
        ),
        line_break_request.operation_id(),
        line_break_request.fragments().to_vec().into_boxed_slice(),
        line_break_request
            .marker_metadata()
            .to_vec()
            .into_boxed_slice(),
    );
    assert!(matches!(
        host.begin_mutation(&store, line_break_collision),
        Err(ComposerHostError::MutationIdentityCollision)
    ));

    let (_home, store, storage, thread) = fixture("phase143-successor-identity", 88);
    let (mut host, binding) = activated(storage, &store, thread, 89, 90);
    let zero = source_position(0);
    let id = InlineObjectId::new(0x8888);
    let order = InlineObjectOrder::new(1);
    let after = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order)),
    );
    let object_request = mutation_request(
        binding,
        91,
        MutationKind::Edit,
        range(zero, zero),
        vec![MutationFragmentPayload::Object(ObjectChange::Insert {
            at: zero,
            object: SuccessorObject::new(id, ByteOffset::new(0), order, 1, 1),
        })],
        MutationPositions::collapsed(after),
        vec![ComposerHostImageMarkerMetadata::new(
            id,
            ImageLabelOrdinal::FIRST,
        )],
    );
    host.begin_mutation(&store, object_request).unwrap();
    host.test_set_mutation_transition_limit(0);
    let successor_admission = host.execute_mutation(&store, &CommandCancellation::new());
    assert!(
        matches!(
            successor_admission,
            Err(ComposerHostError::MutationWorkPending)
        ),
        "unexpected bounded admission: {successor_admission:?}"
    );
    host.test_set_mutation_transition_limit(4096);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(&store, &cancellation).unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
    for object in [
        SuccessorObject::new(id, ByteOffset::new(0), order, 2, 1),
        SuccessorObject::new(id, ByteOffset::new(0), order, 1, 2),
    ] {
        let collision = mutation_request(
            binding,
            91,
            MutationKind::Edit,
            range(zero, zero),
            vec![MutationFragmentPayload::Object(ObjectChange::Insert {
                at: zero,
                object,
            })],
            MutationPositions::collapsed(after),
            vec![ComposerHostImageMarkerMetadata::new(
                id,
                ImageLabelOrdinal::FIRST,
            )],
        );
        assert!(matches!(
            host.begin_mutation(&store, collision),
            Err(ComposerHostError::MutationIdentityCollision)
        ));
    }
    assert_eq!(host.binding(), Some(binding));
}

#[cfg(feature = "test-faults")]
#[test]
fn absent_not_committed_work_remains_staged_and_releases_or_rebinds() {
    let (_home, store, storage, thread) = fixture("phase143-absent", 76);
    let (mut host, base) = activated(storage, &store, thread, 77, 78);
    host.begin_mutation(&store, text_request(base, 79, 0, 0, &["absent"], 6))
        .unwrap();
    arm_mutation_revision_conflict(&mut host, thread);
    host.test_set_mutation_transition_limit(0);
    let first_absent = host.execute_mutation(&store, &CommandCancellation::new());
    assert!(
        matches!(first_absent, Err(ComposerHostError::MutationWorkPending)),
        "unexpected absent reconciliation: {first_absent:?}"
    );
    assert_eq!(
        host.mutation_status(),
        Some(ComposerHostMutationStatus::Staged)
    );

    let ComposerHostActivationOutcome::Activated {
        binding: rebound, ..
    } = host
        .activate(
            &store,
            activation(thread, 80, 81),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("absent pre-admission work blocked direct rebind");
    };
    host.begin_mutation(&store, text_request(rebound, 82, 0, 0, &["release"], 7))
        .unwrap();
    arm_mutation_revision_conflict(&mut host, thread);
    let second_absent = host.execute_mutation(&store, &CommandCancellation::new());
    assert!(
        matches!(second_absent, Err(ComposerHostError::MutationWorkPending)),
        "unexpected absent reconciliation: {second_absent:?}"
    );
    assert_eq!(
        host.mutation_status(),
        Some(ComposerHostMutationStatus::Staged)
    );
    assert_eq!(host.release().unwrap(), true);

    let ComposerHostActivationOutcome::Activated { binding: fresh, .. } = host
        .activate(
            &store,
            activation(thread, 83, 84),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("release after absent work did not permit activation");
    };
    host.test_set_mutation_transition_limit(4096);
    let committed = commit_text(&mut host, &store, fresh, 85, 0, 0, &["ok"], 2);
    assert_eq!(candidate_text(storage, &store, committed), b"ok");
}

#[cfg(feature = "test-faults")]
#[test]
fn intermediate_and_terminal_indeterminate_commands_reconcile_to_exact_commit() {
    use beryl_home_store::test_faults::FaultPoint;
    use support::fault_fixture;

    let (_home, store, storage, thread, faults) = fault_fixture("phase143-indeterminate", 81);
    let (mut host, base) = activated(storage, &store, thread, 82, 83);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let first = commit_text(&mut host, &store, base, 84, 0, 0, &["mid"], 3);
    assert_eq!(candidate_text(storage, &store, first), b"mid");

    faults.fail_times(FaultPoint::AfterCommitBeforePersist, 64);
    let terminal = commit_text(&mut host, &store, first, 85, 3, 3, &["-", "terminal"], 12);
    assert_eq!(candidate_text(storage, &store, terminal), b"mid-terminal");
    assert_eq!(host.binding(), Some(terminal));
}

#[cfg(feature = "test-faults")]
#[test]
fn admitted_indeterminate_and_work_cap_keep_custody_across_release_and_rebind() {
    use beryl_home_store::test_faults::FaultPoint;
    use support::fault_fixture;

    let (_home, store, storage, thread, faults) = fault_fixture("phase143-custody-cap", 86);
    let (mut host, base) = activated(storage, &store, thread, 87, 88);
    host.begin_mutation(&store, text_request(base, 89, 0, 0, &["held"], 4))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    host.test_set_mutation_transition_limit(0);
    assert!(matches!(
        host.execute_mutation(&store, &CommandCancellation::new()),
        Err(ComposerHostError::MutationWorkPending)
    ));
    assert_eq!(
        host.mutation_status(),
        Some(ComposerHostMutationStatus::Admitted)
    );
    assert_eq!(host.binding(), Some(base));
    assert!(matches!(
        host.release(),
        Err(ComposerHostError::MutationCustodyPending)
    ));
    assert!(matches!(
        host.activate(
            &store,
            activation(thread, 90, 91),
            &CommandCancellation::new()
        ),
        Err(ComposerHostError::MutationCustodyPending)
    ));
    assert!(matches!(
        host.begin_mutation(&store, text_request(base, 90, 0, 0, &["aba"], 3)),
        Err(ComposerHostError::MutationPending)
    ));

    host.test_set_mutation_transition_limit(4096);
    let ComposerHostMutationOutcome::Committed { binding, .. } = host
        .execute_mutation(&store, &CommandCancellation::new())
        .unwrap()
    else {
        panic!("admitted work did not drain to its exact terminal settlement");
    };
    assert_eq!(candidate_text(storage, &store, binding), b"held");
    assert_eq!(host.mutation_status(), None);
}

#[cfg(feature = "test-faults")]
#[test]
fn cancellation_after_begin_admission_elects_durable_cancelled_and_drains() {
    use std::time::Duration;

    use beryl_home_store::test_faults::FaultPoint;
    use support::fault_fixture;

    let (home, store, storage, thread, faults) = fault_fixture("phase143-cancel-race", 91);
    let (mut host, base) = activated(storage, &store, thread, 92, 93);
    host.begin_mutation(&store, text_request(base, 94, 0, 0, &["race"], 4))
        .unwrap();
    let cancellation = CommandCancellation::new();
    let worker_cancellation = cancellation.clone();
    let block = faults.block_next(FaultPoint::AfterCommitBeforePersist);
    let worker = std::thread::spawn(move || {
        let outcome = host.execute_mutation(&store, &worker_cancellation);
        (home, store, host, outcome)
    });
    assert!(block.wait_until_reached(Duration::from_secs(10)));
    cancellation.cancel();
    block.release();
    let (_home, store, host, outcome) = worker.join().unwrap();
    assert_eq!(outcome.unwrap(), ComposerHostMutationOutcome::Cancelled);
    assert_eq!(host.binding(), Some(base));
    let session = match storage
        .draft_editor_candidate_session(
            &store,
            base.candidate().draft_id(),
            base.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("candidate session was not active after cancellation: {other:?}"),
    };
    assert!(session.active_operation().is_none());
    let refreshed = DraftEditorCandidateActivationBindingV1::from_head(&session);
    let text = storage
        .candidate_draft_piece_text_demand(
            &store,
            refreshed,
            DraftPieceTextDemandV1::Forward(0),
            65_536,
        )
        .unwrap();
    assert_eq!(text.value().bytes(), b"");
}
