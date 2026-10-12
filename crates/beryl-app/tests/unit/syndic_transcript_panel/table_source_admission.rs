use super::*;
use crate::transcript_provider::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    ResourceDeliveryResponse,
};
use crate::syndic_transcript::IndexedSourceResponse;
use std::sync::Arc;
use syndic_storage::indexed_resource as index;
use syndic_storage::{ProjectionSourceRange, ResourceMetadataRecord, ResourceStructure};

#[gpui::test]
fn table_source_admission_preserves_exact_kind_root_range_charge_and_release(
    cx: &mut TestAppContext,
) {
    let mut binding = code_anchors::binding(900);
    binding.metadata = ResourceMetadataRecord::new(
        binding.resource.id,
        binding.resource.revision,
        binding.metadata.projection_id().unwrap(),
        binding.metadata.item_id(),
        binding.metadata.ordinal().unwrap(),
        syndic_storage::ResourceKind::Table,
        "text/markdown",
        binding.metadata.backing(),
        *binding.metadata.digest().unwrap(),
        None,
        ResourceStructure::table(100, 10, ProjectionSourceRange::new(0, 10).unwrap()).unwrap(),
    )
    .unwrap();
    let mut activation = resource_seed(1, ResourceKind::Table);
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        activation.projection_records_response.as_mut()
    else {
        unreachable!()
    };
    let resource_id = ResourceId(binding.resource.id.to_string());
    let record = &mut projections.records[0];
    record.payload = ProjectionPayload::ResourceReference {
        resource_id: resource_id.clone(),
        resource_kind: ResourceKind::Table,
        label: Some("Table".into()),
    };
    record.provenance.resource_id = Some(resource_id);
    record.provenance.source_range = Some(0..10000);
    let window = mounted_panel(cx, activation);
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, _| {
        let owner = panel.resource_demand().unwrap();
        let inner = IndexedResourceDemand {
            revision: binding.resource.revision,
            root: Some([55; 32]),
            query: IndexedResourceQuery::Source {
                range: index::ByteRange { start: 8, end: 40 },
                bytes: 16384,
            },
        };
        assert!(
            panel
                .host
                .set_indexed_resource_demand(&owner, Some(inner.clone()))
        );
        let demand = panel.resource_demand().unwrap();
        let before = panel
            .host
            .diagnostic_snapshot()
            .resident_data
            .resource_bytes;
        let response = |source_binding: index::Binding, digest, end, payload| {
            let build = index::BuildKey {
                resource: source_binding.resource,
                operation: [22; 16],
            };
            ResourceDeliveryResponse::Indexed(Arc::new(IndexedResourceResponse {
                root: Some(index::Root {
                    key: build,
                    binding_digest: [88; 32],
                    pages: 1,
                    chain: [99; 32],
                    counts: [0, 101, 1010, 10],
                    code_lines: 0,
                    body_rows: 100,
                    columns: 10,
                    header: Some(index::ByteRange { start: 0, end: 10 }),
                    language: None,
                    progress: index::SequenceKey { build, ordinal: 1 },
                    digest,
                }),
                encoded_bytes: 1056,
                value: IndexedResourceValue::Source(IndexedSourceResponse {
                    binding: source_binding,
                    range: index::ByteRange { start: 8, end },
                    bytes: vec![b'x'; payload],
                }),
            }))
        };
        for invalid in [
            response(code_anchors::binding(900), [55; 32], 40, 32),
            response(binding.clone(), [54; 32], 40, 32),
            response(binding.clone(), [55; 32], 41, 32),
            response(binding.clone(), [55; 32], 40, 31),
        ] {
            assert!(
                panel
                    .host
                    .prepare_resource_delivery(&demand, invalid)
                    .is_none()
            );
        }
        let candidate = panel
            .host
            .prepare_resource_delivery(&demand, response(binding.clone(), [55; 32], 40, 32))
            .unwrap();
        assert_eq!(
            candidate.diagnostic_snapshot().resident_data.resource_bytes,
            before + 1056
        );
        panel.host = candidate;
        assert!(panel.host.indexed_resource_response(&demand.resource_id).is_some());
        let mut replacement = inner;
        replacement.query = IndexedResourceQuery::Source {
            range: index::ByteRange { start: 40, end: 72 },
            bytes: 16384,
        };
        assert!(
            panel
                .host
                .set_indexed_resource_demand(&demand, Some(replacement))
        );
        assert_eq!(
            panel.host.diagnostic_snapshot().resident_data.resource_bytes,
            before
        );
        assert!(
            panel
                .host
                .prepare_resource_delivery(&demand, response(binding.clone(), [55; 32], 40, 32),)
                .is_none()
        );
        let current = panel.resource_demand().unwrap();
        assert!(panel.host.set_indexed_resource_demand(&current, None));
        assert!(
            panel
                .host
                .indexed_resource_response(&demand.resource_id)
                .is_none()
        );
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            before
        );
        panel.retire();
        assert!(
            panel
                .host
                .prepare_resource_delivery(&demand, response(binding.clone(), [55; 32], 40, 32),)
                .is_none()
        );
    });
}
