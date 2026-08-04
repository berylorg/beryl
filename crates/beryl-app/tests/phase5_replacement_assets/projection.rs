use beryl_model::{
    ProjectionRevision, SyndicDraftMarkerId, SyndicItemId, SyndicProjectionId, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    FixtureRecord, fixture_advance_item_projection_digest, fixture_advance_transcript_digest,
    fixture_inline_paragraph_projection, fixture_item_projection_digest_seed,
    fixture_transcript_digest_seed,
};
use syndic_storage::*;

use super::time;

pub(super) struct ReplacementProjectionFixture {
    pub(super) item_revision: ProjectionRevision,
    pub(super) records: Vec<FixtureRecord>,
}

pub(super) fn replacement_projection_fixture(
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    content: ContentReference,
    marker_id: Option<SyndicDraftMarkerId>,
) -> ReplacementProjectionFixture {
    let paragraph = fixture_inline_paragraph_projection(item, turn, "historical");
    let mut projections = vec![paragraph];
    if let Some(marker_id) = marker_id {
        projections.push(image_marker_projection(item, turn, content, marker_id));
    }
    let item_revision = projections[0].revision();
    let projection_count = projections.len() as u64;
    let mut item_digest = fixture_item_projection_digest_seed();
    for projection in &projections {
        item_digest = fixture_advance_item_projection_digest(
            item_digest,
            projection.id(),
            projection.revision(),
        );
    }

    let mut records = Vec::new();
    for projection in &projections {
        records.push(FixtureRecord::Projection(projection.clone()));
        records.push(FixtureRecord::StableItemProjection(
            StableItemProjectionIndexRecord::new(
                item,
                projection.ordinal(),
                projection.id(),
                projection.revision(),
            ),
        ));
    }
    let source_bytes = content.summary().logical_utf8_bytes();
    let next_piece = if marker_id.is_some() { 3 } else { 2 };
    records.extend([
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            item_revision,
            ProjectionTextSource::composer(content),
            source_bytes,
            projection_count,
            0,
            item_digest,
            projection_count,
            0,
            item_digest,
            MarkdownParserCheckpoint::new(
                source_bytes,
                source_bytes,
                ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::new(next_piece).unwrap()),
                source_bytes,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            item_revision,
            item_revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
    ]);

    let transcript_entries = transcript_entries(thread, item, item_revision, &projections);
    let mut transcript_digest = fixture_transcript_digest_seed();
    for entry in &transcript_entries {
        transcript_digest = fixture_advance_transcript_digest(transcript_digest, entry);
    }
    let path_digest = root_turn_chain_digest(turn);
    records.extend([
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            item_revision,
            projection_count,
            Some(turn),
            path_digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            item_revision,
            ThreadRevision::new(1).unwrap(),
            Some(turn),
            path_digest,
            1,
            projection_count,
            transcript_digest,
            false,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            turn,
            path_digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Incomplete,
            1,
            1,
            1,
            time(1),
        )),
    ]);
    records.extend(
        transcript_entries
            .into_iter()
            .map(FixtureRecord::TranscriptViewEntry),
    );
    ReplacementProjectionFixture {
        item_revision,
        records,
    }
}

fn transcript_entries(
    thread: SyndicThreadId,
    item: SyndicItemId,
    item_revision: ProjectionRevision,
    projections: &[ProjectionRecord],
) -> Vec<TranscriptViewEntryRecord> {
    projections
        .iter()
        .enumerate()
        .map(|(index, projection)| {
            TranscriptViewEntryRecord::new(
                thread,
                TranscriptGeneration::FIRST,
                TranscriptPosition::new((index + 1) as u64).unwrap(),
                item,
                item_revision,
                ItemProjectionGeneration::FIRST,
                projection.id(),
                projection.revision(),
            )
        })
        .collect()
}

fn image_marker_projection(
    item: SyndicItemId,
    turn: SyndicTurnId,
    content: ContentReference,
    marker_id: SyndicDraftMarkerId,
) -> ProjectionRecord {
    let atom_ordinal = ComposerAtomOrdinal::new(2).unwrap();
    let marker_ordinal = InputMarkerOrdinal::FIRST;
    let source_offset = content.summary().logical_utf8_bytes();
    let ordinal = ProjectionOrdinal::new(2).unwrap();
    let marker = ComposerImageMarker::new(marker_id, ImageLabelOrdinal::FIRST);
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
