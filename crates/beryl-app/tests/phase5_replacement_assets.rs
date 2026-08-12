#![cfg(feature = "test-faults")]

use beryl_app::input_admission::{InputAdmissionBuildError, start_replacement_edit_command};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    BindingRevision, ContentRevision, DraftRevision, ExecutionBinding, InputGateRevision,
    PathFlavor, ProjectionRevision, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SealedAssetReferenceSetProof, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};
use beryl_state::{AssetOwner, BerylState};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::*;

#[path = "phase5_replacement_assets/assets.rs"]
mod assets;
#[path = "phase5_replacement_assets/marker_free.rs"]
mod marker_free;
#[path = "phase5_replacement_assets/projection.rs"]
mod projection;

use assets::{create_historical_asset, historical_content};
use projection::replacement_projection_fixture;

fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([5; 16]),
        RootId::from_bytes([6; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl-phase5-replacement",
        )
        .unwrap(),
    )
}

fn content_records(content: &PreparedContent) -> (ContentReference, Vec<FixtureRecord>) {
    let revision = ContentRevision::new(1).unwrap();
    let mut records = vec![FixtureRecord::ContentManifest(
        content.sealed_manifest(revision),
    )];
    let mut encoded_start = 0;
    for chunk in content.chunks() {
        records.push(FixtureRecord::ContentChunk(chunk.clone()));
        let span = ContentByteSpanRecord::for_chunk(chunk, encoded_start).unwrap();
        encoded_start = span.end();
        records.push(FixtureRecord::ContentByteSpan(span));
    }
    records.extend(
        content
            .text_spans()
            .iter()
            .copied()
            .map(FixtureRecord::ContentTextSpan),
    );
    records.extend(
        content
            .pieces()
            .iter()
            .copied()
            .map(FixtureRecord::ContentPiece),
    );
    (content.reference(revision), records)
}

#[allow(clippy::too_many_arguments)]
fn replacement_fixture(
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    marker_id: Option<SyndicDraftMarkerId>,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
) -> FixtureBatch {
    assert_eq!(marker_id.is_some(), asset_reference_set.is_some());
    let thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let digest = root_turn_chain_digest(turn);
    let selected = SelectedPathProof::new(Some(turn), thread_revision, digest);
    let prepared = match marker_id {
        Some(marker_id) => historical_content(marker_id),
        None => PreparedContent::composer(
            &ComposerPayload::new(vec![ComposerAtom::text("historical").unwrap()]).unwrap(),
        )
        .unwrap(),
    };
    let (target_content, mut records) = content_records(&prepared);
    let projection_fixture =
        replacement_projection_fixture(thread, turn, item, target_content, marker_id);
    let projection_revision = projection_fixture.item_revision;
    let empty = PreparedContent::composer(&ComposerPayload::default()).unwrap();
    let (empty_content, empty_records) = content_records(&empty);
    records.extend(empty_records);
    let frontiers = if asset_reference_set.is_some() {
        ThreadImageLabelFrontiers::new(ImageLabelFrontier::EMPTY, ImageLabelFrontier::from_raw(1))
            .unwrap()
    } else {
        ThreadImageLabelFrontiers::empty()
    };
    let thread_record = ThreadRecord::new(
        thread,
        selected,
        draft,
        ThreadLineageProof::new(
            None,
            None,
            ThreadLineageDepth::FIRST,
            root_thread_lineage_digest(thread),
        ),
        frontiers,
        None,
    );
    let history_summary = HistorySummaryRecord::new(
        thread,
        ProjectionRevision::new(1).unwrap(),
        thread_revision,
        Some(turn),
        digest,
        false,
        time(1),
    );
    let thread_execution = ThreadExecutionRecord::new(thread, execution_binding());
    let thread_attributes = ThreadAttributesRecord::ordinary(thread);
    let thread_catalog = ThreadCatalogSummaryRecord::initial(
        &thread_record,
        &thread_execution,
        &thread_attributes,
        &history_summary,
    );
    records.extend([
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ThreadExecution(thread_execution),
        FixtureRecord::ThreadAttributes(thread_attributes),
        FixtureRecord::ThreadUsage(ThreadUsageRecord::empty(thread)),
        FixtureRecord::ThreadCatalogSummary(thread_catalog),
        FixtureRecord::Draft(DraftRecord::new(
            draft,
            thread,
            draft_revision,
            DraftSubmissionIntent::Ordinary,
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
        FixtureRecord::ActivityQueryHead(ActivityQueryHeadRecord::empty(thread)),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            time(1),
        )),
        FixtureRecord::TurnState(
            TurnStateRecord::with_capture_frontiers(
                turn,
                TurnStateRevision::FIRST,
                TurnLifecycle::Incomplete,
                1,
                1,
                1,
                1,
                0,
                Some(TurnEndStatus::incomplete(
                    TurnIncompleteReason::ItemAuditFailed,
                )),
                time(1),
            )
            .unwrap(),
        ),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
                    TurnIncompleteReason::ItemAuditFailed,
                )),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            projection_revision,
            target_content,
            asset_reference_set,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            projection_revision,
        )),
    ]);
    if let (Some(_), Some(asset_reference_set)) = (marker_id, asset_reference_set) {
        records.push(FixtureRecord::ImageLabelOriginSpan(
            ImageLabelOriginSpanRecord::new(
                thread,
                ImageLabelOrdinal::FIRST,
                ImageLabelOrdinal::FIRST,
                ImageLabelOriginOwner::CanonicalItem(item),
                asset_reference_set,
            )
            .unwrap(),
        ));
    }
    records.extend(projection_fixture.records);
    records.extend([
        FixtureRecord::HistorySummary(history_summary),
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
    let marker_id = SyndicDraftMarkerId::from_bytes([6; 16]);
    let historical_owner = AssetOwner::SubmittedTurnItem(item);
    let (asset, proof) = create_historical_asset(
        &mut store,
        state.clone(),
        Some(historical_owner),
        marker_id,
        7,
        b"replacement image",
    );
    let fixture = replacement_fixture(thread, draft, turn, item, Some(marker_id), Some(proof));
    let mut seed = HomeCommand::new(store.home_revision().unwrap());
    seed.add(syndic.fixture_contribution(syndic.revision(&store).unwrap(), fixture))
        .unwrap();
    match store.execute(seed) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("expected committed seed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("unexpected later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("indeterminate seed: {outcome:?}"),
    }

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
        Some(proof),
        time(2),
    );
    let command = start_replacement_edit_command(&store, syndic, state.assets(), edit).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("expected committed replacement: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("unexpected later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("indeterminate replacement: {outcome:?}")
        }
    }
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let draft_owner = AssetOwner::CurrentDraft(draft);
    assert_eq!(
        state
            .assets()
            .owner_head(&store, historical_owner)
            .unwrap()
            .unwrap()
            .set(),
        proof
    );
    assert_eq!(
        state
            .assets()
            .owner_head(&store, draft_owner)
            .unwrap()
            .unwrap()
            .set(),
        proof
    );
    assert_eq!(
        state
            .assets()
            .marker_reference(&store, proof, marker_id)
            .unwrap()
            .unwrap()
            .asset_id(),
        asset
    );
    let current = syndic
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let DraftSubmissionIntent::Replacement(intent) = current.draft().submission_intent() else {
        panic!("replacement submission intent was not published");
    };
    assert_eq!(intent.target_turn_id(), turn);
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
    let marker_id = SyndicDraftMarkerId::from_bytes([25; 16]);
    let historical_owner = AssetOwner::SubmittedTurnItem(item);
    let (historical_asset, historical_proof) = create_historical_asset(
        &mut store,
        state.clone(),
        Some(historical_owner),
        marker_id,
        26,
        b"historical image",
    );
    let (_conflicting_asset, conflicting_proof) = create_historical_asset(
        &mut store,
        state.clone(),
        None,
        marker_id,
        27,
        b"conflicting image",
    );
    let fixture = replacement_fixture(
        thread,
        draft,
        turn,
        item,
        Some(marker_id),
        Some(conflicting_proof),
    );
    let mut seed = HomeCommand::new(store.home_revision().unwrap());
    seed.add(syndic.fixture_contribution(syndic.revision(&store).unwrap(), fixture))
        .unwrap();
    match store.execute(seed) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("expected committed seed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("unexpected later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("indeterminate seed: {outcome:?}"),
    }

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
        Some(conflicting_proof),
        time(2),
    );
    assert!(matches!(
        start_replacement_edit_command(&store, syndic, state.assets(), edit),
        Err(InputAdmissionBuildError::OwnerHeadMismatch),
    ));
    let current = syndic
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().revision().get(), 1);
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(
        state
            .assets()
            .owner_head(&store, historical_owner)
            .unwrap()
            .unwrap()
            .set(),
        historical_proof,
    );
    assert_eq!(
        state
            .assets()
            .marker_reference(&store, historical_proof, marker_id)
            .unwrap()
            .unwrap()
            .asset_id(),
        historical_asset
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
