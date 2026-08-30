include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use beryl_model::DomainRevision;
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    DraftMarkerAdmissionFixtureSnapshotV1, draft_marker_admission_fixture_contribution,
};
use syndic_storage::{
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionHeadV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionPublicationFixtureV1, DraftMarkerLabelReadinessProvenPageV1,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceSelectorV1, DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1,
    DraftPieceSettlementKeyV1,
};

#[path = "phase216_draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;

use readiness_support::{association, marked_session, owner, two_marked_session};

#[test]
fn page_quanta_advance_once_resume_after_reopen_and_reclaim_replay_state() {
    let (home, store, storage, thread) = fixture("phase220-progression", 1);
    let (session, marker) = marked_session(&storage, &store, thread, 2);
    let operation_owner = owner(&session, 10);
    let first_page = vec![
        association(20, &session, marker.marker_id()),
        association(21, &session, marker.marker_id()),
        association(22, &session, marker.marker_id()),
        association(23, &session, marker.marker_id()),
    ];

    publish_page_quantum(
        &storage,
        &store,
        operation_owner,
        30,
        1,
        false,
        first_page.clone(),
        30,
    );
    assert_progress(&storage, &store, operation_owner, 1, 1, 1);

    let before = durable_identity(&storage, &store, operation_owner);
    let premature = proven_page(
        &storage,
        &store,
        operation_owner,
        31,
        2,
        false,
        vec![association(24, &session, marker.marker_id())],
    );
    let error = not_committed(
        store
            .execute_current(publication(operation_owner, 31).current_command(&storage, premature)),
    );
    assert!(error.contains("PageIncomplete"), "{error}");
    assert_durable_identity(&storage, &store, operation_owner, before);

    drop(storage);
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();

    publish_page_quantum(
        &storage,
        &store,
        operation_owner,
        30,
        1,
        false,
        first_page.clone(),
        30,
    );
    assert_progress(&storage, &store, operation_owner, 2, 1, 2);
    publish_page_quantum(
        &storage,
        &store,
        operation_owner,
        30,
        1,
        false,
        first_page.clone(),
        30,
    );
    assert_progress(&storage, &store, operation_owner, 3, 1, 3);
    let before_last = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    let superseded_nodes = before_last
        .receipt()
        .unwrap()
        .retained_predecessor_nodes()
        .iter()
        .map(|child| child.key())
        .collect::<Vec<_>>();
    assert!(!superseded_nodes.is_empty());

    publish_page_quantum(
        &storage,
        &store,
        operation_owner,
        30,
        1,
        false,
        first_page,
        30,
    );
    assert_progress(&storage, &store, operation_owner, 4, 2, 0);
    let completed = storage
        .draft_marker_admission_publication_snapshot_for_test(
            &store,
            operation_owner,
            &superseded_nodes,
        )
        .unwrap();
    assert!(completed.nodes().iter().all(Option::is_none));
    assert_eq!(
        completed.capacity().unwrap().charge(),
        completed.head().unwrap().charge()
    );

    publish_page_quantum(
        &storage,
        &store,
        operation_owner,
        31,
        2,
        false,
        vec![association(24, &session, marker.marker_id())],
        31,
    );
    assert_progress(&storage, &store, operation_owner, 5, 3, 0);
    assert!(
        storage
            .draft_marker_admission_receipt_for_test(&store, operation_owner, page_id(30))
            .unwrap()
            .is_none()
    );
    let final_snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    assert_eq!(
        final_snapshot.capacity().unwrap().charge(),
        final_snapshot.head().unwrap().charge()
    );
}

#[test]
fn current_page_is_point_replayed_and_differing_or_older_identity_is_refused() {
    let (_home, store, storage, thread) = fixture("phase220-classification", 40);
    let (session, first_marker, second_marker) = two_marked_session(&storage, &store, thread, 41);
    let operation_owner = owner(&session, 50);
    let exact = vec![
        association(60, &session, first_marker.marker_id()),
        association(61, &session, first_marker.marker_id()),
        association(62, &session, first_marker.marker_id()),
    ];
    for expected_cursor in [1, 2] {
        publish_page_quantum(
            &storage,
            &store,
            operation_owner,
            70,
            1,
            false,
            exact.clone(),
            70,
        );
        assert_progress(
            &storage,
            &store,
            operation_owner,
            expected_cursor,
            1,
            expected_cursor,
        );
    }

    let before = durable_identity(&storage, &store, operation_owner);
    let differing_closure = proven_page(
        &storage,
        &store,
        operation_owner,
        70,
        1,
        false,
        vec![
            association(60, &session, first_marker.marker_id()),
            association(61, &session, second_marker.marker_id()),
            association(62, &session, first_marker.marker_id()),
        ],
    );
    let error = not_committed(store.execute_current(
        publication(operation_owner, 70).current_command(&storage, differing_closure),
    ));
    assert!(error.contains("Collision"), "{error}");
    assert_durable_identity(&storage, &store, operation_owner, before);

    let differing_page = proven_page(
        &storage,
        &store,
        operation_owner,
        71,
        1,
        false,
        exact.clone(),
    );
    let error = not_committed(store.execute_current(
        publication(operation_owner, 71).current_command(&storage, differing_page),
    ));
    assert!(error.contains("Collision"), "{error}");

    publish_page_quantum(
        &storage,
        &store,
        operation_owner,
        70,
        1,
        false,
        exact.clone(),
        70,
    );
    let older = proven_page(&storage, &store, operation_owner, 70, 1, false, exact);
    let before = durable_identity(&storage, &store, operation_owner);
    let error = not_committed(
        store.execute_current(publication(operation_owner, 70).current_command(&storage, older)),
    );
    assert!(error.contains("ObsoletePage"), "{error}");
    assert_durable_identity(&storage, &store, operation_owner, before);
}

#[test]
fn duplicate_resource_and_revision_refusals_preserve_durable_state() {
    let (_home, store, storage, thread) = fixture("phase220-refusals", 80);
    let (session, marker) = marked_session(&storage, &store, thread, 81);
    let operation_owner = owner(&session, 90);
    publish_page_quantum(
        &storage,
        &store,
        operation_owner,
        91,
        1,
        false,
        vec![association(92, &session, marker.marker_id())],
        91,
    );

    let duplicate = proven_page(
        &storage,
        &store,
        operation_owner,
        93,
        2,
        false,
        vec![association(92, &session, marker.marker_id())],
    );
    let before = durable_identity(&storage, &store, operation_owner);
    let error = not_committed(
        store
            .execute_current(publication(operation_owner, 93).current_command(&storage, duplicate)),
    );
    assert!(error.contains("Collision"), "{error}");
    assert_durable_identity(&storage, &store, operation_owner, before);

    let bounded = proven_page(
        &storage,
        &store,
        operation_owner,
        94,
        2,
        false,
        vec![association(94, &session, marker.marker_id())],
    );
    let error = not_committed(
        store.execute_current(
            publication(operation_owner, 94)
                .with_command_limit_for_test(1)
                .current_command(&storage, bounded),
        ),
    );
    assert!(error.contains("CommandTooLarge"), "{error}");
    assert_durable_identity(&storage, &store, operation_owner, before);

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
    committed(execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            DraftMarkerAdmissionFixtureSnapshotV1::new(
                snapshot.capacity().unwrap().clone(),
                vec![overflow],
                snapshot
                    .nodes()
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
                vec![snapshot.receipt().unwrap().clone()],
            ),
        ),
    ));
    let before = durable_identity(&storage, &store, operation_owner);
    let page = proven_page(
        &storage,
        &store,
        operation_owner,
        94,
        2,
        false,
        vec![association(94, &session, marker.marker_id())],
    );
    let error = not_committed(
        store.execute_current(publication(operation_owner, 94).current_command(&storage, page)),
    );
    assert!(error.contains("RevisionOverflow"), "{error}");
    assert_durable_identity(&storage, &store, operation_owner, before);
}

#[test]
fn final_eof_is_durable_before_source_order_assignment() {
    let (_home, store, storage, thread) = fixture("phase220-eof", 110);
    let (session, marker) = marked_session(&storage, &store, thread, 111);
    let operation_owner = owner(&session, 112);
    let page = proven_page(
        &storage,
        &store,
        operation_owner,
        113,
        1,
        true,
        vec![association(114, &session, marker.marker_id())],
    );
    committed(
        store.execute_current(publication(operation_owner, 113).current_command(&storage, page)),
    );
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(&store, operation_owner, &[])
        .unwrap();
    let head = snapshot.head().unwrap();
    assert!(head.evidence_eof());
    assert_eq!(head.source_root().count(), 1);
    assert_eq!(head.target_root().count(), 1);
    assert_eq!(head.unassigned_count(), 1);
    assert!(head.assignment_continuation().is_some());
    assert!(snapshot.capacity().is_some());
    assert!(snapshot.receipt().is_some());
}

#[allow(clippy::too_many_arguments)]
fn publish_page_quantum(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    eof: bool,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
    closure: u8,
) {
    let page = proven_page(storage, store, owner, command, ordinal, eof, associations);
    committed(store.execute_current(publication(owner, closure).current_command(storage, page)));
}

#[allow(clippy::too_many_arguments)]
fn proven_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    eof: bool,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
) -> DraftMarkerLabelReadinessProvenPageV1 {
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page_for_test(
            store,
            owner,
            page_id(command),
            NonZeroU64::new(ordinal).unwrap(),
            eof,
            associations.into_boxed_slice(),
            None,
        )
        .unwrap();
    let receipt = store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    attempt.consume(store, receipt).unwrap()
}

fn publication(
    owner: DraftMarkerAdmissionOwnerV1,
    closure: u8,
) -> DraftMarkerAdmissionPublicationFixtureV1 {
    DraftMarkerAdmissionPublicationFixtureV1::new(
        owner,
        NonZeroU64::MIN,
        digest(1),
        digest(2),
        digest(3),
        vec![4, closure].into_boxed_slice(),
        vec![5, closure].into_boxed_slice(),
    )
}

fn assert_progress(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    count: u64,
    next_ordinal: u64,
    cursor: u64,
) {
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(store, owner, &[])
        .unwrap();
    let head = snapshot.head().unwrap();
    assert_eq!(head.source_root().count(), count);
    assert_eq!(head.target_root().count(), count);
    assert_eq!(head.charge().associations(), count);
    assert_eq!(head.next_page_ordinal().get(), next_ordinal);
    assert_eq!(head.ingestion_association_cursor(), cursor);
    assert_eq!(snapshot.capacity().unwrap().charge(), head.charge());
}

fn durable_identity(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
) -> (
    DomainRevision,
    DraftMarkerAdmissionDigestV1,
    DraftMarkerAdmissionDigestV1,
    DraftMarkerAdmissionDigestV1,
) {
    let snapshot = storage
        .draft_marker_admission_publication_snapshot_for_test(store, owner, &[])
        .unwrap();
    (
        storage.revision(store).unwrap(),
        snapshot.capacity().unwrap().digest(),
        snapshot.head().unwrap().digest(),
        snapshot.receipt().unwrap().digest(),
    )
}

fn assert_durable_identity(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    expected: (
        DomainRevision,
        DraftMarkerAdmissionDigestV1,
        DraftMarkerAdmissionDigestV1,
        DraftMarkerAdmissionDigestV1,
    ),
) {
    assert_eq!(durable_identity(storage, store, owner), expected);
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

fn not_committed(outcome: CommandOutcome) -> String {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => format!("{evidence:?}"),
        outcome => panic!("expected not committed, got {outcome:?}"),
    }
}

fn digest(byte: u8) -> DraftMarkerAdmissionDigestV1 {
    DraftMarkerAdmissionDigestV1::from_bytes([byte; 32])
}

fn page_id(byte: u8) -> DraftMarkerAdmissionCommandIdV1 {
    DraftMarkerAdmissionCommandIdV1::from_bytes([byte; 16])
}
