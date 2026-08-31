#![cfg(feature = "test-faults")]

include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use sha2::{Digest, Sha256};
use syndic_storage::{
    DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerLabelReadinessDispositionV1,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceSelectorV1, DraftMutationStagingErrorV1,
    DraftMutationStagingLifecycleV1, DraftMutationStagingTerminalEvidenceV1,
    DraftPieceErrorReasonV1, DraftPieceReconciledCommandV1, DraftPieceRootBuildIdentityV1,
    DraftPieceRootReferenceV1, DraftPieceSettlementKeyV1, DraftPieceSettlementOutcomeV1,
    DraftPieceTransactionOutcomeV1,
};

#[path = "phase216_draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;
#[path = "phase225_draft_marker_writer_admission/support.rs"]
mod support;

use readiness_support::{association, owner};
use support::{
    begin_admitted_marker_edit, complete_admitted_marker_edit, complete_admitted_marker_edits,
    fixture_with_history_budget, marked_session, ready_proof, snapshot, stage_admitted_marker_edit,
};

#[test]
fn exact_marker_insert_consumes_the_target_and_settles_then_releases_writer_custody() {
    let (_home, store, storage, thread) = fixture("phase225-happy", 1);
    let (session, source) = marked_session(&storage, &store, thread, 2);
    let admission = owner(&session, 10);
    let baseline = snapshot(&storage, &store, admission);
    let baseline_charge = baseline.capacity().map(|capacity| capacity.charge());
    assert!(baseline.head().is_none());
    assert!(baseline.receipt().is_none());
    let target = marker_with_source_label_and_asset(4, 0, source);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        5,
        vec![association(4, &session, source.marker_id())],
    );

    let next = complete_admitted_marker_edit(
        &storage,
        &store,
        &session,
        admission,
        proof,
        insert_target(target),
    );

    let terminal = snapshot(&storage, &store, admission);
    let head = terminal
        .head()
        .expect("writer settlement keeps its terminal head");
    assert_eq!(head.lifecycle(), DraftMarkerAdmissionLifecycleV1::Settled);
    assert_eq!(head.target_root().count(), 0);
    assert_eq!(head.remaining_builder_count(), 0);
    assert_eq!(terminal.capacity().unwrap().charge().associations(), 0);
    assert_eq!(
        next.newest_candidate_generation(),
        session.newest_candidate_generation() + 1
    );
    storage
        .release_settled_draft_marker_writer(&store, admission)
        .expect("exact settled empty writer durably reclaims its owner and attachment");
    let released = snapshot(&storage, &store, admission);
    assert!(released.head().is_none());
    assert!(released.receipt().is_none());
    assert_eq!(
        released.capacity().map(|capacity| capacity.charge()),
        baseline_charge
    );
    assert!(
        storage
            .release_settled_draft_marker_writer(&store, admission)
            .is_err()
    );

    let next_admission = owner(&next, 12);
    let next_target = marker_with_source_label_and_asset(6, 2, source);
    let next_proof = ready_proof(
        &storage,
        &store,
        next_admission,
        7,
        vec![association(6, &next, source.marker_id())],
    );
    complete_admitted_marker_edit(
        &storage,
        &store,
        &next,
        next_admission,
        next_proof,
        insert_target_after_all(next_target),
    );
    storage
        .release_settled_draft_marker_writer(&store, next_admission)
        .expect("reclaimed admission capacity accepts a second ordinary writer");
    let reused = snapshot(&storage, &store, next_admission);
    assert!(reused.head().is_none());
    assert!(reused.receipt().is_none());
    assert_eq!(
        reused.capacity().map(|capacity| capacity.charge()),
        baseline_charge
    );
}

#[test]
fn history_capacity_refusal_terminalizes_writer_without_publishing_candidate_history_or_protection()
{
    let (_home, store, storage, thread) =
        fixture_with_history_budget("phase225-history-capacity", 13, 1_520, 1);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 14, 15);
    let session = complete_staged(
        &storage,
        &store,
        &session,
        16,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let target = marker(17, 1, 7);
    let admission = owner(&session, 24);
    let proof = storage
        .seed_draft_marker_writer_ready_target_for_test(&store, &session, admission, target)
        .unwrap();
    let (prepared, identity, fragments) = stage_admitted_marker_edit(
        &storage,
        &store,
        &session,
        admission,
        proof,
        insert_first_target(target),
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
    {
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    let outcome = execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        &outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let reconciled = storage
        .reconcile_draft_piece_command_outcome(&store, &prepared, outcome, |start| {
            fragments
                .iter()
                .skip((start - 1) as usize)
                .cloned()
                .collect()
        })
        .unwrap();
    assert!(
        matches!(
            reconciled,
            DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Error(_))
        ),
        "history-capacity reconciliation disagreed: {reconciled:?}"
    );
    let DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Settled(settlement)) =
        storage
            .draft_piece_operation_status_page(&store, &prepared, 1, &fragments)
            .unwrap()
    else {
        panic!("history-capacity settlement was not durably terminal")
    };
    assert!(matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Error(DraftPieceErrorReasonV1::HistoryCapacityUnavailable)
    ));
    let preserved = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_eq!(
        preserved.newest_candidate_generation(),
        session.newest_candidate_generation()
    );
    assert_eq!(preserved.newest_root(), session.newest_root());
    assert_eq!(preserved.newest_history(), session.newest_history());
    let terminal = snapshot(&storage, &store, admission);
    let writer = terminal.head().unwrap();
    assert_eq!(
        writer.lifecycle(),
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup
    );
    assert_eq!(writer.target_root().count(), 0);
    assert_eq!(writer.remaining_builder_count(), 0);
    assert!(terminal.receipt().is_some());
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&store)
            .unwrap(),
        Some(admission)
    );

    let replay = execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert!(matches!(replay, CommandOutcome::NotCommitted { .. }));
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &prepared, replay, |start| {
                fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect()
            })
            .unwrap(),
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Error(_))
    ));
    let cleanup = storage.advance_draft_marker_admission_cleanup(
        &store,
        admission,
        syndic_storage::DraftMarkerAdmissionCommandIdV1::from_bytes([17; 16]),
    );
    assert!(
        matches!(
            cleanup,
            syndic_storage::DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure
        ),
        "history-capacity compact cleanup did not retain its exact closure"
    );
    let compact = snapshot(&storage, &store, admission);
    assert_eq!(compact.head().unwrap().charge().associations(), 0);
    assert_eq!(compact.capacity().unwrap().charge().associations(), 0);
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&store)
            .unwrap(),
        None
    );
}

#[test]
fn wrong_target_label_cannot_advance_build_or_consume_admission() {
    let (_home, store, storage, thread) = fixture("phase225-wrong-label", 20);
    let (session, source) = marked_session(&storage, &store, thread, 21);
    let admission = owner(&session, 30);
    let target = marker_with_source_label_and_asset(23, 0, source);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        24,
        vec![association(23, &session, source.marker_id())],
    );
    let wrong = syndic_storage::DraftPieceMarkerV1::new(
        target.marker_id(),
        target.order_key(),
        beryl_model::ImageLabelOrdinal::new(target.label().get() + 1).unwrap(),
        target.asset_id(),
    );
    let (_prepared, identity, _) = stage_admitted_marker_edit(
        &storage,
        &store,
        &session,
        admission,
        proof,
        insert_target(wrong),
    );
    loop {
        match storage.prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        ) {
            Ok(Some(advance)) => committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
            )),
            Ok(None) => panic!("mismatched target completed without point authentication"),
            Err(DraftPiecePrepareErrorV1::InvalidRoot) => break,
            Err(error) => {
                panic!("mismatched target failed outside point authentication: {error:?}")
            }
        }
    }
    let before = snapshot(&storage, &store, admission);
    let before_head = before.head().unwrap();
    assert_eq!(
        before_head.lifecycle(),
        DraftMarkerAdmissionLifecycleV1::Building
    );
    assert_eq!(before_head.target_root().count(), 1);

    assert!(matches!(
        storage.prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        ),
        Err(DraftPiecePrepareErrorV1::InvalidRoot)
    ));
    let after = snapshot(&storage, &store, admission);
    assert_eq!(after.head().unwrap().digest(), before_head.digest());
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Building { .. }
    ));
}

#[test]
fn cloned_consuming_advance_replays_without_consuming_the_target_twice() {
    let (_home, store, storage, thread) = fixture("phase225-consumption-replay", 90);
    let (session, source) = marked_session(&storage, &store, thread, 91);
    let admission = owner(&session, 100);
    let target = marker_with_source_label_and_asset(101, 0, source);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        102,
        vec![association(101, &session, source.marker_id())],
    );
    let (prepared, identity, _) = stage_admitted_marker_edit(
        &storage,
        &store,
        &session,
        admission,
        proof,
        insert_target(target),
    );

    loop {
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .unwrap()
            .expect("unfinished admitted build produces a quantum");
        let replay = advance.clone();
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
        let once = snapshot(&storage, &store, admission);
        let once_head = once.head().unwrap();
        let consumed = once_head.target_root().count() == 0;
        let digest = once_head.digest();
        let capacity = once.capacity().unwrap().digest();
        let replay_outcome = execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), replay),
        );
        assert!(matches!(
            replay_outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::EmptyContribution { .. }
            }
        ));
        let replayed = snapshot(&storage, &store, admission);
        assert_eq!(replayed.head().unwrap().digest(), digest);
        assert_eq!(replayed.capacity().unwrap().digest(), capacity);
        if consumed {
            break;
        }
    }
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
    {
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    let settled = snapshot(&storage, &store, admission);
    assert_eq!(
        settled.head().unwrap().lifecycle(),
        DraftMarkerAdmissionLifecycleV1::Settled
    );
    assert_eq!(settled.head().unwrap().target_root().count(), 0);
}

#[test]
fn zero_target_removal_only_edit_adopts_through_the_ordinary_empty_ready_proof() {
    let (_home, store, storage, thread) = fixture("phase225-empty-ready", 120);
    let (session, marker) = marked_session(&storage, &store, thread, 121);
    let admission = owner(&session, 130);
    let proof = ready_proof(&storage, &store, admission, 122, Vec::new());
    let position = syndic_storage::DraftCompositePositionV1::new(
        1,
        syndic_storage::DraftCompositeGapWitnessV1::BeforeAll,
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
        .unwrap()
        .unwrap();

    let next = complete_admitted_marker_edit(
        &storage,
        &store,
        &session,
        admission,
        proof,
        DraftPieceReplacementV1::new(position, position, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(marker),
            },
        ),
    );

    let settled = snapshot(&storage, &store, admission);
    let head = settled.head().unwrap();
    assert_eq!(head.lifecycle(), DraftMarkerAdmissionLifecycleV1::Settled);
    assert_eq!(head.target_root().count(), 0);
    assert_eq!(head.remaining_builder_count(), 0);
    assert_eq!(settled.capacity().unwrap().charge().associations(), 0);
    assert_eq!(
        next.newest_candidate_generation(),
        session.newest_candidate_generation() + 1
    );
}

#[test]
fn reversed_marker_effect_order_consumes_each_ready_association_once() {
    let (_home, store, storage, thread) = fixture("phase225-reversed-effects", 105);
    let (session, source) = marked_session(&storage, &store, thread, 106);
    let admission = owner(&session, 115);
    let first = marker_with_source_label_and_asset(107, 0, source);
    let second = marker_with_source_label_and_asset(108, 2, source);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        109,
        vec![
            association(108, &session, source.marker_id()),
            association(107, &session, source.marker_id()),
        ],
    );

    complete_admitted_marker_edits(
        &storage,
        &store,
        &session,
        admission,
        proof,
        vec![insert_target(first), insert_target_after_all(second)],
    );

    let settled = snapshot(&storage, &store, admission);
    let head = settled.head().unwrap();
    assert_eq!(head.lifecycle(), DraftMarkerAdmissionLifecycleV1::Settled);
    assert_eq!(head.target_root().count(), 0);
    assert_eq!(head.remaining_builder_count(), 0);
    assert_eq!(settled.capacity().unwrap().charge().associations(), 0);
}

#[test]
fn proof_substitution_is_rejected_before_staging_and_text_only_begin_has_no_writer() {
    let (_home, store, storage, thread) = fixture("phase225-begin-boundary", 40);
    let (session, source) = marked_session(&storage, &store, thread, 41);
    let admission = owner(&session, 50);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        43,
        vec![association(44, &session, source.marker_id())],
    );
    let substituted = syndic_storage::DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([250; 16]),
    );
    let begin = begin_input(substituted, &session);

    assert!(matches!(
        storage.prepare_draft_mutation_staging_marker_begin(begin, &session, proof),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    assert!(snapshot(&storage, &store, admission).head().is_some());
    assert!(
        storage
            .draft_mutation_staging_head(&store, substituted)
            .unwrap()
            .is_none()
    );

    let identity = syndic_storage::DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([47; 16]),
    );
    let text = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), text),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert!(head.begin().writer_admission().is_none());
}

#[test]
fn admitted_staging_command_and_page_batch_replay_as_empty_contributions() {
    let (_home, store, storage, thread) = fixture("phase225-admitted-staging-replay", 45);
    let (session, source) = marked_session(&storage, &store, thread, 46);
    let admission = owner(&session, 56);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        48,
        vec![association(49, &session, source.marker_id())],
    );
    let identity = syndic_storage::DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes(*admission.operation_id().as_bytes()),
    );
    let begin = storage
        .prepare_draft_mutation_staging_marker_begin(
            begin_input(identity, &session),
            &session,
            proof,
        )
        .unwrap();
    let active = begin.target_session().unwrap().clone();
    let begin_outcome = execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin.clone()),
    );
    assert!(matches!(
        &begin_outcome,
        CommandOutcome::Committed {
            later_failure: None,
            local_finalization: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_command_outcome(&store, &begin, begin_outcome)
            .unwrap(),
        syndic_storage::DraftMutationStagingReconcileV1::TargetSelected
    );
    let begin_snapshot = snapshot(&storage, &store, admission);
    let begin_head = begin_snapshot.head().unwrap().digest();
    let begin_receipt = begin_snapshot.receipt().map(|receipt| receipt.digest());
    let begin_capacity = begin_snapshot.capacity().unwrap().digest();
    let begin_replay = execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin.clone()),
    );
    assert!(matches!(
        &begin_replay,
        CommandOutcome::NotCommitted {
            evidence: CommandError::EmptyContribution { .. }
        }
    ));
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_command_outcome(&store, &begin, begin_replay)
            .unwrap(),
        syndic_storage::DraftMutationStagingReconcileV1::TargetSelected
    );
    assert_eq!(
        storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap(),
        Some(begin.target_head().clone())
    );
    let replayed_begin = snapshot(&storage, &store, admission);
    assert_eq!(replayed_begin.head().unwrap().digest(), begin_head);
    assert_eq!(
        replayed_begin.receipt().map(|receipt| receipt.digest()),
        begin_receipt
    );
    assert_eq!(replayed_begin.capacity().unwrap().digest(), begin_capacity);

    let page = storage
        .prepare_draft_mutation_staging_page_batch(
            begin.target_head(),
            &active,
            Box::new([DraftMutationStagingPageInputV1::new(
                DraftMutationStagingLaneV1::Proposal,
                begin.target_head().proposal().next_cursor(),
                begin.target_head().proposal().next_cursor() + 1,
                1,
                65_536,
                Box::new([DraftMutationStagingPageItemV1::Proposal(
                    DraftPieceReplacementV1::new(
                        point(1),
                        point(1),
                        vec![DraftPieceV1::Text("replay".to_owned())],
                    ),
                )]),
            )]),
        )
        .unwrap();
    let page_outcome = execute(
        &store,
        storage.draft_mutation_staging_page_batch(storage.revision(&store).unwrap(), page.clone()),
    );
    assert!(matches!(
        &page_outcome,
        CommandOutcome::Committed {
            later_failure: None,
            local_finalization: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_page_batch_outcome(&store, &page, page_outcome)
            .unwrap(),
        syndic_storage::DraftMutationStagingReconcileV1::TargetSelected
    );
    let page_snapshot = snapshot(&storage, &store, admission);
    let page_head = page_snapshot.head().unwrap().digest();
    let page_receipt = page_snapshot.receipt().map(|receipt| receipt.digest());
    let page_capacity = page_snapshot.capacity().unwrap().digest();
    let page_replay = execute(
        &store,
        storage.draft_mutation_staging_page_batch(storage.revision(&store).unwrap(), page.clone()),
    );
    assert!(matches!(
        &page_replay,
        CommandOutcome::NotCommitted {
            evidence: CommandError::EmptyContribution { .. }
        }
    ));
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_page_batch_outcome(&store, &page, page_replay)
            .unwrap(),
        syndic_storage::DraftMutationStagingReconcileV1::TargetSelected
    );
    assert_eq!(
        storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap(),
        Some(page.target_head().clone())
    );
    let replayed_page = snapshot(&storage, &store, admission);
    assert_eq!(replayed_page.head().unwrap().digest(), page_head);
    assert_eq!(
        replayed_page.receipt().map(|receipt| receipt.digest()),
        page_receipt
    );
    assert_eq!(replayed_page.capacity().unwrap().digest(), page_capacity);
}

#[test]
fn admission_free_staging_and_direct_builder_reject_every_marker_effect() {
    let (_home, store, storage, thread) = fixture("phase225-admission-free-markers", 47);
    let (session, source) = marked_session(&storage, &store, thread, 48);
    let identity = syndic_storage::DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([60; 16]),
    );
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    let active = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert!(head.begin().writer_admission().is_none());

    let text = storage.prepare_draft_mutation_staging_page_batch(
        &head,
        &active,
        Box::new([DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Proposal,
            head.proposal().next_cursor(),
            head.proposal().next_cursor() + 1,
            1,
            65_536,
            Box::new([DraftMutationStagingPageItemV1::Proposal(
                DraftPieceReplacementV1::new(
                    point(1),
                    point(1),
                    vec![DraftPieceV1::Text("text-only".to_owned())],
                ),
            )]),
        )]),
    );
    assert!(text.is_ok());

    let position = syndic_storage::DraftCompositePositionV1::new(
        1,
        syndic_storage::DraftCompositeGapWitnessV1::BeforeAll,
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), source.marker_id())
        .unwrap()
        .unwrap();
    let replaced = syndic_storage::DraftPieceMarkerV1::new(
        source.marker_id(),
        2,
        source.label(),
        source.asset_id(),
    );
    let marker_effects = vec![
        insert_target(marker_with_source_label_and_asset(61, 0, source)),
        DraftPieceReplacementV1::new(position, position, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(source),
            },
        ),
        DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(source)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Move {
                removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                insertion: DraftPieceMarkerInsertionV1::new(
                    1,
                    source,
                    DraftPieceMarkerEffectChargesV1::for_marker(source),
                ),
            }),
        DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(replaced)])
            .with_marker_effect(DraftPieceMarkerEffectV1::SameIdReplacement {
                removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                insertion: DraftPieceMarkerInsertionV1::new(
                    1,
                    replaced,
                    DraftPieceMarkerEffectChargesV1::for_marker(replaced),
                ),
            }),
    ];
    for replacement in marker_effects {
        assert!(matches!(
            storage.prepare_draft_mutation_staging_page_batch(
                &head,
                &active,
                Box::new([DraftMutationStagingPageInputV1::new(
                    DraftMutationStagingLaneV1::Proposal,
                    head.proposal().next_cursor(),
                    head.proposal().next_cursor() + 1,
                    1,
                    65_536,
                    Box::new([DraftMutationStagingPageItemV1::Proposal(replacement)]),
                )]),
            ),
            Err(DraftMutationStagingErrorV1::Invalid)
        ));
        assert_eq!(
            storage
                .draft_mutation_staging_head(&store, identity)
                .unwrap()
                .unwrap(),
            head
        );
    }

    let direct_text = DraftPieceReplacementV1::new(
        point(1),
        point(1),
        vec![DraftPieceV1::Text("direct-text".to_owned())],
    );
    let direct_header = syndic_storage::DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        DraftPieceOperationIdV1::from_bytes([62; 16]),
        point(0),
        point(0),
        point(1),
        point(1),
        1,
        syndic_storage::canonical_draft_piece_fragment_chain_v1(&[direct_text.clone()]),
    );
    let direct = storage
        .prepare_draft_piece_edit(&store, direct_header, &session)
        .unwrap();
    assert!(
        storage
            .prepare_draft_piece_fragment(
                &direct,
                1,
                canonical_empty_draft_piece_fragment_chain_v1(),
                direct_text,
            )
            .is_ok()
    );
    assert!(matches!(
        storage.prepare_draft_piece_fragment(
            &direct,
            1,
            canonical_empty_draft_piece_fragment_chain_v1(),
            insert_target(marker_with_source_label_and_asset(63, 0, source)),
        ),
        Err(DraftPiecePrepareErrorV1::InvalidRoot)
    ));
}

#[test]
fn admitted_staging_cancellation_writes_the_terminal_admission_disposition() {
    let (_home, store, storage, thread) = fixture("phase225-staging-cancel", 60);
    let (session, source) = marked_session(&storage, &store, thread, 61);
    let admission = owner(&session, 70);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        63,
        vec![association(64, &session, source.marker_id())],
    );
    let (identity, active, head) =
        begin_admitted_marker_edit(&storage, &store, &session, admission, proof);
    let terminal = storage
        .prepare_draft_mutation_staging_terminal(
            &head,
            &active,
            DraftMutationStagingTerminalEvidenceV1::Cancelled {
                request_id: identity.operation_id(),
                source_lifecycle: DraftMutationStagingLifecycleV1::Receiving,
                writer_admitted: true,
                candidate_generation: active.newest_candidate_generation(),
                root: active.newest_root(),
                history: active.newest_history(),
                session_revision: active.session_generation(),
            },
        )
        .unwrap();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal),
    ));
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Cancelled { .. }
    ));
    let terminal_snapshot = snapshot(&storage, &store, admission);
    let writer = terminal_snapshot.head().unwrap();
    assert_eq!(
        writer.lifecycle(),
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup
    );
    assert_eq!(writer.target_root().count(), 0);
    assert_eq!(writer.remaining_builder_count(), 0);
}

#[test]
fn reopened_staging_writer_is_cleanup_only_until_its_authorized_terminal_path() {
    let (home, store, storage, thread) = fixture("phase225-reopen-staging", 76);
    let (session, source) = marked_session(&storage, &store, thread, 77);
    let admission = owner(&session, 87);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        78,
        vec![association(79, &session, source.marker_id())],
    );
    let (identity, active, head) =
        begin_admitted_marker_edit(&storage, &store, &session, admission, proof);
    let before = snapshot(&storage, &store, admission);
    let head_digest = before.head().unwrap().digest();
    let capacity_digest = before.capacity().unwrap().digest();

    drop(storage);
    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let reopened_head = storage
        .draft_mutation_staging_head(&reopened, identity)
        .unwrap()
        .unwrap();
    assert_eq!(reopened_head, head);
    let restored = snapshot(&storage, &reopened, admission);
    assert_eq!(restored.head().unwrap().digest(), head_digest);
    assert_eq!(restored.capacity().unwrap().digest(), capacity_digest);
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&reopened)
            .unwrap(),
        None
    );
    assert!(
        storage
            .prepare_draft_marker_label_assignment(
                &reopened,
                admission,
                syndic_storage::DraftMarkerAdmissionCommandIdV1::from_bytes([80; 16]),
            )
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_mutation_staging_page_batch(
                &reopened_head,
                &active,
                Box::new([DraftMutationStagingPageInputV1::new(
                    DraftMutationStagingLaneV1::Proposal,
                    reopened_head.proposal().next_cursor(),
                    reopened_head.proposal().next_cursor() + 1,
                    1,
                    65_536,
                    Box::new([DraftMutationStagingPageItemV1::Proposal(
                        DraftPieceReplacementV1::new(
                            point(1),
                            point(1),
                            vec![DraftPieceV1::Text("resume".to_owned())],
                        ),
                    )]),
                )]),
            )
            .is_err()
    );

    let terminal = storage
        .prepare_draft_mutation_staging_terminal(
            &reopened_head,
            &active,
            DraftMutationStagingTerminalEvidenceV1::Cancelled {
                request_id: identity.operation_id(),
                source_lifecycle: DraftMutationStagingLifecycleV1::Receiving,
                writer_admitted: true,
                candidate_generation: active.newest_candidate_generation(),
                root: active.newest_root(),
                history: active.newest_history(),
                session_revision: active.session_generation(),
            },
        )
        .unwrap();
    let outcome = execute(
        &reopened,
        storage
            .draft_mutation_staging_command(storage.revision(&reopened).unwrap(), terminal.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_mutation_staging_command_outcome(&reopened, &terminal, outcome)
            .unwrap(),
        syndic_storage::DraftMutationStagingReconcileV1::Terminal(
            DraftMutationStagingStatusV1::Cancelled { .. }
        )
    ));
    let terminal = snapshot(&storage, &reopened, admission);
    assert_eq!(
        terminal.head().unwrap().lifecycle(),
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup
    );
    assert_eq!(
        terminal.capacity().unwrap().charge(),
        terminal.head().unwrap().charge()
    );
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&reopened)
            .unwrap(),
        Some(admission)
    );
}

#[test]
fn uncertain_and_later_failed_staging_cancellation_reconcile_to_exact_new_terminal_custody() {
    for (index, fault) in [
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ]
    .into_iter()
    .enumerate()
    {
        let seed = 65 + (index as u8 * 10);
        let faults = FaultController::new();
        let (_home, store, storage, thread) = fixture_with_faults(
            &format!("phase225-terminal-fault-{index}"),
            seed,
            faults.clone(),
        );
        let (session, source) = marked_session(&storage, &store, thread, seed + 1);
        let admission = owner(&session, seed + 8);
        let proof = ready_proof(
            &storage,
            &store,
            admission,
            seed + 3,
            vec![association(seed + 4, &session, source.marker_id())],
        );
        let (identity, active, head) =
            begin_admitted_marker_edit(&storage, &store, &session, admission, proof);
        let terminal = storage
            .prepare_draft_mutation_staging_terminal(
                &head,
                &active,
                DraftMutationStagingTerminalEvidenceV1::Cancelled {
                    request_id: identity.operation_id(),
                    source_lifecycle: DraftMutationStagingLifecycleV1::Receiving,
                    writer_admitted: true,
                    candidate_generation: active.newest_candidate_generation(),
                    root: active.newest_root(),
                    history: active.newest_history(),
                    session_revision: active.session_generation(),
                },
            )
            .unwrap();
        faults.fail_next(fault);
        let outcome = execute(
            &store,
            storage.draft_mutation_staging_command(
                storage.revision(&store).unwrap(),
                terminal.clone(),
            ),
        );
        match fault {
            FaultPoint::AfterCommitBeforePersist => {
                assert!(matches!(&outcome, CommandOutcome::Indeterminate { .. }));
            }
            FaultPoint::AfterPersist => {
                assert!(matches!(
                    &outcome,
                    CommandOutcome::Committed {
                        later_failure: Some(CommandError::Persistence { .. }),
                        ..
                    }
                ));
            }
            _ => unreachable!(),
        }
        let (store, storage) = match fault {
            FaultPoint::AfterCommitBeforePersist => {
                let (store, storage) = if store.health().state() == HomeHealthState::Failed {
                    let recovery = store.recover_same_home().unwrap();
                    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
                    (recovery.publish(), storage)
                } else {
                    (store, storage)
                };
                assert!(matches!(
                    storage
                        .reconcile_draft_mutation_staging_command_outcome(
                            &store, &terminal, outcome
                        )
                        .unwrap(),
                    syndic_storage::DraftMutationStagingReconcileV1::Terminal(
                        DraftMutationStagingStatusV1::Cancelled { .. }
                    )
                ));
                (store, storage)
            }
            FaultPoint::AfterPersist => {
                assert!(matches!(
                    storage.reconcile_draft_mutation_staging_command_outcome(
                        &store, &terminal, outcome
                    ),
                    Err(DraftMutationStagingErrorV1::Read(_))
                ));
                let recovery = store.recover_same_home().unwrap();
                let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
                let store = recovery.publish();
                let replay = execute(
                    &store,
                    storage.draft_mutation_staging_command(
                        storage.revision(&store).unwrap(),
                        terminal.clone(),
                    ),
                );
                assert!(matches!(
                    &replay,
                    CommandOutcome::NotCommitted {
                        evidence: CommandError::EmptyContribution { .. }
                    }
                ));
                assert!(matches!(
                    storage
                        .reconcile_draft_mutation_staging_command_outcome(&store, &terminal, replay)
                        .unwrap(),
                    syndic_storage::DraftMutationStagingReconcileV1::Terminal(
                        DraftMutationStagingStatusV1::Cancelled { .. }
                    )
                ));
                (store, storage)
            }
            _ => unreachable!(),
        };
        let terminal = snapshot(&storage, &store, admission);
        let writer = terminal.head().unwrap();
        assert_eq!(
            writer.lifecycle(),
            DraftMarkerAdmissionLifecycleV1::TerminalCleanup
        );
        assert_eq!(writer.target_root().count(), 0);
        assert_eq!(writer.remaining_builder_count(), 0);
        assert_eq!(
            terminal.capacity().unwrap().charge().associations(),
            writer.charge().associations()
        );
    }
}

#[test]
fn structural_later_failure_finalizes_admitted_terminal_custody_before_recovery() {
    let faults = FaultController::new();
    let (_home, store, storage, thread) =
        fixture_with_faults("phase225-structural-later-failure", 89, faults.clone());
    let (session, source) = marked_session(&storage, &store, thread, 90);
    let admission = owner(&session, 98);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        92,
        vec![association(93, &session, source.marker_id())],
    );
    let (identity, active, head) =
        begin_admitted_marker_edit(&storage, &store, &session, admission, proof);
    let terminal = storage
        .prepare_draft_mutation_staging_terminal(
            &head,
            &active,
            DraftMutationStagingTerminalEvidenceV1::Cancelled {
                request_id: identity.operation_id(),
                source_lifecycle: DraftMutationStagingLifecycleV1::Receiving,
                writer_admitted: true,
                candidate_generation: active.newest_candidate_generation(),
                root: active.newest_root(),
                history: active.newest_history(),
                session_revision: active.session_generation(),
            },
        )
        .unwrap();
    let revision_before_terminal = store.home_revision().unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    let outcome = execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal.clone()),
    );
    let CommandOutcome::Committed {
        receipt,
        later_failure: Some(CommandError::Persistence { .. }),
        local_finalization: Some(_),
    } = &outcome
    else {
        panic!("AfterPersist did not retain exact committed local-finalization custody")
    };
    assert_eq!(receipt.generation(), store.health().generation().unwrap());
    assert!(receipt.home_revision().get() > revision_before_terminal.get());

    assert!(matches!(
        storage.reconcile_draft_mutation_staging_command_outcome(&store, &terminal, outcome),
        Err(DraftMutationStagingErrorV1::Read(_))
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert!(
        storage
            .draft_mutation_staging_head(&store, identity)
            .is_err()
    );

    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    let replay = execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), terminal.clone()),
    );
    assert!(matches!(
        &replay,
        CommandOutcome::NotCommitted {
            evidence: CommandError::EmptyContribution { .. }
        }
    ));
    assert!(matches!(
        storage
            .reconcile_draft_mutation_staging_command_outcome(&store, &terminal, replay)
            .unwrap(),
        syndic_storage::DraftMutationStagingReconcileV1::Terminal(
            DraftMutationStagingStatusV1::Cancelled { .. }
        )
    ));
    let terminal = snapshot(&storage, &store, admission);
    assert_eq!(
        terminal.head().unwrap().lifecycle(),
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup
    );
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&store)
            .unwrap(),
        Some(admission)
    );
}

#[test]
fn admitted_builder_cancellation_transfers_building_custody_to_terminal_cleanup() {
    let (_home, store, storage, thread) = fixture("phase225-builder-cancel", 70);
    let (session, source) = marked_session(&storage, &store, thread, 71);
    let admission = owner(&session, 80);
    let proof = ready_proof(
        &storage,
        &store,
        admission,
        74,
        vec![association(73, &session, source.marker_id())],
    );
    let (prepared, _, _) = stage_admitted_marker_edit(
        &storage,
        &store,
        &session,
        admission,
        proof,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Text("b".to_owned())]),
    );
    assert_eq!(
        snapshot(&storage, &store, admission)
            .head()
            .unwrap()
            .lifecycle(),
        DraftMarkerAdmissionLifecycleV1::Building
    );

    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    let terminal_snapshot = snapshot(&storage, &store, admission);
    let writer = terminal_snapshot.head().unwrap();
    assert_eq!(
        writer.lifecycle(),
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup
    );
    assert_eq!(writer.target_root().count(), 0);
    assert_eq!(writer.remaining_builder_count(), 0);
}

fn marker_with_source_label_and_asset(
    target: u8,
    order: u64,
    source: syndic_storage::DraftPieceMarkerV1,
) -> syndic_storage::DraftPieceMarkerV1 {
    syndic_storage::DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([target; 16]),
        order,
        source.label(),
        source.asset_id(),
    )
}

fn insert_target(marker: syndic_storage::DraftPieceMarkerV1) -> DraftPieceReplacementV1 {
    let position = syndic_storage::DraftCompositePositionV1::new(
        1,
        syndic_storage::DraftCompositeGapWitnessV1::BeforeAll,
    );
    DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(marker)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                1,
                marker,
                DraftPieceMarkerEffectChargesV1::for_marker(marker),
            ),
        ))
}

fn insert_first_target(marker: syndic_storage::DraftPieceMarkerV1) -> DraftPieceReplacementV1 {
    DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                1,
                marker,
                DraftPieceMarkerEffectChargesV1::for_marker(marker),
            ),
        ))
}

fn insert_target_after_all(marker: syndic_storage::DraftPieceMarkerV1) -> DraftPieceReplacementV1 {
    let position = syndic_storage::DraftCompositePositionV1::new(
        1,
        syndic_storage::DraftCompositeGapWitnessV1::AfterAll,
    );
    DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(marker)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                1,
                marker,
                DraftPieceMarkerEffectChargesV1::for_marker(marker),
            ),
        ))
}
