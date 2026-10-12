use super::*;

#[gpui::test]
fn replacing_code_resource_in_the_same_record_retires_its_scroll_model(cx: &mut TestAppContext) {
    let (window, panel) = mounted_code(cx);
    select_code(cx, window, &panel);
    let old_scroll = panel.update(cx, |panel, _| {
        let old_scroll = Rc::downgrade(&panel.code_panels[0].viewport.scroll);
        let mut replacement = activation();
        let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
            replacement.projection_records_response.as_mut()
        else {
            unreachable!()
        };
        let resource_id = ResourceId(
            binding_with_resource(900, 1, [12; 16])
                .resource
                .id
                .to_string(),
        );
        let projection = &mut projections.records[0];
        projection.payload = ProjectionPayload::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind: ResourceKind::Code,
            label: Some("Code".into()),
        };
        projection.provenance.resource_id = Some(resource_id);
        panel.stage_activation(&replacement, None);
        old_scroll
    });
    draw_frames(cx, window, 5);
    assert!(old_scroll.upgrade().is_none());
    panel.read_with(cx, |panel, _| {
        assert!(panel.code_panels.is_empty());
        assert_eq!(panel.code_wheel_owner.get(), None);
    });
}

#[gpui::test]
fn measured_code_revision_preserves_mount_and_restores_its_source_anchor(cx: &mut TestAppContext) {
    code_revision_continuity(cx, false);
}

#[gpui::test]
fn appended_code_snapshot_preserves_mount_and_restores_its_source_anchor(cx: &mut TestAppContext) {
    code_revision_continuity(cx, true);
}

fn code_revision_continuity(cx: &mut TestAppContext, replace_snapshot: bool) {
    let (window, panel) = mounted_code(cx);
    select_code(cx, window, &panel);
    panel.update(cx, |panel, cx| {
        let mut scroll = panel.code_panels[0].viewport.scroll.borrow_mut();
        scroll.set_offset(1007.5);
        scroll.scroll_horizontal_by(45.5);
        drop(scroll);
        cx.notify();
    });
    draw_frames(cx, window, 5);
    let old_demand = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        let mut response = range_response(binding(900), 50);
        response.encoded_bytes = 1;
        let IndexedResourceValue::Range(range) = &mut response.value else {
            unreachable!()
        };
        range.encoded_bytes = 1;
        assert!(panel.stage_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(Arc::new(response)),
            None,
        ));
        demand
    });
    publish_candidate(cx, window, &old_demand);
    let (id, scroll, diagnostic_key, activation_revision, outer) = panel.update(cx, |panel, _| {
        let mounted = &panel.code_panels[0];
        assert_eq!(mounted.viewport.anchor.unwrap().position, 250);
        let retained = (
            mounted.id,
            Rc::downgrade(&mounted.viewport.scroll),
            panel.diagnostic_snapshot().code_panels[0].key,
            panel.snapshot().activation_revision,
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
        );
        let mut refresh = activation();
        let Some(TranscriptProviderResponseKind::ViewPage(page)) =
            refresh.view_page_response.as_mut()
        else {
            unreachable!()
        };
        page.revision = ProviderRevision(901);
        if replace_snapshot {
            page.records[0].id = TranscriptViewRecordId("successor-entry".into());
            page.records[0].projection_id = ProjectionRecordId("successor-code".into());
            page.records[0].provenance.projection_id = Some(page.records[0].projection_id.clone());
        }
        let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
            refresh.projection_records_response.as_mut()
        else {
            unreachable!()
        };
        projections.revision = ProviderRevision(901);
        for record in &mut projections.records {
            record.revision = ProviderRevision(901);
            if replace_snapshot {
                record.id = ProjectionRecordId("successor-code".into());
                record.provenance.projection_id = Some(record.id.clone());
                record.provenance.source_range = Some(0..20000);
                let resource_id = ResourceId(
                    binding_with_resource(901, 1, [12; 16])
                        .resource
                        .id
                        .to_string(),
                );
                record.provenance.resource_id = Some(resource_id.clone());
                record.payload = ProjectionPayload::ResourceReference {
                    resource_id,
                    resource_kind: ResourceKind::Code,
                    label: Some("Code".into()),
                };
            }
        }
        panel.stage_activation(&refresh, None);
        retained
    });
    draw_frames(cx, window, 5);
    let metadata_demand = panel.update(cx, |panel, _| {
        assert!(panel.snapshot().activation_revision > activation_revision);
        assert_eq!(panel.code_panels[0].id, id);
        assert_eq!(panel.code_wheel_owner.get(), Some(id));
        assert!(Rc::ptr_eq(
            &scroll.upgrade().unwrap(),
            &panel.code_panels[0].viewport.scroll
        ));
        let mut stale = range_response(binding(900), 50);
        stale.encoded_bytes = 1;
        assert!(!panel.stage_resource_delivery(
            &old_demand,
            ResourceDeliveryResponse::Indexed(Arc::new(stale)),
            None,
        ));
        let demand = panel.resource_demand().unwrap();
        assert_eq!(demand.revision, ProviderRevision(901));
        assert!(demand.indexed.is_none());
        let TranscriptProviderResponseKind::ResourceMetadata(mut metadata) =
            metadata_response(&demand)
        else {
            unreachable!()
        };
        metadata.revision = ProviderRevision(901);
        metadata.byte_len = 10000;
        metadata.line_count = Some(2000);
        assert!(panel.stage_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(metadata),
            None,
        ));
        demand
    });
    publish_candidate(cx, window, &metadata_demand);
    let anchor_demand = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        let inner = demand.indexed.as_ref().unwrap();
        assert_eq!(inner.revision.get(), 901);
        assert_eq!(inner.root, None);
        assert!(
            matches!(inner.query, IndexedResourceQuery::Anchor { anchor, .. }
            if anchor.position == 250 && anchor.visual_offset == -8)
        );
        let mut response = successor(&panel.code_panels[0].viewport, 2000, Some(75));
        if replace_snapshot {
            let binding = narrative_binding(901, 1, [12; 16], 20000);
            response.root = Some(root(&binding, 2000));
            let IndexedResourceValue::Anchor(anchor) = &mut response.value else {
                unreachable!()
            };
            anchor.resolved.as_mut().unwrap().anchor.source = binding.source;
            anchor.binding = binding;
        }
        response.encoded_bytes = 1;
        let IndexedResourceValue::Anchor(anchor) = &mut response.value else {
            unreachable!()
        };
        anchor.encoded_bytes = 1;
        assert!(panel.stage_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(Arc::new(response)),
            None,
        ));
        demand
    });
    publish_candidate(cx, window, &anchor_demand);
    panel.read_with(cx, |panel, _| {
        let mounted = &panel.code_panels[0];
        assert_eq!(mounted.id, id);
        assert_eq!(panel.code_wheel_owner.get(), Some(id));
        assert!(Rc::ptr_eq(
            &scroll.upgrade().unwrap(),
            &mounted.viewport.scroll
        ));
        let scroll = mounted.viewport.scroll.borrow();
        assert_eq!(scroll.top_line, 75);
        assert_eq!(scroll.remainder, 7.5);
        assert_eq!(scroll.horizontal, 45.5);
        assert_eq!(
            panel.diagnostic_snapshot().code_panels[0].key,
            diagnostic_key
        );
        assert_eq!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
            outer
        );
        let demand = panel.resource_demand().unwrap();
        assert!(matches!(
            demand.indexed.unwrap().query,
            IndexedResourceQuery::Range {
                request: index::Request::Code {
                    lines: index::OrdinalRange { start: 73, end: 89 }
                },
                ..
            }
        ));
    });
}
