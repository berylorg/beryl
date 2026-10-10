use super::*;
use crate::syndic_transcript::{
    ProviderRevision, ResidentPresentationRecordId, ResourceId, ResourceKind,
    ResourceTranscriptDemand, TranscriptProviderResponseKind, TranscriptViewId,
};
use beryl_model::{ContentRevision, SyndicItemId, SyndicProjectionId, SyndicResourceId};
use syndic_storage::{
    ComposerAtom, ComposerPayload, MarkdownBlockId, MarkdownBlockKind, PreparedContent,
    ProjectionOrdinal, ProjectionRecord, ProjectionSourceRange, ProjectionTextSource,
    ResourceBacking, ResourceMetadataRecord, ResourceOrdinal, ResourceStructure,
};

impl Fixture {
    fn seed_resource(
        &self,
        text: &str,
        start: u64,
        end: u64,
        preview: Option<std::ops::Range<u64>>,
        revision: u64,
    ) -> ResourceMetadataRecord {
        self.seed_projection("resource owner", 1);
        let content = PreparedContent::composer(
            &ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap(),
        )
        .unwrap();
        let reference = content.reference(ContentRevision::new(1).unwrap());
        let item = SyndicItemId::from_bytes([11; 16]);
        let projection = SyndicProjectionId::from_bytes([13; 16]);
        let resource = SyndicResourceId::from_bytes([17; 16]);
        let metadata = ResourceMetadataRecord::new(
            resource,
            ProjectionRevision::new(revision).unwrap(),
            projection,
            item,
            ResourceOrdinal::new(1).unwrap(),
            syndic_storage::ResourceKind::Code,
            "text/plain",
            ResourceBacking::TextRange {
                source: ProjectionTextSource::composer(reference),
                range: ProjectionSourceRange::new(start, end).unwrap(),
            },
            [22; 32],
            preview.map(|range| ProjectionSourceRange::new(range.start, range.end).unwrap()),
            ResourceStructure::code(None, 1).unwrap(),
        )
        .unwrap();
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::CanonicalItem(
                syndic_storage::CanonicalItemRecord::local_user_input(
                    item,
                    beryl_model::SyndicTurnId::from_bytes([12; 16]),
                    syndic_storage::TurnItemOrdinal::new(1).unwrap(),
                    ProjectionRevision::new(1).unwrap(),
                    reference,
                    None,
                ),
            ))
            .unwrap();
        batch
            .put(FixtureRecord::ContentManifest(
                content.sealed_manifest(ContentRevision::new(1).unwrap()),
            ))
            .unwrap();
        for record in content.chunks() {
            batch
                .put(FixtureRecord::ContentChunk(record.clone()))
                .unwrap();
        }
        for record in content.text_spans() {
            batch
                .put(FixtureRecord::ContentTextSpan(record.clone()))
                .unwrap();
        }
        for record in content.pieces() {
            batch
                .put(FixtureRecord::ContentPiece(record.clone()))
                .unwrap();
        }
        batch
            .put(FixtureRecord::Projection(ProjectionRecord::new(
                projection,
                ProjectionRevision::new(1).unwrap(),
                item,
                beryl_model::SyndicTurnId::from_bytes([12; 16]),
                ProjectionOrdinal::new(1).unwrap(),
                syndic_storage::ProjectionPayload::resource_reference(
                    MarkdownBlockId::from_bytes([16; 32]),
                    MarkdownBlockKind::FencedCode,
                    ProjectionSourceRange::new(start, end).unwrap(),
                    resource,
                    "resource preview",
                )
                .unwrap(),
            )))
            .unwrap();
        batch
            .put(FixtureRecord::Resource(metadata.clone()))
            .unwrap();
        execute(
            &self.home,
            self.storage
                .fixture_contribution(self.storage.revision(&self.home).unwrap(), batch),
        );
        metadata
    }

    fn resource_request(&self, request_id: u64) -> TranscriptResourceRequest {
        let cancelled = AtomicBool::new(false);
        let attached = self
            .reader
            .prepare_attachment(self.request(1), &cancelled)
            .unwrap();
        let source = attached.source_identity();
        let provenance = match &attached.activation().projection_records_response {
            Some(TranscriptProviderResponseKind::ProjectionRecords(records)) => {
                records.records[0].provenance.clone()
            }
            _ => panic!("resource projection"),
        };
        drop(attached);
        let mut attachment = self.request(request_id);
        attachment.purpose = TranscriptAttachmentPurpose::Resource;
        let view_id = TranscriptViewId(attachment.thread_id.to_string());
        TranscriptResourceRequest {
            attachment,
            source,
            demand: ResourceTranscriptDemand {
                activation_revision: 1,
                presentation_revision: 1,
                view_id: view_id.clone(),
                revision: ProviderRevision(1),
                record_id: ResidentPresentationRecordId("resource owner".into()),
                retained_records: vec![ResidentPresentationRecordId("resource owner".into())],
                pins: Default::default(),
                provenance,
                resource_id: ResourceId(SyndicResourceId::from_bytes([17; 16]).to_string()),
                kind: ResourceKind::Code,
                metadata: None,
                range: None,
            },
        }
    }
}

fn read_metadata(
    fixture: &Fixture,
    request: &TranscriptResourceRequest,
) -> crate::syndic_transcript::ResourceMetadata {
    let cancel = AtomicBool::new(false);
    let prepared = fixture
        .reader
        .prepare_resource(request.clone(), &cancel)
        .unwrap();
    fixture
        .reader
        .publish_resource_if_current(prepared, request, &cancel, |response| {
            let TranscriptProviderResponseKind::ResourceMetadata(metadata) = response else {
                panic!("text metadata required");
            };
            metadata
        })
        .unwrap()
}

#[test]
fn resource_delivery_is_metadata_first_and_reads_exact_relative_offsets_and_continuation() {
    let fixture = Fixture::new();
    let text = format!("{}{}", "p".repeat(1000), "a💎é".repeat(20000));
    fixture.seed_resource(&text, 1000, text.len() as u64, Some(200..80000), 7);
    let mut request = fixture.resource_request(2);
    let metadata = read_metadata(&fixture, &request);
    assert_eq!(metadata.revision, ProviderRevision(7));
    assert_eq!(request.demand.revision, ProviderRevision(1));
    request.demand.metadata = Some(metadata);
    request.demand.range = Some(200..80000);
    let cancel = AtomicBool::new(false);
    let mut observed = Vec::new();
    for (id, start, end, complete) in [(3, 200, 65736, false), (4, 65736, 80000, true)] {
        request.attachment.request_id = id;
        request.demand.range = Some(start..80000);
        let prepared = fixture
            .reader
            .prepare_resource(request.clone(), &cancel)
            .unwrap();
        let response = fixture
            .reader
            .publish_resource_if_current(prepared, &request, &cancel, |response| response)
            .unwrap();
        let TranscriptProviderResponseKind::ResourceRange(range) = response else {
            panic!("text bytes required");
        };
        assert_eq!(range.range, start..end);
        assert_eq!(range.complete, complete);
        if !complete {
            assert!(
                std::str::from_utf8(&range.bytes).is_err(),
                "delivery preserves split UTF-8 bytes"
            );
        }
        assert_eq!(
            range.bytes,
            text.as_bytes()[1000 + start as usize..1000 + end as usize]
        );
        observed.extend(range.bytes);
    }
    assert_eq!(observed, text.as_bytes()[1200..81000]);
    assert!(fixture.reader.retire());
}

#[test]
fn resource_prepared_output_rejects_request_supersession_cancel_and_backing_replacement() {
    for dimension in 0..8 {
        let mut fixture = Fixture::new();
        let metadata = fixture.seed_resource("offset heavy resource", 7, 21, Some(0..3), 9);
        let request = fixture.resource_request(2);
        let cancel = AtomicBool::new(false);
        let prepared = fixture
            .reader
            .prepare_resource(request.clone(), &cancel)
            .unwrap();
        let mut expected = request.clone();
        match dimension {
            0 => expected.attachment.request_id += 1,
            1 => expected.demand.presentation_revision += 1,
            2 => cancel.store(true, Ordering::Release),
            3 => fixture.host = Arc::new(()),
            4 => fixture.graph = Arc::new(()),
            5 => {
                fixture.reader.retire();
            }
            6 => {
                fixture.seed_resource("another text backing!", 7, 21, Some(0..3), 9);
            }
            _ => fixture.seed_head(2),
        }
        let called = AtomicBool::new(false);
        assert!(
            fixture
                .reader
                .publish_resource_if_current(prepared, &expected, &cancel, |_| {
                    called.store(true, Ordering::Release);
                })
                .is_err()
        );
        assert!(!called.load(Ordering::Acquire));
        assert!(fixture.reader.retire());
        assert_eq!(metadata.revision().get(), 9);
    }
}

#[test]
fn resource_preview_extents_remain_exact_and_storage_refuses_empty_backing() {
    for (text, start, end, preview) in [
        ("prefixbody", 6, 10, Some(1..2)),
        ("prefixbody", 6, 10, None),
    ] {
        let fixture = Fixture::new();
        fixture.seed_resource(text, start, end, preview.clone(), 5);
        let mut request = fixture.resource_request(2);
        let metadata = read_metadata(&fixture, &request);
        assert_eq!(metadata.preview_range, preview);
        assert_eq!(metadata.byte_len, end - start);
        request.demand.metadata = Some(metadata);
        request.demand.range = Some(0..end - start + 1);
        let cancel = AtomicBool::new(false);
        let prepared = fixture
            .reader
            .prepare_resource(request.clone(), &cancel)
            .unwrap();
        assert!(
            matches!(prepared.response(), TranscriptProviderResponseKind::Rejected(rejection)
            if rejection.reason == crate::syndic_transcript::TranscriptProviderRejectionReason::RangeOutOfBounds)
        );
        drop(prepared);
        assert!(fixture.reader.retire());
    }
    assert!(ProjectionSourceRange::new(0, 0).is_err());
    assert!(ProjectionSourceRange::new(6, 6).is_err());
}

#[test]
fn resource_metadata_missing_is_terminal_and_unqualified_generated_media_fails_identity() {
    use crate::syndic_transcript::TranscriptProviderRejectionReason;
    for unsupported in [false, true] {
        let fixture = Fixture::new();
        let metadata = fixture.seed_resource("prefixbody", 6, 10, None, 5);
        let mut request = fixture.resource_request(2);
        let mut batch = FixtureBatch::new();
        if unsupported {
            batch
                .put(FixtureRecord::Resource(
                    ResourceMetadataRecord::pending_generated_media(
                        metadata.id(),
                        metadata.revision(),
                        metadata.item_id(),
                    ),
                ))
                .unwrap();
            request.demand.kind = ResourceKind::Image;
        } else {
            batch
                .delete(FixtureDelete::Resource(metadata.id()))
                .unwrap();
        }
        execute(
            &fixture.home,
            fixture
                .storage
                .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
        );
        let cancel = AtomicBool::new(false);
        let prepared = fixture.reader.prepare_resource(request.clone(), &cancel);
        if unsupported {
            assert!(matches!(prepared, Err(TranscriptAttachmentError::Identity)));
            assert!(fixture.reader.retire());
            continue;
        }
        let prepared = prepared.unwrap();
        let response = fixture
            .reader
            .publish_resource_if_current(prepared, &request, &cancel, |response| response)
            .unwrap();
        assert!(
            matches!(response, TranscriptProviderResponseKind::Rejected(rejection)
            if rejection.reason == TranscriptProviderRejectionReason::MissingResource)
        );
        assert!(fixture.reader.retire());
    }
}

#[test]
fn owned_nontext_resource_descriptor_is_locally_unsupported_before_text_loading() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("prefixbody", 6, 10, None, 5);
    let replacement = ResourceMetadataRecord::new(
        metadata.id(),
        metadata.revision(),
        metadata.projection_id().unwrap(),
        metadata.item_id(),
        metadata.ordinal().unwrap(),
        syndic_storage::ResourceKind::Image,
        "image/png",
        metadata.backing(),
        *metadata.digest().unwrap(),
        metadata.preview_range(),
        ResourceStructure::Opaque,
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Resource(replacement)).unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
    let mut request = fixture.resource_request(2);
    request.demand.kind = ResourceKind::Image;
    let cancel = AtomicBool::new(false);
    let prepared = fixture
        .reader
        .prepare_resource(request.clone(), &cancel)
        .unwrap();
    let response = fixture
        .reader
        .publish_resource_if_current(prepared, &request, &cancel, |response| response)
        .unwrap();
    assert!(
        matches!(response, TranscriptProviderResponseKind::Rejected(rejection)
        if rejection.reason == crate::syndic_transcript::TranscriptProviderRejectionReason::UnsupportedResourceKind)
    );
    assert!(fixture.reader.retire());
}
