use super::indexed::{complete, complete_with_limits, indexed_request};
use super::*;
use crate::syndic_transcript::{
    DemandFact, DemandFactKind, ResidentTranscriptCore, ResidentTranscriptPolicy,
    TranscriptActivationSource,
};
use syndic_storage::indexed_resource as index;

#[path = "anchors.rs"]
mod anchors;

fn resident(fixture: &Fixture, policy: ResidentTranscriptPolicy) -> ResidentTranscriptCore {
    let cancel = AtomicBool::new(false);
    let prepared = fixture
        .reader
        .prepare_attachment(fixture.request(1), &cancel)
        .unwrap();
    let mut core = ResidentTranscriptCore::new(policy);
    core.apply_prepared_activation(
        prepared.activation().clone(),
        TranscriptActivationSource::Test,
    );
    let revision = core.presentation_snapshot().presentation_revision;
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    core
}

fn demand(fixture: &Fixture, core: &mut ResidentTranscriptCore) -> TranscriptResourceRequest {
    let mut request = fixture.resource_request(2);
    let inner = indexed_request(fixture, 7);
    let raw = core.resource_demand().unwrap();
    assert!(core.set_indexed_resource_demand(
        &raw,
        Some(IndexedResourceDemand {
            revision: inner.revision,
            root: inner.root,
            query: inner.query,
        })
    ));
    request.demand = core.resource_demand().unwrap();
    request
}

#[test]
fn selected_pending_refresh_reads_completed_companion_without_changing_outer_visibility() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    let cancel = AtomicBool::new(false);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let mut request = demand(&fixture, &mut core);
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    let weak = Arc::downgrade(&response);
    core = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    let prior = core.core_snapshot();
    complete(&fixture, &metadata);
    assert!(core.resource_demand().is_none());
    let mut obsolete = request.demand.clone();
    obsolete.indexed.as_mut().unwrap().root = Some([91; 32]);
    assert!(!core.refresh_pending_indexed_resource(&obsolete));
    assert_eq!(core.core_snapshot().resident.resource_bytes, 1);
    assert!(core.refresh_pending_indexed_resource(&request.demand));
    assert!(weak.upgrade().is_none());
    assert_eq!(core.core_snapshot().resident.resource_bytes, 0);
    assert!(!core.refresh_pending_indexed_resource(&request.demand));
    request.demand = core.resource_demand().unwrap();
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    assert!(matches!(response.value, IndexedResourceValue::Display(_)));
    core = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    assert!(!core.refresh_pending_indexed_resource(&request.demand));
    assert!(core.resource_demand().is_none());
    assert_eq!(
        core.core_snapshot().resident.visible_range,
        prior.resident.visible_range
    );
    assert_eq!(
        core.presentation_snapshot().activation_revision,
        prior.presentation.activation_revision
    );
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn selected_late_code_line_delivery_preserves_exact_source_and_bounded_resident_charge() {
    let fixture = Fixture::new();
    let text = "prefix\n".repeat(4096) + "late💎\n";
    let metadata = fixture.seed_structured_resource(
        &text,
        0,
        text.len() as u64,
        None,
        7,
        ResourceStructure::code(Some("rust"), 4097).unwrap(),
    );
    complete_with_limits(&fixture, &metadata, 65_536, 64);
    let cancel = AtomicBool::new(false);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let mut request = demand(&fixture, &mut core);
    let mut inner = request.demand.indexed.clone().unwrap();
    inner.query = IndexedResourceQuery::Range {
        request: index::Request::Code {
            lines: index::OrdinalRange {
                start: 4096,
                end: 4097,
            },
        },
        cursor: None,
        descriptors: 1,
        bytes: 8192,
    };
    assert!(core.set_indexed_resource_demand(&request.demand, Some(inner)));
    request.demand = core.resource_demand().unwrap();
    let prior = core.core_snapshot();
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    let root = response.root.as_ref().unwrap();
    assert_eq!(root.code_lines, 4097);
    assert_eq!(root.language.as_deref(), Some("rust"));
    let IndexedResourceValue::Range(range) = &response.value else {
        panic!("range")
    };
    assert_eq!(range.binding.metadata, metadata);
    assert!(range.request_complete);
    assert_eq!(range.items.len(), 1);
    assert!(matches!(&range.items[0], index::Item::Descriptor(value)
        if matches!(value.descriptor, index::Descriptor::CodeLine { ordinal: 4096, raw, .. }
            if raw.start == 4096 * 7 && raw.end == text.len() as u64)));
    assert_eq!(
        range.presentations.as_ref().unwrap()[0]
            .fragment
            .as_ref()
            .unwrap()
            .text,
        "late💎"
    );
    let bytes = response.encoded_bytes;
    assert!(bytes <= 8192);
    core = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    assert_eq!(core.core_snapshot().resident.resource_bytes, bytes);
    assert_eq!(
        core.core_snapshot().resident.visible_range,
        prior.resident.visible_range
    );
    assert!(core.resource_demand().is_none());
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn selected_narrow_table_delivery_preserves_empty_missing_alignment_and_exact_source() {
    let fixture = Fixture::new();
    let text = "a|b|c\n---|---:|:---:\nx|z||\ny|z\n";
    let metadata = fixture.seed_structured_resource(
        text,
        0,
        text.len() as u64,
        None,
        7,
        ResourceStructure::table(2, 3, ProjectionSourceRange::new(0, 6).unwrap()).unwrap(),
    );
    complete_with_limits(&fixture, &metadata, 65_536, 64);
    let cancel = AtomicBool::new(false);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let raw = core.resource_demand().unwrap();
    assert_eq!(raw.kind, ResourceKind::Table);
    let inner = IndexedResourceDemand {
        revision: metadata.revision(),
        root: None,
        query: IndexedResourceQuery::Range {
            request: index::Request::Table {
                role: index::RowRole::Body,
                rows: index::OrdinalRange { start: 0, end: 2 },
                columns: index::OrdinalRange { start: 2, end: 3 },
            },
            cursor: None,
            descriptors: 2,
            bytes: 8192,
        },
    };
    assert!(core.set_indexed_resource_demand(&raw, Some(inner)));
    let mut request = fixture.resource_request(2);
    request.demand = core.resource_demand().unwrap();
    let prior = core.core_snapshot();
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    let root = response.root.as_ref().unwrap();
    assert_eq!((root.body_rows, root.columns), (2, 3));
    assert!(root.header.is_some());
    let IndexedResourceValue::Range(range) = &response.value else {
        panic!("range")
    };
    assert_eq!(range.binding.metadata, metadata);
    assert!(range.request_complete);
    assert_eq!(range.items.len(), 2);
    assert!(matches!(&range.items[0], index::Item::Descriptor(value)
        if matches!(value.descriptor, index::Descriptor::TableCell { row: 0, column: 2, raw, .. } if raw.start == raw.end)));
    assert!(matches!(
        range.items[1],
        index::Item::Missing {
            row: 1,
            column: 2,
            ..
        }
    ));
    let presentations = range.presentations.as_ref().unwrap();
    assert_eq!(presentations[0].alignment, Some(index::Alignment::Center));
    assert_eq!(presentations[0].fragment.as_ref().unwrap().text, "");
    assert!(presentations[1].fragment.is_none());
    let bytes = response.encoded_bytes;
    assert!(bytes <= 8192);
    core = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    assert_eq!(core.core_snapshot().resident.resource_bytes, bytes);
    assert_eq!(
        core.core_snapshot().resident.visible_range,
        prior.resident.visible_range
    );
    assert!(core.resource_demand().is_none());
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn selected_resource_delivery_admits_exact_indexed_payload_and_releases_changed_inner_demand() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one💎\n", 0, 8, None, 7);
    complete(&fixture, &metadata);
    let cancel = AtomicBool::new(false);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let request = demand(&fixture, &mut core);
    let prior = core.core_snapshot();
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    let bytes = response.encoded_bytes;
    let weak = Arc::downgrade(&response);
    let mut candidate = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    assert!(
        core.indexed_resource_response(&request.demand.resource_id)
            .is_none()
    );
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    assert_eq!(candidate.core_snapshot().resident.resource_bytes, bytes);
    assert_eq!(
        candidate.core_snapshot().resident.visible_range,
        prior.resident.visible_range
    );
    assert_eq!(
        candidate.presentation_snapshot().activation_revision,
        prior.presentation.activation_revision
    );
    assert!(candidate.resource_demand().is_none());
    let obsolete = candidate
        .indexed_resource_response(&request.demand.resource_id)
        .unwrap();
    let mut inner = request.demand.indexed.clone().unwrap();
    inner.query = IndexedResourceQuery::Display {
        request: index::DisplayRequest {
            target: index::Target::Code(0),
            offset: 3,
        },
        cursor: None,
        bytes: 65_536,
    };
    assert!(candidate.set_indexed_resource_demand(&request.demand, Some(inner)));
    assert!(
        candidate
            .prepare_indexed_resource_response(&candidate.resource_demand().unwrap(), obsolete)
            .is_none()
    );
    assert!(weak.upgrade().is_none());
    assert_eq!(candidate.core_snapshot().resident.resource_bytes, 0);
    assert_ne!(
        candidate.resource_demand().unwrap().indexed,
        request.demand.indexed
    );
    let pending = Arc::new(IndexedResourceResponse {
        root: None,
        value: IndexedResourceValue::Pending,
        encoded_bytes: 1,
    });
    assert!(
        candidate
            .prepare_indexed_resource_response(&request.demand, pending)
            .is_none()
    );
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn selected_resource_range_and_anchor_transport_exact_root_and_source_into_residency() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one💎\n", 0, 8, None, 7);
    complete(&fixture, &metadata);
    let cancel = AtomicBool::new(false);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let mut request = demand(&fixture, &mut core);
    let query = IndexedResourceQuery::Range {
        request: index::Request::Code {
            lines: index::OrdinalRange { start: 0, end: 1 },
        },
        cursor: None,
        descriptors: 1,
        bytes: 65_536,
    };
    let mut inner = request.demand.indexed.clone().unwrap();
    inner.query = query;
    assert!(core.set_indexed_resource_demand(&request.demand, Some(inner)));
    request.demand = core.resource_demand().unwrap();
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    let root = response.root.as_ref().unwrap().digest;
    let IndexedResourceValue::Range(range) = &response.value else {
        panic!("range")
    };
    assert_eq!(range.binding.metadata, metadata);
    assert_eq!(range.items.len(), 1);
    let source = range.binding.source;
    core = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    let inner = IndexedResourceDemand {
        revision: metadata.revision(),
        root: Some(root),
        query: IndexedResourceQuery::Anchor {
            anchor: index::SourceAnchor {
                source,
                block_start: 0,
                kind: metadata.kind(),
                position: 2,
                visual_offset: -9,
            },
            bytes: 65_536,
        },
    };
    assert!(core.set_indexed_resource_demand(&request.demand, Some(inner)));
    request.demand = core.resource_demand().unwrap();
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    assert_eq!(response.root.as_ref().unwrap().digest, root);
    let IndexedResourceValue::Anchor(anchor) = &response.value else {
        panic!("anchor")
    };
    assert_eq!(anchor.binding.metadata, metadata);
    assert_eq!(anchor.resolved.as_ref().unwrap().anchor.visual_offset, -9);
    core = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    assert!(
        core.indexed_resource_response(&request.demand.resource_id)
            .is_some()
    );
    let successor = core.empty_successor();
    assert!(
        successor
            .indexed_resource_response(&request.demand.resource_id)
            .is_none()
    );
    assert_eq!(successor.core_snapshot().resident.resource_bytes, 0);
}

#[test]
fn selected_resource_pending_is_explicit_and_disposal_releases_its_charge() {
    let fixture = Fixture::new();
    fixture.seed_resource("one\n", 0, 4, None, 7);
    let cancel = AtomicBool::new(false);
    let mut core = resident(&fixture, ResidentTranscriptPolicy::default());
    let request = demand(&fixture, &mut core);
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    assert!(matches!(response.value, IndexedResourceValue::Pending));
    let weak = Arc::downgrade(&response);
    core = core
        .prepare_indexed_resource_response(&request.demand, response)
        .unwrap();
    fixture
        .reader
        .publish_resource_delivery_if_current(&prepared, &request, &cancel, || ())
        .unwrap();
    drop(prepared);
    assert_eq!(core.core_snapshot().resident.resource_bytes, 1);
    assert!(core.resource_demand().is_none());
    assert!(core.set_indexed_resource_demand(&request.demand, None));
    assert!(weak.upgrade().is_none());
    assert_eq!(core.core_snapshot().resident.resource_bytes, 0);
    assert!(core.resource_demand().unwrap().indexed.is_none());
}

#[test]
fn selected_indexed_delivery_rejects_capacity_stale_inner_query_and_obsolete_outer_owner() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    complete(&fixture, &metadata);
    let cancel = AtomicBool::new(false);
    let mut core = resident(
        &fixture,
        ResidentTranscriptPolicy {
            max_resource_bytes: 1,
            ..Default::default()
        },
    );
    let request = demand(&fixture, &mut core);
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    assert!(
        core.prepare_indexed_resource_response(&request.demand, response)
            .is_none()
    );
    assert_eq!(core.core_snapshot().resident.resource_bytes, 0);
    let mut changed = request.clone();
    changed.demand.indexed.as_mut().unwrap().root = Some([99; 32]);
    assert!(
        fixture
            .reader
            .publish_resource_delivery_if_current(&prepared, &changed, &cancel, || panic!(
                "stale publication"
            ))
            .is_err()
    );
    drop(prepared);
    let prepared = fixture
        .reader
        .prepare_resource_delivery(request.clone(), &cancel)
        .unwrap();
    let ResourceDeliveryResponse::Indexed(response) = prepared.response() else {
        panic!("indexed")
    };
    let revision = core.presentation_snapshot().presentation_revision;
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::VisibleRange { range: 0..0 },
    ));
    core.reconcile_resource_demand();
    assert!(
        core.prepare_indexed_resource_response(&request.demand, response)
            .is_none()
    );
    assert!(core.resource_demand().is_none());
    drop(prepared);
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert!(fixture.reader.retire());
}
