use super::*;

#[path = "code_anchors.rs"]
mod code_anchors;

#[path = "code_mount.rs"]
mod code_mount;

#[path = "table_source_admission.rs"]
mod table_source_admission;

#[gpui::test]
fn refreshed_indexed_resources_share_the_queue_without_outer_scrolling(cx: &mut TestAppContext) {
    use crate::transcript_provider::{
        IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
        ResourceDeliveryResponse,
    };
    use syndic_storage::indexed_resource as index;
    let window =
        mounted_panel_with_initial_layout(cx, resource_seed(3, ResourceKind::Code), 640.0, 0.0);
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let (owners, visible, activation) = panel.update(cx, |panel, _| {
        let owners = panel.host.resource_demands();
        assert_eq!(owners.len(), 3);
        for owner in &owners {
            assert!(panel.set_indexed_resource_demand(
                owner,
                Some(IndexedResourceDemand {
                    revision: beryl_model::ProjectionRevision::new(7).unwrap(),
                    root: None,
                    query: IndexedResourceQuery::Display {
                        request: index::DisplayRequest {
                            target: index::Target::Code(0),
                            offset: 0,
                        },
                        cursor: None,
                        bytes: 8192,
                    },
                })
            ));
        }
        let snapshot = panel.snapshot();
        (
            owners,
            snapshot.visible_range.clone(),
            snapshot.activation_revision,
        )
    });
    let mut served = Vec::new();
    for _ in 0..6 {
        let demand = panel.update(cx, |panel, _| {
            let demand = panel.resource_demand().unwrap();
            let pending = || {
                ResourceDeliveryResponse::Indexed(std::sync::Arc::new(IndexedResourceResponse {
                    root: None,
                    value: IndexedResourceValue::Pending,
                    encoded_bytes: 1,
                }))
            };
            assert!(panel.stage_resource_delivery(&demand, pending(), None));
            panel.discard_staged_activation();
            assert_eq!(panel.resource_demand(), Some(demand.clone()));
            assert!(panel.stage_resource_delivery(&demand, pending(), None));
            demand
        });
        draw_frames(cx, window, 5);
        window
            .update(cx, |root, window, cx| {
                root.panel.update(cx, |panel, _| {
                    assert!(panel.measurement_current(window));
                    panel.publish_resource_candidate(&demand);
                    assert!(panel.refresh_pending_indexed_resource(&demand));
                    assert_ne!(panel.resource_demand().unwrap().record_id, demand.record_id);
                    let snapshot = panel.snapshot();
                    assert_eq!(snapshot.visible_range, visible);
                    assert_eq!(snapshot.activation_revision, activation);
                });
            })
            .unwrap();
        served.push(demand.record_id);
    }
    assert_eq!(&served[..3], &served[3..]);
    for owner in owners {
        assert_eq!(
            served.iter().filter(|id| **id == owner.record_id).count(),
            2
        );
    }
    panel.update(cx, |panel, _| {
        let first = panel.resource_demand().unwrap();
        panel.reject_resource_demand(first.clone());
        let mut inner = first.indexed.clone().unwrap();
        inner.root = Some([73; 32]);
        assert!(panel.set_indexed_resource_demand(&first, Some(inner)));
        assert_ne!(panel.resource_demand().unwrap().record_id, first.record_id);
        panel
            .resource_queue_position
            .as_mut()
            .unwrap()
            .activation_revision += 1;
        assert_eq!(
            panel.resource_demand().unwrap().record_id,
            panel.host.resource_demands()[0].record_id
        );
        panel.retire();
        assert!(panel.resource_queue_position.is_none());
        assert!(panel.resource_demand().is_none());
    });
}

#[gpui::test]
fn indexed_inner_demand_stages_pending_and_rejects_completion_after_inner_replacement(
    cx: &mut TestAppContext,
) {
    use crate::transcript_provider::{
        IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
        ResourceDeliveryResponse,
    };
    use syndic_storage::indexed_resource as index;
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let raw = panel.read_with(cx, |panel, _| panel.resource_demand().unwrap());
    let mut inner = IndexedResourceDemand {
        revision: beryl_model::ProjectionRevision::new(7).unwrap(),
        root: None,
        query: IndexedResourceQuery::Display {
            request: index::DisplayRequest {
                target: index::Target::Code(4096),
                offset: 0,
            },
            cursor: None,
            bytes: 65_536,
        },
    };
    let pending = || {
        ResourceDeliveryResponse::Indexed(std::sync::Arc::new(IndexedResourceResponse {
            root: None,
            value: IndexedResourceValue::Pending,
            encoded_bytes: 1,
        }))
    };
    let indexed = panel.update(cx, |panel, _| {
        assert!(panel.set_indexed_resource_demand(&raw, Some(inner.clone())));
        let demand = panel.resource_demand().unwrap();
        assert!(panel.stage_resource_delivery(&demand, pending(), None));
        assert!(!panel.refresh_pending_indexed_resource(&demand));
        assert!(
            panel
                .host
                .indexed_resource_response(&demand.resource_id)
                .is_none()
        );
        demand
    });
    inner.root = Some([12; 32]);
    panel.update(cx, |panel, _| {
        assert!(panel.set_indexed_resource_demand(&indexed, Some(inner)));
        assert!(!panel.resource_candidate_current(&indexed));
        assert!(!panel.stage_resource_delivery(&indexed, pending(), None));
        let current = panel.resource_demand().unwrap();
        assert!(panel.stage_resource_delivery(&current, pending(), None));
    });
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                let demand = panel.resource_demand().unwrap();
                assert!(panel.measurement_current(window));
                panel.publish_resource_candidate(&demand);
                assert!(matches!(
                    panel
                        .host
                        .indexed_resource_response(&demand.resource_id)
                        .unwrap()
                        .value,
                    IndexedResourceValue::Pending
                ));
                assert!(panel.resource_demand().is_none());
                assert!(!panel.refresh_pending_indexed_resource(&indexed));
                assert!(panel.refresh_pending_indexed_resource(&demand));
                assert_eq!(panel.resource_demand().unwrap().indexed, demand.indexed);
                assert!(!panel.refresh_pending_indexed_resource(&demand));
                assert!(panel.set_indexed_resource_demand(&demand, None));
                assert!(
                    panel
                        .host
                        .indexed_resource_response(&demand.resource_id)
                        .is_none()
                );
            });
        })
        .unwrap();
}

fn resource_seed(count: u64, kind: ResourceKind) -> PreparedTranscriptActivation {
    let mut activation = seed(count);
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        activation.projection_records_response.as_mut()
    else {
        unreachable!()
    };
    for (index, projection) in projections.records.iter_mut().enumerate() {
        let resource_id = ResourceId(format!("resource-{index}"));
        projection.kind = ProjectionRecordKind::ResourceReference;
        projection.payload = ProjectionPayload::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind: kind.clone(),
            label: Some("resource".into()),
        };
        projection.provenance.resource_id = Some(resource_id);
    }
    activation
}

fn metadata_response(
    demand: &super::super::super::ResourceTranscriptDemand,
) -> TranscriptProviderResponseKind {
    TranscriptProviderResponseKind::ResourceMetadata(ResourceMetadata {
        resource_id: demand.resource_id.clone(),
        revision: ProviderRevision(900),
        kind: demand.kind.clone(),
        media_type: Some("text/plain".into()),
        byte_len: 10,
        digest: None,
        line_count: Some(1),
        row_count: None,
        column_count: None,
        preview_range: Some(0..10),
    })
}

#[gpui::test]
fn code_panel_rebinds_lost_indexed_residency_without_replacing_its_mount(cx: &mut TestAppContext) {
    use super::super::super::IndexedResourceQuery;

    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let demand = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        assert!(panel.stage_resource_response(&demand, metadata_response(&demand), None));
        demand
    });
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                assert!(panel.measurement_current(window));
                panel.publish_resource_candidate(&demand);
            });
        })
        .unwrap();
    draw_frames(cx, window, 5);
    let (id, anchor) = panel.update(cx, |panel, _| {
        assert_eq!(panel.code_panels.len(), 1);
        let mounted = &mut panel.code_panels[0];
        mounted.viewport.scroll.borrow_mut().set_lines(2000);
        mounted.viewport.scroll.borrow_mut().set_offset(1007.0);
        mounted.viewport.scroll.borrow_mut().horizontal = 45.0;
        mounted
            .viewport
            .adopt(&code_anchors::range_response(code_anchors::binding(1), 50));
        let anchor = mounted.viewport.anchor.unwrap();
        let id = mounted.id;
        panel.code_wheel_owner.set(Some(id));
        let owner = panel
            .host
            .indexed_resource_owner(&demand.resource_id)
            .unwrap();
        assert!(panel.host.set_indexed_resource_demand(&owner, None));
        (id, anchor)
    });
    draw_frames(cx, window, 5);
    panel.update(cx, |panel, _| {
        let mounted = &panel.code_panels[0];
        assert_eq!(mounted.id, id);
        assert_eq!(panel.code_wheel_owner.get(), Some(id));
        assert_eq!(mounted.viewport.scroll.borrow().top_line, 50);
        assert_eq!(mounted.viewport.scroll.borrow().remainder, 7.0);
        assert_eq!(mounted.viewport.scroll.borrow().horizontal, 45.0);
        assert!(mounted.viewport.remapping);
        let owner = panel
            .host
            .indexed_resource_owner(&demand.resource_id)
            .unwrap();
        let inner = owner.indexed.unwrap();
        assert_eq!(inner.root, None);
        assert!(
            matches!(inner.query, IndexedResourceQuery::Anchor { anchor: requested, .. }
            if requested == anchor)
        );
    });
}

#[gpui::test]
fn resource_publication_waits_for_current_allocation_and_preserves_prior_snapshot(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let (demand, prior, anchor) = panel.read_with(cx, |panel, _| {
        (
            panel.resource_demand().unwrap(),
            panel.snapshot(),
            panel.host.scroll_state().anchor,
        )
    });
    panel.update(cx, |panel, _| {
        assert!(panel.stage_resource_response(
            &demand,
            metadata_response(&demand),
            Some(gpui::size(px(640.0), px(180.0)))
        ));
        assert!(panel.snapshot().resources.metadata.is_empty());
    });
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                assert!(panel.measurement_current(window));
                assert!(
                    panel.update_resource_allocation(&demand, gpui::size(px(640.0), px(200.0)))
                );
                assert!(!panel.measurement_current(window));
            })
        })
        .unwrap();
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                assert!(panel.measurement_current(window));
                panel.publish_resource_candidate(&demand);
                assert_eq!(
                    panel.snapshot().resources.metadata[0].revision,
                    ProviderRevision(900)
                );
                assert_eq!(
                    panel.snapshot().activation_revision,
                    prior.activation_revision
                );
                assert_eq!(
                    panel
                        .host
                        .scroll_state()
                        .anchor
                        .as_ref()
                        .map(|anchor| &anchor.record_id),
                    anchor.as_ref().map(|anchor| &anchor.record_id)
                );
            })
        })
        .unwrap();
    assert!(prior.resources.metadata.is_empty());
}

#[gpui::test]
fn rejected_first_resource_does_not_starve_other_current_resources(cx: &mut TestAppContext) {
    let window =
        mounted_panel_with_initial_layout(cx, resource_seed(2, ResourceKind::Code), 640.0, 0.0);
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, _| {
        let first = panel.resource_demand().unwrap();
        panel.reject_resource_demand(first.clone());
        let second = panel.resource_demand().unwrap();
        assert_ne!(first.resource_id, second.resource_id);
        assert!(panel.stage_resource_response(&second, metadata_response(&second), None));
        assert!(panel.resource_candidate_current(&second));
        panel.discard_staged_activation();
        let revision = panel.snapshot().presentation_revision;
        for kind in [
            DemandFactKind::VisibleRange { range: 0..0 },
            DemandFactKind::OverscanRange { range: 0..0 },
            DemandFactKind::CurrentAnchor {
                record_id: None,
                position: None,
            },
        ] {
            panel.host.push_demand_fact(DemandFact::new(revision, kind));
        }
        panel.prune_rejected_resources();
        assert!(panel.rejected_resource.is_empty());
    });
}

#[gpui::test]
fn automatic_nontext_demand_does_not_create_permanent_media_pins(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Image));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        assert!(!panel.host.drain_demand_facts().iter().any(|fact| matches!(fact.kind, DemandFactKind::MediaPreviewPin { .. })));
        let frame = panel.last_frame_window.as_ref().unwrap();
        assert!(!frame.records.is_empty());
        assert!(panel.stage_resource_response(&demand,
            TranscriptProviderResponseKind::Rejected(super::super::super::TranscriptProviderRejection {
                target: super::super::super::TranscriptProviderTarget::Resource(demand.resource_id.clone()),
                reason: super::super::super::TranscriptProviderRejectionReason::UnsupportedResourceKind,
                revision: None, message: None,
            }), None));
    });
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                let demand = panel.staged.as_ref().unwrap().resource.clone().unwrap();
                assert!(panel.measurement_current(window));
                panel.publish_resource_candidate(&demand);
                assert!(panel.resource_demand().is_none());
                assert_eq!(panel.snapshot().records[0].id, demand.record_id);
                let revision = panel.snapshot().presentation_revision;
                for kind in [
                    DemandFactKind::VisibleRange { range: 0..0 },
                    DemandFactKind::OverscanRange { range: 0..0 },
                    DemandFactKind::CurrentAnchor {
                        record_id: None,
                        position: None,
                    },
                ] {
                    panel.host.push_demand_fact(DemandFact::new(revision, kind));
                }
                assert!(panel.host.resource_demands().is_empty());
                assert!(panel.host.adjacent_demand().is_none());
            })
        })
        .unwrap();
}

#[gpui::test]
fn resource_pin_changes_revoke_candidate_even_when_its_record_is_already_visible(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Code));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        assert!(panel.stage_resource_response(&demand, metadata_response(&demand), None));
        panel.host.push_demand_fact(DemandFact::new(
            demand.presentation_revision,
            DemandFactKind::OpenMenuPin {
                record_id: demand.record_id.clone(),
            },
        ));
        let current = panel.resource_demand().unwrap();
        assert_eq!(current.retained_records, demand.retained_records);
        assert_ne!(current.pins, demand.pins);
        assert!(!panel.resource_candidate_current(&demand));
        panel.discard_staged_activation();
        assert!(panel.snapshot().resources.metadata.is_empty());
    });
}
