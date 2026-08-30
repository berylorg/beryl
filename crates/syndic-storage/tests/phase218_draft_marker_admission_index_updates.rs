include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use sha2::{Digest, Sha256};
use syndic_storage::{
    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
    DRAFT_MARKER_ADMISSION_TREE_FANOUT, DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT,
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionIndexTestErrorV1,
    DraftMarkerAdmissionIndexTestStateV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionRootV1, DraftMarkerAdmissionSchemaErrorV1,
    DraftMarkerAdmissionTreeV1, DraftMarkerLabelReadinessProvenPageV1,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceSelectorV1, DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1,
    DraftPieceSettlementKeyV1,
};

#[path = "phase216_draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;

use readiness_support::{association, marked_session, owner};

#[test]
fn empty_and_ordinary_insertions_return_exact_successors_and_reclaim_only_prior_closure() {
    let (_home, store, storage, thread) = fixture("phase218-ordinary", 1);
    let (session, marker) = marked_session(&storage, &store, thread, 2);
    let operation_owner = owner(&session, 40);
    let proven = proven_page(
        &storage,
        &store,
        operation_owner,
        41,
        1,
        (50..54)
            .map(|target| association(target, &session, marker.marker_id()))
            .collect(),
    );
    let durable_revision = storage.revision(&store).unwrap();
    let mut state = DraftMarkerAdmissionIndexTestStateV1::new(operation_owner);

    let first = state.apply(&proven, 0).unwrap();
    assert_eq!(first.source_root().count(), 1);
    assert_eq!(first.target_root().count(), 1);
    assert_eq!(first.source_root().height(), 1);
    assert_eq!(first.target_root().height(), 1);
    assert_eq!(first.puts().len(), 2);
    assert!(first.deletions().is_empty());
    assert!(first.retained_predecessor_nodes().is_empty());
    assert_eq!(first.added_charge().associations(), 1);
    assert_eq!(first.added_charge().encoded_bytes(), first.write_bytes());
    assert_eq!(first.removed_charge().encoded_bytes(), 0);
    assert_footprint(&first);

    let second = state.apply(&proven, 1).unwrap();
    assert_eq!(second.source_root().count(), 2);
    assert_eq!(second.target_root().count(), 2);
    assert_eq!(second.source_root().height(), 2);
    assert_eq!(second.target_root().height(), 2);
    assert_eq!(second.puts().len(), 4);
    assert!(second.deletions().is_empty());
    assert!(second.retained_predecessor_nodes().is_empty());
    assert_footprint(&second);

    let third = state.apply(&proven, 2).unwrap();
    assert_eq!(third.puts().len(), 4);
    assert!(third.deletions().is_empty());
    assert_eq!(third.retained_predecessor_nodes().len(), 2);
    let predecessor = third.retained_predecessor_nodes().to_vec();
    assert_footprint(&third);

    let fourth = state.apply(&proven, 3).unwrap();
    assert_eq!(fourth.deletions(), predecessor);
    assert_eq!(fourth.retained_predecessor_nodes().len(), 2);
    assert_eq!(
        fourth.removed_charge().encoded_bytes(),
        fourth.delete_bytes()
    );
    assert_footprint(&fourth);
    assert_eq!(storage.revision(&store).unwrap(), durable_revision);
}

#[test]
fn fanout_split_and_internal_split_propagation_are_bounded() {
    let (_home, store, storage, thread) = fixture("phase218-splits", 60);
    let (session, marker) = marked_session(&storage, &store, thread, 61);
    let operation_owner = owner(&session, 70);
    let first_page = proven_page(
        &storage,
        &store,
        operation_owner,
        71,
        1,
        (0..150)
            .map(|target| association(target, &session, marker.marker_id()))
            .collect(),
    );
    let second_page = proven_page(
        &storage,
        &store,
        operation_owner,
        72,
        2,
        (150..200)
            .map(|target| association(target, &session, marker.marker_id()))
            .collect(),
    );
    let durable_revision = storage.revision(&store).unwrap();
    let mut state = DraftMarkerAdmissionIndexTestStateV1::new(operation_owner);
    for index in 0..128 {
        state.apply(&first_page, index).unwrap();
    }
    assert_eq!(state.source_root().height(), 2);
    assert_eq!(state.target_root().height(), 2);

    let mut height_limited = state.clone();
    height_limited.set_maximum_height_for_test(2);
    assert!(matches!(
        height_limited.prepare(&first_page, 128),
        Err(DraftMarkerAdmissionIndexTestErrorV1::Schema(
            DraftMarkerAdmissionSchemaErrorV1::TreeHeight
        ))
    ));

    let root_split = state.apply(&first_page, 128).unwrap();
    assert_eq!(root_split.source_root().height(), 3);
    assert_eq!(root_split.target_root().height(), 3);
    assert_eq!(root_split.source_root().count(), 129);
    assert_eq!(root_split.target_root().count(), 129);
    assert_eq!(root_split.puts().len(), 8);
    assert_eq!(root_split.retained_predecessor_nodes().len(), 2);
    assert_footprint(&root_split);

    for index in 129..150 {
        state.apply(&first_page, index).unwrap();
    }
    for index in 0..42 {
        state.apply(&second_page, index).unwrap();
    }
    let propagated = state.apply(&second_page, 42).unwrap();
    assert_eq!(propagated.source_root().height(), 3);
    assert_eq!(propagated.target_root().height(), 3);
    assert_eq!(propagated.source_root().count(), 193);
    assert_eq!(propagated.target_root().count(), 193);
    assert_eq!(propagated.puts().len(), 8);
    assert_eq!(propagated.retained_predecessor_nodes().len(), 4);
    assert_footprint(&propagated);
    assert_eq!(storage.revision(&store).unwrap(), durable_revision);
}

#[test]
fn target_point_lookup_rejects_a_prior_page_equivalent_target_without_mutation() {
    let (_home, store, storage, thread) = fixture("phase218-duplicate", 80);
    let (session, marker) = marked_session(&storage, &store, thread, 81);
    let operation_owner = owner(&session, 90);
    let first_page = proven_page(
        &storage,
        &store,
        operation_owner,
        91,
        1,
        vec![association(92, &session, marker.marker_id())],
    );
    let later_page = proven_page(
        &storage,
        &store,
        operation_owner,
        93,
        2,
        vec![association(92, &session, marker.marker_id())],
    );
    let durable_revision = storage.revision(&store).unwrap();
    let mut state = DraftMarkerAdmissionIndexTestStateV1::new(operation_owner);
    state.apply(&first_page, 0).unwrap();
    let roots = (state.source_root(), state.target_root());
    let node_count = state.nodes().len();
    assert!(matches!(
        state.apply(&later_page, 0),
        Err(DraftMarkerAdmissionIndexTestErrorV1::DuplicateTarget)
    ));
    assert_eq!((state.source_root(), state.target_root()), roots);
    assert_eq!(state.nodes().len(), node_count);
    assert_eq!(storage.revision(&store).unwrap(), durable_revision);
}

#[test]
fn malformed_or_detached_paths_and_owner_custody_are_refused() {
    let (_home, store, storage, thread) = fixture("phase218-path-refusal", 100);
    let (session, marker) = marked_session(&storage, &store, thread, 101);
    let operation_owner = owner(&session, 110);
    let proven = proven_page(
        &storage,
        &store,
        operation_owner,
        111,
        1,
        (112..116)
            .map(|target| association(target, &session, marker.marker_id()))
            .collect(),
    );
    let wrong_owner_page = proven_page(
        &storage,
        &store,
        owner(&session, 117),
        118,
        1,
        vec![association(119, &session, marker.marker_id())],
    );
    let durable_revision = storage.revision(&store).unwrap();
    let mut baseline = DraftMarkerAdmissionIndexTestStateV1::new(operation_owner);
    for index in 0..3 {
        baseline.apply(&proven, index).unwrap();
    }

    assert!(matches!(
        baseline.prepare(&wrong_owner_page, 0),
        Err(DraftMarkerAdmissionIndexTestErrorV1::ProvenPageOwner)
    ));

    let mut missing = baseline.clone();
    assert!(missing.remove_node_for_test(missing.source_root().node().unwrap()));
    assert!(matches!(
        missing.prepare(&proven, 3),
        Err(DraftMarkerAdmissionIndexTestErrorV1::MissingNode)
    ));

    let mut wrong_owner = baseline.clone();
    let source = wrong_owner.source_root();
    let detached_key = syndic_storage::DraftMarkerAdmissionNodeKeyV1::new(
        owner(&session, 120),
        source.node().unwrap().kind(),
        source.node().unwrap().node_id(),
    );
    wrong_owner.set_roots_for_test(
        DraftMarkerAdmissionRootV1::new(
            DraftMarkerAdmissionTreeV1::SourceOrder,
            detached_key,
            source.height(),
            source.digest(),
            source.count(),
        )
        .unwrap(),
        wrong_owner.target_root(),
    );
    assert!(matches!(
        wrong_owner.prepare(&proven, 3),
        Err(DraftMarkerAdmissionIndexTestErrorV1::PathAuthentication)
    ));

    let mut wrong_tree = baseline.clone();
    wrong_tree.set_roots_for_test(wrong_tree.target_root(), wrong_tree.target_root());
    assert!(matches!(
        wrong_tree.prepare(&proven, 3),
        Err(DraftMarkerAdmissionIndexTestErrorV1::Schema(
            DraftMarkerAdmissionSchemaErrorV1::InvalidRoot
        ))
    ));

    let mut digest = baseline.clone();
    assert!(digest.corrupt_node_digest_for_test(digest.source_root().node().unwrap()));
    assert!(digest.prepare(&proven, 3).is_err());

    let mut count = baseline.clone();
    assert!(count.corrupt_first_child_count_for_test(count.source_root().node().unwrap()));
    assert!(count.prepare(&proven, 3).is_err());

    let mut envelope = baseline.clone();
    assert!(envelope.corrupt_first_child_envelope_for_test(envelope.source_root().node().unwrap()));
    assert!(envelope.prepare(&proven, 3).is_err());

    let mut fanout = baseline.clone();
    assert!(fanout.corrupt_node_fanout_for_test(fanout.source_root().node().unwrap()));
    assert!(fanout.prepare(&proven, 3).is_err());

    let mut height = baseline.clone();
    height.corrupt_source_root_height_for_test(65);
    assert!(matches!(
        height.prepare(&proven, 3),
        Err(DraftMarkerAdmissionIndexTestErrorV1::Schema(
            DraftMarkerAdmissionSchemaErrorV1::InvalidRoot
        ))
    ));
    assert_eq!(storage.revision(&store).unwrap(), durable_revision);
}

#[test]
fn exact_association_height_and_command_limits_refuse_before_state_change() {
    assert_eq!(DRAFT_MARKER_ADMISSION_TREE_FANOUT, 128);
    assert_eq!(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT, 64);
    assert_eq!(DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS, 65_536);
    assert_eq!(DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, 4_194_304);

    let (_home, store, storage, thread) = fixture("phase218-limits", 130);
    let (session, marker) = marked_session(&storage, &store, thread, 131);
    let operation_owner = owner(&session, 140);
    let proven = proven_page(
        &storage,
        &store,
        operation_owner,
        141,
        1,
        (142..146)
            .map(|target| association(target, &session, marker.marker_id()))
            .collect(),
    );
    let durable_revision = storage.revision(&store).unwrap();
    let mut state = DraftMarkerAdmissionIndexTestStateV1::new(operation_owner);
    state.apply(&proven, 0).unwrap();
    let exact = state.prepare(&proven, 1).unwrap();
    assert!(exact.command_bytes() < DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);

    let roots = (state.source_root(), state.target_root());
    let nodes = state.nodes().len();
    state.set_command_limit_for_test(exact.command_bytes() - 1);
    assert!(matches!(
        state.apply(&proven, 1),
        Err(DraftMarkerAdmissionIndexTestErrorV1::Schema(
            DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge
        ))
    ));
    assert_eq!((state.source_root(), state.target_root()), roots);
    assert_eq!(state.nodes().len(), nodes);

    let source = roots.0;
    let target = roots.1;
    let mut association_limit = state.clone();
    association_limit.set_command_limit_for_test(DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES);
    association_limit.set_roots_for_test(
        DraftMarkerAdmissionRootV1::new(
            source.tree(),
            source.node().unwrap(),
            source.height(),
            source.digest(),
            DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
        )
        .unwrap(),
        DraftMarkerAdmissionRootV1::new(
            target.tree(),
            target.node().unwrap(),
            target.height(),
            target.digest(),
            DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
        )
        .unwrap(),
    );
    assert!(matches!(
        association_limit.prepare(&proven, 1),
        Err(DraftMarkerAdmissionIndexTestErrorV1::Schema(
            DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded
        ))
    ));
    assert_eq!(storage.revision(&store).unwrap(), durable_revision);
}

fn proven_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
) -> DraftMarkerLabelReadinessProvenPageV1 {
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            owner,
            DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
            NonZeroU64::new(ordinal).unwrap(),
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

fn assert_footprint(step: &syndic_storage::DraftMarkerAdmissionIndexTestStepV1) {
    assert_eq!(
        step.command_bytes(),
        step.read_bytes() + step.write_bytes() + step.delete_bytes()
    );
    assert!(step.read_bytes() > 0 || step.source_root().count() == 1);
    assert!(step.write_bytes() > 0);
}
