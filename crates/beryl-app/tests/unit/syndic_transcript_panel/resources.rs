use super::*;

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
