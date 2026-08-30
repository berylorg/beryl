#![cfg(feature = "test-faults")]

use std::{
    num::NonZeroU64,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    WholeHomeScrubTrigger,
};
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId};
use syndic_storage::{
    DRAFT_MARKER_ADMISSION_PAGE_MAX_ASSOCIATIONS, DraftEditorCandidateSessionIdV1,
    DraftMarkerAdmissionAssignmentContinuationV1, DraftMarkerAdmissionChildV1,
    DraftMarkerAdmissionCleanupCursorV1, DraftMarkerAdmissionCodecFixtureV1,
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionEvidenceV1,
    DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodeIdV1,
    DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodeKindV1, DraftMarkerAdmissionNodeV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionPageIdentityV1, DraftMarkerAdmissionReceiptTransitionV1,
    DraftMarkerAdmissionReplayReceiptV1, DraftMarkerAdmissionRetainedChargeV1,
    DraftMarkerAdmissionRootV1, DraftMarkerAdmissionSchemaErrorV1, DraftMarkerAdmissionSourceKeyV1,
    DraftMarkerAdmissionTargetDispositionV1, DraftMarkerAdmissionTreeV1, SyndicStorage,
    canonical_empty_draft_marker_admission_root_v1, draft_marker_admission_codec_accepts,
    draft_marker_admission_corrupted_value_rejected, draft_marker_admission_head_encoded_charge_v1,
    draft_marker_admission_node_encoded_charge_v1,
    draft_marker_admission_receipt_encoded_charge_v1,
    test_faults::{
        DraftMarkerAdmissionFixtureSnapshotV1,
        draft_marker_admission_capacity_without_heads_contribution,
        draft_marker_admission_fixture_contribution,
        inject_malformed_draft_marker_admission_capacity,
        inject_malformed_draft_marker_admission_head, reset_validation_page_metrics,
        syndic_v7_family_names, validation_page_metrics,
    },
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase213-{name}-{}-{sequence}",
            std::process::id()
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

fn open(home: &TestHome) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn owner() -> DraftMarkerAdmissionOwnerV1 {
    owner_with_seed(1)
}

fn owner_with_seed(seed: u8) -> DraftMarkerAdmissionOwnerV1 {
    DraftMarkerAdmissionOwnerV1::new(
        SyndicDraftId::from_bytes([seed; 16]),
        DraftEditorCandidateSessionIdV1::from_bytes([seed.wrapping_add(1); 16]),
        DraftMarkerAdmissionOperationIdV1::from_bytes([seed.wrapping_add(2); 16]),
    )
}

fn settled_head(encoded_bytes: u64) -> DraftMarkerAdmissionHeadV1 {
    settled_head_for(owner(), encoded_bytes)
}

fn settled_head_for(
    owner: DraftMarkerAdmissionOwnerV1,
    encoded_bytes: u64,
) -> DraftMarkerAdmissionHeadV1 {
    DraftMarkerAdmissionHeadV1::new(
        owner,
        NonZeroU64::MIN,
        NonZeroU64::MIN,
        DraftMarkerAdmissionLifecycleV1::Settled,
        DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
        DraftMarkerAdmissionDigestV1::from_bytes([5; 32]),
        NonZeroU64::MIN,
        0,
        true,
        None,
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder),
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId),
        DraftMarkerAdmissionDigestV1::from_bytes([6; 32]),
        0,
        None,
        0,
        DraftMarkerAdmissionRetainedChargeV1::new(1, 0, encoded_bytes),
        None,
    )
    .unwrap()
}

fn source_leaf(
    owner: DraftMarkerAdmissionOwnerV1,
    id: u8,
    label: u64,
    marker: u8,
) -> DraftMarkerAdmissionNodeV1 {
    DraftMarkerAdmissionNodeV1::source_leaf(
        DraftMarkerAdmissionNodeKeyV1::new(
            owner,
            DraftMarkerAdmissionNodeKindV1::Leaf,
            DraftMarkerAdmissionNodeIdV1::from_bytes([id; 16]),
        ),
        DraftMarkerAdmissionSourceKeyV1::new(
            ImageLabelOrdinal::new(label).unwrap(),
            SyndicDraftMarkerId::from_bytes([marker; 16]),
        ),
        DraftMarkerAdmissionEvidenceV1::new([id, marker].as_slice()).unwrap(),
        AssetId::sha256_v1([id; 32], NonZeroU64::MIN),
    )
    .unwrap()
}

fn target_leaf(
    owner: DraftMarkerAdmissionOwnerV1,
    id: u8,
    label: u64,
    marker: u8,
) -> DraftMarkerAdmissionNodeV1 {
    target_leaf_with_disposition(
        owner,
        id,
        label,
        marker,
        DraftMarkerAdmissionTargetDispositionV1::Unassigned,
    )
}

fn target_leaf_with_disposition(
    owner: DraftMarkerAdmissionOwnerV1,
    id: u8,
    label: u64,
    marker: u8,
    disposition: DraftMarkerAdmissionTargetDispositionV1,
) -> DraftMarkerAdmissionNodeV1 {
    DraftMarkerAdmissionNodeV1::target_leaf(
        DraftMarkerAdmissionNodeKeyV1::new(
            owner,
            DraftMarkerAdmissionNodeKindV1::Leaf,
            DraftMarkerAdmissionNodeIdV1::from_bytes([id; 16]),
        ),
        SyndicDraftMarkerId::from_bytes([marker; 16]),
        DraftMarkerAdmissionPageIdentityV1::new(
            DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16]),
            NonZeroU64::MIN,
        ),
        DraftMarkerAdmissionEvidenceV1::new([id, marker].as_slice()).unwrap(),
        ImageLabelOrdinal::new(label).unwrap(),
        AssetId::sha256_v1([id; 32], NonZeroU64::MIN),
        disposition,
    )
    .unwrap()
}

fn root(node: &DraftMarkerAdmissionNodeV1) -> DraftMarkerAdmissionRootV1 {
    DraftMarkerAdmissionRootV1::new(
        node.tree(),
        node.key(),
        node.height(),
        node.digest(),
        node.count().unwrap(),
    )
    .unwrap()
}

fn child(node: &DraftMarkerAdmissionNodeV1) -> DraftMarkerAdmissionChildV1 {
    DraftMarkerAdmissionChildV1::new(
        node.key(),
        node.digest(),
        node.count().unwrap(),
        node.envelope().unwrap(),
    )
}

#[derive(Clone, Copy)]
enum ActiveFixtureFault {
    None,
    ExactPredecessorClosure,
    MismatchedAfterRoot,
    CrossOwnerRoot,
    WrongTransition,
    MissingPredecessorClosure,
}

fn active_fixture(fault: ActiveFixtureFault) -> DraftMarkerAdmissionFixtureSnapshotV1 {
    let owner = owner();
    let source = source_leaf(owner, 40, 10, 50);
    let target = target_leaf(owner, 41, 10, 50);
    let source_root = root(&source);
    let target_root = root(&target);
    let empty_source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let empty_target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    let mut nodes = vec![source, target];
    let mut source_before = empty_source;
    let mut target_before = empty_target;
    let mut source_after = source_root;
    let mut transition = DraftMarkerAdmissionReceiptTransitionV1::Ingestion;
    let mut retained_predecessor_nodes = Vec::new();
    match fault {
        ActiveFixtureFault::None => {}
        ActiveFixtureFault::ExactPredecessorClosure => {
            let predecessor = source_leaf(owner, 42, 9, 49);
            let predecessor_target = target_leaf(owner, 43, 10, 50);
            source_before = root(&predecessor);
            target_before = root(&predecessor_target);
            retained_predecessor_nodes.push(child(&predecessor));
            retained_predecessor_nodes.push(child(&predecessor_target));
            nodes.push(predecessor);
            nodes.push(predecessor_target);
        }
        ActiveFixtureFault::MismatchedAfterRoot => source_after = empty_source,
        ActiveFixtureFault::CrossOwnerRoot => {
            let foreign_owner = DraftMarkerAdmissionOwnerV1::new(
                SyndicDraftId::from_bytes([60; 16]),
                DraftEditorCandidateSessionIdV1::from_bytes([61; 16]),
                DraftMarkerAdmissionOperationIdV1::from_bytes([62; 16]),
            );
            source_before = root(&source_leaf(foreign_owner, 63, 9, 49));
        }
        ActiveFixtureFault::WrongTransition => {
            transition = DraftMarkerAdmissionReceiptTransitionV1::Assignment;
        }
        ActiveFixtureFault::MissingPredecessorClosure => {
            let predecessor = source_leaf(owner, 42, 9, 49);
            source_before = root(&predecessor);
            nodes.push(predecessor);
        }
    }
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner,
        DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16]),
        NonZeroU64::MIN,
        DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
        [9].as_slice(),
        [10].as_slice(),
        source_before,
        source_after,
        target_before,
        target_root,
        retained_predecessor_nodes,
        transition,
    )
    .unwrap();
    let retained_associations = nodes
        .iter()
        .filter(|node| node.tree() == DraftMarkerAdmissionTreeV1::TargetId && node.height() == 1)
        .count() as u64;
    let head_with_charge = |encoded_bytes| {
        DraftMarkerAdmissionHeadV1::new(
            owner,
            NonZeroU64::MIN,
            NonZeroU64::MIN,
            DraftMarkerAdmissionLifecycleV1::Ingesting,
            DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
            DraftMarkerAdmissionDigestV1::from_bytes([5; 32]),
            NonZeroU64::MIN,
            1,
            false,
            Some(DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16])),
            source_root,
            target_root,
            DraftMarkerAdmissionDigestV1::from_bytes([6; 32]),
            1,
            None,
            0,
            DraftMarkerAdmissionRetainedChargeV1::new(1, retained_associations, encoded_bytes),
            None,
        )
        .unwrap()
    };
    let provisional = head_with_charge(0);
    let node_bytes = nodes.iter().try_fold(0_u64, |total, node| {
        total.checked_add(draft_marker_admission_node_encoded_charge_v1(node).unwrap())
    });
    let encoded_bytes = draft_marker_admission_head_encoded_charge_v1(&provisional)
        .unwrap()
        .checked_add(node_bytes.unwrap())
        .unwrap()
        .checked_add(draft_marker_admission_receipt_encoded_charge_v1(&receipt).unwrap())
        .unwrap();
    let head = head_with_charge(encoded_bytes);
    let capacity =
        syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, head.charge())
            .unwrap();
    DraftMarkerAdmissionFixtureSnapshotV1::new(capacity, vec![head], nodes, vec![receipt])
}

fn terminal_fixture(cursor_after: u8) -> DraftMarkerAdmissionFixtureSnapshotV1 {
    let owner = owner();
    let source = source_leaf(owner, 90, 10, 91);
    let target = target_leaf(owner, 92, 10, 91);
    let nodes = vec![source, target];
    let empty_source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let empty_target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner,
        DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16]),
        NonZeroU64::MIN,
        DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
        [9].as_slice(),
        [10].as_slice(),
        empty_source,
        empty_source,
        empty_target,
        empty_target,
        Vec::new(),
        DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup,
    )
    .unwrap();
    let head_with_charge = |encoded_bytes| {
        DraftMarkerAdmissionHeadV1::new(
            owner,
            NonZeroU64::MIN,
            NonZeroU64::MIN,
            DraftMarkerAdmissionLifecycleV1::TerminalCleanup,
            DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
            DraftMarkerAdmissionDigestV1::from_bytes([5; 32]),
            NonZeroU64::MIN,
            0,
            true,
            None,
            empty_source,
            empty_target,
            DraftMarkerAdmissionDigestV1::from_bytes([6; 32]),
            0,
            None,
            0,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 1, encoded_bytes),
            Some(DraftMarkerAdmissionCleanupCursorV1::new(
                DraftMarkerAdmissionTreeV1::SourceOrder,
                Some(DraftMarkerAdmissionNodeKeyV1::new(
                    owner,
                    DraftMarkerAdmissionNodeKindV1::Leaf,
                    DraftMarkerAdmissionNodeIdV1::from_bytes([cursor_after; 16]),
                )),
            )),
        )
        .unwrap()
    };
    let provisional = head_with_charge(0);
    let encoded_bytes = nodes
        .iter()
        .try_fold(
            draft_marker_admission_head_encoded_charge_v1(&provisional).unwrap(),
            |total, node| {
                total.checked_add(draft_marker_admission_node_encoded_charge_v1(node).unwrap())
            },
        )
        .unwrap()
        .checked_add(draft_marker_admission_receipt_encoded_charge_v1(&receipt).unwrap())
        .unwrap();
    let head = head_with_charge(encoded_bytes);
    let capacity =
        syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, head.charge())
            .unwrap();
    DraftMarkerAdmissionFixtureSnapshotV1::new(capacity, vec![head], nodes, vec![receipt])
}

fn assigning_fixture(
    prior_label: u64,
    prior_asset_seed: u8,
    processed_target_unassigned: bool,
) -> DraftMarkerAdmissionFixtureSnapshotV1 {
    let owner = owner();
    let source = source_leaf(owner, 80, 11, 81);
    let processed_disposition = if processed_target_unassigned {
        DraftMarkerAdmissionTargetDispositionV1::Unassigned
    } else {
        DraftMarkerAdmissionTargetDispositionV1::Assigned(ImageLabelOrdinal::new(100).unwrap())
    };
    let target_a = target_leaf_with_disposition(owner, 82, 10, 80, processed_disposition);
    let target_b = target_leaf(owner, 83, 11, 81);
    let target = DraftMarkerAdmissionNodeV1::internal(
        DraftMarkerAdmissionNodeKeyV1::new(
            owner,
            DraftMarkerAdmissionNodeKindV1::Internal,
            DraftMarkerAdmissionNodeIdV1::from_bytes([84; 16]),
        ),
        DraftMarkerAdmissionTreeV1::TargetId,
        2,
        vec![child(&target_a), child(&target_b)],
    )
    .unwrap();
    let source_root = root(&source);
    let target_root = root(&target);
    let nodes = vec![source, target_a, target_b, target];
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner,
        DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16]),
        NonZeroU64::MIN,
        DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
        [9].as_slice(),
        [10].as_slice(),
        source_root,
        source_root,
        target_root,
        target_root,
        Vec::new(),
        DraftMarkerAdmissionReceiptTransitionV1::Assignment,
    )
    .unwrap();
    let continuation = DraftMarkerAdmissionAssignmentContinuationV1::new(
        ImageLabelOrdinal::new(100).unwrap(),
        ImageLabelOrdinal::new(101).unwrap(),
        ImageLabelOrdinal::new(100).unwrap(),
        Some((
            ImageLabelOrdinal::new(prior_label).unwrap(),
            AssetId::sha256_v1([prior_asset_seed; 32], NonZeroU64::MIN),
        )),
    )
    .unwrap();
    let head_with_charge = |encoded_bytes| {
        DraftMarkerAdmissionHeadV1::new(
            owner,
            NonZeroU64::MIN,
            NonZeroU64::MIN,
            DraftMarkerAdmissionLifecycleV1::Assigning,
            DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
            DraftMarkerAdmissionDigestV1::from_bytes([5; 32]),
            NonZeroU64::MIN,
            0,
            true,
            Some(DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16])),
            source_root,
            target_root,
            DraftMarkerAdmissionDigestV1::from_bytes([6; 32]),
            1,
            Some(continuation),
            0,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 2, encoded_bytes),
            None,
        )
        .unwrap()
    };
    let provisional = head_with_charge(0);
    let encoded_bytes = nodes
        .iter()
        .try_fold(
            draft_marker_admission_head_encoded_charge_v1(&provisional).unwrap(),
            |total, node| {
                total.checked_add(draft_marker_admission_node_encoded_charge_v1(node).unwrap())
            },
        )
        .unwrap()
        .checked_add(draft_marker_admission_receipt_encoded_charge_v1(&receipt).unwrap())
        .unwrap();
    let head = head_with_charge(encoded_bytes);
    let capacity =
        syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, head.charge())
            .unwrap();
    DraftMarkerAdmissionFixtureSnapshotV1::new(capacity, vec![head], nodes, vec![receipt])
}

fn persist_and_scrub(name: &str, snapshot: DraftMarkerAdmissionFixtureSnapshotV1) -> bool {
    let home = TestHome::new(name);
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            snapshot,
        ),
    );
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .is_ok()
}

#[test]
fn page_cursor_and_assignment_continuation_enforce_their_bounds() {
    let first = ImageLabelOrdinal::new(10).unwrap();
    let last = ImageLabelOrdinal::new(20).unwrap();
    let next = ImageLabelOrdinal::new(15).unwrap();
    let continuation = DraftMarkerAdmissionAssignmentContinuationV1::new(
        first,
        last,
        next,
        Some((first, AssetId::sha256_v1([30; 32], NonZeroU64::MIN))),
    )
    .unwrap();
    assert_eq!(continuation.reserved_first(), first);
    assert_eq!(continuation.reserved_last(), last);
    assert_eq!(continuation.next_allocation(), next);
    assert_eq!(
        DraftMarkerAdmissionAssignmentContinuationV1::new(last, first, next, None),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)
    );

    let empty_source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let empty_target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    assert_eq!(
        DraftMarkerAdmissionHeadV1::new(
            owner(),
            NonZeroU64::MIN,
            NonZeroU64::MIN,
            DraftMarkerAdmissionLifecycleV1::Settled,
            DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
            DraftMarkerAdmissionDigestV1::from_bytes([5; 32]),
            NonZeroU64::MIN,
            DRAFT_MARKER_ADMISSION_PAGE_MAX_ASSOCIATIONS + 1,
            true,
            None,
            empty_source,
            empty_target,
            DraftMarkerAdmissionDigestV1::from_bytes([6; 32]),
            0,
            None,
            0,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 0, 0),
            None,
        ),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)
    );
}

#[test]
fn assigning_heads_enforce_reservation_cardinality_and_progress() {
    let owner = owner();
    let remaining_source = source_leaf(owner, 80, 11, 81);
    let target_a = target_leaf(owner, 82, 10, 80);
    let target_b = target_leaf(owner, 83, 11, 81);
    let target = DraftMarkerAdmissionNodeV1::internal(
        DraftMarkerAdmissionNodeKeyV1::new(
            owner,
            DraftMarkerAdmissionNodeKindV1::Internal,
            DraftMarkerAdmissionNodeIdV1::from_bytes([84; 16]),
        ),
        DraftMarkerAdmissionTreeV1::TargetId,
        2,
        vec![child(&target_a), child(&target_b)],
    )
    .unwrap();
    let build = |continuation| {
        DraftMarkerAdmissionHeadV1::new(
            owner,
            NonZeroU64::MIN,
            NonZeroU64::MIN,
            DraftMarkerAdmissionLifecycleV1::Assigning,
            DraftMarkerAdmissionDigestV1::from_bytes([4; 32]),
            DraftMarkerAdmissionDigestV1::from_bytes([5; 32]),
            NonZeroU64::MIN,
            0,
            true,
            Some(DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16])),
            root(&remaining_source),
            root(&target),
            DraftMarkerAdmissionDigestV1::from_bytes([6; 32]),
            1,
            Some(continuation),
            0,
            DraftMarkerAdmissionRetainedChargeV1::new(1, 2, 0),
            None,
        )
    };
    let prior = Some((
        ImageLabelOrdinal::new(10).unwrap(),
        AssetId::sha256_v1([82; 32], NonZeroU64::MIN),
    ));
    let exact = DraftMarkerAdmissionAssignmentContinuationV1::new(
        ImageLabelOrdinal::new(100).unwrap(),
        ImageLabelOrdinal::new(101).unwrap(),
        ImageLabelOrdinal::new(100).unwrap(),
        prior,
    )
    .unwrap();
    assert!(build(exact).is_ok());

    let oversized = DraftMarkerAdmissionAssignmentContinuationV1::new(
        ImageLabelOrdinal::new(100).unwrap(),
        ImageLabelOrdinal::new(102).unwrap(),
        ImageLabelOrdinal::new(100).unwrap(),
        prior,
    )
    .unwrap();
    assert_eq!(
        build(oversized),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)
    );
    let missing_prior = DraftMarkerAdmissionAssignmentContinuationV1::new(
        ImageLabelOrdinal::new(100).unwrap(),
        ImageLabelOrdinal::new(101).unwrap(),
        ImageLabelOrdinal::new(100).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        build(missing_prior),
        Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)
    );
}

#[test]
fn four_admission_families_are_appended_without_reordering_prior_families() {
    let names = syndic_v7_family_names();
    assert_eq!(names.len(), 86);
    assert_eq!(
        &names[81..],
        &[
            "provider-observation-chunks",
            "draft-marker-label-admission-capacity",
            "draft-marker-label-admission-heads",
            "draft-marker-label-admission-nodes",
            "draft-marker-label-admission-receipts",
        ]
    );
}

#[test]
fn canonical_head_receipt_and_capacity_codecs_round_trip_and_reject_digest_damage() {
    let provisional = settled_head(0);
    let head = settled_head(draft_marker_admission_head_encoded_charge_v1(&provisional).unwrap());
    let capacity =
        syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, head.charge())
            .unwrap();
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner(),
        DraftMarkerAdmissionCommandIdV1::from_bytes([7; 16]),
        NonZeroU64::MIN,
        DraftMarkerAdmissionDigestV1::from_bytes([8; 32]),
        [9].as_slice(),
        [10].as_slice(),
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder),
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder),
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId),
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId),
        Vec::new(),
        DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup,
    )
    .unwrap();
    for fixture in [
        DraftMarkerAdmissionCodecFixtureV1::Capacity(capacity),
        DraftMarkerAdmissionCodecFixtureV1::Head(head),
        DraftMarkerAdmissionCodecFixtureV1::Receipt(receipt),
    ] {
        assert!(draft_marker_admission_codec_accepts(fixture.clone()));
        assert!(draft_marker_admission_corrupted_value_rejected(fixture));
    }
}

#[test]
fn explicit_validation_accepts_exact_charges_in_bounded_pages() {
    let home = TestHome::new("valid-bounded");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let provisional = settled_head(0);
    let head = settled_head(draft_marker_admission_head_encoded_charge_v1(&provisional).unwrap());
    let capacity =
        syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, head.charge())
            .unwrap();
    execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            DraftMarkerAdmissionFixtureSnapshotV1::new(
                capacity,
                vec![head],
                Vec::new(),
                Vec::new(),
            ),
        ),
    );
    reset_validation_page_metrics();
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let metrics = validation_page_metrics();
    assert!(metrics.page_count() > 0);
    assert!(metrics.max_page_items() <= metrics.item_limit());
    assert!(metrics.max_page_stored_bytes() <= metrics.byte_limit());
}

#[test]
fn explicit_validation_refuses_malformed_and_aggregate_only_capacity_state() {
    let malformed = TestHome::new("malformed");
    let mut store = open(&malformed);
    let storage = SyndicStorage::register(&mut store).unwrap();
    inject_malformed_draft_marker_admission_capacity(&store, storage).unwrap();
    assert!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );

    let aggregate_only = TestHome::new("aggregate-only");
    let mut store = open(&aggregate_only);
    let storage = SyndicStorage::register(&mut store).unwrap();
    execute(
        &store,
        draft_marker_admission_capacity_without_heads_contribution(
            &storage,
            storage.revision(&store).unwrap(),
        ),
    );
    assert!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}

#[test]
fn registration_reconstructs_empty_and_bounded_persisted_admission_state() {
    let empty = TestHome::new("attachment-empty");
    let mut store = open(&empty);
    SyndicStorage::register(&mut store).unwrap();
    store.close().unwrap();
    let mut reopened = open(&empty);
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.close().unwrap();

    let populated = TestHome::new("attachment-populated");
    let mut store = open(&populated);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let provisional = settled_head(0);
    let head = settled_head(draft_marker_admission_head_encoded_charge_v1(&provisional).unwrap());
    let capacity =
        syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, head.charge())
            .unwrap();
    execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            DraftMarkerAdmissionFixtureSnapshotV1::new(
                capacity,
                vec![head],
                Vec::new(),
                Vec::new(),
            ),
        ),
    );
    store.close().unwrap();
    let mut reopened = open(&populated);
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.close().unwrap();
}

#[test]
fn registration_refuses_capacity_disagreement_without_publishing_the_domain() {
    let home = TestHome::new("attachment-capacity-disagreement");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let provisional = settled_head(0);
    let head = settled_head(draft_marker_admission_head_encoded_charge_v1(&provisional).unwrap());
    let capacity = syndic_storage::DraftMarkerAdmissionCapacityV1::new(
        NonZeroU64::MIN,
        DraftMarkerAdmissionRetainedChargeV1::ZERO,
    )
    .unwrap();
    execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            DraftMarkerAdmissionFixtureSnapshotV1::new(
                capacity,
                vec![head],
                Vec::new(),
                Vec::new(),
            ),
        ),
    );
    store.close().unwrap();
    let mut reopened = open(&home);
    assert!(SyndicStorage::register(&mut reopened).is_err());
    assert!(SyndicStorage::reacquire(&reopened).is_err());
    reopened.close().unwrap();
}

fn settled_heads(count: u8) -> Vec<DraftMarkerAdmissionHeadV1> {
    (1..=count)
        .map(|seed| {
            let owner = owner_with_seed(seed);
            let provisional = settled_head_for(owner, 0);
            settled_head_for(
                owner,
                draft_marker_admission_head_encoded_charge_v1(&provisional).unwrap(),
            )
        })
        .collect()
}

fn capacity_for_heads(
    heads: &[DraftMarkerAdmissionHeadV1],
) -> syndic_storage::DraftMarkerAdmissionCapacityV1 {
    let charge = heads
        .iter()
        .fold(DraftMarkerAdmissionRetainedChargeV1::ZERO, |total, head| {
            total.checked_add(head.charge()).unwrap()
        });
    syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, charge).unwrap()
}

#[test]
fn registration_reconstructs_exactly_sixty_four_distinct_heads() {
    let home = TestHome::new("attachment-sixty-four-heads");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let heads = settled_heads(64);
    execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            DraftMarkerAdmissionFixtureSnapshotV1::new(
                capacity_for_heads(&heads),
                heads,
                Vec::new(),
                Vec::new(),
            ),
        ),
    );
    store.close().unwrap();
    let mut reopened = open(&home);
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.close().unwrap();
}

#[test]
fn registration_refuses_the_sixty_fifth_distinct_head_without_publishing() {
    let home = TestHome::new("attachment-sixty-five-heads");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let heads = settled_heads(65);
    execute(
        &store,
        draft_marker_admission_fixture_contribution(
            &storage,
            storage.revision(&store).unwrap(),
            DraftMarkerAdmissionFixtureSnapshotV1::new(
                capacity_for_heads(&heads[..64]),
                heads,
                Vec::new(),
                Vec::new(),
            ),
        ),
    );
    store.close().unwrap();
    let mut reopened = open(&home);
    assert!(SyndicStorage::register(&mut reopened).is_err());
    assert!(SyndicStorage::reacquire(&reopened).is_err());
    reopened.close().unwrap();
}

#[test]
fn registration_refuses_malformed_persisted_singleton_and_head_authority() {
    for malformed_head in [false, true] {
        let home = TestHome::new("attachment-malformed-authority");
        let mut store = open(&home);
        let storage = SyndicStorage::register(&mut store).unwrap();
        let head = settled_heads(1).pop().unwrap();
        execute(
            &store,
            draft_marker_admission_fixture_contribution(
                &storage,
                storage.revision(&store).unwrap(),
                DraftMarkerAdmissionFixtureSnapshotV1::new(
                    capacity_for_heads(std::slice::from_ref(&head)),
                    vec![head.clone()],
                    Vec::new(),
                    Vec::new(),
                ),
            ),
        );
        if malformed_head {
            inject_malformed_draft_marker_admission_head(&store, storage, head.owner()).unwrap();
        } else {
            inject_malformed_draft_marker_admission_capacity(&store, storage).unwrap();
        }
        store.close().unwrap();
        let mut reopened = open(&home);
        assert!(SyndicStorage::register(&mut reopened).is_err());
        assert!(SyndicStorage::reacquire(&reopened).is_err());
        reopened.close().unwrap();
    }
}

#[test]
fn explicit_validation_refuses_charged_owner_local_orphans() {
    let node = source_leaf(owner(), 70, 10, 71);
    let provisional = settled_head(0);
    let encoded_bytes = draft_marker_admission_head_encoded_charge_v1(&provisional)
        .unwrap()
        .checked_add(draft_marker_admission_node_encoded_charge_v1(&node).unwrap())
        .unwrap();
    let head = settled_head(encoded_bytes);
    let capacity =
        syndic_storage::DraftMarkerAdmissionCapacityV1::new(NonZeroU64::MIN, head.charge())
            .unwrap();
    assert!(!persist_and_scrub(
        "owner-local-orphan",
        DraftMarkerAdmissionFixtureSnapshotV1::new(capacity, vec![head], vec![node], Vec::new(),),
    ));
}

#[test]
fn explicit_validation_ties_selected_receipt_to_head_and_exact_predecessor_closure() {
    assert!(persist_and_scrub(
        "active-valid",
        active_fixture(ActiveFixtureFault::None),
    ));
    assert!(persist_and_scrub(
        "active-valid-predecessor-closure",
        active_fixture(ActiveFixtureFault::ExactPredecessorClosure),
    ));
    for (name, fault) in [
        (
            "mismatched-after-root",
            ActiveFixtureFault::MismatchedAfterRoot,
        ),
        ("cross-owner-root", ActiveFixtureFault::CrossOwnerRoot),
        ("wrong-transition", ActiveFixtureFault::WrongTransition),
        (
            "missing-predecessor-closure",
            ActiveFixtureFault::MissingPredecessorClosure,
        ),
    ] {
        assert!(!persist_and_scrub(name, active_fixture(fault)), "{name}");
    }
}

#[test]
fn explicit_validation_accepts_only_terminal_cleanup_cursor_residue() {
    assert!(persist_and_scrub(
        "terminal-cursor-valid",
        terminal_fixture(89),
    ));
    assert!(!persist_and_scrub(
        "terminal-cursor-preceded",
        terminal_fixture(91),
    ));
}

#[test]
fn explicit_validation_checks_assigning_prior_source_against_authenticated_least_leaf() {
    assert!(persist_and_scrub(
        "assigning-prior-before",
        assigning_fixture(10, 99, false),
    ));
    assert!(persist_and_scrub(
        "assigning-prior-equal",
        assigning_fixture(11, 80, false),
    ));
    assert!(!persist_and_scrub(
        "assigning-prior-after",
        assigning_fixture(12, 99, false),
    ));
    assert!(!persist_and_scrub(
        "assigning-prior-asset-mismatch",
        assigning_fixture(11, 99, false),
    ));
}

#[test]
fn explicit_validation_refuses_assigning_unassigned_disposition_mismatch() {
    assert!(!persist_and_scrub(
        "assigning-unassigned-disposition-mismatch",
        assigning_fixture(10, 99, true),
    ));
}
