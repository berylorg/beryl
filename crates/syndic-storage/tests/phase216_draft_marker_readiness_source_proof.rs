include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use sha2::{Digest, Sha256};
use syndic_storage::{
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerReadinessCandidateSourceV1,
    DraftMarkerReadinessCutSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceErrorV1, DraftMarkerReadinessSourceSelectorV1,
    DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1, DraftPieceSettlementKeyV1,
};

#[path = "phase216_draft_marker_readiness_source_proof/support.rs"]
mod support;

use support::*;

#[test]
fn source_page_rejects_empty_nonterminal_and_excessive_input_without_mutation() {
    let (_home, store, storage, thread) = fixture("phase216-shape", 1);
    let current = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &current, 2, 3);
    let revision = storage.revision(&store).unwrap();
    let empty = storage.prepare_draft_marker_label_readiness_source_page(
        &store,
        owner(&session, 4),
        DraftMarkerAdmissionCommandIdV1::from_bytes([5; 16]),
        NonZeroU64::MIN,
        false,
        Box::new([]),
    );
    assert!(matches!(
        empty,
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    let entry = association(6, &session, SyndicDraftMarkerId::from_bytes([7; 16]));
    let oversized = storage.prepare_draft_marker_label_readiness_source_page(
        &store,
        owner(&session, 4),
        DraftMarkerAdmissionCommandIdV1::from_bytes([8; 16]),
        NonZeroU64::MIN,
        true,
        vec![entry; 257].into_boxed_slice(),
    );
    assert!(matches!(
        oversized,
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    let (marked, marker) = marked_session(&storage, &store, thread, 9);
    let marked_revision = storage.revision(&store).unwrap();
    let duplicate = storage.prepare_draft_marker_label_readiness_source_page(
        &store,
        owner(&marked, 10),
        DraftMarkerAdmissionCommandIdV1::from_bytes([11; 16]),
        NonZeroU64::MIN,
        true,
        Box::new([
            association(12, &marked, marker.marker_id()),
            association(12, &marked, marker.marker_id()),
        ]),
    );
    assert!(matches!(
        duplicate,
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert_eq!(storage.revision(&store).unwrap(), marked_revision);
}

#[test]
fn source_page_enforces_raw_candidate_and_cut_boundaries() {
    let (_home, store, storage, thread) = fixture("phase216-byte-boundaries", 13);
    let (marked, marker) = marked_session(&storage, &store, thread, 14);
    assert_eq!(151 * manual_candidate_entry(&marked, marker).len(), 65_534);
    assert_eq!(152 * manual_candidate_entry(&marked, marker).len(), 65_968);
    let candidate_151 = (0_u8..151)
        .map(|target| association(target, &marked, marker.marker_id()))
        .collect::<Vec<_>>();
    let mut accepted_candidate = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&marked, 20),
            DraftMarkerAdmissionCommandIdV1::from_bytes([21; 16]),
            NonZeroU64::MIN,
            true,
            candidate_151.into_boxed_slice(),
        )
        .unwrap();
    let candidate_receipt = store
        .compose_proof(accepted_candidate.take_command().unwrap())
        .unwrap();
    let _candidate = accepted_candidate
        .consume(&store, candidate_receipt)
        .unwrap();
    let candidate_152 = (0_u8..152)
        .map(|target| association(target, &marked, marker.marker_id()))
        .collect::<Vec<_>>();
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&marked, 22),
            DraftMarkerAdmissionCommandIdV1::from_bytes([23; 16]),
            NonZeroU64::MIN,
            true,
            candidate_152.into_boxed_slice(),
        ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));

    let occurrence = storage
        .draft_marker_identity(&store, marked.newest_root(), marker.marker_id())
        .unwrap()
        .unwrap();
    let position = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let removal_operation = 24;
    let successor = complete_marker_edit(
        &storage,
        &store,
        &marked,
        removal_operation,
        DraftPieceReplacementV1::new(position, position, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(marker),
            },
        ),
    );
    let cut = DraftMarkerReadinessSourceSelectorV1::Cut(DraftMarkerReadinessCutSourceV1::new(
        DraftPieceSettlementKeyV1::new(
            successor.draft_id(),
            successor.session_id(),
            DraftPieceOperationIdV1::from_bytes([removal_operation; 16]),
        ),
        successor.newest_candidate_generation(),
        successor.newest_root(),
        marker.marker_id(),
    ));
    let settlement = DraftPieceSettlementKeyV1::new(
        successor.draft_id(),
        successor.session_id(),
        DraftPieceOperationIdV1::from_bytes([removal_operation; 16]),
    );
    assert_eq!(
        145 * manual_cut_entry(settlement, &successor, marker).len(),
        65_250
    );
    assert_eq!(
        146 * manual_cut_entry(settlement, &successor, marker).len(),
        65_700
    );
    let cut_145 = (0_u8..145)
        .map(|target| {
            DraftMarkerReadinessSourceAssociationV1::new(
                SyndicDraftMarkerId::from_bytes([target; 16]),
                cut,
            )
        })
        .collect::<Vec<_>>();
    let mut accepted_cut = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&successor, 25),
            DraftMarkerAdmissionCommandIdV1::from_bytes([26; 16]),
            NonZeroU64::MIN,
            true,
            cut_145.into_boxed_slice(),
        )
        .unwrap();
    let cut_receipt = store
        .compose_proof(accepted_cut.take_command().unwrap())
        .unwrap();
    let _cut = accepted_cut.consume(&store, cut_receipt).unwrap();
    let cut_146 = (0_u8..146)
        .map(|target| {
            DraftMarkerReadinessSourceAssociationV1::new(
                SyndicDraftMarkerId::from_bytes([target; 16]),
                cut,
            )
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&successor, 27),
            DraftMarkerAdmissionCommandIdV1::from_bytes([28; 16]),
            NonZeroU64::MIN,
            true,
            cut_146.into_boxed_slice(),
        ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&successor, 29),
            DraftMarkerAdmissionCommandIdV1::from_bytes([30; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([
                association(201, &marked, marker.marker_id()),
                DraftMarkerReadinessSourceAssociationV1::new(
                    SyndicDraftMarkerId::from_bytes([202; 16]),
                    cut,
                ),
            ]),
        ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
}

#[test]
fn authenticated_candidate_page_composes_once_without_durable_mutation() {
    let (_home, store, storage, thread) = fixture("phase216-candidate", 20);
    let durable = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &durable, 21, 22);
    session = complete_staged(
        &storage,
        &store,
        &session,
        23,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let marker = marker(24, 1, 7);
    session = complete_marker_edit(
        &storage,
        &store,
        &session,
        25,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
    );
    let revision = storage.revision(&store).unwrap();
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&session, 26),
            DraftMarkerAdmissionCommandIdV1::from_bytes([27; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([association(28, &session, marker.marker_id())]),
        )
        .unwrap();
    let entry = manual_candidate_entry(&session, marker);
    assert_eq!(entry.len(), 434);
    assert_eq!(
        attempt.expected_source_correlation_for_test(),
        manual_correlation(NonZeroU64::MIN, true, &entry)
    );
    let command = attempt.take_command().unwrap();
    let receipt = store.compose_proof(command).unwrap();
    let _proven = attempt.consume(&store, receipt).unwrap();
    assert_eq!(storage.revision(&store).unwrap(), revision);
}

#[test]
fn missing_marker_is_refused_during_preflight_without_mutation() {
    let (_home, store, storage, thread) = fixture("phase216-missing", 40);
    let current = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &current, 41, 42);
    let revision = storage.revision(&store).unwrap();
    let result = storage.prepare_draft_marker_label_readiness_source_page(
        &store,
        owner(&session, 43),
        DraftMarkerAdmissionCommandIdV1::from_bytes([44; 16]),
        NonZeroU64::MIN,
        true,
        Box::new([association(
            45,
            &session,
            SyndicDraftMarkerId::from_bytes([46; 16]),
        )]),
    );
    assert!(matches!(
        result,
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
}

#[test]
fn authenticated_cut_page_requires_a_committed_marker_removal() {
    let (_home, store, storage, thread) = fixture("phase216-cut", 60);
    let (marked, marker) = marked_session(&storage, &store, thread, 61);
    let nonremoving_operation = 66;
    let nonremoving = complete_staged(
        &storage,
        &store,
        &marked,
        nonremoving_operation,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("b".to_owned())]),
        DraftLogicalExtentV1::new(2, 1),
    );
    let nonremoving_selector =
        DraftMarkerReadinessSourceSelectorV1::Cut(DraftMarkerReadinessCutSourceV1::new(
            DraftPieceSettlementKeyV1::new(
                nonremoving.draft_id(),
                nonremoving.session_id(),
                DraftPieceOperationIdV1::from_bytes([nonremoving_operation; 16]),
            ),
            nonremoving.newest_candidate_generation(),
            nonremoving.newest_root(),
            marker.marker_id(),
        ));
    assert!(matches!(
        storage.prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&nonremoving, 67),
            DraftMarkerAdmissionCommandIdV1::from_bytes([68; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([DraftMarkerReadinessSourceAssociationV1::new(
                SyndicDraftMarkerId::from_bytes([69; 16]),
                nonremoving_selector,
            )]),
        ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    let occurrence = storage
        .draft_marker_identity(&store, nonremoving.newest_root(), marker.marker_id())
        .unwrap()
        .unwrap();
    let removal_position = DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll);
    let removal_operation = 70;
    let successor = complete_marker_edit(
        &storage,
        &store,
        &nonremoving,
        removal_operation,
        DraftPieceReplacementV1::new(removal_position, removal_position, Vec::new())
            .with_marker_effect(DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(removal_position, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(marker),
            }),
    );
    let selector = DraftMarkerReadinessSourceSelectorV1::Cut(DraftMarkerReadinessCutSourceV1::new(
        DraftPieceSettlementKeyV1::new(
            successor.draft_id(),
            successor.session_id(),
            DraftPieceOperationIdV1::from_bytes([removal_operation; 16]),
        ),
        successor.newest_candidate_generation(),
        successor.newest_root(),
        marker.marker_id(),
    ));
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&successor, 71),
            DraftMarkerAdmissionCommandIdV1::from_bytes([72; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([DraftMarkerReadinessSourceAssociationV1::new(
                SyndicDraftMarkerId::from_bytes([73; 16]),
                selector,
            )]),
        )
        .unwrap();
    let entry = manual_cut_entry(
        DraftPieceSettlementKeyV1::new(
            successor.draft_id(),
            successor.session_id(),
            DraftPieceOperationIdV1::from_bytes([removal_operation; 16]),
        ),
        &successor,
        marker,
    );
    assert_eq!(entry.len(), 450);
    assert_eq!(
        attempt.expected_source_correlation_for_test(),
        manual_correlation(NonZeroU64::MIN, true, &entry)
    );
    let receipt = store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    let _proven = attempt.consume(&store, receipt).unwrap();
}

#[test]
fn caller_association_order_does_not_prevent_canonical_source_proofs() {
    let (_home, store, storage, thread) = fixture("phase216-order", 70);
    let (session, first, second) = two_marked_session(&storage, &store, thread, 71);
    let mut forward = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&session, 77),
            DraftMarkerAdmissionCommandIdV1::from_bytes([78; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([
                association(79, &session, first.marker_id()),
                association(80, &session, second.marker_id()),
            ]),
        )
        .unwrap();
    let mut reverse = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&session, 77),
            DraftMarkerAdmissionCommandIdV1::from_bytes([81; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([
                association(82, &session, second.marker_id()),
                association(83, &session, first.marker_id()),
            ]),
        )
        .unwrap();
    assert_eq!(
        forward.expected_source_correlation_for_test(),
        reverse.expected_source_correlation_for_test()
    );
    let forward_receipt = store
        .compose_proof(forward.take_command().unwrap())
        .unwrap();
    let reverse_receipt = store
        .compose_proof(reverse.take_command().unwrap())
        .unwrap();
    let _forward = forward.consume(&store, forward_receipt).unwrap();
    let _reverse = reverse.consume(&store, reverse_receipt).unwrap();
}

#[test]
fn private_target_attempts_cannot_exchange_receipts_or_survive_reopen() {
    let (home, store, storage, thread) = fixture("phase216-pairing", 80);
    let (session, marker) = marked_session(&storage, &store, thread, 81);
    let source = association(82, &session, marker.marker_id());
    let mut first = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&session, 83),
            DraftMarkerAdmissionCommandIdV1::from_bytes([84; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([source]),
        )
        .unwrap();
    let mut second = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&session, 83),
            DraftMarkerAdmissionCommandIdV1::from_bytes([85; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([association(86, &session, marker.marker_id())]),
        )
        .unwrap();
    let _first_command = first.take_command().unwrap();
    let second_receipt = store.compose_proof(second.take_command().unwrap()).unwrap();
    let pairing_error = match first.consume(&store, second_receipt) {
        Ok(_) => panic!("private-target attempt accepted a foreign receipt"),
        Err(error) => error,
    };
    assert!(
        matches!(
            pairing_error,
            DraftMarkerReadinessSourceErrorV1::Receipt(
                beryl_home_store::ProofReceiptError::SourceFenceMismatch
                    | beryl_home_store::ProofReceiptError::CorrelationMismatch
            )
        ),
        "unexpected receipt pairing result: {pairing_error:?}"
    );

    let mut stale = storage
        .prepare_draft_marker_label_readiness_source_page(
            &store,
            owner(&session, 83),
            DraftMarkerAdmissionCommandIdV1::from_bytes([87; 16]),
            NonZeroU64::MIN,
            true,
            Box::new([association(88, &session, marker.marker_id())]),
        )
        .unwrap();
    let stale_receipt = store.compose_proof(stale.take_command().unwrap()).unwrap();
    drop(storage);
    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let _reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        stale.consume(&reopened, stale_receipt),
        Err(DraftMarkerReadinessSourceErrorV1::Receipt(
            beryl_home_store::ProofReceiptError::StaleOrForeign
        ))
    ));
}
