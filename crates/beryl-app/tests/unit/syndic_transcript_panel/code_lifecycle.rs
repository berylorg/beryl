use super::*;
use crate::transcript_provider::ResourceDeliveryResponse;
use std::rc::Rc;
use std::sync::Arc;

#[path = "code_wheel.rs"]
mod code_wheel;

#[path = "code_ownership.rs"]
mod code_ownership;

#[path = "code_revisions.rs"]
mod code_revisions;

#[path = "code_continuations.rs"]
mod code_continuations;

#[path = "code_capacity.rs"]
mod code_capacity;

fn activation() -> PreparedTranscriptActivation {
    let mut activation = resource_seed(1, ResourceKind::Code);
    let resource_id = ResourceId(binding(900).resource.id.to_string());
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        activation.projection_records_response.as_mut()
    else {
        unreachable!()
    };
    let projection = &mut projections.records[0];
    projection.payload = ProjectionPayload::ResourceReference {
        resource_id: resource_id.clone(),
        resource_kind: ResourceKind::Code,
        label: Some("Code".into()),
    };
    projection.provenance.resource_id = Some(resource_id);
    projection.provenance.item_id = Some(crate::syndic_transcript::provider::SyndicItemId(
        "saved-code".into(),
    ));
    projection.provenance.source_range = Some(0..10000);
    activation
}

fn publish_metadata(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<NestedPanel>,
    panel: &Entity<SyndicTranscriptPanel>,
) {
    let demand = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
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
        demand
    });
    publish_candidate(cx, window, &demand);
}

fn publish_candidate(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<NestedPanel>,
    demand: &super::super::super::super::super::ResourceTranscriptDemand,
) {
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                assert!(panel.measurement_current(window));
                panel.publish_resource_candidate(demand);
            });
        })
        .unwrap();
    draw_frames(cx, window, 5);
}

fn publish_extent(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<NestedPanel>,
    panel: &Entity<SyndicTranscriptPanel>,
    lines: u64,
) {
    panel.update(cx, |panel, _| {
        let resource = &panel.code_panels[0].viewport.owner.resource_id;
        let owner = panel.host.indexed_resource_owner(resource).unwrap();
        assert!(panel.host.set_indexed_resource_demand(&owner, None));
    });
    draw_frames(cx, window, 5);
    let demand = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        let inner = demand.indexed.as_ref().unwrap();
        let IndexedResourceQuery::Range { request, .. } = inner.query else {
            panic!("expected a Code range demand")
        };
        let binding = binding(inner.revision.get());
        let root = root(&binding, lines);
        let response = IndexedResourceResponse {
            value: IndexedResourceValue::Range(index::Response {
                binding,
                cursor: index::Cursor {
                    build: root.key,
                    root_digest: root.digest,
                    request,
                    next: index::Position::Code(lines),
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
        demand
    });
    publish_candidate(cx, window, &demand);
}

fn mounted_code(
    cx: &mut TestAppContext,
) -> (
    gpui::WindowHandle<NestedPanel>,
    Entity<SyndicTranscriptPanel>,
) {
    let window = mounted_panel(cx, activation());
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    publish_metadata(cx, window, &panel);
    publish_extent(cx, window, &panel, 2000);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.code_panels.len(), 1);
        assert!(panel.code_panels[0].viewport.scroll.borrow().overflow() > 0.0);
    });
    (window, panel)
}

fn select_code(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<NestedPanel>,
    panel: &Entity<SyndicTranscriptPanel>,
) {
    let position = panel.read_with(cx, |panel, _| {
        panel.code_panels[0]
            .viewport
            .scroll
            .borrow()
            .bounds
            .center()
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
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
        assert_eq!(panel.code_wheel_owner.get(), Some(panel.code_panels[0].id));
    });
}

#[gpui::test]
fn code_overflow_loss_releases_wheel_without_reacquiring_on_growth(cx: &mut TestAppContext) {
    let (window, panel) = mounted_code(cx);
    select_code(cx, window, &panel);
    let (id, scroll, outer) = panel.update(cx, |panel, _| {
        let mounted = &panel.code_panels[0];
        mounted.viewport.scroll.borrow_mut().set_offset(1007.5);
        (
            mounted.id,
            Rc::downgrade(&mounted.viewport.scroll),
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
        )
    });
    publish_extent(cx, window, &panel, 3);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.code_panels[0].id, id);
        assert!(Rc::ptr_eq(
            &panel.code_panels[0].viewport.scroll,
            &scroll.upgrade().unwrap(),
        ));
        assert_eq!(panel.code_wheel_owner.get(), None);
        assert_eq!(
            panel.code_panels[0].viewport.scroll.borrow().overflow(),
            0.0
        );
    });
    publish_extent(cx, window, &panel, 2000);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.code_panels[0].id, id);
        assert_eq!(panel.code_wheel_owner.get(), None);
        let scroll = panel.code_panels[0].viewport.scroll.borrow();
        assert!(scroll.overflow() > 0.0);
        assert_eq!(scroll.top_line, 0);
        assert_eq!(scroll.remainder, 0.0);
        assert_eq!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
            outer
        );
    });
}

#[gpui::test]
fn retiring_code_releases_scroll_model_and_indexed_demand(cx: &mut TestAppContext) {
    let (window, panel) = mounted_code(cx);
    select_code(cx, window, &panel);
    let scroll = panel.update(cx, |panel, _| {
        let mounted = &panel.code_panels[0];
        let scroll = Rc::downgrade(&mounted.viewport.scroll);
        panel.retire();
        assert!(panel.code_panels.is_empty());
        assert_eq!(panel.code_wheel_owner.get(), None);
        assert!(panel.resource_demand().is_none());
        scroll
    });
    draw_frames(cx, window, 5);
    panel.read_with(cx, |panel, _| {
        assert!(panel.retired_code_panels.is_empty());
        assert!(panel.host.code_panel_demands().is_empty());
        assert!(panel.diagnostic_snapshot().code_panels.is_empty());
    });
    assert!(scroll.upgrade().is_none());
}

#[gpui::test]
fn replacing_activation_releases_code_and_does_not_restore_wheel_selection(
    cx: &mut TestAppContext,
) {
    let (window, panel) = mounted_code(cx);
    select_code(cx, window, &panel);
    let (id, scroll) = panel.update(cx, |panel, _| {
        let mounted = &panel.code_panels[0];
        let retained = (mounted.id, Rc::downgrade(&mounted.viewport.scroll));
        let mut replacement = activation();
        let view_id = TranscriptViewId("replacement-code-view".into());
        replacement.view_id = Some(view_id.clone());
        let Some(TranscriptProviderResponseKind::ViewPage(page)) =
            replacement.view_page_response.as_mut()
        else {
            unreachable!()
        };
        page.view_id = view_id.clone();
        for record in &mut page.records {
            record.provenance.view_id = view_id.clone();
        }
        let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
            replacement.projection_records_response.as_mut()
        else {
            unreachable!()
        };
        projections.view_id = view_id.clone();
        for record in &mut projections.records {
            record.provenance.view_id = view_id.clone();
        }
        panel.stage_activation(&replacement, None);
        retained
    });
    draw_frames(cx, window, 5);
    assert!(scroll.upgrade().is_none());
    publish_metadata(cx, window, &panel);
    publish_extent(cx, window, &panel, 2000);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.code_panels.len(), 1);
        assert_ne!(panel.code_panels[0].id, id);
        assert_eq!(panel.code_wheel_owner.get(), None);
        assert_eq!(panel.code_panels[0].viewport.scroll.borrow().top_line, 0);
    });
}
