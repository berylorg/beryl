#![cfg(feature = "test-faults")]

mod support;

use std::num::NonZeroU64;

use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    AssetId, DraftRevision, InputGateRevision, ProjectionRevision, SyndicDraftMarkerId,
    SyndicItemId, SyndicProjectionId, ThreadRevision,
};
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, fixture_advance_item_projection_digest,
    fixture_advance_transcript_digest, fixture_inline_paragraph_projection,
    fixture_item_projection_digest_seed, fixture_transcript_digest_seed,
};
use syndic_storage::*;

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

fn target_marker() -> ResolvedImageMarker {
    ResolvedImageMarker::new(
        SyndicDraftMarkerId::from_bytes([25; 16]),
        ImageLabelOrdinal::FIRST,
        AssetId::sha256_v1([24; 32], NonZeroU64::new(24).unwrap()),
    )
}

fn target_markers() -> AdmissionMarkers {
    AdmissionMarkers::new(vec![target_marker()]).unwrap()
}

fn target_marker_projection(
    item: SyndicItemId,
    turn: beryl_model::SyndicTurnId,
) -> ProjectionRecord {
    let marker = target_marker();
    let atom_ordinal = ComposerAtomOrdinal::new(2).unwrap();
    let marker_ordinal = InputMarkerOrdinal::FIRST;
    let source_offset = 8_u64;
    let ordinal = ProjectionOrdinal::new(2).unwrap();
    let payload =
        ProjectionPayload::image_marker(atom_ordinal, marker_ordinal, source_offset, marker);
    let mut hash = Sha256::new();
    hash.update(b"beryl/syndic/projection/v1\0");
    hash.update([1]);
    hash.update(item.as_bytes());
    hash.update(source_offset.to_be_bytes());
    hash.update(ordinal.get().to_be_bytes());
    hash.update([3]);
    hash.update(atom_ordinal.get().to_be_bytes());
    hash.update(marker_ordinal.get().to_be_bytes());
    hash.update(source_offset.to_be_bytes());
    hash.update(marker.marker_id().as_bytes());
    hash.update(marker.label().get().to_be_bytes());
    hash.update([marker.asset_id().version() as u8]);
    hash.update(marker.asset_id().digest());
    hash.update(marker.asset_id().length().get().to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    ProjectionRecord::new(
        SyndicProjectionId::from_bytes(id),
        ProjectionRevision::new(1).unwrap(),
        item,
        turn,
        ordinal,
        payload,
    )
}

fn selected_source() -> SelectedPathProof {
    let source = source_turn();
    let digest = child_turn_chain_digest(source, root_turn(), root_turn_chain_digest(root_turn()));
    SelectedPathProof::new(Some(source), ThreadRevision::new(1).unwrap(), digest)
}

fn target_entry() -> CurrentTranscriptEntryProof {
    CurrentTranscriptEntryProof::new(
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(2).unwrap(),
    )
}

fn replacement_seed() -> FixtureBatch {
    let turn = source_turn();
    let item = target_item();
    let revision = ProjectionRevision::new(1).unwrap();
    let marker = target_marker();
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("original").unwrap(),
        ComposerAtom::image_marker(marker.marker_id(), marker.label()),
    ])
    .unwrap();
    let (content, content_records) = composer_content_records(&payload);
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
        revision,
        ItemProjectionGeneration::FIRST,
        source_projection(),
        revision,
    );
    let paragraph_entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(2).unwrap(),
        item,
        revision,
        ItemProjectionGeneration::FIRST,
        paragraph.id(),
        paragraph.revision(),
    );
    let marker_entry = TranscriptViewEntryRecord::new(
        id(30),
        TranscriptGeneration::FIRST,
        TranscriptPosition::new(3).unwrap(),
        item,
        revision,
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
            1,
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
            revision,
            content,
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
                ContentPieceOrdinal::new(3).unwrap(),
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
            revision,
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
        FixtureRecord::InputMarkerResolution(InputMarkerResolutionRecord::new(
            InputMarkerOwner::CanonicalItem(item),
            InputMarkerOrdinal::FIRST,
            marker,
        )),
    ]);
    correlate_source_user_item(&mut records, item, revision, content, 1, timestamp(4));
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
                target_markers(),
                timestamp(5),
            ),
        ),
    );
}

#[test]
fn replacement_start_and_cancel_preserve_parent_payload_and_committed_binding() {
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
    assert_eq!(
        editing.draft().parent(),
        ConversationParent::Turn(source_turn())
    );
    let intent = editing.draft().replacement_edit_intent().unwrap();
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
    assert!(cancelled.draft().replacement_edit_intent().is_none());
    assert_eq!(cancelled.draft().parent(), editing.draft().parent());
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
                target_markers(),
                timestamp(6),
            ),
        ),
    );
    store.validate_registered_domains().unwrap();

    let replacement = storage
        .turn(&store, replacement_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        replacement.record().parent(),
        ConversationParent::Turn(root_turn())
    );
    let original = storage
        .turn(&store, source_turn(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        original.record().parent(),
        ConversationParent::Turn(root_turn())
    );
    let current = storage
        .current_draft(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.thread().committed_tail(), Some(replacement_turn));
    assert_eq!(current.draft().id(), next_draft);
    assert_eq!(
        current.draft().parent(),
        ConversationParent::Turn(replacement_turn)
    );
    let gate = storage
        .input_gate(&store, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.record().state(),
        &InputGateState::PendingTurn(replacement_turn)
    );

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
        target_markers(),
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
    assert!(current.draft().replacement_edit_intent().is_none());
    assert_eq!(
        read_composer_payload(&store, storage, &current),
        ComposerPayload::default()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
