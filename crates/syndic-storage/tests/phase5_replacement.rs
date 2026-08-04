#![cfg(feature = "test-faults")]

mod support;

#[path = "phase5_replacement/fixtures.rs"]
mod fixtures;

#[path = "phase5_replacement/catalog_title.rs"]
mod catalog_title;

use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, DraftRevision, InputGateRevision,
    ProjectionRevision, SealedAssetReferenceSetProof, SyndicDraftMarkerId, SyndicItemId,
    SyndicProjectionId, ThreadRevision,
};
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_advance_transcript_digest, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use fixtures::*;
use support::populated::{
    correlate_source_user_item, populated_records, source_item, source_projection, source_turn,
};
use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn root_turn() -> beryl_model::SyndicTurnId {
    beryl_model::SyndicTurnId::from_bytes([29; 16])
}

fn target_item() -> SyndicItemId {
    SyndicItemId::from_bytes([27; 16])
}

fn target_marker() -> ComposerImageMarker {
    ComposerImageMarker::new(
        SyndicDraftMarkerId::from_bytes([25; 16]),
        ImageLabelOrdinal::FIRST,
    )
}

fn target_content_reference() -> ContentReference {
    let marker = target_marker();
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("original").unwrap(),
        ComposerAtom::image_marker(marker.marker_id(), marker.label()),
    ])
    .unwrap();
    PreparedContent::composer(&payload)
        .unwrap()
        .reference(beryl_model::ContentRevision::new(1).unwrap())
}

fn replacement_seed() -> FixtureBatch {
    let turn = source_turn();
    let item = target_item();
    let revision = ProjectionRevision::new(1).unwrap();
    let item_revision = ProjectionRevision::new(4).unwrap();
    let marker = target_marker();
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("original").unwrap(),
        ComposerAtom::image_marker(marker.marker_id(), marker.label()),
    ])
    .unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let asset_reference_set = target_asset_reference_set(content);
    let paragraph = fixture_inline_paragraph_projection(item, turn, "original");
    let marker_projection = target_marker_projection(item, turn);
    let projection_digest = fixture_advance_item_projection_digest(
        fixture_advance_item_projection_digest(
            fixture_item_projection_digest_seed(),
            paragraph.id(),
            paragraph.revision(),
        ),
        marker_projection.id(),
        marker_projection.revision(),
    );
    let source_entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        source_item(),
        item_revision,
        ItemProjectionGeneration::FIRST,
        source_projection(),
        revision,
    );
    let paragraph_entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(2).unwrap(),
        item,
        item_revision,
        ItemProjectionGeneration::FIRST,
        paragraph.id(),
        paragraph.revision(),
    );
    let marker_entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(3).unwrap(),
        item,
        item_revision,
        ItemProjectionGeneration::FIRST,
        marker_projection.id(),
        marker_projection.revision(),
    );
    let transcript_digest = fixture_advance_transcript_digest(
        fixture_advance_transcript_digest(
            fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &source_entry),
            &paragraph_entry,
        ),
        &marker_entry,
    );
    let mut records = populated_records();
    let source_thread = records
        .iter_mut()
        .find(|record| matches!(record, FixtureRecord::Thread(thread) if thread.id() == id(30)))
        .unwrap();
    let FixtureRecord::Thread(thread) = source_thread else {
        unreachable!()
    };
    *thread = ThreadRecord::new(
        thread.id(),
        thread.selected_path(),
        thread.current_draft_id(),
        thread.lineage(),
        ThreadImageLabelFrontiers::new(ImageLabelFrontier::EMPTY, ImageLabelFrontier::from_raw(1))
            .unwrap(),
        thread.context_owner_id(),
    );
    records.retain(|record| {
        !matches!(record, FixtureRecord::TurnState(state) if state.turn_id() == turn)
            && !matches!(record, FixtureRecord::TranscriptViewHead(head) if head.thread_id() == id(30))
            && !matches!(record, FixtureRecord::TranscriptBuild(build)
                if build.thread_id() == id(30)
                    && build.generation() == TranscriptGeneration::FIRST)
            && !matches!(record, FixtureRecord::TranscriptPathTurn(path)
                if path.thread_id() == id(30)
                    && path.generation() == TranscriptGeneration::FIRST
                    && path.depth() == TurnDepth::new(2).unwrap())
    });
    records.extend(content_records);
    records.extend([
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            2,
            timestamp(4),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::new(2).unwrap(),
            revision,
            content,
            Some(asset_reference_set),
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::new(2).unwrap(),
            item,
            revision,
        )),
        FixtureRecord::Projection(paragraph.clone()),
        FixtureRecord::Projection(marker_projection.clone()),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            paragraph.id(),
            paragraph.revision(),
        )),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::new(2).unwrap(),
            marker_projection.id(),
            marker_projection.revision(),
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            item_revision,
            ProjectionTextSource::composer(content),
            8,
            2,
            0,
            projection_digest,
            2,
            0,
            projection_digest,
            MarkdownParserCheckpoint::new(
                8,
                8,
                ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::new(3).unwrap()),
                8,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            revision,
            item_revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            revision,
            3,
            Some(turn),
            selected_source().digest(),
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            revision,
            ThreadRevision::new(1).unwrap(),
            Some(turn),
            selected_source().digest(),
            2,
            3,
            transcript_digest,
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            id(30),
            TranscriptGeneration::FIRST,
            TurnDepth::new(2).unwrap(),
            turn,
            selected_source().digest(),
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            2,
            2,
            timestamp(4),
        )),
        FixtureRecord::TranscriptViewEntry(paragraph_entry),
        FixtureRecord::TranscriptViewEntry(marker_entry),
        FixtureRecord::ImageLabelOriginSpan(
            ImageLabelOriginSpanRecord::new(
                id(30),
                ImageLabelOrdinal::FIRST,
                ImageLabelOrdinal::FIRST,
                ImageLabelOriginOwner::CanonicalItem(item),
                asset_reference_set,
            )
            .unwrap(),
        ),
    ]);
    correlate_source_user_item(
        &mut records,
        item,
        revision,
        content,
        Some(asset_reference_set),
        timestamp(4),
    );
    batch(records)
}

fn start_edit(storage: SyndicStorage, store: &HomeStore) {
    execute(
        store,
        storage.start_replacement_edit(
            storage.revision(store).unwrap(),
            StartReplacementEdit::new(
                id(30),
                ThreadRevision::new(1).unwrap(),
                draft_id(31),
                DraftRevision::new(1).unwrap(),
                InputGateRevision::new(1).unwrap(),
                source_turn(),
                target_item(),
                selected_source(),
                target_entry(),
                Some(target_asset_reference_set(target_content_reference())),
                timestamp(5),
            ),
        ),
    );
}

#[test]
fn replacement_start_and_cancel_preserve_payload_and_committed_binding() {
    let home = TestHome::new("phase5-replacement-cancel");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, replacement_seed());

    let binding_before = storage
        .current_binding(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    start_edit(storage, &store);
    store.validate_registered_domains().unwrap();

    let editing = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(editing.draft().revision().get(), 2);
    let DraftSubmissionIntent::Replacement(intent) = editing.draft().submission_intent() else {
        panic!("replacement intent was not published");
    };
    assert_eq!(intent.target_turn_id(), source_turn());
    assert_eq!(intent.selected_path(), selected_source());
    assert_eq!(intent.transcript_entry(), target_entry());
    assert_eq!(
        read_composer_payload(&store, storage, &editing).atoms()[0].text_value(),
        Some("original")
    );
    let binding_during = storage
        .current_binding(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(binding_during, binding_before);

    execute(
        &store,
        storage.cancel_replacement_edit(
            storage.revision(&store).unwrap(),
            CancelReplacementEdit::new(
                id(30),
                ThreadRevision::new(1).unwrap(),
                draft_id(31),
                DraftRevision::new(2).unwrap(),
                InputGateRevision::new(1).unwrap(),
                timestamp(6),
            ),
        ),
    );
    store.validate_registered_domains().unwrap();
    let cancelled = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(cancelled.draft().revision().get(), 3);
    assert_eq!(
        cancelled.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(cancelled.draft().content(), editing.draft().content());

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn accepted_replacement_creates_a_sibling_and_keeps_the_old_path_immutable() {
    let home = TestHome::new("phase5-replacement-submit");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, replacement_seed());
    start_edit(storage, &store);

    let replacement_turn = draft_id(31).submitted_turn_id();
    let next_draft = draft_id(70);
    let expected_content = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    execute(
        &store,
        storage.submit_idle_draft(
            storage.revision(&store).unwrap(),
            IdleSubmission::new(
                id(30),
                ThreadRevision::new(1).unwrap(),
                draft_id(31),
                DraftRevision::new(2).unwrap(),
                expected_content,
                InputGateRevision::new(1).unwrap(),
                next_draft,
                SyndicItemId::from_bytes([71; 16]),
                Some(target_asset_reference_set(target_content_reference())),
                timestamp(6),
            ),
        ),
    );
    store.validate_registered_domains().unwrap();

    let replacement = storage
        .turn(&store, replacement_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(replacement.parent(), ConversationParent::Turn(root_turn()));
    let original = storage
        .turn(&store, source_turn(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(original.parent(), ConversationParent::Turn(root_turn()));
    let current = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.thread().committed_tail(), Some(replacement_turn));
    assert_eq!(current.draft().id(), next_draft);
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    let gate = storage
        .input_gate(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(replacement_turn));

    store.close().unwrap();
}

#[test]
fn stale_selected_path_rejects_replacement_edit_without_changing_the_draft() {
    let home = TestHome::new("phase5-replacement-stale-path");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, replacement_seed());
    let stale = SelectedPathProof::new(
        Some(source_turn()),
        ThreadRevision::new(2).unwrap(),
        selected_source().digest(),
    );
    let edit = StartReplacementEdit::new(
        id(30),
        ThreadRevision::new(1).unwrap(),
        draft_id(31),
        DraftRevision::new(1).unwrap(),
        InputGateRevision::new(1).unwrap(),
        source_turn(),
        target_item(),
        stale,
        target_entry(),
        Some(target_asset_reference_set(target_content_reference())),
        timestamp(5),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.start_replacement_edit(storage.revision(&store).unwrap(), edit))
        .unwrap();
    assert!(store.execute(command).is_err());

    let current = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().revision().get(), 1);
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(
        read_composer_payload(&store, storage, &current),
        ComposerPayload::default()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
