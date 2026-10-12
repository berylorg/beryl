use super::table_export_tests::{Fixture, fixture};
use super::*;
use crate::syndic_transcript::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    ResourceDeliveryResponse,
};
use std::sync::Arc;
use syndic_storage::indexed_resource as index;

#[gpui::test]
fn table_one_item_pages_complete_the_maximum_window_and_refuse_over_capacity(
    cx: &mut TestAppContext,
) {
    let rows = vec![vec![Some("cell"); 8]; 17];
    let fixture = fixture(&rows, "|---|---|---|---|---|---|---|---|");
    let resource = ResourceId(fixture.binding.resource.id.to_string());
    let mut host = host(cx, &fixture);
    let mut accepted = 0;
    for (role, rows) in [
        (index::RowRole::Header, 0..1),
        (index::RowRole::Body, 0..15),
    ] {
        let mut cursor = None;
        loop {
            let query = query(&fixture, role, rows.clone(), 0..8, cursor);
            let demand = request(&mut host, &resource, query.clone());
            let response = page(&fixture, &query, 1);
            let IndexedResourceValue::Range(range) = &response.value else {
                panic!()
            };
            cursor = Some(range.cursor);
            let complete = range.request_complete;
            assert_eq!(range.items.len(), 1);
            admit(&mut host, &demand, response);
            accepted += 1;
            assert_eq!(
                host.diagnostic_snapshot().resident_data.resource_bytes,
                accepted * 4096
            );
            if accepted < 128 {
                assert!(host.indexed_table_window(&resource).is_empty());
            }
            if complete {
                break;
            }
        }
    }
    assert_eq!(accepted, 128);
    let window = host.indexed_table_window(&resource);
    assert_eq!(window.len(), 128);
    let oversized = query(&fixture, index::RowRole::Body, 0..16, 0..8, None);
    let demand = request(&mut host, &resource, oversized.clone());
    assert!(
        host.prepare_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(page(&fixture, &oversized, 128))
        )
        .is_none()
    );
    assert_eq!(
        host.diagnostic_snapshot().resident_data.resource_bytes,
        128 * 4096
    );
    assert!(
        host.indexed_table_window(&resource)
            .iter()
            .zip(&window)
            .all(|(a, b)| Arc::ptr_eq(a, b))
    );
    let owner = host.indexed_resource_owner(&resource).unwrap();
    assert!(host.set_indexed_resource_demand(&owner, None));
    assert_eq!(host.diagnostic_snapshot().resident_data.resource_bytes, 0);
    assert!(host.indexed_table_window(&resource).is_empty());
}

#[gpui::test]
fn table_replacement_peak_stays_charged_until_measured_exact_adoption(cx: &mut TestAppContext) {
    let fixture = fixture(
        &[vec![Some("h0"), Some("h1")], vec![Some("a"), Some("b")]],
        "|---|---|",
    );
    let resource = ResourceId(fixture.binding.resource.id.to_string());
    let mut host = host(cx, &fixture);
    for role in [index::RowRole::Header, index::RowRole::Body] {
        let query = query(&fixture, role, 0..1, 0..2, None);
        let demand = request(&mut host, &resource, query.clone());
        admit(&mut host, &demand, page(&fixture, &query, 2));
    }
    let old = host.indexed_table_window(&resource);
    assert_eq!(old.len(), 2);
    let owner = host.indexed_resource_owner(&resource).unwrap();
    let limit = 8 * 1024 * 1024;
    let tight_layout = limit - 13 * 1024;
    assert!(host.reserve_indexed_layout(&owner, tight_layout));
    let header = query(&fixture, index::RowRole::Header, 0..1, 1..2, None);
    let demand = request(&mut host, &resource, header.clone());
    admit(&mut host, &demand, page(&fixture, &header, 1));
    let body = query(&fixture, index::RowRole::Body, 0..1, 1..2, None);
    let demand = request(&mut host, &resource, body.clone());
    let completion = page(&fixture, &body, 1);
    assert!(
        host.prepare_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(completion.clone())
        )
        .is_none()
    );
    assert_eq!(
        host.diagnostic_snapshot().resident_data.resource_bytes,
        tight_layout + 12 * 1024
    );
    assert!(
        host.indexed_table_window(&resource)
            .iter()
            .zip(&old)
            .all(|(a, b)| Arc::ptr_eq(a, b))
    );
    let roomy_layout = limit - 16 * 1024;
    let reservation_owner = host.indexed_resource_owner(&resource).unwrap();
    assert!(host.reserve_indexed_layout(&reservation_owner, roomy_layout));
    let mut candidate = host
        .prepare_resource_delivery(&demand, ResourceDeliveryResponse::Indexed(completion))
        .unwrap();
    let adopted = candidate.indexed_table_window(&resource);
    let owner = candidate.indexed_resource_owner(&resource).unwrap();
    assert_eq!(
        candidate.diagnostic_snapshot().resident_data.resource_bytes,
        limit
    );
    assert!(!candidate.acknowledge_table_window(&owner, &adopted));
    let mut source = body.clone();
    source.query = IndexedResourceQuery::Source {
        range: index::ByteRange {
            start: fixture.binding.range.start,
            end: fixture.binding.range.start + 1,
        },
        bytes: 8192,
    };
    assert!(!candidate.set_indexed_resource_demand(&owner, Some(source.clone())));
    let measured =
        candidate.realize_frame(crate::syndic_transcript::RealizedFrameRequest::default());
    assert!(!candidate.acknowledge_table_window(&owner, &adopted));
    candidate.accept_measured_frame(measured);
    let mut stale = owner.clone();
    stale.presentation_revision += 1;
    assert!(!candidate.acknowledge_table_window(&stale, &adopted));
    assert!(!candidate.acknowledge_table_window(&owner, &old));
    assert_eq!(
        candidate.diagnostic_snapshot().resident_data.resource_bytes,
        limit
    );
    assert!(candidate.acknowledge_table_window(&owner, &adopted));
    assert_eq!(
        candidate.diagnostic_snapshot().resident_data.resource_bytes,
        roomy_layout + 8 * 1024
    );
    assert!(!candidate.acknowledge_table_window(&owner, &adopted));
    assert!(candidate.set_indexed_resource_demand(&owner, Some(source)));
    let owner = candidate.indexed_resource_owner(&resource).unwrap();
    assert!(candidate.set_indexed_resource_demand(&owner, None));
    assert_eq!(
        candidate.diagnostic_snapshot().resident_data.resource_bytes,
        0
    );
}

#[gpui::test]
fn table_completed_standalone_body_requests_replace_staging_before_mounted_window(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(
        &[
            vec![Some("h0"), Some("h1")],
            vec![Some("a"), Some("b")],
            vec![Some("c"), None],
        ],
        "|---|---|",
    );
    let resource = ResourceId(fixture.binding.resource.id.to_string());
    let mut host = host(cx, &fixture);
    for columns in [0..1, 1..2] {
        let body = query(&fixture, index::RowRole::Body, 0..2, columns, None);
        let demand = request(&mut host, &resource, body.clone());
        admit(&mut host, &demand, page(&fixture, &body, 2));
        assert!(host.indexed_table_window(&resource).is_empty());
        assert_eq!(
            host.diagnostic_snapshot().resident_data.resource_bytes,
            4096
        );
    }
    let header = query(&fixture, index::RowRole::Header, 0..1, 0..2, None);
    let demand = request(&mut host, &resource, header.clone());
    admit(&mut host, &demand, page(&fixture, &header, 2));
    assert!(host.indexed_table_window(&resource).is_empty());
    let body = query(&fixture, index::RowRole::Body, 0..2, 0..2, None);
    let demand = request(&mut host, &resource, body.clone());
    admit(&mut host, &demand, page(&fixture, &body, 4));
    assert_eq!(host.indexed_table_window(&resource).len(), 2);
    assert_eq!(
        host.diagnostic_snapshot().resident_data.resource_bytes,
        8192
    );
}

fn host(cx: &mut TestAppContext, fixture: &Fixture) -> SyndicTranscriptHost {
    let mut activation = resource_seed(1, ResourceKind::Table);
    let Some(TranscriptProviderResponseKind::ProjectionRecords(records)) =
        &mut activation.projection_records_response
    else {
        panic!();
    };
    let id = ResourceId(fixture.binding.resource.id.to_string());
    let record = &mut records.records[0];
    record.payload = ProjectionPayload::ResourceReference {
        resource_id: id.clone(),
        resource_kind: ResourceKind::Table,
        label: None,
    };
    record.provenance.resource_id = Some(id);
    record.provenance.source_range = Some(fixture.binding.range.start..fixture.binding.range.end);
    let window = mounted_panel(cx, activation);
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, _, app| {
            root.panel.update(app, |panel, _| {
                std::mem::replace(&mut panel.host, SyndicTranscriptHost::empty())
            })
        })
        .unwrap()
}
fn query(
    fixture: &Fixture,
    role: index::RowRole,
    rows: std::ops::Range<u64>,
    columns: std::ops::Range<u64>,
    cursor: Option<index::Cursor>,
) -> IndexedResourceDemand {
    IndexedResourceDemand {
        revision: fixture.binding.resource.revision,
        root: Some(fixture.page.root.as_ref().unwrap().digest),
        query: IndexedResourceQuery::Range {
            request: index::Request::Table {
                role,
                rows: index::OrdinalRange {
                    start: rows.start,
                    end: rows.end,
                },
                columns: index::OrdinalRange {
                    start: columns.start,
                    end: columns.end,
                },
            },
            cursor,
            descriptors: 128,
            bytes: 65536,
        },
    }
}
fn page(
    fixture: &Fixture,
    query: &IndexedResourceDemand,
    count: usize,
) -> Arc<IndexedResourceResponse> {
    let IndexedResourceQuery::Range {
        request:
            index::Request::Table {
                role,
                rows,
                columns,
            },
        cursor,
        ..
    } = &query.query
    else {
        panic!();
    };
    let (mut row, mut column) = match cursor.map(|cursor| cursor.next) {
        Some(index::Position::Table { row, column }) => (row, column),
        _ => (rows.start, columns.start),
    };
    let mut items = Vec::new();
    while row < rows.end && items.len() < count {
        items.push(
            fixture
                .cells
                .iter()
                .find(|(cell, _)| cell.role == *role && cell.row == row && cell.column == column)
                .unwrap()
                .1
                .clone(),
        );
        column += 1;
        if column == columns.end {
            column = columns.start;
            row += 1;
        }
    }
    Arc::new(IndexedResourceResponse {
        root: fixture.page.root.clone(),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Range(index::Response {
            binding: fixture.binding.clone(),
            cursor: index::Cursor {
                build: fixture.page.root.as_ref().unwrap().key,
                root_digest: fixture.page.root.as_ref().unwrap().digest,
                request: match &query.query {
                    IndexedResourceQuery::Range { request, .. } => *request,
                    _ => panic!(),
                },
                next: index::Position::Table { row, column },
                fragment_offset: 0,
            },
            items,
            request_complete: row == rows.end,
            resource_complete: false,
            encoded_bytes: 4096,
            presentations: None,
        }),
    })
}
fn request(
    host: &mut SyndicTranscriptHost,
    resource: &ResourceId,
    query: IndexedResourceDemand,
) -> crate::syndic_transcript::ResourceTranscriptDemand {
    let owner = host
        .indexed_resource_owner(resource)
        .unwrap_or_else(|| host.resource_demands()[0].clone());
    assert!(host.set_indexed_resource_demand(&owner, Some(query)));
    host.resource_demands()[0].clone()
}
fn admit(
    host: &mut SyndicTranscriptHost,
    demand: &crate::syndic_transcript::ResourceTranscriptDemand,
    response: Arc<IndexedResourceResponse>,
) {
    *host = host
        .prepare_resource_delivery(demand, ResourceDeliveryResponse::Indexed(response))
        .expect("bounded exact Table response");
}

#[gpui::test]
fn table_header_body_continuations_publish_one_coherent_window_and_release(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(
        &[
            vec![Some("h0"), Some("h1"), Some("h2")],
            vec![Some("a"), Some(""), None],
            vec![Some("c"), Some("d"), Some("e")],
        ],
        "|---|---|---|",
    );
    let resource = ResourceId(fixture.binding.resource.id.to_string());
    let mut host = host(cx, &fixture);
    let header = query(&fixture, index::RowRole::Header, 0..1, 0..3, None);
    let demand = request(&mut host, &resource, header.clone());
    let first = page(&fixture, &header, 1);
    admit(&mut host, &demand, first.clone());
    assert!(host.indexed_table_window(&resource).is_empty());
    let IndexedResourceValue::Range(first_range) = &first.value else {
        panic!();
    };
    let header_next = query(
        &fixture,
        index::RowRole::Header,
        0..1,
        0..3,
        Some(first_range.cursor),
    );
    let demand = request(&mut host, &resource, header_next.clone());
    admit(&mut host, &demand, page(&fixture, &header_next, 2));
    assert!(host.indexed_table_window(&resource).is_empty());
    let body = query(&fixture, index::RowRole::Body, 0..2, 0..3, None);
    let demand = request(&mut host, &resource, body.clone());
    let first_body = page(&fixture, &body, 2);
    admit(&mut host, &demand, first_body.clone());
    assert!(host.indexed_table_window(&resource).is_empty());
    let IndexedResourceValue::Range(body_range) = &first_body.value else {
        panic!();
    };
    let tail = query(
        &fixture,
        index::RowRole::Body,
        0..2,
        0..3,
        Some(body_range.cursor),
    );
    let demand = request(&mut host, &resource, tail.clone());
    admit(&mut host, &demand, page(&fixture, &tail, 4));
    let window = host.indexed_table_window(&resource);
    assert_eq!(window.len(), 4);
    assert!(host.diagnostic_snapshot().resident_data.resource_bytes >= 4 * 4096);
    let next = query(&fixture, index::RowRole::Header, 0..1, 1..3, None);
    let demand = request(&mut host, &resource, next.clone());
    admit(&mut host, &demand, page(&fixture, &next, 2));
    assert!(
        host.indexed_table_window(&resource)
            .iter()
            .zip(&window)
            .all(|(a, b)| Arc::ptr_eq(a, b))
    );
    let owner = host.indexed_resource_owner(&resource).unwrap();
    assert!(host.set_indexed_resource_demand(&owner, None));
    assert!(host.indexed_table_window(&resource).is_empty());
    assert_eq!(host.diagnostic_snapshot().resident_data.resource_bytes, 0);
}

#[gpui::test]
fn table_window_refuses_wrong_root_coordinates_incomplete_cursor_and_wide_request(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(
        &[vec![Some("h0"), Some("h1")], vec![Some("a"), Some("b")]],
        "|---|---|",
    );
    let resource = ResourceId(fixture.binding.resource.id.to_string());
    let mut host = host(cx, &fixture);
    let header = query(&fixture, index::RowRole::Header, 0..1, 0..2, None);
    let demand = request(&mut host, &resource, header.clone());
    let valid = page(&fixture, &header, 2);
    let IndexedResourceValue::Range(range) = &valid.value else {
        panic!();
    };
    let invalid = Arc::new(IndexedResourceResponse {
        root: valid.root.clone(),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Range(index::Response {
            binding: range.binding.clone(),
            cursor: range.cursor,
            items: range.items.iter().rev().cloned().collect(),
            request_complete: true,
            resource_complete: false,
            encoded_bytes: 4096,
            presentations: None,
        }),
    });
    assert!(
        host.prepare_resource_delivery(&demand, ResourceDeliveryResponse::Indexed(invalid))
            .is_none()
    );
    assert!(host.indexed_table_window(&resource).is_empty());
    let mut wrong_root = valid.root.clone().unwrap();
    wrong_root.digest = [44; 32];
    let wrong = Arc::new(IndexedResourceResponse {
        root: Some(wrong_root),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Pending,
    });
    assert!(
        host.prepare_resource_delivery(&demand, ResourceDeliveryResponse::Indexed(wrong))
            .is_none()
    );
    admit(&mut host, &demand, valid);
    let body = query(&fixture, index::RowRole::Body, 0..1, 0..2, None);
    let demand = request(&mut host, &resource, body.clone());
    admit(&mut host, &demand, page(&fixture, &body, 2));
    let before = host.indexed_table_window(&resource);
    let wide = query(&fixture, index::RowRole::Header, 0..1, 0..9, None);
    let demand = request(&mut host, &resource, wide);
    assert!(
        host.prepare_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(page(&fixture, &header, 2))
        )
        .is_none()
    );
    assert_eq!(host.indexed_table_window(&resource).len(), before.len());
}
