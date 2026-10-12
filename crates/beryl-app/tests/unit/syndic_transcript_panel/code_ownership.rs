use super::*;

fn mounted_pair(
    cx: &mut TestAppContext,
) -> (
    gpui::WindowHandle<NestedPanel>,
    Entity<SyndicTranscriptPanel>,
) {
    let bindings = [
        binding_with_resource(900, 1, [11; 16]),
        binding_with_resource(900, 1, [12; 16]),
    ];
    let mut activation = resource_seed(2, ResourceKind::Code);
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        activation.projection_records_response.as_mut()
    else {
        unreachable!()
    };
    for (projection, binding) in projections.records.iter_mut().zip(&bindings) {
        let resource_id = ResourceId(binding.resource.id.to_string());
        projection.payload = ProjectionPayload::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind: ResourceKind::Code,
            label: Some("Code".into()),
        };
        projection.provenance.resource_id = Some(resource_id);
        projection.provenance.source_range = Some(0..10000);
    }
    let window = mounted_panel_with_initial_layout(cx, activation, 620.0, 0.0);
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    for _ in 0..4 {
        let demand = panel.update(cx, |panel, _| {
            let demand = panel.resource_demand().unwrap();
            if let Some(inner) = &demand.indexed {
                let IndexedResourceQuery::Range { request, .. } = inner.query else {
                    panic!("expected initial Code range")
                };
                let binding = bindings
                    .iter()
                    .find(|binding| binding.resource.id.to_string() == demand.resource_id.0)
                    .unwrap()
                    .clone();
                let root = root(&binding, 2000);
                let response = IndexedResourceResponse {
                    value: IndexedResourceValue::Range(index::Response {
                        binding,
                        cursor: index::Cursor {
                            build: root.key,
                            root_digest: root.digest,
                            request,
                            next: index::Position::Code(2000),
                            fragment_offset: 0,
                        },
                        items: Vec::new(),
                        request_complete: true,
                        resource_complete: true,
                        encoded_bytes: 1,
                        presentations: None,
                    }),
                    root: Some(root),
                    encoded_bytes: 1,
                };
                assert!(panel.stage_resource_delivery(
                    &demand,
                    ResourceDeliveryResponse::Indexed(Arc::new(response)),
                    None,
                ));
            } else {
                let TranscriptProviderResponseKind::ResourceMetadata(mut metadata) =
                    metadata_response(&demand)
                else {
                    unreachable!()
                };
                metadata.byte_len = 10000;
                metadata.line_count = Some(2000);
                assert!(panel.stage_resource_response(
                    &demand,
                    TranscriptProviderResponseKind::ResourceMetadata(metadata),
                    None,
                ));
            }
            demand
        });
        publish_candidate(cx, window, &demand);
    }
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.code_panels.len(), 2);
        assert!(
            panel
                .code_panels
                .iter()
                .all(|mounted| mounted.viewport.scroll.borrow().overflow() > 0.0)
        );
        assert_ne!(panel.code_panels[0].id, panel.code_panels[1].id);
        assert_eq!(panel.code_wheel_owner.get(), None);
    });
    (window, panel)
}

fn press_panel(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<NestedPanel>,
    panel: &Entity<SyndicTranscriptPanel>,
    ordinal: usize,
) -> gpui::Point<gpui::Pixels> {
    let position = panel.read_with(cx, |panel, _| {
        panel.code_panels[ordinal]
            .viewport
            .scroll
            .borrow()
            .bounds
            .center()
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let outer = visual.debug_bounds("nested-transcript-body").unwrap();
    assert!(outer.contains(&position));
    visual.simulate_event(gpui::MouseDownEvent {
        button: MouseButton::Left,
        position,
        click_count: 1,
        ..Default::default()
    });
    visual.simulate_event(gpui::MouseUpEvent {
        button: MouseButton::Left,
        position,
        click_count: 1,
        ..Default::default()
    });
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel.code_wheel_owner.get(),
            Some(panel.code_panels[ordinal].id)
        );
    });
    position
}

#[gpui::test]
fn primary_press_transfers_code_wheel_owner_and_only_selected_panel_scrolls(
    cx: &mut TestAppContext,
) {
    let (window, panel) = mounted_pair(cx);
    let outer = code_wheel::outer_offset(cx, &panel);
    let first = press_panel(cx, window, &panel, 0);
    code_wheel::wheel(cx, window, first, 0.0, -25.5);
    let second = press_panel(cx, window, &panel, 1);
    code_wheel::wheel(cx, window, second, 0.0, -45.5);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.code_panels[0].viewport.scroll.borrow().offset(), 25.5);
        assert_eq!(panel.code_panels[1].viewport.scroll.borrow().offset(), 45.5);
        let diagnostics = panel.diagnostic_snapshot();
        assert_eq!(
            diagnostics
                .code_panels
                .iter()
                .filter(|facts| facts.selected_for_wheel)
                .count(),
            1
        );
    });
    assert_eq!(code_wheel::outer_offset(cx, &panel), outer);
    press_panel(cx, window, &panel, 0);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.code_panels[0].viewport.scroll.borrow().offset(), 25.5);
        assert_eq!(panel.code_panels[1].viewport.scroll.borrow().offset(), 45.5);
    });
}

#[gpui::test]
fn retiring_prior_code_owner_preserves_current_owner_and_retiring_current_releases_it(
    cx: &mut TestAppContext,
) {
    let (window, panel) = mounted_pair(cx);
    press_panel(cx, window, &panel, 0);
    press_panel(cx, window, &panel, 1);
    let selected = panel.read_with(cx, |panel, _| panel.code_panels[1].id);
    for expected in [Some(selected), None] {
        let scroll = panel.update(cx, |panel, _| {
            let mounted = &panel.code_panels[0];
            let scroll = Rc::downgrade(&mounted.viewport.scroll);
            let owner = panel
                .host
                .indexed_resource_owner(&mounted.viewport.owner.resource_id)
                .unwrap();
            let query = mounted.viewport.query();
            assert!(panel.set_indexed_resource_demand(&owner, Some(query)));
            scroll
        });
        draw_frames(cx, window, 5);
        panel.read_with(cx, |panel, _| {
            assert_eq!(panel.code_wheel_owner.get(), expected)
        });
        assert!(scroll.upgrade().is_none());
    }
    panel.read_with(cx, |panel, _| assert!(panel.code_panels.is_empty()));
}
