use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AssetId, SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId,
};

use super::*;

fn accepted(index: u16) -> AssetReferenceOwner {
    let bytes = index.to_be_bytes();
    let mut input = [0; 16];
    input[..2].copy_from_slice(&bytes);
    let mut marker = [0; 16];
    marker[..2].copy_from_slice(&bytes);
    marker[15] = 1;
    AssetReferenceOwner::AcceptedInputMarker {
        input_id: SyndicAcceptedInputId::from_bytes(input),
        marker_id: SyndicDraftMarkerId::from_bytes(marker),
    }
}

fn submitted(index: u16) -> AssetReferenceOwner {
    let bytes = index.to_be_bytes();
    let mut item = [0; 16];
    item[..2].copy_from_slice(&bytes);
    let mut marker = [0; 16];
    marker[..2].copy_from_slice(&bytes);
    marker[15] = 2;
    AssetReferenceOwner::SubmittedTurnItemMarker {
        item_id: SyndicItemId::from_bytes(item),
        marker_id: SyndicDraftMarkerId::from_bytes(marker),
    }
}

fn draft(index: u16) -> AssetReferenceOwner {
    let bytes = index.to_be_bytes();
    let mut draft = [0; 16];
    draft[..2].copy_from_slice(&bytes);
    let mut marker = [0; 16];
    marker[..2].copy_from_slice(&bytes);
    marker[15] = 3;
    AssetReferenceOwner::CurrentDraftMarker {
        draft_id: SyndicDraftId::from_bytes(draft),
        marker_id: SyndicDraftMarkerId::from_bytes(marker),
    }
}

fn admit_asset(
    store: &mut HomeStore,
    assets: AssetState,
    bytes: &[u8],
    owner: AssetReferenceOwner,
) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new(ASSET_NAMESPACE).unwrap(),
            bytes,
            sidecar_limit(),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let revision = assets.revision(store).unwrap();
    let create = CreateAssetWithReference::new(
        asset_id,
        AssetMediaType::new("image/png").unwrap(),
        None,
        revision.checked_next().unwrap(),
        owner,
        UnixMillis::new(10),
    )
    .unwrap();
    let first = assets
        .create_with_reference(revision, sidecar, create)
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    first.add_to(&mut command).unwrap();
    store.execute(command).unwrap();
    asset_id
}

fn add_one(
    store: &mut HomeStore,
    assets: AssetState,
    asset_id: AssetId,
    owner: AssetReferenceOwner,
) {
    let metadata = assets.metadata(store, asset_id).unwrap().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            assets.add_reference(
                assets.revision(store).unwrap(),
                AddAssetReference::new(asset_id, metadata.revision(), owner, UnixMillis::new(11))
                    .unwrap(),
            ),
        )
        .unwrap();
    store.execute(command).unwrap();
}

#[test]
fn move_batch_preserves_repeated_asset_counts_revisions_and_exact_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let asset_a = admit_asset(&mut store, assets, b"move asset a", accepted(1));
    add_one(&mut store, assets, asset_a, accepted(2));
    let asset_b = admit_asset(&mut store, assets, b"move asset b", accepted(3));
    let metadata_a = assets.metadata(&store, asset_a).unwrap().unwrap();
    let metadata_b = assets.metadata(&store, asset_b).unwrap().unwrap();

    let moves = MoveAssetReferences::new(vec![
        AssetReferenceMove::new(accepted(1), submitted(11), asset_a).unwrap(),
        AssetReferenceMove::new(accepted(2), submitted(12), asset_a).unwrap(),
        AssetReferenceMove::new(accepted(3), submitted(13), asset_b).unwrap(),
    ])
    .unwrap();
    assert_eq!(
        assets.reference_move_status(&store, &moves).unwrap(),
        AssetReferenceMoveStatus::Source
    );

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(assets.move_references(assets.revision(&store).unwrap(), moves.clone()))
        .unwrap();
    store.execute(command).unwrap();

    assert_eq!(
        assets.reference_move_status(&store, &moves).unwrap(),
        AssetReferenceMoveStatus::Target
    );
    assert_eq!(assets.metadata(&store, asset_a).unwrap(), Some(metadata_a));
    assert_eq!(assets.metadata(&store, asset_b).unwrap(), Some(metadata_b));
    for reference_move in moves.moves() {
        assert!(
            assets
                .reference(&store, reference_move.source())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            assets
                .reference(&store, reference_move.destination())
                .unwrap()
                .unwrap()
                .asset_id(),
            reference_move.asset_id()
        );
    }
    let page = assets
        .references_for_asset(
            &store,
            asset_a,
            None,
            CursorReadLimits::new(8, 8 * 1_024).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 2);
}

#[test]
fn addition_batch_retains_history_and_advances_each_repeated_asset_once() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let historical_a = submitted(20);
    let historical_b = submitted(21);
    let asset_a = admit_asset(&mut store, assets, b"copy asset a", historical_a);
    let asset_b = admit_asset(&mut store, assets, b"copy asset b", historical_b);
    let before_a = assets.metadata(&store, asset_a).unwrap().unwrap();
    let before_b = assets.metadata(&store, asset_b).unwrap().unwrap();

    let additions = AddAssetReferences::new(vec![
        AssetReferenceAddition::new(draft(30), asset_a, before_a.revision(), UnixMillis::new(30))
            .unwrap(),
        AssetReferenceAddition::new(draft(31), asset_a, before_a.revision(), UnixMillis::new(31))
            .unwrap(),
        AssetReferenceAddition::new(draft(32), asset_b, before_b.revision(), UnixMillis::new(32))
            .unwrap(),
    ])
    .unwrap();
    assert_eq!(
        assets
            .reference_addition_status(&store, &additions)
            .unwrap(),
        AssetReferenceAdditionStatus::Absent
    );

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(assets.add_references(assets.revision(&store).unwrap(), additions.clone()))
        .unwrap();
    store.execute(command).unwrap();

    assert_eq!(
        assets
            .reference_addition_status(&store, &additions)
            .unwrap(),
        AssetReferenceAdditionStatus::Target
    );
    let after_a = assets.metadata(&store, asset_a).unwrap().unwrap();
    let after_b = assets.metadata(&store, asset_b).unwrap().unwrap();
    assert_eq!(after_a.reference_count(), before_a.reference_count() + 2);
    assert_eq!(after_b.reference_count(), before_b.reference_count() + 1);
    assert_eq!(
        after_a.revision(),
        before_a.revision().checked_next().unwrap()
    );
    assert_eq!(
        after_b.revision(),
        before_b.revision().checked_next().unwrap()
    );
    assert!(assets.reference(&store, historical_a).unwrap().is_some());
    assert!(assets.reference(&store, historical_b).unwrap().is_some());
}

#[test]
fn move_status_never_infers_target_from_an_occupied_or_mismatched_partial_set() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = open(directory.path());
    let assets = AssetState::register(&mut store).unwrap();
    let asset_a = admit_asset(&mut store, assets, b"collision asset a", accepted(40));
    let asset_b = admit_asset(&mut store, assets, b"collision asset b", accepted(41));
    add_one(&mut store, assets, asset_a, submitted(40));

    let occupied = MoveAssetReferences::new(vec![
        AssetReferenceMove::new(accepted(40), submitted(40), asset_a).unwrap(),
    ])
    .unwrap();
    assert_eq!(
        assets.reference_move_status(&store, &occupied).unwrap(),
        AssetReferenceMoveStatus::CollisionOrMixed
    );
    let count = assets
        .metadata(&store, asset_a)
        .unwrap()
        .unwrap()
        .reference_count();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(assets.move_references(assets.revision(&store).unwrap(), occupied))
        .unwrap();
    assert!(store.execute(command).is_err());
    assert_eq!(
        assets
            .metadata(&store, asset_a)
            .unwrap()
            .unwrap()
            .reference_count(),
        count
    );

    let wrong_asset = MoveAssetReferences::new(vec![
        AssetReferenceMove::new(accepted(40), submitted(42), asset_b).unwrap(),
    ])
    .unwrap();
    assert_eq!(
        assets.reference_move_status(&store, &wrong_asset).unwrap(),
        AssetReferenceMoveStatus::CollisionOrMixed
    );
}

#[test]
fn batch_descriptions_reject_empty_duplicate_overlapping_and_oversized_sets() {
    let asset_id = AssetId::sha256_v1([8; 32], NonZeroU64::new(8).unwrap());
    assert!(matches!(
        MoveAssetReferences::new(Vec::new()),
        Err(AssetReferenceBatchError::Empty)
    ));
    assert!(matches!(
        AddAssetReferences::new(Vec::new()),
        Err(AssetReferenceBatchError::Empty)
    ));

    let first = AssetReferenceMove::new(accepted(1), submitted(1), asset_id).unwrap();
    let duplicate_source = AssetReferenceMove::new(accepted(1), submitted(2), asset_id).unwrap();
    assert!(matches!(
        MoveAssetReferences::new(vec![first, duplicate_source]),
        Err(AssetReferenceBatchError::DuplicateSource(_))
    ));
    let duplicate_destination =
        AssetReferenceMove::new(accepted(2), submitted(1), asset_id).unwrap();
    assert!(matches!(
        MoveAssetReferences::new(vec![first, duplicate_destination]),
        Err(AssetReferenceBatchError::DuplicateDestination(_))
    ));
    let overlap_a = AssetReferenceMove::new(accepted(3), accepted(4), asset_id).unwrap();
    let overlap_b = AssetReferenceMove::new(accepted(4), submitted(4), asset_id).unwrap();
    assert!(matches!(
        MoveAssetReferences::new(vec![overlap_a, overlap_b]),
        Err(AssetReferenceBatchError::SourceDestinationOverlap(_))
    ));

    let oversized = (0..=MAX_ASSET_REFERENCE_BATCH)
        .map(|index| {
            AssetReferenceMove::new(
                accepted(u16::try_from(index).unwrap()),
                submitted(u16::try_from(index).unwrap()),
                asset_id,
            )
            .unwrap()
        })
        .collect();
    assert!(matches!(
        MoveAssetReferences::new(oversized),
        Err(AssetReferenceBatchError::TooMany { .. })
    ));
}

#[test]
fn addition_description_rejects_duplicate_destinations_and_conflicting_asset_revisions() {
    let asset_id = AssetId::sha256_v1([7; 32], NonZeroU64::new(7).unwrap());
    let initial = RecordRevision::INITIAL;
    let next = initial.checked_next().unwrap();
    let first =
        AssetReferenceAddition::new(draft(50), asset_id, initial, UnixMillis::new(1)).unwrap();
    let duplicate =
        AssetReferenceAddition::new(draft(50), asset_id, initial, UnixMillis::new(2)).unwrap();
    assert!(matches!(
        AddAssetReferences::new(vec![first, duplicate]),
        Err(AssetReferenceBatchError::DuplicateDestination(_))
    ));

    let conflicting =
        AssetReferenceAddition::new(draft(51), asset_id, next, UnixMillis::new(3)).unwrap();
    assert!(matches!(
        AddAssetReferences::new(vec![first, conflicting]),
        Err(AssetReferenceBatchError::ConflictingRecordRevision { .. })
    ));
}
