#![cfg(feature = "test-faults")]

use std::num::NonZeroU64;

use beryl_app::input_admission::{InputAdmissionBuildError, start_replacement_edit_command};
use beryl_home_store::{
    HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{
    AssetId, BindingRevision, ContentRevision, DraftRevision, InputGateRevision,
    ProjectionRevision, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicProjectionId,
    SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use beryl_state::{
    AssetMediaType, AssetReferenceOwner, BerylState, CreateAssetWithReference, UnixMillis,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::*;

fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn content_records(content: &PreparedContent) -> (ContentReference, Vec<FixtureRecord>) {
    let revision = ContentRevision::new(1).unwrap();
    let mut records = vec![FixtureRecord::ContentManifest(
        content.sealed_manifest(revision),
    )];
    records.extend(
        content
            .chunks()
            .iter()
            .cloned()
            .map(FixtureRecord::ContentChunk),
    );
    (content.reference(revision), records)
}

#[allow(clippy::too_many_arguments)]
fn replacement_fixture(
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    projection: SyndicProjectionId,
    marker: ResolvedImageMarker,
) -> FixtureBatch {
    let thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let digest = root_turn_chain_digest(turn);
    let selected = SelectedPathProof::new(Some(turn), thread_revision, digest);
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("historical").unwrap(),
        ComposerAtom::image_marker(marker.marker_id(), marker.label()),
    ])
    .unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    let (target_content, mut records) = content_records(&prepared);
    let empty = PreparedContent::composer(&ComposerPayload::default()).unwrap();
    let (empty_content, empty_records) = content_records(&empty);
    records.extend(empty_records);
    records.extend([
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            thread_revision,
            Some(turn),
            draft,
            None,
            None,
            digest,
        )),
        FixtureRecord::Draft(DraftRecord::new(
            draft,
            thread,
            draft_revision,
            ConversationParent::Turn(turn),
            None,
            None,
            empty_content,
            time(1),
            time(1),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::InputGate(InputGateRecord::idle(thread)),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            TurnDepth::FIRST,
            digest,
            time(1),
        )),
        FixtureRecord::TurnState(TurnStateRecord::new(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Complete,
            0,
            1,
            time(1),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::new(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            projection_revision,
            None,
            None,
            1,
            CanonicalItemPayload::user_input(target_content, 1),
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            projection_revision,
        )),
    ]);
    records.extend([
        FixtureRecord::InputMarkerResolution(InputMarkerResolutionRecord::new(
            InputMarkerOwner::CanonicalItem(item),
            InputMarkerOrdinal::FIRST,
            marker,
        )),
        FixtureRecord::Projection(
            ProjectionRecord::new(
                projection,
                projection_revision,
                ProjectionLifecycle::Current,
                item,
                turn,
                ProjectionOrdinal::FIRST,
                0,
                "historical",
            )
            .unwrap(),
        ),
        FixtureRecord::ItemProjection(ItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection,
            projection_revision,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            1,
            Some(turn),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptViewEntry(TranscriptViewEntryRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TranscriptPosition::FIRST,
            item,
            projection_revision,
            projection,
            projection_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            thread_revision,
            Some(turn),
            digest,
            true,
            time(1),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            BindingState::unbound("replacement fixture").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
    ]);
    let mut batch = FixtureBatch::new();
    for record in records {
        batch.put(record).unwrap();
    }
    batch
}

fn create_historical_asset(
    store: &mut HomeStore,
    state: BerylState,
    owner: AssetReferenceOwner,
) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"replacement image",
            SidecarByteLimit::new(NonZeroU64::new(1_024 * 1_024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = state.assets();
    let revision = assets.revision(store).unwrap();
    let creation = CreateAssetWithReference::new(
        asset,
        AssetMediaType::new("image/png").unwrap(),
        None,
        revision.checked_next().unwrap(),
        owner,
        UnixMillis::new(1),
    )
    .unwrap();
    let first = assets
        .create_with_reference(revision, sidecar, creation)
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    first.add_to(&mut command).unwrap();
    store.execute(command).unwrap();
    asset
}

#[test]
fn replacement_start_copies_asset_ownership_without_moving_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut store).unwrap();
    let syndic = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([1; 16]);
    let draft = SyndicDraftId::from_bytes([2; 16]);
    let turn = SyndicTurnId::from_bytes([3; 16]);
    let item = SyndicItemId::from_bytes([4; 16]);
    let projection = SyndicProjectionId::from_bytes([5; 16]);
    let marker_id = SyndicDraftMarkerId::from_bytes([6; 16]);
    let historical_owner = AssetReferenceOwner::SubmittedTurnItemMarker {
        item_id: item,
        marker_id,
    };
    let asset = create_historical_asset(&mut store, state, historical_owner);
    let marker = ResolvedImageMarker::new(marker_id, ImageLabelOrdinal::FIRST, asset);
    let fixture = replacement_fixture(thread, draft, turn, item, projection, marker);
    let mut seed = HomeCommand::new(store.home_revision().unwrap());
    seed.add(syndic.fixture_contribution(syndic.revision(&store).unwrap(), fixture))
        .unwrap();
    store.execute(seed).unwrap();

    let selected = SelectedPathProof::new(
        Some(turn),
        ThreadRevision::new(1).unwrap(),
        root_turn_chain_digest(turn),
    );
    let edit = StartReplacementEdit::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(1).unwrap(),
        InputGateRevision::new(1).unwrap(),
        turn,
        item,
        selected,
        CurrentTranscriptEntryProof::new(TranscriptGeneration::FIRST, TranscriptPosition::FIRST),
        AdmissionMarkers::new(vec![marker]).unwrap(),
        time(2),
    );
    let command = start_replacement_edit_command(&store, syndic, state.assets(), edit).unwrap();
    store.execute(command).unwrap();
    store.validate_registered_domains().unwrap();

    let draft_owner = AssetReferenceOwner::CurrentDraftMarker {
        draft_id: draft,
        marker_id,
    };
    assert!(
        state
            .assets()
            .reference(&store, historical_owner)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        state
            .assets()
            .reference(&store, draft_owner)
            .unwrap()
            .unwrap()
            .asset_id(),
        asset
    );
    assert_eq!(
        state
            .assets()
            .metadata(&store, asset)
            .unwrap()
            .unwrap()
            .reference_count(),
        2
    );
    let current = syndic
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        current
            .draft()
            .replacement_edit_intent()
            .unwrap()
            .target_turn_id(),
        turn
    );
    store.close().unwrap();
}

#[test]
fn replacement_start_rejects_historical_asset_disagreement_before_building_a_command() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut store).unwrap();
    let syndic = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([20; 16]);
    let draft = SyndicDraftId::from_bytes([21; 16]);
    let turn = SyndicTurnId::from_bytes([22; 16]);
    let item = SyndicItemId::from_bytes([23; 16]);
    let projection = SyndicProjectionId::from_bytes([24; 16]);
    let marker_id = SyndicDraftMarkerId::from_bytes([25; 16]);
    let historical_owner = AssetReferenceOwner::SubmittedTurnItemMarker {
        item_id: item,
        marker_id,
    };
    let historical_asset = create_historical_asset(&mut store, state, historical_owner);
    let conflicting_asset = AssetId::sha256_v1(
        [26; 32],
        NonZeroU64::new(historical_asset.length().get() + 1).unwrap(),
    );
    let marker = ResolvedImageMarker::new(marker_id, ImageLabelOrdinal::FIRST, conflicting_asset);
    let fixture = replacement_fixture(thread, draft, turn, item, projection, marker);
    let mut seed = HomeCommand::new(store.home_revision().unwrap());
    seed.add(syndic.fixture_contribution(syndic.revision(&store).unwrap(), fixture))
        .unwrap();
    store.execute(seed).unwrap();

    let selected = SelectedPathProof::new(
        Some(turn),
        ThreadRevision::new(1).unwrap(),
        root_turn_chain_digest(turn),
    );
    let edit = StartReplacementEdit::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(1).unwrap(),
        InputGateRevision::new(1).unwrap(),
        turn,
        item,
        selected,
        CurrentTranscriptEntryProof::new(TranscriptGeneration::FIRST, TranscriptPosition::FIRST),
        AdmissionMarkers::new(vec![marker]).unwrap(),
        time(2),
    );
    assert!(matches!(
        start_replacement_edit_command(&store, syndic, state.assets(), edit),
        Err(InputAdmissionBuildError::HistoricalReferenceMismatch),
    ));
    let current = syndic
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().revision().get(), 1);
    assert!(current.draft().replacement_edit_intent().is_none());
    assert_eq!(
        state
            .assets()
            .reference(&store, historical_owner)
            .unwrap()
            .unwrap()
            .asset_id(),
        historical_asset,
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
