use super::*;

#[gpui::test]
fn indexed_code_source_admission_is_exact_charged_and_refuses_superseded_or_retired_owners(
    cx: &mut TestAppContext,
) {
    use crate::syndic_transcript::IndexedSourceResponse;
    let window = mounted_panel(cx, activation());
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    publish_metadata(cx, window, &panel);
    panel.update(cx, |panel, _| {
        let owner = panel.resource_demand().unwrap();
        let diagnostics = panel.diagnostic_snapshot();
        let before = diagnostics.resident_data.resource_bytes
            - diagnostics.code_panels[0].layout_charged_bytes;
        assert!(panel.host.reserve_code_layout(&owner, 131_072));
        let mut inner = owner.indexed.clone().unwrap();
        inner.root = Some([55; 32]);
        inner.query = IndexedResourceQuery::Source {
            range: index::ByteRange { start: 0, end: 32 },
            bytes: 16384,
        };
        assert!(
            panel
                .host
                .set_indexed_resource_demand(&owner, Some(inner.clone()))
        );
        let demand = panel.resource_demand().unwrap();
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            before + 131_072
        );
        let response = |digest, span_end, payload| {
            let binding = binding(900);
            let mut root = root(&binding, 2000);
            root.digest = digest;
            ResourceDeliveryResponse::Indexed(Arc::new(IndexedResourceResponse {
                root: Some(root),
                encoded_bytes: 1056,
                value: IndexedResourceValue::Source(IndexedSourceResponse {
                    binding,
                    range: index::ByteRange {
                        start: 0,
                        end: span_end,
                    },
                    bytes: vec![b'x'; payload],
                }),
            }))
        };
        for invalid in [
            response([54; 32], 32, 32),
            response([55; 32], 33, 32),
            response([55; 32], 32, 31),
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
            .prepare_resource_delivery(&demand, response([55; 32], 32, 32))
            .unwrap();
        assert_eq!(
            candidate.diagnostic_snapshot().resident_data.resource_bytes,
            before + 131_072 + 1056
        );
        drop(candidate);
        let mut foreground = inner;
        foreground.query = IndexedResourceQuery::Range {
            request: index::Request::Code {
                lines: index::OrdinalRange { start: 0, end: 16 },
            },
            cursor: None,
            descriptors: 16,
            bytes: 65536,
        };
        assert!(
            panel
                .host
                .set_indexed_resource_demand(&demand, Some(foreground))
        );
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            before + 131_072
        );
        assert!(
            panel
                .host
                .prepare_resource_delivery(&demand, response([55; 32], 32, 32))
                .is_none()
        );
        panel.retire();
        assert!(
            panel
                .host
                .prepare_resource_delivery(&demand, response([55; 32], 32, 32))
                .is_none()
        );
        assert!(!panel.host.reserve_code_layout(&demand, 131_072));
    });
}

#[gpui::test]
fn code_layout_reservation_shares_source_capacity_and_releases_with_its_owner(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(cx, activation());
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    publish_metadata(cx, window, &panel);
    panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        let diagnostics = panel.diagnostic_snapshot();
        let existing = diagnostics.code_panels[0].layout_charged_bytes;
        let before = diagnostics.resident_data.resource_bytes - existing;
        assert!(!panel.host.reserve_code_layout(&demand, usize::MAX));
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            before + existing
        );
        assert!(panel.host.reserve_code_layout(&demand, 131_072));
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            before + 131_072
        );
        let mut inner = demand.indexed.clone().unwrap();
        inner.query = IndexedResourceQuery::Display {
            request: index::DisplayRequest {
                target: index::Target::Code(0),
                offset: 8192,
            },
            cursor: None,
            bytes: 16384,
        };
        assert!(panel.host.set_indexed_resource_demand(&demand, Some(inner)));
        let owner = panel
            .host
            .indexed_resource_owner(&demand.resource_id)
            .unwrap();
        assert!(!panel.host.reserve_code_layout(&demand, 0));
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            before + 131_072
        );
        assert!(panel.host.set_indexed_resource_demand(&owner, None));
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            before
        );
        assert!(!panel.host.reserve_code_layout(&owner, 131_072));
    });
}
