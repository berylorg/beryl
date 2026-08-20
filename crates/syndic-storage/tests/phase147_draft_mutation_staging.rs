use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    DraftPieceCandidateRootCollision, delete_draft_mutation_staging_head,
    delete_draft_mutation_staging_page, delete_draft_mutation_staging_receipt,
    inject_draft_mutation_staging_head_ahead, inject_draft_mutation_staging_head_digest_corruption,
    inject_draft_mutation_staging_head_fork, inject_draft_mutation_staging_occupied_page,
    inject_draft_mutation_staging_page_ceiling_corruption,
    inject_draft_mutation_staging_page_digest_corruption,
    inject_draft_mutation_staging_receipt_digest_corruption,
    inject_draft_mutation_terminal_same_operation_custody,
    inject_draft_piece_candidate_root_collision, inject_draft_piece_session_generation_inflation,
};
use syndic_storage::{
    CreateThread, DraftComposerBuildKeyV1, DraftComposerFormatV1,
    DraftComposerMaterializationOperationIdV1, DraftComposerMaterializationStatusV1,
    DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftLogicalExtentV1, DraftMutationBeginV1,
    DraftMutationFinishInputV1, DraftMutationOperationIdV1, DraftMutationStagingErrorEvidenceV1,
    DraftMutationStagingErrorReasonV1, DraftMutationStagingErrorV1, DraftMutationStagingHeadV1,
    DraftMutationStagingIdentityV1, DraftMutationStagingLaneFrontierV1, DraftMutationStagingLaneV1,
    DraftMutationStagingLifecycleV1, DraftMutationStagingPageItemV1,
    DraftMutationStagingReconcileV1, DraftMutationStagingRejectedReasonV1,
    DraftMutationStagingStatusV1, DraftMutationStagingTerminalAnchorV1,
    DraftMutationStagingTerminalEvidenceV1, DraftPieceEditHeaderV1, DraftPieceOperationIdV1,
    DraftPieceReplacementV1, DraftPieceTextDemandV1, DraftPieceV1, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1, draft_piece_fragment_chain_link_v1,
};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    DraftMutationStagingComparedByteV1, DraftMutationStagingOccupiedKeyV1,
    DraftMutationStagingPageKeyV1, DraftMutationStagingProgressReceiptKeyV1, DraftPieceRootKeyV1,
    DraftPieceRootReferenceV1,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase146-{name}-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn one_page_payload_is_durable_before_bounded_builder_construction() {
    let (_home, store, storage, thread) = fixture("one-page", 1);
    let before = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &before, 3, 4);
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([5; 16]),
    );
    let begin = DraftMutationBeginV1::new(
        identity,
        session.session_generation(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        session.logical_extent(),
        point(0),
        point(0),
        point(0),
        point(0),
        point(0),
        0,
        0,
    );
    let prepared = storage
        .prepare_draft_mutation_staging_begin(begin, &session)
        .unwrap();
    session = prepared.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
    ));

    let replacement =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("x".to_owned())]);
    let chain = draft_piece_fragment_chain_link_v1(
        syndic_storage::canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        &replacement,
    );
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let prepared = storage
        .prepare_draft_mutation_staging_page(
            &head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            256,
            65_536,
            Box::new([DraftMutationStagingPageItemV1::Proposal(replacement)]),
        )
        .unwrap();
    session = prepared.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
    ));
    let durable_root = before.draft().piece_root();
    assert!(
        storage
            .draft_piece_text_demand(
                &store,
                durable_root,
                DraftPieceTextDemandV1::Forward(0),
                65_536,
            )
            .unwrap()
            .bytes()
            .is_empty()
    );
    let materialization = DraftComposerBuildKeyV1::new(
        durable_root,
        DraftComposerFormatV1::ComposerV1,
        DraftComposerMaterializationOperationIdV1::from_bytes([6; 16]),
    );
    assert_eq!(
        storage
            .draft_composer_materialization_status(&store, materialization)
            .unwrap(),
        DraftComposerMaterializationStatusV1::Absent
    );

    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let finish = DraftMutationFinishInputV1::new(
        head.source(),
        head.proposal(),
        DraftLogicalExtentV1::new(1, 1),
        point(1),
        point(1),
        point(1),
        chain,
    );
    let prepared = storage
        .prepare_draft_mutation_staging_finish(&head, &session, finish)
        .unwrap();
    session = prepared.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
    ));

    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &session)
        .unwrap();
    let transfer_replay = transfer.clone();
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Finished { .. }
    ));
    committed(execute(
        &store,
        storage.transfer_draft_mutation_staging_to_builder(
            storage.revision(&store).unwrap(),
            transfer,
        ),
    ));
    replay_succeeded(execute(
        &store,
        storage.transfer_draft_mutation_staging_to_builder(
            storage.revision(&store).unwrap(),
            transfer_replay,
        ),
    ));
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Building { .. }
    ));

    let page = storage
        .prepare_next_durable_draft_piece_page(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(page.lane(), DraftMutationStagingLaneV1::Proposal);
    assert_eq!(page.page_ordinal(), 1);
    assert_eq!(page.fragment_count(), 1);
    committed(execute(
        &store,
        storage.stage_next_durable_draft_piece_page(storage.revision(&store).unwrap(), page),
    ));
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(&store, identity)
        .unwrap()
    else {
        panic!("staging status left building");
    };
    assert_eq!(build.key().transition_ordinal(), 2);
    assert!(
        storage
            .prepare_next_durable_draft_piece_page(&store, identity)
            .unwrap()
            .is_none()
    );
    assert_eq!(current(storage, &store, thread), before);
}

#[test]
fn lanes_advance_independently_and_commands_reconcile_exactly() {
    let (_home, store, storage, thread) = fixture("lanes-reconcile", 11);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 12, 13);
    let identity = staging_identity(&session, 14);
    let begin = begin_input(identity, &session);
    let prepared = storage
        .prepare_draft_mutation_staging_begin(begin, &session)
        .unwrap();
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_command(&store, &prepared)
            .unwrap(),
        DraftMutationStagingReconcileV1::SourceSelected,
    );
    session = prepared.target_session().unwrap().clone();
    let replay = prepared.clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
    ));
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_command(&store, &replay)
            .unwrap(),
        DraftMutationStagingReconcileV1::TargetSelected,
    );
    replay_succeeded(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), replay),
    ));

    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let source = storage
        .prepare_draft_mutation_staging_page(
            &head,
            &session,
            DraftMutationStagingLaneV1::Source,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(0))]),
        )
        .unwrap();
    session = source.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), source),
    ));
    let after_source = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(after_source.source().next_ordinal(), 2);
    assert_eq!(after_source.source().item_total(), 1);
    assert_eq!(after_source.proposal().next_ordinal(), 1);
    assert_eq!(after_source.proposal().item_total(), 0);

    let proposal =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("p".to_owned())]);
    let prepared = storage
        .prepare_draft_mutation_staging_page(
            &after_source,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(proposal)]),
        )
        .unwrap();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
    ));
    let final_head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(final_head.source(), after_source.source());
    assert_eq!(final_head.proposal().next_ordinal(), 2);
    assert_eq!(final_head.proposal().item_total(), 1);
    assert_eq!(final_head.receipt().transition_ordinal(), 3);
}

#[test]
fn terminal_first_and_admitted_terminal_evidence_are_exact() {
    let (_home, store, storage, thread) = fixture("terminals", 21);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 22, 23);
    for index in 0..3 {
        let identity = staging_identity(&session, 30 + index as u8);
        let begin = begin_input(identity, &session);
        let preview = storage
            .prepare_draft_mutation_staging_begin(begin, &session)
            .unwrap();
        let facts = (
            session.newest_candidate_generation(),
            session.newest_root(),
            session.newest_history(),
            session.session_generation(),
        );
        let evidence = match index {
            0 => DraftMutationStagingTerminalEvidenceV1::Rejected {
                reason: DraftMutationStagingRejectedReasonV1::InvalidEnvelope,
                anchor: DraftMutationStagingTerminalAnchorV1::Begin(identity),
                digest: preview.target_head().begin_digest(),
                candidate_generation: facts.0,
                root: facts.1,
                history: facts.2,
                session_revision: facts.3,
            },
            1 => DraftMutationStagingTerminalEvidenceV1::Cancelled {
                request_id: identity.operation_id(),
                source_lifecycle: DraftMutationStagingLifecycleV1::Receiving,
                writer_admitted: false,
                candidate_generation: facts.0,
                root: facts.1,
                history: facts.2,
                session_revision: facts.3,
            },
            _ => DraftMutationStagingTerminalEvidenceV1::Error {
                error: DraftMutationStagingErrorEvidenceV1::Operational {
                    reason: DraftMutationStagingErrorReasonV1::Operational,
                    anchor: DraftMutationStagingTerminalAnchorV1::Begin(identity),
                },
                candidate_generation: facts.0,
                root: facts.1,
                history: facts.2,
                session_revision: facts.3,
            },
        };
        let prepared = storage
            .prepare_draft_mutation_terminal_before_begin(begin, &session, evidence)
            .unwrap();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
        ));
        let status = storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap();
        assert!(matches!(
            (evidence, status),
            (
                DraftMutationStagingTerminalEvidenceV1::Rejected { .. },
                DraftMutationStagingStatusV1::Rejected { .. }
            ) | (
                DraftMutationStagingTerminalEvidenceV1::Conflict { .. },
                DraftMutationStagingStatusV1::Conflict { .. }
            ) | (
                DraftMutationStagingTerminalEvidenceV1::Cancelled { .. },
                DraftMutationStagingStatusV1::Cancelled { .. }
            ) | (
                DraftMutationStagingTerminalEvidenceV1::Error { .. },
                DraftMutationStagingStatusV1::Error { .. }
            )
        ));
    }

    let identity = staging_identity(&session, 40);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let admitted_conflict = DraftMutationStagingTerminalEvidenceV1::Conflict {
        expected_generation: head.begin().predecessor_candidate_generation(),
        expected_root: head.begin().predecessor_root(),
        expected_history: head.begin().predecessor_history(),
        observed_generation: session.newest_candidate_generation(),
        observed_root: session.newest_root(),
        observed_history: session.newest_history(),
        session_revision: session.session_generation(),
    };
    assert!(matches!(
        storage.prepare_draft_mutation_staging_terminal(&head, &session, admitted_conflict),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Receiving { .. }
    ));
    let evidence = DraftMutationStagingTerminalEvidenceV1::Cancelled {
        request_id: identity.operation_id(),
        source_lifecycle: DraftMutationStagingLifecycleV1::Receiving,
        writer_admitted: true,
        candidate_generation: session.newest_candidate_generation(),
        root: session.newest_root(),
        history: session.newest_history(),
        session_revision: session.session_generation(),
    };
    let terminal = storage
        .prepare_draft_mutation_staging_terminal(&head, &session, evidence)
        .unwrap();
    session = terminal.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
    ));
    assert!(matches!(
        storage.draft_mutation_staging_status(&store, identity).unwrap(),
        DraftMutationStagingStatusV1::Cancelled { evidence: stored, .. } if stored == evidence
    ));

    let conflict_identity = staging_identity(&session, 41);
    let stale_begin = begin_input(conflict_identity, &session);
    let advanced = advance_candidate(storage, &store, &session, 42);
    assert!(matches!(
        storage.draft_mutation_staging_status(&store, identity).unwrap(),
        DraftMutationStagingStatusV1::Cancelled { evidence: stored, .. } if stored == evidence
    ));
    assert_eq!(advanced.draft_id(), session.draft_id());
    assert_eq!(advanced.session_id(), session.session_id());
    assert!(advanced.active_operation().is_none(), "{advanced:?}");
    assert_ne!(
        (
            stale_begin.predecessor_candidate_generation(),
            stale_begin.predecessor_root(),
            stale_begin.predecessor_history(),
        ),
        (
            advanced.newest_candidate_generation(),
            advanced.newest_root(),
            advanced.newest_history(),
        )
    );
    let conflict = DraftMutationStagingTerminalEvidenceV1::Conflict {
        expected_generation: stale_begin.predecessor_candidate_generation(),
        expected_root: stale_begin.predecessor_root(),
        expected_history: stale_begin.predecessor_history(),
        observed_generation: advanced.newest_candidate_generation(),
        observed_root: advanced.newest_root(),
        observed_history: advanced.newest_history(),
        session_revision: advanced.session_generation(),
    };
    let terminal = storage
        .prepare_draft_mutation_terminal_before_begin(stale_begin, &advanced, conflict)
        .unwrap();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
    ));
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, conflict_identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Conflict { evidence, .. } if evidence == conflict
    ));
}

#[test]
fn occupied_next_fork_cursor_totals_and_overflow_fail_closed() {
    let (_home, store, storage, thread) = fixture("collisions-overflow", 41);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 42, 43);
    let identity = staging_identity(&session, 44);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let source_head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert!(matches!(
        storage.prepare_draft_mutation_staging_page(
            &source_head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            0,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("x".to_owned())],
                ),
            )]),
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    let max_items = (0..256)
        .map(|offset| DraftMutationStagingPageItemV1::SourcePosition(point(offset)))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    assert!(
        storage
            .prepare_draft_mutation_staging_page(
                &source_head,
                &session,
                DraftMutationStagingLaneV1::Source,
                256,
                256,
                65_536,
                max_items,
            )
            .is_ok()
    );
    let too_many_items = (0..257)
        .map(|offset| DraftMutationStagingPageItemV1::SourcePosition(point(offset)))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    assert!(matches!(
        storage.prepare_draft_mutation_staging_page(
            &source_head,
            &session,
            DraftMutationStagingLaneV1::Source,
            257,
            256,
            65_536,
            too_many_items,
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    assert!(matches!(
        storage.prepare_draft_mutation_staging_page(
            &source_head,
            &session,
            DraftMutationStagingLaneV1::Source,
            1,
            257,
            65_536,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(0))]),
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    assert!(matches!(
        storage.prepare_draft_mutation_staging_page(
            &source_head,
            &session,
            DraftMutationStagingLaneV1::Source,
            1,
            256,
            65_537,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(0))]),
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    assert!(matches!(
        storage.prepare_draft_mutation_staging_page(
            &source_head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            65_536,
            Box::new([DraftMutationStagingPageItemV1::Proposal(
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("x".repeat(65_537))],
                ),
            )]),
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));

    let first_replacement =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]);
    let accepted = storage
        .prepare_draft_mutation_staging_page(
            &source_head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(first_replacement)]),
        )
        .unwrap();
    let collision = storage
        .prepare_draft_mutation_staging_page(
            &source_head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("b".to_owned())],
                ),
            )]),
        )
        .unwrap();
    let page = accepted.page().unwrap();
    assert_eq!(page.key().ordinal(), 1);
    assert_eq!(page.transition_ordinal(), 2);
    assert_eq!(page.input_cursor(), 0);
    assert_eq!(page.successor_cursor(), 1);
    assert_eq!(
        page.prior_cumulative_identity(),
        source_head.proposal().cumulative_identity()
    );
    assert_eq!(page.cumulative_item_total(), 1);
    assert!(page.cumulative_byte_total() > 0);
    session = accepted.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), accepted),
    ));
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(
                storage.revision(&store).unwrap(),
                collision.clone(),
            ),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(
        storage
            .reconcile_draft_mutation_staging_command(&store, &collision)
            .is_err()
    );

    let overflow_frontier = DraftMutationStagingLaneFrontierV1::new(
        source_head.proposal().next_cursor(),
        source_head.proposal().next_ordinal(),
        u64::MAX,
        source_head.proposal().canonical_byte_total(),
        source_head.proposal().cumulative_identity(),
    )
    .unwrap();
    let overflow_head = DraftMutationStagingHeadV1::from_parts(
        source_head.identity(),
        source_head.begin(),
        source_head.begin_digest(),
        source_head.source(),
        overflow_frontier,
        source_head.receipt(),
        source_head.lifecycle(),
        source_head.digest(),
    );
    assert!(matches!(
        storage.prepare_draft_mutation_staging_page(
            &overflow_head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(
                DraftPieceReplacementV1::continuation(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("c".to_owned())],
                ),
            )]),
        ),
        Err(DraftMutationStagingErrorV1::Overflow)
    ));
    let byte_overflow_frontier = DraftMutationStagingLaneFrontierV1::new(
        source_head.proposal().next_cursor(),
        source_head.proposal().next_ordinal(),
        source_head.proposal().item_total(),
        u64::MAX,
        source_head.proposal().cumulative_identity(),
    )
    .unwrap();
    let byte_overflow_head = DraftMutationStagingHeadV1::from_parts(
        source_head.identity(),
        source_head.begin(),
        source_head.begin_digest(),
        source_head.source(),
        byte_overflow_frontier,
        source_head.receipt(),
        source_head.lifecycle(),
        source_head.digest(),
    );
    assert!(matches!(
        storage.prepare_draft_mutation_staging_page(
            &byte_overflow_head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(
                DraftPieceReplacementV1::continuation(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("d".to_owned())],
                ),
            )]),
        ),
        Err(DraftMutationStagingErrorV1::Overflow)
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn occupied_next_error_is_derived_from_durable_canonical_bytes() {
    let (_home, store, storage, thread) = fixture("occupied-error", 84);
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 85, 86);
    let identity = staging_identity(&session, 87);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let page = |text: &str| {
        storage
            .prepare_draft_mutation_staging_page(
                &head,
                &session,
                DraftMutationStagingLaneV1::Proposal,
                1,
                1,
                1024,
                Box::new([DraftMutationStagingPageItemV1::Proposal(
                    DraftPieceReplacementV1::new(
                        point(0),
                        point(0),
                        vec![DraftPieceV1::Text(text.to_owned())],
                    ),
                )]),
            )
            .unwrap()
    };
    let stored = page("stored");
    let requested = page("requested");
    committed(execute(
        &store,
        inject_draft_mutation_staging_occupied_page(
            &store,
            storage,
            stored.page().unwrap().clone(),
        ),
    ));
    let terminal = storage
        .prepare_draft_mutation_staging_occupied_page_error(&store, &head, &session, &requested)
        .unwrap();
    let evidence = terminal.receipt().terminal_evidence().unwrap();
    let DraftMutationStagingTerminalEvidenceV1::Error {
        error:
            DraftMutationStagingErrorEvidenceV1::OccupiedIdentity {
                key: DraftMutationStagingOccupiedKeyV1::Page(key),
                stored_digest,
                requested_digest,
                first_difference,
                stored: stored_byte,
                requested: requested_byte,
            },
        ..
    } = evidence
    else {
        panic!("occupied page did not produce typed occupied evidence: {evidence:?}");
    };
    assert_eq!(key, stored.page().unwrap().key());
    assert_eq!(stored_digest, stored.page().unwrap().digest());
    assert_eq!(requested_digest, requested.page().unwrap().digest());
    assert_ne!(stored_digest, requested_digest);
    assert!(first_difference > 0);
    assert!(matches!(
        stored_byte,
        DraftMutationStagingComparedByteV1::Byte(_) | DraftMutationStagingComparedByteV1::End
    ));
    assert!(matches!(
        requested_byte,
        DraftMutationStagingComparedByteV1::Byte(_) | DraftMutationStagingComparedByteV1::End
    ));
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
    ));
    assert!(matches!(
        storage.draft_mutation_staging_status(&store, identity).unwrap(),
        DraftMutationStagingStatusV1::Error { evidence: stored, .. } if stored == evidence
    ));
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn admitted_rejected_and_operational_error_are_derived_and_exact() {
    {
        let (_home, store, storage, thread) = fixture("admitted-rejected", 88);
        let durable = current(storage, &store, thread);
        let mut session = open_session(storage, &store, &durable, 89, 90);
        let identity = staging_identity(&session, 91);
        let begin = storage
            .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
            .unwrap();
        session = begin.target_session().unwrap().clone();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
        ));
        let head = storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap()
            .unwrap();
        let requested = storage
            .prepare_draft_mutation_staging_page(
                &head,
                &session,
                DraftMutationStagingLaneV1::Proposal,
                1,
                1,
                1024,
                Box::new([DraftMutationStagingPageItemV1::Proposal(
                    DraftPieceReplacementV1::new(
                        point(0),
                        point(0),
                        vec![DraftPieceV1::Text("rejected".to_owned())],
                    ),
                )]),
            )
            .unwrap();
        let terminal = storage
            .prepare_draft_mutation_staging_rejected_page(&head, &session, &requested)
            .unwrap();
        let evidence = terminal.receipt().terminal_evidence().unwrap();
        assert!(matches!(
            evidence,
            DraftMutationStagingTerminalEvidenceV1::Rejected {
                reason: DraftMutationStagingRejectedReasonV1::InvalidPage,
                anchor: DraftMutationStagingTerminalAnchorV1::Page(key),
                digest,
                ..
            } if key == requested.page().unwrap().key() && digest == requested.page().unwrap().digest()
        ));
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
        ));
        assert!(matches!(
            storage.draft_mutation_staging_status(&store, identity).unwrap(),
            DraftMutationStagingStatusV1::Rejected { evidence: stored, .. } if stored == evidence
        ));
    }
    {
        let (_home, store, storage, thread) = fixture("admitted-error", 92);
        let durable = current(storage, &store, thread);
        let mut session = open_session(storage, &store, &durable, 93, 94);
        let identity = staging_identity(&session, 95);
        let begin = storage
            .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
            .unwrap();
        session = begin.target_session().unwrap().clone();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
        ));
        let head = storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap()
            .unwrap();
        let anchor = DraftMutationStagingTerminalAnchorV1::Finish(identity);
        let terminal = storage
            .prepare_draft_mutation_staging_operational_error(
                &head,
                &session,
                DraftMutationStagingErrorReasonV1::Operational,
                anchor,
            )
            .unwrap();
        let evidence = terminal.receipt().terminal_evidence().unwrap();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
        ));
        assert!(matches!(
            storage.draft_mutation_staging_status(&store, identity).unwrap(),
            DraftMutationStagingStatusV1::Error { evidence: stored, .. } if stored == evidence
        ));
    }
}

#[test]
fn begin_rejects_occupied_build_and_settlement_natural_identity() {
    let (_home, store, storage, thread) = fixture("occupied-begin", 45);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 46, 47);
    let identity = staging_identity(&session, 48);
    let replacement =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("x".to_owned())]);
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        identity.operation_id().as_piece_operation(),
        point(0),
        point(0),
        point(1),
        point(1),
        1,
        canonical_draft_piece_fragment_chain_v1(std::slice::from_ref(&replacement)),
    );
    let ordinary = storage
        .prepare_draft_piece_edit(&store, header, &session)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), ordinary.clone()),
    ));
    let occupied = execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), ordinary),
    );
    assert!(
        matches!(
            occupied,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{occupied:?}"
    );
    let staging = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), staging),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Absent
    );

    #[cfg(feature = "test-faults")]
    {
        let (_home, store, storage, thread) = fixture("occupied-candidate-root", 96);
        let current = current(storage, &store, thread);
        let session = open_session(storage, &store, &current, 97, 98);
        let identity = staging_identity(&session, 99);
        let root = session.newest_root();
        let occupied_root = DraftPieceRootReferenceV1::new(
            DraftPieceRootKeyV1::editor_candidate(
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            ),
            root.root_node(),
            root.summary(),
            root.marker_index_root(),
            root.marker_index_summary(),
            root.combined_digest(),
        );
        committed(execute(
            &store,
            inject_draft_piece_candidate_root_collision(
                &store,
                storage,
                occupied_root,
                DraftPieceCandidateRootCollision::Exact,
            ),
        ));
        let staging = storage
            .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
            .unwrap();
        assert!(matches!(
            execute(
                &store,
                storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), staging),
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .unwrap(),
            DraftMutationStagingStatusV1::Absent
        );
    }
}

#[test]
fn more_than_257_pages_keep_one_page_per_command_and_reopen_exactly() {
    let (home, store, storage, thread) = fixture("many-pages", 51);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 52, 53);
    let identity = staging_identity(&session, 54);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let mut chain = syndic_storage::canonical_empty_draft_piece_fragment_chain_v1();
    for page_ordinal in 1..=258_u64 {
        let replacement = if page_ordinal == 1 {
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("x".to_owned())],
            )
        } else {
            DraftPieceReplacementV1::continuation(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("x".to_owned())],
            )
        };
        chain = draft_piece_fragment_chain_link_v1(chain, page_ordinal, &replacement);
        let head = storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap()
            .unwrap();
        let prepared = storage
            .prepare_draft_mutation_staging_page(
                &head,
                &session,
                DraftMutationStagingLaneV1::Proposal,
                page_ordinal,
                1,
                1024,
                Box::new([DraftMutationStagingPageItemV1::Proposal(replacement)]),
            )
            .unwrap();
        assert_eq!(prepared.page().unwrap().items().len(), 1);
        assert_eq!(prepared.page().unwrap().key().ordinal(), page_ordinal);
        session = prepared.target_session().unwrap().clone();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
        ));
    }
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_ordinal(), 259);
    assert_eq!(head.proposal().item_total(), 258);
    assert_eq!(head.receipt().transition_ordinal(), 259);
    let finish = DraftMutationFinishInputV1::new(
        head.source(),
        head.proposal(),
        DraftLogicalExtentV1::new(258, 1),
        point(258),
        point(258),
        point(258),
        chain,
    );
    let finish = storage
        .prepare_draft_mutation_staging_finish(&head, &session, finish)
        .unwrap();
    session = finish.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), finish),
    ));
    let finished = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(finished.receipt().transition_ordinal(), 260);
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&finished, &session)
        .unwrap();
    committed(execute(
        &store,
        storage.transfer_draft_mutation_staging_to_builder(
            storage.revision(&store).unwrap(),
            transfer,
        ),
    ));
    for page_ordinal in 1..=258_u64 {
        let prepared = storage
            .prepare_next_durable_draft_piece_page(&store, identity)
            .unwrap()
            .unwrap();
        assert_eq!(prepared.lane(), DraftMutationStagingLaneV1::Proposal);
        assert_eq!(prepared.page_ordinal(), page_ordinal);
        assert_eq!(prepared.fragment_count(), 1);
        committed(execute(
            &store,
            storage
                .stage_next_durable_draft_piece_page(storage.revision(&store).unwrap(), prepared),
        ));
    }
    assert!(
        storage
            .prepare_next_durable_draft_piece_page(&store, identity)
            .unwrap()
            .is_none()
    );
    let before_reopen = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(before_reopen.receipt().transition_ordinal(), 261);
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(&store, identity)
        .unwrap()
    else {
        panic!("fully drained staging did not retain builder custody");
    };
    assert_eq!(build.key().transition_ordinal(), 259);
    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .draft_mutation_staging_head(&reopened, identity)
            .unwrap()
            .unwrap(),
        before_reopen,
    );
    assert!(matches!(
        reopened_storage
            .draft_mutation_staging_status(&reopened, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Building { .. }
    ));
    assert!(
        reopened_storage
            .prepare_next_durable_draft_piece_page(&reopened, identity)
            .unwrap()
            .is_none()
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn every_staging_command_class_reconciles_after_an_indeterminate_commit() {
    let home = TestHome::new("indeterminate-commands");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([61; 16]);
    let draft = SyndicDraftId::from_bytes([62; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase146",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 63, 64);

    let terminal_identity = staging_identity(&session, 65);
    let terminal_evidence = DraftMutationStagingTerminalEvidenceV1::Rejected {
        reason: DraftMutationStagingRejectedReasonV1::InvalidEnvelope,
        anchor: DraftMutationStagingTerminalAnchorV1::Begin(terminal_identity),
        digest: storage
            .prepare_draft_mutation_staging_begin(
                begin_input(terminal_identity, &session),
                &session,
            )
            .unwrap()
            .target_head()
            .begin_digest(),
        candidate_generation: session.newest_candidate_generation(),
        root: session.newest_root(),
        history: session.newest_history(),
        session_revision: session.session_generation(),
    };
    let terminal = storage
        .prepare_draft_mutation_terminal_before_begin(
            begin_input(terminal_identity, &session),
            &session,
            terminal_evidence,
        )
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
        ),
        CommandOutcome::Indeterminate { .. }
    ));
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, terminal_identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Rejected { .. }
    ));

    let identity = staging_identity(&session, 66);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
        ),
        CommandOutcome::Indeterminate { .. }
    ));

    let replacement =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("i".to_owned())]);
    let chain = draft_piece_fragment_chain_link_v1(
        syndic_storage::canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        &replacement,
    );
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let page = storage
        .prepare_draft_mutation_staging_page(
            &head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(replacement)]),
        )
        .unwrap();
    session = page.target_session().unwrap().clone();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), page),
        ),
        CommandOutcome::Indeterminate { .. }
    ));

    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &head,
            &session,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
                DraftLogicalExtentV1::new(1, 1),
                point(1),
                point(1),
                point(1),
                chain,
            ),
        )
        .unwrap();
    session = finish.target_session().unwrap().clone();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), finish),
        ),
        CommandOutcome::Indeterminate { .. }
    ));

    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &session)
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage.transfer_draft_mutation_staging_to_builder(
                storage.revision(&store).unwrap(),
                transfer,
            ),
        ),
        CommandOutcome::Indeterminate { .. }
    ));
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(&store, identity)
        .unwrap()
    else {
        panic!("atomic transfer did not publish all closure effects");
    };
    assert_eq!(build.key().transition_ordinal(), 1);

    let page = storage
        .prepare_next_durable_draft_piece_page(&store, identity)
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage.stage_next_durable_draft_piece_page(storage.revision(&store).unwrap(), page),
        ),
        CommandOutcome::Indeterminate { .. }
    ));
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(&store, identity)
        .unwrap()
    else {
        panic!("durable-page construction did not remain building");
    };
    assert_eq!(build.key().transition_ordinal(), 2);
    assert_eq!(current(storage, &store, thread), durable);
}

#[cfg(feature = "test-faults")]
#[test]
fn missing_and_replaced_staging_closure_records_fail_closed() {
    {
        let (_home, store, storage, identity, _, _) = staged_page_fixture("missing-head", 70);
        committed(execute(
            &store,
            delete_draft_mutation_staging_head(&store, storage, identity),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, page, _) = staged_page_fixture("missing-page", 71);
        committed(execute(
            &store,
            delete_draft_mutation_staging_page(&store, storage, page),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, page, _) = staged_page_fixture("replaced-page", 72);
        committed(execute(
            &store,
            inject_draft_mutation_staging_page_digest_corruption(&store, storage, page),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, _, receipt) =
            staged_page_fixture("missing-receipt", 73);
        committed(execute(
            &store,
            delete_draft_mutation_staging_receipt(&store, storage, receipt),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, _, receipt) =
            staged_page_fixture("replaced-receipt", 74);
        committed(execute(
            &store,
            inject_draft_mutation_staging_receipt_digest_corruption(&store, storage, receipt),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, _, _) =
            staged_page_fixture("replaced-prior-receipt", 77);
        let prior = DraftMutationStagingProgressReceiptKeyV1::new(identity, 1).unwrap();
        committed(execute(
            &store,
            inject_draft_mutation_staging_receipt_digest_corruption(&store, storage, prior),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, _, _) = staged_page_fixture("corrupt-head", 78);
        committed(execute(
            &store,
            inject_draft_mutation_staging_head_digest_corruption(&store, storage, identity),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, _, _) = staged_page_fixture("head-ahead", 75);
        committed(execute(
            &store,
            inject_draft_mutation_staging_head_ahead(&store, storage, identity),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, identity, _, _) = staged_page_fixture("head-fork", 76);
        committed(execute(
            &store,
            inject_draft_mutation_staging_head_fork(&store, storage, identity),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn staging_session_corruption_and_disposal_are_closed() {
    {
        let (_home, store, storage, thread) = fixture("session-corruption", 100);
        let durable = current(storage, &store, thread);
        let mut session = open_session(storage, &store, &durable, 101, 102);
        let identity = staging_identity(&session, 103);
        let begin = storage
            .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
            .unwrap();
        session = begin.target_session().unwrap().clone();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
        ));
        assert!(matches!(
            execute(
                &store,
                storage.test_dispose_draft_editor_candidate_session(
                    storage.revision(&store).unwrap(),
                    session.draft_id(),
                    session.session_id(),
                ),
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        committed(execute(
            &store,
            inject_draft_piece_session_generation_inflation(
                &store,
                storage,
                session.draft_id(),
                session.session_id(),
            ),
        ));
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }
    {
        let (_home, store, storage, thread) = fixture("terminal-disposal", 104);
        let durable = current(storage, &store, thread);
        let mut session = open_session(storage, &store, &durable, 105, 106);
        let identity = staging_identity(&session, 107);
        let begin = storage
            .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
            .unwrap();
        session = begin.target_session().unwrap().clone();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
        ));
        let head = storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap()
            .unwrap();
        let evidence = DraftMutationStagingTerminalEvidenceV1::Cancelled {
            request_id: identity.operation_id(),
            source_lifecycle: head.lifecycle(),
            writer_admitted: true,
            candidate_generation: session.newest_candidate_generation(),
            root: session.newest_root(),
            history: session.newest_history(),
            session_revision: session.session_generation(),
        };
        let terminal = storage
            .prepare_draft_mutation_staging_terminal(&head, &session, evidence)
            .unwrap();
        session = terminal.target_session().unwrap().clone();
        committed(execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
        ));
        committed(execute(
            &store,
            storage.test_dispose_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                session.draft_id(),
                session.session_id(),
            ),
        ));
        assert!(matches!(
            storage.draft_mutation_staging_status(&store, identity).unwrap(),
            DraftMutationStagingStatusV1::Cancelled { evidence: stored, .. } if stored == evidence
        ));
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn page_ceiling_commitment_fails_decode_status_and_reconstruction() {
    {
        let (_home, store, storage, identity, page_key, _) =
            staged_page_fixture("page-ceiling-status", 128);
        committed(execute(
            &store,
            inject_draft_mutation_staging_page_ceiling_corruption(&store, storage, page_key),
        ));
        assert!(
            storage
                .draft_mutation_staging_page(&store, page_key)
                .is_err()
        );
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
    }

    let (_home, store, storage, thread) = fixture("page-ceiling-builder", 129);
    let before = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &before, 130, 131);
    let identity = staging_identity(&session, 132);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let replacement =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("c".into())]);
    let chain = draft_piece_fragment_chain_link_v1(
        canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        &replacement,
    );
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let page = storage
        .prepare_draft_mutation_staging_page(
            &head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(replacement)]),
        )
        .unwrap();
    let page_key = page.page().unwrap().key();
    session = page.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), page),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let finish = DraftMutationFinishInputV1::new(
        head.source(),
        head.proposal(),
        DraftLogicalExtentV1::new(1, 1),
        point(1),
        point(1),
        point(1),
        chain,
    );
    let prepared = storage
        .prepare_draft_mutation_staging_finish(&head, &session, finish)
        .unwrap();
    session = prepared.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), prepared),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &session)
        .unwrap();
    committed(execute(
        &store,
        storage.transfer_draft_mutation_staging_to_builder(
            storage.revision(&store).unwrap(),
            transfer,
        ),
    ));
    committed(execute(
        &store,
        inject_draft_mutation_staging_page_ceiling_corruption(&store, storage, page_key),
    ));
    assert!(
        storage
            .prepare_next_durable_draft_piece_page(&store, identity)
            .is_err()
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn terminal_status_and_replay_reject_same_operation_custody() {
    let (_home, store, storage, thread) = fixture("terminal-same-custody", 133);
    let before = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &before, 134, 135);
    let identity = staging_identity(&session, 136);
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let evidence = DraftMutationStagingTerminalEvidenceV1::Cancelled {
        request_id: identity.operation_id(),
        source_lifecycle: head.lifecycle(),
        writer_admitted: true,
        candidate_generation: session.newest_candidate_generation(),
        root: session.newest_root(),
        history: session.newest_history(),
        session_revision: session.session_generation(),
    };
    let terminal = storage
        .prepare_draft_mutation_staging_terminal(&head, &session, evidence)
        .unwrap();
    let replay = terminal.clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
    ));
    assert!(matches!(
        storage.draft_mutation_staging_status(&store, identity).unwrap(),
        DraftMutationStagingStatusV1::Cancelled { evidence: stored, .. } if stored == evidence
    ));
    committed(execute(
        &store,
        inject_draft_mutation_terminal_same_operation_custody(&store, storage, identity),
    ));
    assert!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .is_err()
    );
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), replay),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
}

fn fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase146",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}

#[cfg(feature = "test-faults")]
fn staged_page_fixture(
    name: &str,
    seed: u8,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    DraftMutationStagingIdentityV1,
    DraftMutationStagingPageKeyV1,
    DraftMutationStagingProgressReceiptKeyV1,
) {
    let (home, store, storage, thread) = fixture(name, seed);
    let current = current(storage, &store, thread);
    let mut session = open_session(
        storage,
        &store,
        &current,
        seed.wrapping_add(2),
        seed.wrapping_add(3),
    );
    let identity = staging_identity(&session, seed.wrapping_add(4));
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let page = storage
        .prepare_draft_mutation_staging_page(
            &head,
            &session,
            DraftMutationStagingLaneV1::Proposal,
            1,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::Proposal(
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("f".to_owned())],
                ),
            )]),
        )
        .unwrap();
    let page_key = page.page().unwrap().key();
    let receipt_key = page.receipt().key();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), page),
    ));
    (home, store, storage, identity, page_key, receipt_key)
}

fn open_session(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
    session: u8,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector(current),
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
    );
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(store, request)
        .unwrap();
    let outcome = execute(
        store,
        storage.open_draft_editor_candidate_session(
            storage.revision(store).unwrap(),
            prepared.clone(),
        ),
    );
    match storage
        .reconcile_draft_editor_candidate_session_open(store, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
        | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => head,
        other => panic!("session did not open: {other:?}"),
    }
}

fn advance_candidate(
    storage: SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let replacement = DraftPieceReplacementV1::new(
        point(0),
        point(0),
        vec![DraftPieceV1::Text("history".to_owned())],
    );
    let chain = canonical_draft_piece_fragment_chain_v1(std::slice::from_ref(&replacement));
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        point(0),
        point(0),
        point(7),
        point(7),
        1,
        chain,
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, session)
        .unwrap();
    let fragment = storage
        .prepare_draft_piece_fragment(
            &prepared,
            1,
            canonical_empty_draft_piece_fragment_chain_v1(),
            replacement,
        )
        .unwrap();
    committed(execute(
        store,
        storage.begin_draft_piece_edit(storage.revision(store).unwrap(), prepared.clone()),
    ));
    committed(execute(
        store,
        storage.stage_draft_piece_fragment(
            storage.revision(store).unwrap(),
            prepared.clone(),
            fragment,
        ),
    ));
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            session.draft_id(),
            session.session_id(),
            header.operation_id(),
        )
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    match storage
        .draft_editor_candidate_session(store, session.draft_id(), session.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        other => panic!("candidate did not advance: {other:?}"),
    }
}

fn current(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> syndic_storage::SyndicCurrentDraft {
    storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
}

fn selector(current: &syndic_storage::SyndicCurrentDraft) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    )
}

fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn committed(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn replay_succeeded(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::NotCommitted { .. }
            | CommandOutcome::Committed {
                later_failure: None,
                ..
            }
    ));
}

fn staging_identity(
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> DraftMutationStagingIdentityV1 {
    DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([operation; 16]),
    )
}

fn begin_input(
    identity: DraftMutationStagingIdentityV1,
    session: &DraftEditorCandidateSessionV1,
) -> DraftMutationBeginV1 {
    DraftMutationBeginV1::new(
        identity,
        session.session_generation(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        session.logical_extent(),
        point(0),
        point(0),
        point(0),
        point(0),
        point(0),
        0,
        0,
    )
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}
