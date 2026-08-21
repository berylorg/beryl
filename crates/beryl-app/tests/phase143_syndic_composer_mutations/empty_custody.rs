use super::*;

#[cfg(feature = "test-faults")]
#[test]
fn internal_empty_page_source_retry_and_pre_admission_cancel_keep_exact_custody() {
    let (_home, store, storage, thread) = fixture("phase155-empty-source-retry", 141);
    let (mut host, base) = activated(storage, &store, thread, 142, 143);
    let text = commit_text(&mut host, &store, base, 144, 0, 0, "old", 3, 1);
    let retry_key = begin_deletion(&mut host, &store, text, 145, 3);
    arm_source_selection(&mut host, thread, 146);
    let finish = finish_input(retry_key, empty_finish(), empty_finish(), 0, 0);
    assert!(matches!(
        host.finish_mutation_input(&store, finish),
        Err(ComposerHostError::MutationWorkPending)
    ));
    let custody = host.test_mutation_in_flight_custody().unwrap();
    assert!(custody.1);
    assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));
    assert_eq!(
        storage
            .draft_mutation_staging_head(&store, staging_identity(text, 145))
            .unwrap()
            .unwrap()
            .proposal()
            .next_cursor(),
        0
    );
    let differing_extent = MutationFinishInput::new(
        retry_key,
        empty_finish(),
        empty_finish(),
        LogicalExtent::new(1, 0),
        MutationPositions::collapsed(source_position(0)),
    );
    assert!(matches!(
        host.finish_mutation_input(&store, differing_extent),
        Err(ComposerHostError::MutationIdentityCollision)
    ));
    assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));
    assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));
    let differing_positions = MutationFinishInput::new(
        retry_key,
        empty_finish(),
        empty_finish(),
        LogicalExtent::new(0, 0),
        MutationPositions::collapsed(source_position(1)),
    );
    assert!(matches!(
        host.finish_mutation_input(&store, differing_positions),
        Err(ComposerHostError::MutationIdentityCollision)
    ));
    assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));
    assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));
    assert_eq!(
        storage
            .draft_mutation_staging_head(&store, staging_identity(text, 145))
            .unwrap()
            .unwrap()
            .proposal()
            .next_cursor(),
        0
    );
    host.test_set_mutation_transition_limit(4096);
    host.finish_mutation_input(&store, finish).unwrap();
    let empty = commit(&mut host, &store, retry_key);
    assert_eq!(candidate_text(storage, &store, empty), b"");

    let (_home, store, storage, thread) = fixture("phase155-empty-source-cancel", 151);
    let (mut host, base) = activated(storage, &store, thread, 152, 153);
    let text = commit_text(&mut host, &store, base, 154, 0, 0, "old", 3, 1);
    let cancel_key = begin_deletion(&mut host, &store, text, 155, 3);
    arm_source_selection(&mut host, thread, 156);
    assert!(matches!(
        host.finish_mutation_input(
            &store,
            finish_input(cancel_key, empty_finish(), empty_finish(), 0, 0),
        ),
        Err(ComposerHostError::MutationWorkPending)
    ));
    host.test_set_mutation_transition_limit(4096);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(cancel_key, MutationIdentity::ROOT),
            &cancellation,
        )
        .unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
    assert_eq!(host.binding(), Some(text));
}

#[cfg(feature = "test-faults")]
#[test]
fn internal_empty_page_indeterminate_target_reconciles_without_reconstruction() {
    use beryl_home_store::test_faults::FaultPoint;
    use support::fault_fixture;

    let (_home, store, storage, thread, faults) =
        fault_fixture("phase155-empty-indeterminate", 161);
    let (mut host, base) = activated(storage, &store, thread, 162, 163);
    let text = commit_text(&mut host, &store, base, 164, 0, 0, "old", 3, 1);
    let key = begin_deletion(&mut host, &store, text, 165, 3);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    host.finish_mutation_input(
        &store,
        finish_input(key, empty_finish(), empty_finish(), 0, 0),
    )
    .unwrap();
    let empty = commit(&mut host, &store, key);
    assert_eq!(candidate_text(storage, &store, empty), b"");
}

#[cfg(feature = "test-faults")]
#[test]
fn indeterminate_cancellation_while_building_settles_without_fresh_binding_adoption() {
    use beryl_home_store::test_faults::FaultPoint;
    use support::fault_fixture;

    let (_home, store, storage, thread, faults) = fault_fixture("phase155-building-cancel", 171);
    let current_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 172, 173);
    let (key, finish) = stage_text(&mut host, &store, base, 174, 0, 0, "cancel", 6, 1);
    host.finish_mutation_input(&store, finish).unwrap();
    host.test_set_mutation_transition_limit(1);
    assert!(matches!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::MutationWorkPending)
    ));
    host.test_set_mutation_transition_limit(4096);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let outcome = loop {
        match host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        ) {
            Err(ComposerHostError::MutationWorkPending) => continue,
            result => break result.unwrap(),
        }
    };
    assert_eq!(outcome, ComposerHostMutationOutcome::Cancelled);
    assert_eq!(host.binding(), Some(base));
    assert_eq!(current(storage, &store, thread), current_before);
}

#[cfg(feature = "test-faults")]
#[test]
fn widget_page_fail_closed_reconciliation_retains_exact_custody() {
    let (_home, store, storage, thread) = fixture("phase155-widget-fail-closed", 181);
    let (mut host, base) = activated(storage, &store, thread, 182, 183);
    let key = mutation_key(base, 184);
    let zero = source_position(0);
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "retained through corruption".into(),
        }],
    )
    .unwrap();
    let payload = page.clone();
    arm_head_fork(&mut host, staging_identity(base, 184));
    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([])),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Invariant
        ))
    ));
    let custody = host.test_mutation_in_flight_custody().unwrap();
    assert!(!custody.1);
    assert_eq!(payload.payload_owner_count(), 2);

    assert!(matches!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(payload.clone()),
            Box::new([]),
        ),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Invariant
        ))
    ));
    assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));
    assert_eq!(payload.payload_owner_count(), 2);

    let differing = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "different".into(),
        }],
    )
    .unwrap();
    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(differing), Box::new([])),
        Err(ComposerHostError::MutationPending)
    ));
    assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));

    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        ),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Invariant
        ))
    ));
    assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));
    assert_eq!(payload.payload_owner_count(), 2);
    let head = storage
        .draft_mutation_staging_head(&store, staging_identity(base, 184))
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 0);
}

#[cfg(feature = "test-faults")]
#[test]
fn synthetic_deletion_fail_closed_reconciliation_retains_exact_custody() {
    let (_home, store, storage, thread) = fixture("phase155-empty-fail-closed", 191);
    let (mut host, base) = activated(storage, &store, thread, 192, 193);
    let text = commit_text(&mut host, &store, base, 194, 0, 0, "old", 3, 1);
    let key = begin_deletion(&mut host, &store, text, 195, 3);
    arm_head_fork(&mut host, staging_identity(text, 195));
    let finish = finish_input(key, empty_finish(), empty_finish(), 0, 0);
    assert!(matches!(
        host.finish_mutation_input(&store, finish),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Invariant
        ))
    ));
    let custody = host.test_mutation_in_flight_custody().unwrap();
    assert!(custody.1);
    assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));

    assert!(matches!(
        host.finish_mutation_input(&store, finish),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Invariant
        ))
    ));
    assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));
    assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));

    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation
        ),
        Err(ComposerHostError::MutationStaging(
            syndic_storage::DraftMutationStagingErrorV1::Invariant
        ))
    ));
    assert_eq!(host.test_mutation_in_flight_custody(), Some(custody));
    assert_eq!(host.test_mutation_in_flight_finish(), Some(finish));
    let head = storage
        .draft_mutation_staging_head(&store, staging_identity(text, 195))
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 0);
}

#[cfg(feature = "test-faults")]
fn begin_deletion(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    end: u64,
) -> MutationKey {
    let key = mutation_key(binding, operation);
    let start = source_position(0);
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(start),
                range(start, source_position(end)),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    key
}

#[cfg(feature = "test-faults")]
fn arm_source_selection(
    host: &mut SyndicComposerHost,
    thread: beryl_model::SyndicThreadId,
    session_seed: u8,
) {
    host.test_set_mutation_transition_limit(1);
    host.test_arm_mutation_before_execute_fault(move |store, storage| {
        let _ = thread;
        support::bump_home_revision(storage, store, session_seed);
    });
}

#[cfg(feature = "test-faults")]
fn arm_head_fork(host: &mut SyndicComposerHost, identity: DraftMutationStagingIdentityV1) {
    host.test_arm_mutation_before_execute_fault(move |store, storage| {
        let contribution = syndic_storage::test_faults::inject_draft_mutation_staging_head_fork(
            store, storage, identity,
        );
        support::committed(support::execute(store, contribution));
    });
}
