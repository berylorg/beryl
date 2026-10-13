use super::*;
use crate::syndic_transcript::{
    ResidentPresentationRecordKind, ResidentTranscriptCore, TranscriptActivationSource,
};
use beryl_model::{ImageLabelOrdinal, SyndicDraftMarkerId, SyndicItemId, SyndicProjectionId, SyndicTurnId};
use syndic_storage::{ComposerAtomOrdinal, ComposerImageMarker, InputMarkerOrdinal, ProjectionOrdinal};

#[test]
fn image_marker_attachment_preserves_exact_zero_width_identity_without_text_or_resource() {
    let fixture = Fixture::new();
    fixture.seed_projection("fixture", 1);
    let marker = ComposerImageMarker::new(
        SyndicDraftMarkerId::from_bytes([52; 16]),
        ImageLabelOrdinal::new(27).unwrap(),
    );
    let mut batch = FixtureBatch::new();
    let payload = syndic_storage::ComposerPayload::new(vec![
        syndic_storage::ComposerAtom::ImageMarker(marker),
    ]).unwrap();
    let prepared = syndic_storage::PreparedContent::composer(&payload).unwrap();
    batch.put(FixtureRecord::CanonicalItem(syndic_storage::CanonicalItemRecord::local_user_input(
        SyndicItemId::from_bytes([11; 16]),
        SyndicTurnId::from_bytes([12; 16]),
        syndic_storage::TurnItemOrdinal::FIRST,
        ProjectionRevision::new(1).unwrap(),
        prepared.reference(beryl_model::ContentRevision::new(1).unwrap()),
        None,
    ))).unwrap();
    batch.put(FixtureRecord::Projection(syndic_storage::ProjectionRecord::new(
        SyndicProjectionId::from_bytes([13; 16]),
        ProjectionRevision::new(1).unwrap(),
        SyndicItemId::from_bytes([11; 16]),
        SyndicTurnId::from_bytes([12; 16]),
        ProjectionOrdinal::new(1).unwrap(),
        syndic_storage::ProjectionPayload::ImageMarker {
            atom_ordinal: ComposerAtomOrdinal::new(1).unwrap(),
            marker_ordinal: InputMarkerOrdinal::new(1).unwrap(),
            source_offset: 0,
            marker,
        },
    ))).unwrap();
    execute(&fixture.home, fixture.storage.fixture_contribution(
        fixture.storage.revision(&fixture.home).unwrap(), batch,
    ));
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture.reader.prepare_attachment(request.clone(), &cancel).unwrap();
    let seed = fixture.reader.publish_if_current(prepared, &request, &cancel, |seed| seed).unwrap();
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    let snapshot = core.presentation_snapshot();
    assert_eq!(snapshot.records.len(), 1);
    let record = &snapshot.records[0];
    let ResidentPresentationRecordKind::ImageMarker(descriptor) = &record.kind else {
        panic!("A marker must remain a typed atom");
    };
    assert_eq!(descriptor.marker, marker);
    assert_eq!(descriptor.marker.label().to_string(), "AA");
    assert_eq!(descriptor.source_offset, 0);
    assert_eq!(descriptor.atom_ordinal.get(), 1);
    assert_eq!(descriptor.marker_ordinal.get(), 1);
    let crate::syndic_transcript::ResidentRecordSource::Syndic(source) = &record.provenance.source else {
        panic!("Marker source provenance must remain authoritative");
    };
    assert_eq!(source.source_range, Some(0..0));
    assert!(source.resource_id.is_none());
    assert!(source.resource_range.is_none());
    assert_eq!(source.transcript_entry.unwrap().position().get(), 1);
    assert!(core.narrative_demand().is_none());
    let record_id = record.id.clone();
    let presentation = snapshot.presentation_revision;
    for kind in [
        crate::syndic_transcript::DemandFactKind::OverscanRange { range: 0..1 },
        crate::syndic_transcript::DemandFactKind::VisibleRange { range: 0..1 },
    ] {
        core.push_demand_fact(crate::syndic_transcript::DemandFact::new(presentation, kind));
    }
    let outcome = core.apply_resident_context_menu_target(
        crate::syndic_transcript::ResidentContextMenuCommand::new(
            presentation,
            crate::syndic_transcript::ResidentSelectionRecordGeometry::new(record_id, 0.0, 20.0),
        ),
    );
    let crate::syndic_transcript::ResidentContextMenuOutcome::Targeted(target) = outcome else {
        panic!("An exact realized marker must supply the turn menu origin");
    };
    let historical = target.record.historical_target().unwrap();
    assert_eq!(historical.item_id, SyndicItemId::from_bytes([11; 16]));
    assert_eq!(historical.turn_id, SyndicTurnId::from_bytes([12; 16]));
    assert_eq!(historical.thread_id, fixture.request(1).thread_id);
    assert_eq!(historical.transcript.position().get(), 1);
    assert!(fixture.reader.retire());
}

#[test]
fn inline_marker_group_prepares_each_exact_part_and_keeps_its_turn_target() {
    use syndic_storage::{CanonicalItemRecord, ComposerAtom, ComposerPayload, MarkdownBlockId, MarkdownBlockKind,
        ProjectionSourceRange, TranscriptPosition, TranscriptViewEntryRecord, TurnItemOrdinal, ItemProjectionGeneration};
    let fixture = Fixture::new();
    fixture.seed_projection("fixture", 1);
    let marker = ComposerImageMarker::new(SyndicDraftMarkerId::from_bytes([53; 16]), ImageLabelOrdinal::FIRST);
    let second_marker = ComposerImageMarker::new(SyndicDraftMarkerId::from_bytes([54; 16]), ImageLabelOrdinal::new(2).unwrap());
    let item = SyndicItemId::from_bytes([11; 16]);
    let turn = SyndicTurnId::from_bytes([12; 16]);
    let payload = ComposerPayload::new(vec![ComposerAtom::Text("before ".into()), ComposerAtom::ImageMarker(marker), ComposerAtom::ImageMarker(second_marker), ComposerAtom::Text(" after".into())]).unwrap();
    let content = syndic_storage::PreparedContent::composer(&payload).unwrap();
    let revision = ProjectionRevision::new(1).unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(item, turn,
        TurnItemOrdinal::FIRST, revision, content.reference(beryl_model::ContentRevision::new(1).unwrap()), None))).unwrap();
    for (index, payload) in [
        syndic_storage::ProjectionPayload::inline_markdown(MarkdownBlockId::from_bytes([17; 32]), MarkdownBlockKind::Paragraph, 1, ProjectionSourceRange::new(0, 7).unwrap(), "before ").unwrap(),
        syndic_storage::ProjectionPayload::ImageMarker { atom_ordinal: ComposerAtomOrdinal::new(2).unwrap(), marker_ordinal: InputMarkerOrdinal::FIRST, source_offset: 7, marker },
        syndic_storage::ProjectionPayload::ImageMarker { atom_ordinal: ComposerAtomOrdinal::new(3).unwrap(), marker_ordinal: InputMarkerOrdinal::new(2).unwrap(), source_offset: 7, marker: second_marker },
        syndic_storage::ProjectionPayload::inline_markdown(MarkdownBlockId::from_bytes([18; 32]), MarkdownBlockKind::Paragraph, 1, ProjectionSourceRange::new(7, 13).unwrap(), " after").unwrap(),
    ].into_iter().enumerate() {
        let projection = SyndicProjectionId::from_bytes([60 + index as u8; 16]);
        batch.put(FixtureRecord::Projection(syndic_storage::ProjectionRecord::new(projection, revision, item, turn,
            ProjectionOrdinal::new(index as u64 + 1).unwrap(), payload))).unwrap();
        batch.put(FixtureRecord::TranscriptViewEntry(TranscriptViewEntryRecord::new(fixture.request(1).thread_id,
            TranscriptGeneration::FIRST, TranscriptPosition::new(index as u64 + 1).unwrap(), item, revision,
            ItemProjectionGeneration::FIRST, projection, revision))).unwrap();
    }
    let thread = fixture.storage.thread(&fixture.home, fixture.request(1).thread_id, SyndicPointReadLimit::new(65_536).unwrap()).unwrap().unwrap();
    batch.put(FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(fixture.request(1).thread_id, TranscriptGeneration::FIRST,
        revision, 4, thread.committed_tail(), thread.selected_path_digest(), ProjectionLifecycle::Current))).unwrap();
    execute(&fixture.home, fixture.storage.fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch));
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture.reader.prepare_attachment(request.clone(), &cancel).unwrap();
    let source = prepared.source_identity();
    let seed = fixture.reader.publish_if_current(prepared, &request, &cancel, |seed| seed).unwrap();
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    for request_id in 2..4 {
        expose_group(&mut core);
        let demand = core.narrative_demand().unwrap();
        let allowance = core.reserve_narrative_preparation(&demand).unwrap();
        let mut attachment = fixture.request(request_id);
        attachment.purpose = TranscriptAttachmentPurpose::Narrative;
        let request = TranscriptNarrativeRequest { attachment, source: source.clone(), demand: demand.clone(), allowance };
        let prepared = fixture.reader.prepare_narrative(request.clone(), &cancel).unwrap();
        let next = core.prepare_narrative_delivery(&demand, prepared.response()).unwrap();
        fixture.reader.publish_narrative_if_current(prepared, &request, &cancel, |_| ()).unwrap();
        core = next;
    }
    expose_group(&mut core);
    assert!(core.narrative_demand().is_none());
    let snapshot = core.presentation_snapshot();
    assert_eq!(snapshot.records.len(), 1);
    let parts = snapshot.records[0].authored_parts();
    assert_eq!(parts.len(), 4);
    assert!(matches!(parts[0].kind, ResidentPresentationRecordKind::NarrativeReady { .. }));
    assert!(matches!(parts[3].kind, ResidentPresentationRecordKind::NarrativeReady { .. }));
    assert_ne!(parts[1].id, parts[2].id);
    let id = parts[1].id.clone();
    let revision = snapshot.presentation_revision;
    let outcome = core.apply_resident_context_menu_target(crate::syndic_transcript::ResidentContextMenuCommand::new(revision,
        crate::syndic_transcript::ResidentSelectionRecordGeometry::new(id.clone(), 0.0, 36.0)));
    let crate::syndic_transcript::ResidentContextMenuOutcome::Targeted(target) = outcome else { panic!("Exact marker part should remain targetable"); };
    assert_eq!(target.record.record_id, id);
    assert_eq!(target.record.historical_target().unwrap().transcript.position().get(), 2);
    assert_eq!(target.record.historical_target().unwrap().item_id, item);
    assert!(fixture.reader.retire());
}

fn expose_group(core: &mut ResidentTranscriptCore) {
    let revision = core.presentation_snapshot().presentation_revision;
    for kind in [crate::syndic_transcript::DemandFactKind::OverscanRange { range: 0..1 },
        crate::syndic_transcript::DemandFactKind::VisibleRange { range: 0..1 }] {
        core.push_demand_fact(crate::syndic_transcript::DemandFact::new(revision, kind));
    }
}
