use std::ops::Range;

#[path = "support/syndic_transcript_contract.rs"]
mod syndic_transcript_contract;

use syndic_transcript_contract::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_contract::*;

const INITIAL_REVISION: ProviderRevision = ProviderRevision(41);
const UPDATED_REVISION: ProviderRevision = ProviderRevision(42);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("thread-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn resource_id(name: &str) -> ResourceId {
    ResourceId(format!("resource-{name}"))
}

fn provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
    resource_id: Option<ResourceId>,
    source_range: Option<Range<u64>>,
    resource_range: Option<Range<u64>>,
    copy_source_range: Option<Range<u64>>,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(SyndicTurnId(format!("turn-{position}"))),
        item_id: Some(SyndicItemId(format!("item-{position}"))),
        projection_id: Some(projection_id.clone()),
        resource_id,
        source_range,
        resource_range,
        copy_source_range,
    }
}

fn view_record(
    view_id: &TranscriptViewId,
    position: u64,
    id: &str,
    projection_id: ProjectionRecordId,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(id.to_string()),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantFinalAnswer,
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            None,
            Some(position..position + 8),
            None,
            Some(position..position + 8),
        ),
    }
}

fn text_projection(
    view_id: &TranscriptViewId,
    projection_id: ProjectionRecordId,
    position: u64,
    text: &str,
) -> ProjectionRecord {
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text {
            text: text.to_string(),
        },
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            None,
            Some(position..position + text.len() as u64),
            None,
            Some(position..position + text.len() as u64),
        ),
    }
}

fn metadata(resource_id: ResourceId, digest: &str, preview_range: Range<u64>) -> ResourceMetadata {
    ResourceMetadata {
        resource_id,
        revision: ProviderRevision(0),
        kind: ResourceKind::Attachment,
        media_type: Some("application/octet-stream".to_string()),
        byte_len: 0,
        digest: Some(digest.to_string()),
        line_count: None,
        row_count: None,
        column_count: None,
        preview_range: Some(preview_range),
    }
}

fn seeded_provider() -> (
    InMemorySyndicTranscriptProvider,
    TranscriptViewId,
    ProjectionRecordId,
    ResourceId,
) {
    let view_id = view_id();
    let projection_id = projection_id("primary");
    let resource_id = resource_id("primary");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(INITIAL_REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(
                &view_id,
                10,
                "initial-record",
                projection_id.clone(),
            )],
        )
        .insert_projection_record(text_projection(
            &view_id,
            projection_id.clone(),
            10,
            "initial projection text",
        ))
        .insert_resource(
            metadata(resource_id.clone(), "sha256:initial", 0..7),
            b"initial-bytes".to_vec(),
        );
    (provider, view_id, projection_id, resource_id)
}

fn read_page(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    view_id: &TranscriptViewId,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadViewPage(TranscriptViewPageRequest {
                view_id: view_id.clone(),
                anchor: TranscriptPageAnchor::Start,
                direction: TranscriptPageDirection::Forward,
                limit: 8,
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn read_projection_records(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    view_id: &TranscriptViewId,
    projection_id: &ProjectionRecordId,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadProjectionRecords(ProjectionRecordsRequest {
                view_id: view_id.clone(),
                projection_ids: vec![projection_id.clone()],
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn read_metadata(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    resource_id: &ResourceId,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadResourceMetadata(ResourceMetadataRequest {
                resource_id: resource_id.clone(),
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn read_range(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    resource_id: &ResourceId,
    range: Range<u64>,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadResourceRange(ResourceRangeRequest {
                resource_id: resource_id.clone(),
                range,
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn expect_page(kind: TranscriptProviderResponseKind) -> TranscriptViewPage {
    match kind {
        TranscriptProviderResponseKind::ViewPage(page) => page,
        other => panic!("expected view page, got {other:?}"),
    }
}

fn expect_projection_set(kind: TranscriptProviderResponseKind) -> ProjectionRecordSet {
    match kind {
        TranscriptProviderResponseKind::ProjectionRecords(set) => set,
        other => panic!("expected projection records, got {other:?}"),
    }
}

fn expect_metadata(kind: TranscriptProviderResponseKind) -> ResourceMetadata {
    match kind {
        TranscriptProviderResponseKind::ResourceMetadata(metadata) => metadata,
        other => panic!("expected resource metadata, got {other:?}"),
    }
}

fn expect_range(kind: TranscriptProviderResponseKind) -> ResourceRangeResponse {
    match kind {
        TranscriptProviderResponseKind::ResourceRange(range) => range,
        other => panic!("expected resource range, got {other:?}"),
    }
}

fn expect_stale(kind: TranscriptProviderResponseKind) -> TranscriptProviderStale {
    match kind {
        TranscriptProviderResponseKind::Stale(stale) => stale,
        other => panic!("expected stale provider response, got {other:?}"),
    }
}

fn assert_stale(
    stale: TranscriptProviderStale,
    target: TranscriptProviderTarget,
    observed_revision: ProviderRevision,
    current_revision: ProviderRevision,
) {
    assert_eq!(stale.target, target);
    assert_eq!(stale.observed_revision, Some(observed_revision));
    assert_eq!(stale.current_revision, current_revision);
}

fn projection_text(set: &ProjectionRecordSet) -> &str {
    match &set.records[0].payload {
        ProjectionPayload::Text { text } => text.as_str(),
        other => panic!("expected text projection payload, got {other:?}"),
    }
}

#[test]
fn stale_results_use_provider_response_variant_for_each_request_kind() {
    let (mut provider, view_id, projection_id, resource_id) = seeded_provider();
    let stale_revision = ProviderRevision(40);

    let stale_page = expect_stale(read_page(&mut provider, 1, &view_id, Some(stale_revision)));
    assert_stale(
        stale_page,
        TranscriptProviderTarget::View(view_id.clone()),
        stale_revision,
        INITIAL_REVISION,
    );

    let stale_projection = expect_stale(read_projection_records(
        &mut provider,
        2,
        &view_id,
        &projection_id,
        Some(stale_revision),
    ));
    assert_stale(
        stale_projection,
        TranscriptProviderTarget::View(view_id),
        stale_revision,
        INITIAL_REVISION,
    );

    let stale_metadata = expect_stale(read_metadata(
        &mut provider,
        3,
        &resource_id,
        Some(stale_revision),
    ));
    assert_stale(
        stale_metadata,
        TranscriptProviderTarget::Resource(resource_id.clone()),
        stale_revision,
        INITIAL_REVISION,
    );

    let range = 2..8;
    let stale_range = expect_stale(read_range(
        &mut provider,
        4,
        &resource_id,
        range.clone(),
        Some(stale_revision),
    ));
    assert_stale(
        stale_range,
        TranscriptProviderTarget::ResourceRange { resource_id, range },
        stale_revision,
        INITIAL_REVISION,
    );
}

#[test]
fn newer_revision_replaces_view_projection_and_resource_data() {
    let (mut provider, view_id, projection_id, resource_id) = seeded_provider();

    let initial_page = expect_page(read_page(
        &mut provider,
        5,
        &view_id,
        Some(INITIAL_REVISION),
    ));
    assert_eq!(initial_page.revision, INITIAL_REVISION);
    assert_eq!(initial_page.records[0].id.0, "initial-record");

    let initial_projection = expect_projection_set(read_projection_records(
        &mut provider,
        6,
        &view_id,
        &projection_id,
        Some(INITIAL_REVISION),
    ));
    assert_eq!(initial_projection.revision, INITIAL_REVISION);
    assert_eq!(initial_projection.records[0].revision, INITIAL_REVISION);
    assert_eq!(
        projection_text(&initial_projection),
        "initial projection text"
    );

    let initial_metadata = expect_metadata(read_metadata(
        &mut provider,
        7,
        &resource_id,
        Some(INITIAL_REVISION),
    ));
    assert_eq!(initial_metadata.revision, INITIAL_REVISION);
    assert_eq!(initial_metadata.digest.as_deref(), Some("sha256:initial"));
    assert_eq!(initial_metadata.byte_len, b"initial-bytes".len() as u64);

    provider
        .set_revision(UPDATED_REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(
                &view_id,
                20,
                "updated-record",
                projection_id.clone(),
            )],
        )
        .insert_projection_record(text_projection(
            &view_id,
            projection_id.clone(),
            20,
            "updated projection text",
        ))
        .insert_resource(
            metadata(resource_id.clone(), "sha256:updated", 0..7),
            b"updated-resource-bytes".to_vec(),
        );

    let stale_page = expect_stale(read_page(
        &mut provider,
        8,
        &view_id,
        Some(INITIAL_REVISION),
    ));
    assert_stale(
        stale_page,
        TranscriptProviderTarget::View(view_id.clone()),
        INITIAL_REVISION,
        UPDATED_REVISION,
    );

    let stale_projection = expect_stale(read_projection_records(
        &mut provider,
        9,
        &view_id,
        &projection_id,
        Some(INITIAL_REVISION),
    ));
    assert_stale(
        stale_projection,
        TranscriptProviderTarget::View(view_id.clone()),
        INITIAL_REVISION,
        UPDATED_REVISION,
    );

    let stale_metadata = expect_stale(read_metadata(
        &mut provider,
        10,
        &resource_id,
        Some(INITIAL_REVISION),
    ));
    assert_stale(
        stale_metadata,
        TranscriptProviderTarget::Resource(resource_id.clone()),
        INITIAL_REVISION,
        UPDATED_REVISION,
    );

    let stale_range = expect_stale(read_range(
        &mut provider,
        11,
        &resource_id,
        0..7,
        Some(INITIAL_REVISION),
    ));
    assert_stale(
        stale_range,
        TranscriptProviderTarget::ResourceRange {
            resource_id: resource_id.clone(),
            range: 0..7,
        },
        INITIAL_REVISION,
        UPDATED_REVISION,
    );

    let updated_page = expect_page(read_page(
        &mut provider,
        12,
        &view_id,
        Some(UPDATED_REVISION),
    ));
    assert_eq!(updated_page.revision, UPDATED_REVISION);
    assert_eq!(updated_page.records.len(), 1);
    assert_eq!(updated_page.records[0].id.0, "updated-record");
    assert_eq!(updated_page.records[0].position, TranscriptViewPosition(20));

    let updated_projection = expect_projection_set(read_projection_records(
        &mut provider,
        13,
        &view_id,
        &projection_id,
        Some(UPDATED_REVISION),
    ));
    assert_eq!(updated_projection.revision, UPDATED_REVISION);
    assert_eq!(updated_projection.records[0].revision, UPDATED_REVISION);
    assert_eq!(
        projection_text(&updated_projection),
        "updated projection text"
    );

    let updated_metadata = expect_metadata(read_metadata(
        &mut provider,
        14,
        &resource_id,
        Some(UPDATED_REVISION),
    ));
    assert_eq!(updated_metadata.revision, UPDATED_REVISION);
    assert_eq!(updated_metadata.digest.as_deref(), Some("sha256:updated"));
    assert_eq!(
        updated_metadata.byte_len,
        b"updated-resource-bytes".len() as u64
    );

    let updated_range = expect_range(read_range(
        &mut provider,
        15,
        &resource_id,
        0..7,
        Some(UPDATED_REVISION),
    ));
    assert_eq!(updated_range.revision, UPDATED_REVISION);
    assert_eq!(updated_range.bytes, b"updated".to_vec());
    assert!(!updated_range.complete);
}

#[test]
fn fixture_revision_advancement_is_explicit_and_deterministic() {
    let view_id = view_id();
    let early_projection_id = projection_id("early");
    let middle_projection_id = projection_id("middle");
    let late_projection_id = projection_id("late");
    let mut provider = InMemorySyndicTranscriptProvider::new();

    assert_eq!(provider.revision(), ProviderRevision(0));
    assert_eq!(provider.advance_revision(), ProviderRevision(1));
    assert_eq!(provider.advance_revision(), ProviderRevision(2));

    provider
        .set_revision(ProviderRevision(70))
        .insert_view_records(
            view_id.clone(),
            vec![
                view_record(&view_id, 30, "late", late_projection_id.clone()),
                view_record(&view_id, 10, "early", early_projection_id.clone()),
            ],
        );

    let initial_order = expect_page(read_page(
        &mut provider,
        16,
        &view_id,
        Some(ProviderRevision(70)),
    ));
    assert_eq!(initial_order.revision, ProviderRevision(70));
    assert_eq!(
        initial_order
            .records
            .iter()
            .map(|record| record.id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["early", "late"]
    );

    assert_eq!(provider.advance_revision(), ProviderRevision(71));
    provider.push_view_record(
        view_id.clone(),
        view_record(&view_id, 20, "middle", middle_projection_id),
    );

    let stale_page = expect_stale(read_page(
        &mut provider,
        17,
        &view_id,
        Some(ProviderRevision(70)),
    ));
    assert_stale(
        stale_page,
        TranscriptProviderTarget::View(view_id.clone()),
        ProviderRevision(70),
        ProviderRevision(71),
    );

    let updated_order = expect_page(read_page(
        &mut provider,
        18,
        &view_id,
        Some(ProviderRevision(71)),
    ));
    assert_eq!(updated_order.revision, ProviderRevision(71));
    assert_eq!(
        updated_order
            .records
            .iter()
            .map(|record| record.id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["early", "middle", "late"]
    );
}
