#[cfg(feature = "test-faults")]
#[path = "phase152_atomic_staging_batches/faults.rs"]
mod faults;
#[path = "phase152_atomic_staging_batches/support.rs"]
mod support;

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
#[cfg(feature = "test-faults")]
use beryl_home_store::{
    HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    delete_draft_mutation_staging_page, draft_mutation_staging_batch_target,
    draft_mutation_staging_batch_target_records, draft_mutation_staging_locally_exact_source_head,
    inject_draft_mutation_staging_batch_prefix, inject_draft_mutation_staging_occupied_page,
};
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftLogicalExtentV1, DraftMutationFinishInputV1,
    DraftMutationStagingErrorV1, DraftMutationStagingLaneV1, DraftMutationStagingPageInputV1,
    DraftMutationStagingPageItemV1, DraftMutationStagingReconcileV1, DraftMutationStagingStatusV1,
    DraftPieceDurableBuildWindowLimitsV1, PreparedDraftPieceStagingWindowV1, SyndicStorage,
    canonical_empty_draft_piece_fragment_chain_v1, draft_piece_fragment_chain_link_v1,
};

use support::*;

fn prepare_durable_page_window(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: syndic_storage::DraftMutationStagingIdentityV1,
) -> Result<Option<PreparedDraftPieceStagingWindowV1>, DraftMutationStagingErrorV1> {
    let DraftMutationStagingStatusV1::Building { build, .. } =
        storage.draft_mutation_staging_status(store, identity)?
    else {
        return Err(DraftMutationStagingErrorV1::Invalid);
    };
    storage.prepare_next_durable_draft_piece_window(
        store,
        identity,
        build,
        DraftPieceDurableBuildWindowLimitsV1::new(1, 1, 65_536).unwrap(),
    )
}

#[test]
fn two_page_commit_one_page_fast_path_replay_and_pre_admission_cancellation_are_exact() {
    let fixture = receiving_fixture("ordinary", 1);
    let prepared = prepare(&fixture, proposal_inputs(&["a", "b"]));
    assert_eq!(prepared.page_count(), 2);
    assert_eq!(prepared.item_count(), 2);
    let target_head = prepared.target_head().clone();
    let target_session = prepared.target_session().unwrap().clone();
    assert_eq!(
        fixture
            .storage
            .reconcile_draft_mutation_staging_page_batch(&fixture.store, &prepared)
            .unwrap(),
        DraftMutationStagingReconcileV1::SourceSelected,
    );
    let replay = prepared.clone();
    committed(execute(
        &fixture.store,
        fixture.storage.draft_mutation_staging_page_batch(
            fixture.storage.revision(&fixture.store).unwrap(),
            prepared,
        ),
    ));
    assert_eq!(
        fixture
            .storage
            .reconcile_draft_mutation_staging_page_batch(&fixture.store, &replay)
            .unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected,
    );
    let replay_outcome = execute(
        &fixture.store,
        fixture.storage.draft_mutation_staging_page_batch(
            fixture.storage.revision(&fixture.store).unwrap(),
            replay.clone(),
        ),
    );
    assert!(
        matches!(
            &replay_outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::EmptyContribution { domain: "syndic" },
            }
        ),
        "unexpected replay outcome: {replay_outcome:?}"
    );
    assert_eq!(
        fixture
            .storage
            .reconcile_draft_mutation_staging_page_batch(&fixture.store, &replay)
            .unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected,
    );
    assert_eq!(
        fixture
            .storage
            .draft_mutation_staging_status(&fixture.store, fixture.identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Receiving {
            head: target_head.receipt(),
        },
    );
    assert_eq!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                target_session.draft_id(),
                target_session.session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(target_session),
    );

    let cancelled = receiving_fixture("cancel-before-admission", 10);
    let abandoned = prepare(
        &cancelled,
        proposal_inputs(&["not-admitted", "still-not-admitted"]),
    );
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let contribution = cancelled.storage.draft_mutation_staging_page_batch(
        cancelled.storage.revision(&cancelled.store).unwrap(),
        abandoned.clone(),
    );
    let mut command =
        HomeCommand::new(cancelled.store.home_revision().unwrap()).with_cancellation(cancellation);
    command.add(contribution).unwrap();
    assert!(matches!(
        cancelled.store.execute(command),
        CommandOutcome::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission,
        }
    ));
    assert_eq!(
        cancelled
            .storage
            .reconcile_draft_mutation_staging_page_batch(&cancelled.store, &abandoned)
            .unwrap(),
        DraftMutationStagingReconcileV1::SourceSelected,
    );
    assert_eq!(
        cancelled
            .storage
            .draft_mutation_staging_head(&cancelled.store, cancelled.identity)
            .unwrap(),
        Some(cancelled.head.clone()),
    );
    assert_eq!(
        cancelled
            .storage
            .draft_editor_candidate_session(
                &cancelled.store,
                cancelled.session.draft_id(),
                cancelled.session.session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(cancelled.session.clone()),
    );
    #[cfg(feature = "test-faults")]
    for index in 0..abandoned.page_count() {
        assert_eq!(
            draft_mutation_staging_batch_target_records(
                &cancelled.store,
                &cancelled.storage,
                &abandoned,
                index,
            )
            .unwrap(),
            (None, None),
        );
    }

    let one = receiving_fixture("one-page", 20);
    let prepared = prepare(&one, proposal_inputs(&["x"]));
    assert_eq!(prepared.page_count(), 1);
    let target = prepared.target_head().clone();
    committed(execute(
        &one.store,
        one.storage
            .draft_mutation_staging_page_batch(one.storage.revision(&one.store).unwrap(), prepared),
    ));
    assert_eq!(
        one.storage
            .draft_mutation_staging_head(&one.store, one.identity)
            .unwrap(),
        Some(target),
    );
}

#[test]
fn maximum_batch_commits_and_zero_258_mixed_cursor_and_aggregate_excess_reject() {
    let maximum = receiving_fixture("maximum", 30);
    let prepared = prepare(&maximum, source_inputs(257, 256));
    assert_eq!(prepared.page_count(), 257);
    assert_eq!(prepared.item_count(), 65_792);
    assert!(prepared.encoded_page_bytes() <= 16_842_752);
    let target = prepared.target_head().clone();
    committed(execute(
        &maximum.store,
        maximum.storage.draft_mutation_staging_page_batch(
            maximum.storage.revision(&maximum.store).unwrap(),
            prepared,
        ),
    ));
    assert_eq!(target.source().next_ordinal(), 258);
    assert_eq!(target.source().item_total(), 65_792);
    assert_eq!(target.receipt().transition_ordinal(), 258);

    let invalid = receiving_fixture("invalid-bounds", 40);
    assert!(matches!(
        invalid.storage.prepare_draft_mutation_staging_page_batch(
            &invalid.head,
            &invalid.session,
            Box::new([]),
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    assert!(matches!(
        invalid.storage.prepare_draft_mutation_staging_page_batch(
            &invalid.head,
            &invalid.session,
            source_inputs(258, 1),
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    let mixed = Box::new([
        DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Source,
            0,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(0))]),
        ),
        proposal_inputs(&["x"]).into_vec().remove(0),
    ]);
    assert!(matches!(
        invalid.storage.prepare_draft_mutation_staging_page_batch(
            &invalid.head,
            &invalid.session,
            mixed,
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    let gap = Box::new([
        DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Source,
            0,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(0))]),
        ),
        DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Source,
            2,
            3,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(2))]),
        ),
    ]);
    assert!(matches!(
        invalid.storage.prepare_draft_mutation_staging_page_batch(
            &invalid.head,
            &invalid.session,
            gap,
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    let cursor_overflow = Box::new([
        DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Source,
            0,
            u64::MAX,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(0))]),
        ),
        DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Source,
            u64::MAX,
            0,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(0))]),
        ),
    ]);
    assert!(
        invalid
            .storage
            .prepare_draft_mutation_staging_page_batch(
                &invalid.head,
                &invalid.session,
                cursor_overflow,
            )
            .is_err()
    );
}

#[test]
fn reopen_preserves_complete_target_and_finish_transfer_and_builder_drain() {
    let fixture = receiving_fixture("reopen", 50);
    let mut chain = canonical_empty_draft_piece_fragment_chain_v1();
    let inputs = proposal_inputs(&["a", "b"]);
    for (index, input) in inputs.iter().enumerate() {
        let DraftMutationStagingPageItemV1::Proposal(replacement) = input.items()[0].clone() else {
            unreachable!()
        };
        chain = draft_piece_fragment_chain_link_v1(chain, index as u64 + 1, &replacement);
    }
    let prepared = prepare(&fixture, inputs);
    let replay = prepared.clone();
    let mut session = prepared.target_session().unwrap().clone();
    committed(execute(
        &fixture.store,
        fixture.storage.draft_mutation_staging_page_batch(
            fixture.storage.revision(&fixture.store).unwrap(),
            prepared,
        ),
    ));
    let home_path = fixture.home.0.clone();
    let identity = fixture.identity;
    drop(fixture.store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home_path, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_page_batch(&reopened, &replay)
            .unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected,
    );
    let head = storage
        .draft_mutation_staging_head(&reopened, identity)
        .unwrap()
        .unwrap();
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &head,
            &session,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
                DraftLogicalExtentV1::new(2, 1),
                point(2),
                point(2),
                point(2),
                chain,
            ),
        )
        .unwrap();
    session = finish.target_session().unwrap().clone();
    committed(execute(
        &reopened,
        storage.draft_mutation_staging_command(storage.revision(&reopened).unwrap(), finish),
    ));
    let head = storage
        .draft_mutation_staging_head(&reopened, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &session)
        .unwrap();
    committed(execute(
        &reopened,
        storage.transfer_draft_mutation_staging_to_builder(
            storage.revision(&reopened).unwrap(),
            transfer,
        ),
    ));
    for ordinal in 1..=2 {
        let page = prepare_durable_page_window(&storage, &reopened, identity)
            .unwrap()
            .unwrap();
        assert_eq!(page.first_page_ordinal(), ordinal);
        committed(execute(
            &reopened,
            storage
                .stage_next_durable_draft_piece_window(storage.revision(&reopened).unwrap(), page),
        ));
    }
    assert!(
        prepare_durable_page_window(&storage, &reopened, identity)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&reopened, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Building { .. }
    ));
}
