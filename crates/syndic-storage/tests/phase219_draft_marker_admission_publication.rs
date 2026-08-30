include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use beryl_model::DomainRevision;
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    DraftMarkerAdmissionFixtureSnapshotV1, draft_marker_admission_fixture_contribution,
};
use syndic_storage::{
    DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS, DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    DRAFT_MARKER_ADMISSION_MAX_HEADS, DraftMarkerAdmissionCommandIdV1,
    DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionPublicationFixtureV1,
    DraftMarkerAdmissionRetainedChargeV1, DraftMarkerLabelReadinessProvenPageV1,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceSelectorV1, DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1,
    DraftPieceSettlementKeyV1,
};

#[path = "phase216_draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;

use readiness_support::{association, marked_session, owner};

#[test]
fn first_publication_is_atomic_and_reopens_coherently() {
    let (home, store, storage, thread) = fixture("phase219-first", 1);
    let (session, marker) = marked_session(&storage, &store, thread, 2);
    let operation_owner = owner(&session, 40);
    let page = proven_page(
        &storage,
        &store,
        operation_owner,
        41,
        vec![association(50, &session, marker.marker_id())],
    );
    let revision = storage.revision(&store).unwrap();
    committed(store.execute_current(publication(operation_owner).current_command(&storage, page)));
    assert_eq!(storage.revision(&store).unwrap().get(), revision.get() + 1);

    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    let head = snapshot.head().unwrap();
    let capacity = snapshot.capacity().unwrap();
    assert_eq!(head.revision(), NonZeroU64::MIN);
    assert_eq!(head.source_root().count(), 1);
    assert_eq!(head.target_root().count(), 1);
    assert_eq!(head.ingestion_association_cursor(), 0);
    assert_eq!(head.next_page_ordinal().get(), 2);
    assert_eq!(capacity.revision(), NonZeroU64::MIN);
    assert_eq!(capacity.charge(), head.charge());
    assert_eq!(snapshot.receipt().unwrap().command_id(), page_id(41));

    drop(storage);
    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&reopened, operation_owner, &[])
        .unwrap();
    assert_eq!(snapshot.head().unwrap().source_root().count(), 1);
    assert_eq!(
        snapshot.capacity().unwrap().charge(),
        snapshot.head().unwrap().charge()
    );
}

#[test]
fn same_page_successors_advance_exact_charge_and_reclaim_prior_replay_paths() {
    let (_home, store, storage, thread) = fixture("phase219-successors", 60);
    let (session, marker) = marked_session(&storage, &store, thread, 61);
    let operation_owner = owner(&session, 70);
    let associations = (80..84)
        .map(|target| association(target, &session, marker.marker_id()))
        .collect::<Vec<_>>();

    for expected in 1..=3 {
        let page = proven_page(&storage, &store, operation_owner, 71, associations.clone());
        committed(
            store.execute_current(publication(operation_owner).current_command(&storage, page)),
        );
        let snapshot = storage
            .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
            .unwrap();
        let head = snapshot.head().unwrap();
        assert_eq!(head.revision().get(), expected);
        assert_eq!(head.source_root().count(), expected);
        assert_eq!(head.charge().associations(), expected);
        assert_eq!(snapshot.capacity().unwrap().charge(), head.charge());
    }

    let third = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    let prior_replay = third
        .receipt()
        .unwrap()
        .retained_predecessor_nodes()
        .iter()
        .map(|child| child.key())
        .collect::<Vec<_>>();
    assert!(!prior_replay.is_empty());
    let prior_bytes = third.head().unwrap().charge().encoded_bytes();

    let page = proven_page(&storage, &store, operation_owner, 71, associations);
    committed(store.execute_current(publication(operation_owner).current_command(&storage, page)));
    let fourth = storage
        .draft_marker_admission_publication_snapshot_for_test(
            &store,
            operation_owner,
            &prior_replay,
        )
        .unwrap();
    assert_eq!(fourth.head().unwrap().revision().get(), 4);
    assert_eq!(fourth.head().unwrap().charge().associations(), 4);
    assert_ne!(fourth.head().unwrap().charge().encoded_bytes(), prior_bytes);
    assert!(fourth.nodes().iter().all(Option::is_none));
    let newest = fourth
        .receipt()
        .unwrap()
        .retained_predecessor_nodes()
        .iter()
        .map(|child| child.key())
        .collect::<Vec<_>>();
    let retained = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &newest)
        .unwrap();
    assert!(retained.nodes().iter().all(Option::is_some));
}

#[test]
fn singleton_capacity_aggregates_two_operation_owners() {
    let (_home, store, storage, thread) = fixture("phase219-owners", 100);
    let (session, marker) = marked_session(&storage, &store, thread, 101);
    let first_owner = owner(&session, 110);
    let second_owner = DraftMarkerAdmissionOwnerV1::new(
        first_owner.draft_id(),
        first_owner.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes([111; 16]),
    );
    for (operation_owner, command, target) in [(first_owner, 112, 114), (second_owner, 113, 115)] {
        let page = proven_page(
            &storage,
            &store,
            operation_owner,
            command,
            vec![association(target, &session, marker.marker_id())],
        );
        committed(
            store.execute_current(publication(operation_owner).current_command(&storage, page)),
        );
    }
    let first = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, first_owner, &[])
        .unwrap();
    let second = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, second_owner, &[])
        .unwrap();
    let capacity = second.capacity().unwrap();
    assert_eq!(capacity.revision().get(), 2);
    assert_eq!(capacity.charge().heads(), 2);
    assert_eq!(capacity.charge().associations(), 2);
    assert_eq!(
        capacity.charge().encoded_bytes(),
        first.head().unwrap().charge().encoded_bytes()
            + second.head().unwrap().charge().encoded_bytes()
    );
}

#[test]
fn command_footprint_and_authority_refusals_leave_durable_state_unchanged() {
    let (_home, store, storage, thread) = fixture("phase219-refusals", 130);
    let (session, marker) = marked_session(&storage, &store, thread, 131);
    let operation_owner = owner(&session, 140);
    let associations = vec![
        association(141, &session, marker.marker_id()),
        association(142, &session, marker.marker_id()),
    ];
    let page = proven_page(&storage, &store, operation_owner, 143, associations.clone());
    committed(store.execute_current(publication(operation_owner).current_command(&storage, page)));
    let before_revision = storage.revision(&store).unwrap();
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    let expected = (
        before.capacity().unwrap().digest(),
        before.head().unwrap().digest(),
        before.receipt().unwrap().digest(),
    );

    let page = proven_page(&storage, &store, operation_owner, 143, associations.clone());
    assert!(!matches!(
        store.execute_current(
            publication(operation_owner)
                .with_command_limit_for_test(1)
                .current_command(&storage, page)
        ),
        CommandOutcome::Committed { .. }
    ));
    assert_unchanged(&storage, &store, operation_owner, before_revision, expected);

    let page = proven_page(&storage, &store, operation_owner, 143, associations);
    assert!(!matches!(
        store.execute_current(
            DraftMarkerAdmissionPublicationFixtureV1::new(
                operation_owner,
                NonZeroU64::MIN,
                digest(9),
                digest(2),
                digest(3),
                vec![4].into_boxed_slice(),
                vec![5].into_boxed_slice(),
            )
            .current_command(&storage, page)
        ),
        CommandOutcome::Committed { .. }
    ));
    assert_unchanged(&storage, &store, operation_owner, before_revision, expected);
}

#[test]
fn missing_head_selected_receipt_is_refused_without_mutation() {
    let (_home, store, storage, thread) = fixture("phase219-selected-receipt", 180);
    let (session, marker) = marked_session(&storage, &store, thread, 181);
    let operation_owner = owner(&session, 182);
    let associations = vec![
        association(183, &session, marker.marker_id()),
        association(184, &session, marker.marker_id()),
    ];
    let page = proven_page(&storage, &store, operation_owner, 185, associations.clone());
    committed(store.execute_current(publication(operation_owner).current_command(&storage, page)));
    let source_key = snapshot_root_key(&storage, &store, operation_owner, true);
    let target_key = snapshot_root_key(&storage, &store, operation_owner, false);
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(
            &store,
            operation_owner,
            &[source_key, target_key],
        )
        .unwrap();
    let original = snapshot.head().unwrap();
    let mismatched = DraftMarkerAdmissionHeadV1::new(
        original.owner(),
        original.revision(),
        original.home_generation(),
        original.lifecycle(),
        original.request_commitment(),
        original.custody_commitment(),
        original.next_page_ordinal(),
        original.ingestion_association_cursor(),
        original.evidence_eof(),
        Some(page_id(186)),
        original.source_root(),
        original.target_root(),
        original.occurrence_commitment(),
        original.unassigned_count(),
        original.assignment_continuation(),
        original.remaining_builder_count(),
        original.charge(),
        original.cleanup_cursor(),
    )
    .unwrap();
    let injected = DraftMarkerAdmissionFixtureSnapshotV1::new(
        snapshot.capacity().unwrap().clone(),
        vec![mismatched],
        snapshot
            .nodes()
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>(),
        vec![snapshot.receipt().unwrap().clone()],
    );
    committed(execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            injected,
        ),
    ));
    let before_revision = storage.revision(&store).unwrap();
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    let expected = (
        before.capacity().unwrap().digest(),
        before.head().unwrap().digest(),
    );
    let page = proven_page(&storage, &store, operation_owner, 185, associations);
    assert!(!matches!(
        store.execute_current(publication(operation_owner).current_command(&storage, page)),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    let after = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    assert_eq!(after.capacity().unwrap().digest(), expected.0);
    assert_eq!(after.head().unwrap().digest(), expected.1);
    assert!(after.receipt().is_none());
}

#[test]
fn production_limit_boundaries_and_revision_overflow_are_wired_without_large_fixtures() {
    let at_operation_limit = DraftMarkerAdmissionRetainedChargeV1::new(
        1,
        DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
        DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    );
    let at_home_limit = DraftMarkerAdmissionRetainedChargeV1::new(
        DRAFT_MARKER_ADMISSION_MAX_HEADS,
        DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
        DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
    );
    assert!(
        DraftMarkerAdmissionPublicationFixtureV1::limits_accept_for_test(
            at_operation_limit,
            at_home_limit,
        )
    );
    assert!(
        !DraftMarkerAdmissionPublicationFixtureV1::limits_accept_for_test(
            DraftMarkerAdmissionRetainedChargeV1::new(
                1,
                DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS + 1,
                1,
            ),
            at_home_limit,
        )
    );
    assert!(
        !DraftMarkerAdmissionPublicationFixtureV1::limits_accept_for_test(
            DraftMarkerAdmissionRetainedChargeV1::new(
                1,
                1,
                DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES + 1,
            ),
            at_home_limit,
        )
    );
    assert!(
        !DraftMarkerAdmissionPublicationFixtureV1::limits_accept_for_test(
            at_operation_limit,
            DraftMarkerAdmissionRetainedChargeV1::new(
                DRAFT_MARKER_ADMISSION_MAX_HEADS + 1,
                DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
                DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
            ),
        )
    );
    assert!(
        !DraftMarkerAdmissionPublicationFixtureV1::limits_accept_for_test(
            at_operation_limit,
            DraftMarkerAdmissionRetainedChargeV1::new(
                DRAFT_MARKER_ADMISSION_MAX_HEADS,
                DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS + 1,
                DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES,
            ),
        )
    );
    assert!(
        !DraftMarkerAdmissionPublicationFixtureV1::limits_accept_for_test(
            at_operation_limit,
            DraftMarkerAdmissionRetainedChargeV1::new(
                DRAFT_MARKER_ADMISSION_MAX_HEADS,
                DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
                DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES + 1,
            ),
        )
    );

    let (_home, store, storage, thread) = fixture("phase219-overflow", 160);
    let (session, marker) = marked_session(&storage, &store, thread, 161);
    let operation_owner = owner(&session, 170);
    let associations = vec![
        association(171, &session, marker.marker_id()),
        association(172, &session, marker.marker_id()),
    ];
    let page = proven_page(&storage, &store, operation_owner, 173, associations.clone());
    committed(store.execute_current(publication(operation_owner).current_command(&storage, page)));
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(
            &store,
            operation_owner,
            &[
                snapshot_root_key(&storage, &store, operation_owner, true),
                snapshot_root_key(&storage, &store, operation_owner, false),
            ],
        )
        .unwrap();
    let original = snapshot.head().unwrap();
    let overflow = DraftMarkerAdmissionHeadV1::new(
        original.owner(),
        NonZeroU64::MAX,
        original.home_generation(),
        original.lifecycle(),
        original.request_commitment(),
        original.custody_commitment(),
        original.next_page_ordinal(),
        original.ingestion_association_cursor(),
        original.evidence_eof(),
        original.selected_receipt(),
        original.source_root(),
        original.target_root(),
        original.occurrence_commitment(),
        original.unassigned_count(),
        original.assignment_continuation(),
        original.remaining_builder_count(),
        original.charge(),
        original.cleanup_cursor(),
    )
    .unwrap();
    let nodes = snapshot
        .nodes()
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let injected = DraftMarkerAdmissionFixtureSnapshotV1::new(
        snapshot.capacity().unwrap().clone(),
        vec![overflow],
        nodes,
        vec![snapshot.receipt().unwrap().clone()],
    );
    committed(execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            injected,
        ),
    ));
    let before_revision = storage.revision(&store).unwrap();
    let before = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    let expected = (
        before.capacity().unwrap().digest(),
        before.head().unwrap().digest(),
        before.receipt().unwrap().digest(),
    );
    let page = proven_page(&storage, &store, operation_owner, 173, associations);
    assert!(!matches!(
        store.execute_current(publication(operation_owner).current_command(&storage, page)),
        CommandOutcome::Committed { .. }
    ));
    assert_unchanged(&storage, &store, operation_owner, before_revision, expected);
}

fn publication(owner: DraftMarkerAdmissionOwnerV1) -> DraftMarkerAdmissionPublicationFixtureV1 {
    DraftMarkerAdmissionPublicationFixtureV1::new(
        owner,
        NonZeroU64::MIN,
        digest(1),
        digest(2),
        digest(3),
        vec![4].into_boxed_slice(),
        vec![5].into_boxed_slice(),
    )
}

fn digest(byte: u8) -> DraftMarkerAdmissionDigestV1 {
    DraftMarkerAdmissionDigestV1::from_bytes([byte; 32])
}

fn page_id(byte: u8) -> DraftMarkerAdmissionCommandIdV1 {
    DraftMarkerAdmissionCommandIdV1::from_bytes([byte; 16])
}

fn proven_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
) -> DraftMarkerLabelReadinessProvenPageV1 {
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            owner,
            page_id(command),
            NonZeroU64::MIN,
            false,
            associations.into_boxed_slice(),
            None,
        )
        .unwrap();
    let receipt = store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    attempt.consume(store, receipt).unwrap()
}

fn assert_unchanged(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    revision: DomainRevision,
    expected: (
        DraftMarkerAdmissionDigestV1,
        DraftMarkerAdmissionDigestV1,
        DraftMarkerAdmissionDigestV1,
    ),
) {
    assert_eq!(storage.revision(store).unwrap(), revision);
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(store, owner, &[])
        .unwrap();
    assert_eq!(snapshot.capacity().unwrap().digest(), expected.0);
    assert_eq!(snapshot.head().unwrap().digest(), expected.1);
    assert_eq!(snapshot.receipt().unwrap().digest(), expected.2);
}

fn snapshot_root_key(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    source: bool,
) -> syndic_storage::DraftMarkerAdmissionNodeKeyV1 {
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(store, owner, &[])
        .unwrap();
    let head = snapshot.head().unwrap();
    if source {
        head.source_root().node().unwrap()
    } else {
        head.target_root().node().unwrap()
    }
}
