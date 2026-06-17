use std::ops::Range;

#[path = "support/syndic_transcript_contract.rs"]
mod syndic_transcript_contract;

use syndic_transcript_contract::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_contract::*;

const REVISION: ProviderRevision = ProviderRevision(19);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("thread-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
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

fn text_projection(
    id: ProjectionRecordId,
    text: impl Into<String>,
    provenance: SyndicSourceProvenance,
) -> ProjectionRecord {
    ProjectionRecord {
        id,
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text { text: text.into() },
        provenance,
    }
}

fn resource_projection(
    id: ProjectionRecordId,
    resource_id: ResourceId,
    resource_kind: ResourceKind,
    label: Option<&str>,
    provenance: SyndicSourceProvenance,
) -> ProjectionRecord {
    ProjectionRecord {
        id,
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::ResourceReference,
        payload: ProjectionPayload::ResourceReference {
            resource_id,
            resource_kind,
            label: label.map(str::to_string),
        },
        provenance,
    }
}

fn view_record(
    view_id: &TranscriptViewId,
    position: u64,
    id: &str,
    projection_id: ProjectionRecordId,
    narrative_kind: TranscriptNarrativeKind,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(id.to_string()),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind,
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            None,
            Some(position..position + 10),
            None,
            Some(position..position + 10),
        ),
    }
}

fn read_projection_records(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    view_id: &TranscriptViewId,
    projection_ids: Vec<ProjectionRecordId>,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadProjectionRecords(ProjectionRecordsRequest {
                view_id: view_id.clone(),
                projection_ids,
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn read_start_page(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    view_id: &TranscriptViewId,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadViewPage(TranscriptViewPageRequest {
                view_id: view_id.clone(),
                anchor: TranscriptPageAnchor::Start,
                direction: TranscriptPageDirection::Forward,
                limit: 8,
                observed_revision: None,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn expect_projection_set(kind: TranscriptProviderResponseKind) -> ProjectionRecordSet {
    match kind {
        TranscriptProviderResponseKind::ProjectionRecords(set) => set,
        other => panic!("expected projection record set, got {other:?}"),
    }
}

fn expect_page(kind: TranscriptProviderResponseKind) -> TranscriptViewPage {
    match kind {
        TranscriptProviderResponseKind::ViewPage(page) => page,
        other => panic!("expected view page, got {other:?}"),
    }
}

fn projection_record_ids(set: &ProjectionRecordSet) -> Vec<String> {
    set.records
        .iter()
        .map(|record| record.id.0.clone())
        .collect()
}

#[test]
fn projection_records_preserve_request_order_revision_and_provenance() {
    let view_id = view_id();
    let text_id = projection_id("text");
    let image_id = projection_id("image");
    let image_resource_id = ResourceId("generated-image-1".to_string());
    let text_provenance = provenance(
        &view_id,
        20,
        &text_id,
        None,
        Some(20..44),
        None,
        Some(24..44),
    );
    let image_provenance = provenance(
        &view_id,
        45,
        &image_id,
        Some(image_resource_id.clone()),
        Some(45..46),
        Some(0..1024),
        Some(45..46),
    );
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_projection_record(resource_projection(
            image_id.clone(),
            image_resource_id.clone(),
            ResourceKind::GeneratedImage,
            Some("generated image"),
            image_provenance.clone(),
        ))
        .insert_projection_record(text_projection(
            text_id.clone(),
            "assistant text chunk",
            text_provenance.clone(),
        ));

    let set = expect_projection_set(read_projection_records(
        &mut provider,
        1,
        &view_id,
        vec![image_id.clone(), text_id.clone()],
        None,
    ));

    assert_eq!(set.view_id, view_id);
    assert_eq!(set.revision, REVISION);
    assert!(set.rejections.is_empty());
    assert_eq!(
        projection_record_ids(&set),
        vec!["projection-image", "projection-text"]
    );
    assert!(set.records.iter().all(|record| record.revision == REVISION));
    assert_eq!(set.records[0].provenance, image_provenance);
    assert_eq!(set.records[1].provenance, text_provenance);
    assert_eq!(set.records[1].provenance.source_range, Some(20..44));
    assert_eq!(set.records[1].provenance.copy_source_range, Some(24..44));
    assert_eq!(
        set.records[0].payload,
        ProjectionPayload::ResourceReference {
            resource_id: image_resource_id,
            resource_kind: ResourceKind::GeneratedImage,
            label: Some("generated image".to_string()),
        }
    );
}

#[test]
fn transcript_view_admission_excludes_unreferenced_operational_projection_records() {
    let view_id = view_id();
    let narrative_id = projection_id("narrative");
    let operational_id = projection_id("operational");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_projection_record(text_projection(
            narrative_id.clone(),
            "visible assistant chunk",
            provenance(
                &view_id,
                10,
                &narrative_id,
                None,
                Some(10..32),
                None,
                Some(10..32),
            ),
        ))
        .insert_projection_record(text_projection(
            operational_id.clone(),
            "operational activity chunk",
            provenance(
                &view_id,
                12,
                &operational_id,
                None,
                Some(12..36),
                None,
                Some(12..36),
            ),
        ))
        .insert_view_records(
            view_id.clone(),
            vec![view_record(
                &view_id,
                10,
                "visible-record",
                narrative_id.clone(),
                TranscriptNarrativeKind::AssistantFinalAnswer,
            )],
        );

    let page = expect_page(read_start_page(&mut provider, 2, &view_id));
    let requested_projection_ids = page
        .records
        .iter()
        .map(|record| record.projection_id.clone())
        .collect::<Vec<_>>();

    assert_eq!(requested_projection_ids, vec![narrative_id.clone()]);
    assert!(
        !requested_projection_ids
            .iter()
            .any(|projection_id| projection_id == &operational_id)
    );

    let set = expect_projection_set(read_projection_records(
        &mut provider,
        3,
        &view_id,
        requested_projection_ids,
        None,
    ));

    assert_eq!(projection_record_ids(&set), vec!["projection-narrative"]);
    assert!(set.rejections.is_empty());
    assert!(!set.records.iter().any(|record| record.id == operational_id));
}

#[test]
fn missing_and_rejected_projection_records_stay_itemized() {
    let view_id = view_id();
    let present_id = projection_id("present");
    let missing_id = projection_id("missing");
    let rejected_id = projection_id("rejected");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_projection_record(text_projection(
            present_id.clone(),
            "resident projection text",
            provenance(
                &view_id,
                30,
                &present_id,
                None,
                Some(30..54),
                None,
                Some(30..54),
            ),
        ))
        .reject_projection_record_with_message(
            rejected_id.clone(),
            TranscriptProviderRejectionReason::BudgetExceeded,
            "projection budget exceeded",
        );

    let set = expect_projection_set(read_projection_records(
        &mut provider,
        4,
        &view_id,
        vec![present_id.clone(), missing_id.clone(), rejected_id.clone()],
        Some(REVISION),
    ));

    assert_eq!(projection_record_ids(&set), vec!["projection-present"]);
    assert_eq!(set.records[0].revision, REVISION);
    assert_eq!(set.rejections.len(), 2);
    assert_eq!(
        set.rejections[0].target,
        TranscriptProviderTarget::ProjectionRecord(missing_id)
    );
    assert_eq!(
        set.rejections[0].reason,
        TranscriptProviderRejectionReason::MissingProjectionRecord
    );
    assert_eq!(set.rejections[0].revision, Some(REVISION));
    assert_eq!(set.rejections[0].message, None);
    assert_eq!(
        set.rejections[1].target,
        TranscriptProviderTarget::ProjectionRecord(rejected_id)
    );
    assert_eq!(
        set.rejections[1].reason,
        TranscriptProviderRejectionReason::BudgetExceeded
    );
    assert_eq!(set.rejections[1].revision, Some(REVISION));
    assert_eq!(
        set.rejections[1].message.as_deref(),
        Some("projection budget exceeded")
    );
}

#[test]
fn observed_revision_controls_projection_response_identity() {
    let view_id = view_id();
    let text_id = projection_id("current");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_projection_record(text_projection(
            text_id.clone(),
            "current projection text",
            provenance(
                &view_id,
                40,
                &text_id,
                None,
                Some(40..64),
                None,
                Some(40..64),
            ),
        ));

    let current = expect_projection_set(read_projection_records(
        &mut provider,
        5,
        &view_id,
        vec![text_id.clone()],
        Some(REVISION),
    ));
    assert_eq!(current.revision, REVISION);
    assert_eq!(current.records[0].revision, REVISION);

    let stale = match read_projection_records(
        &mut provider,
        6,
        &view_id,
        vec![text_id],
        Some(ProviderRevision(18)),
    ) {
        TranscriptProviderResponseKind::Stale(stale) => stale,
        other => panic!("expected stale projection response, got {other:?}"),
    };
    assert_eq!(stale.target, TranscriptProviderTarget::View(view_id));
    assert_eq!(stale.observed_revision, Some(ProviderRevision(18)));
    assert_eq!(stale.current_revision, REVISION);
}
