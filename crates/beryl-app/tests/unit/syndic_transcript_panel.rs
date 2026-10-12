use super::super::{
    ProjectionPayload, ProjectionRecord, ProjectionRecordId, ProjectionRecordKind,
    ProjectionRecordSet, ProviderRevision, SyndicSourceProvenance, TranscriptNarrativeKind,
    TranscriptProviderHistoryState, TranscriptProviderResponseKind, TranscriptViewId,
    TranscriptViewPage, TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId,
};
use super::*;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};

#[path = "syndic_transcript_panel/copy_keyboard.rs"]
mod copy_keyboard_tests;

#[path = "syndic_transcript_panel/resources.rs"]
mod resources;

#[path = "syndic_transcript_panel/code_viewport.rs"]
mod code_viewport_tests;

#[path = "syndic_transcript_panel/code_appearance.rs"]
mod code_appearance_tests;

#[path = "syndic_transcript_panel/code_layout.rs"]
mod code_layout_tests;

struct NestedPanel {
    panel: Entity<SyndicTranscriptPanel>,
    body_height: f32,
}

fn mounted_panel(
    cx: &mut TestAppContext,
    activation: PreparedTranscriptActivation,
) -> gpui::WindowHandle<NestedPanel> {
    mounted_panel_with_initial_layout(cx, activation, 180.0, 0.0)
}

fn mounted_panel_with_initial_layout(
    cx: &mut TestAppContext,
    activation: PreparedTranscriptActivation,
    body_height: f32,
    delta: f32,
) -> gpui::WindowHandle<NestedPanel> {
    cx.update(|app| {
        app.open_window(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                    None,
                    gpui::size(px(640.0), px(640.0)),
                    app,
                ))),
                ..Default::default()
            },
            |_, app| {
                let panel = app.new(|cx| {
                    let mut panel = SyndicTranscriptPanel::new(cx);
                    panel.stage_activation(&activation, None);
                    panel.manual_scroll_delta(body_height, delta, cx);
                    panel
                });
                app.new(|_| NestedPanel { panel, body_height })
            },
        )
        .unwrap()
    })
}

fn draw_frames(cx: &mut TestAppContext, window: gpui::WindowHandle<NestedPanel>, count: usize) {
    cx.update(|app| {
        for _ in 0..count {
            app.update_window(window.into(), |_, window, app| window.draw(app).clear())
                .unwrap();
        }
    });
}

fn long_tail_seed() -> PreparedTranscriptActivation {
    let mut activation = seed(64);
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        activation.projection_records_response.as_mut()
    else {
        unreachable!()
    };
    projections.records.last_mut().unwrap().payload = ProjectionPayload::Text {
        plain_text: None,
        text: "Long transcript paragraph ".repeat(500).into(),
    };
    activation
}

#[gpui::test]
fn manual_input_before_first_content_uses_the_measured_tail_start(cx: &mut TestAppContext) {
    let window = mounted_panel_with_initial_layout(cx, long_tail_seed(), 180.0, -63.25);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    draw_frames(cx, window, 4);
    panel.read_with(cx, |panel, _| {
        let frame = panel.last_frame_window.as_ref().unwrap();
        let last = frame.records.last().unwrap();
        assert_eq!(last.index, 63);
        assert_eq!(frame.manual_scroll_total_px, -63.25);
        assert!((last.top_px + last.height_px - 243.25).abs() < 0.01);
        assert!(matches!(
            panel.refresh_placement(),
            TranscriptActivationPlacement::Position(_)
        ));
    });
}

#[gpui::test]
fn shrinking_last_record_without_input_qualifies_the_revealed_clamp_rows(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, long_tail_seed());
    draw_frames(cx, window, 4);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let delta = panel.read_with(cx, |panel, _| {
        panel
            .last_frame_window
            .as_ref()
            .unwrap()
            .records
            .last()
            .unwrap()
            .top_px
            + 1000.0
    });
    panel.update(cx, |panel, cx| panel.manual_scroll_delta(180.0, delta, cx));
    draw_frames(cx, window, 4);
    panel.read_with(cx, |panel, _| {
        assert!(
            (panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .records
                .last()
                .unwrap()
                .top_px
                + 1000.0)
                .abs()
                < 0.01
        )
    });
    panel.update(cx, |panel, cx| {
        panel.stage_activation(&seed(64), None);
        cx.notify();
    });
    draw_frames(cx, window, 8);
    panel.read_with(cx, |panel, _| {
        let frame = panel.last_frame_window.as_ref().unwrap();
        assert!(!frame.records.is_empty());
        let last = frame.records.last().unwrap();
        assert!((last.top_px + last.height_px - 180.0).abs() < 0.01);
        assert!(frame.records.iter().all(|record| record.height_px == 36.0));
        assert_eq!(frame.manual_scroll_total_px, delta);
    });
}

#[gpui::test]
fn measured_scroll_keeps_coherent_output_until_bounded_batches_complete(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, seed(64));
    draw_frames(cx, window, 4);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let before = panel.read_with(cx, |panel, _| panel.last_frame_window.clone().unwrap());
    panel.update(cx, |panel, cx| {
        panel.manual_scroll_delta(180.0, -2000.25, cx)
    });
    draw_frames(cx, window, 1);
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel.last_frame_window.as_ref().unwrap().anchor,
            before.anchor
        );
        assert_eq!(panel.measurement_diagnostics().2, 32);
        assert!(panel.measurement_diagnostics().0);
        assert!(matches!(
            panel.refresh_placement(),
            TranscriptActivationPlacement::Position(_)
        ));
    });
    draw_frames(cx, window, 8);
    panel.read_with(cx, |panel, _| {
        let frame = panel.last_frame_window.as_ref().unwrap();
        assert_eq!(frame.manual_scroll_total_px, -2000.25);
        assert_ne!(frame.anchor, before.anchor);
        assert!(!frame.records.is_empty());
        assert!(panel.measurement_diagnostics().2 <= 32);
    });
    panel.update(cx, |panel, cx| {
        panel.stage_activation(&seed(65), None);
        panel.manual_scroll_delta(180.0, -63.25, cx);
    });
    draw_frames(cx, window, 8);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.snapshot().records.len(), 65);
        assert_eq!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
            -2063.5
        );
    });
}

#[gpui::test]
fn cancelled_replacement_retires_input_and_preparation_ownership(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, seed(64));
    draw_frames(cx, window, 4);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let original = panel.read_with(cx, |panel, _| panel.snapshot());
    let mut replacement = seed(65);
    replacement.view_id = Some(TranscriptViewId("replacement-view".into()));
    panel.update(cx, |panel, cx| {
        panel.stage_activation(&replacement, None);
        panel.manual_scroll_delta(180.0, -90.25, cx);
        panel.discard_staged_activation();
    });
    draw_frames(cx, window, 4);
    panel.read_with(cx, |panel, _| {
        assert!(std::sync::Arc::ptr_eq(&panel.snapshot(), &original));
        assert_eq!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
            0.0
        );
    });
    let weak = std::sync::Arc::downgrade(&original);
    drop(original);
    panel.update(cx, |panel, _| panel.retire());
    assert!(weak.upgrade().is_none());
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.measurement_diagnostics(), (false, 0, 0))
    });
}

#[gpui::test]
fn variable_rows_remeasure_on_resize_and_route_real_wheel_and_keyboard(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, long_tail_seed());
    draw_frames(cx, window, 4);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let previous = panel.read_with(cx, |panel, _| {
        panel
            .last_frame_window
            .as_ref()
            .unwrap()
            .records
            .last()
            .unwrap()
            .height_px
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(320.0), px(640.0)));
    draw_frames(cx, window, 4);
    panel.read_with(cx, |panel, _| {
        let frame = panel.last_frame_window.as_ref().unwrap();
        let last = frame.records.last().unwrap();
        assert!(last.height_px > previous);
        assert!((last.top_px + last.height_px - 180.0).abs() < 0.01);
    });
    let bounds = visual.debug_bounds("nested-transcript-body").unwrap();
    let previous = panel.read_with(cx, |panel, _| {
        panel
            .last_frame_window
            .as_ref()
            .unwrap()
            .records
            .last()
            .unwrap()
            .height_px
    });
    visual.update(|window, _| window.set_rem_size(px(24.0)));
    draw_frames(cx, window, 4);
    panel.read_with(cx, |panel, _| {
        assert!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .records
                .last()
                .unwrap()
                .height_px
                > previous
        )
    });
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: bounds.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(83.25))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    draw_frames(cx, window, 4);
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
            -83.25
        )
    });
    visual.simulate_event(gpui::MouseDownEvent {
        button: MouseButton::Left,
        position: bounds.center(),
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    });
    visual.simulate_keystrokes("up");
    draw_frames(cx, window, 4);
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
            -123.25
        )
    });
}

#[gpui::test]
fn first_content_waits_for_the_complete_measured_tail_frame(cx: &mut TestAppContext) {
    let window = mounted_panel_with_initial_layout(cx, seed(64), 2000.0, 0.0);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    draw_frames(cx, window, 1);
    panel.read_with(cx, |panel, _| {
        assert!(panel.snapshot().records.is_empty());
        assert!(panel.last_frame_window.is_none());
        assert_eq!(panel.measurement_diagnostics().2, 32);
    });
    draw_frames(cx, window, 8);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.snapshot().records.len(), 64);
        let frame = panel.last_frame_window.as_ref().unwrap();
        let last = frame.records.last().unwrap();
        assert!((last.top_px + last.height_px - 2000.0).abs() < 0.01);
        assert!(frame.records.len() > 32);
        assert!(panel.measurement_diagnostics().2 <= 32);
    });
}

impl Render for NestedPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.panel.update(cx, |panel, _| {
            if panel.measurement_current(window) {
                if let Some(activation) = panel.staged.as_ref().unwrap().activation.clone() {
                    panel.publish_coherent_activation(activation);
                }
            }
        });
        div().size_full().flex().flex_col().child(
            div()
                .id("nested-transcript-body")
                .debug_selector(|| "nested-transcript-body".into())
                .w_full()
                .h(px(self.body_height))
                .flex_shrink_0()
                .child(self.panel.clone()),
        )
    }
}

fn page_seed(range: std::ops::Range<u64>, end: u64) -> PreparedTranscriptActivation {
    let mut activation = seed(end);
    let Some(TranscriptProviderResponseKind::ViewPage(page)) = &mut activation.view_page_response
    else {
        unreachable!()
    };
    page.records
        .retain(|record| range.contains(&record.position.0));
    page.at_start = range.start == 1;
    page.at_end = range.end == end;
    let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
        &mut activation.projection_records_response
    else {
        unreachable!()
    };
    projections
        .records
        .retain(|record| range.contains(&record.provenance.position.unwrap().0));
    activation
}

#[gpui::test]
fn measured_adjacent_prepend_preserves_anchor_and_prior_snapshot(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, page_seed(65..129, 129));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, cx| {
        panel.manual_scroll_delta(180.0, -100_000.25, cx)
    });
    draw_frames(cx, window, 5);
    let (demand, anchor, prior) = panel.read_with(cx, |panel, _| {
        (
            panel.adjacent_demand().unwrap(),
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .anchor
                .clone()
                .unwrap(),
            panel.snapshot(),
        )
    });
    assert_eq!(demand.range, 1..65);
    assert_eq!(
        demand,
        panel.read_with(cx, |panel, _| panel.adjacent_demand().unwrap())
    );
    let extension = page_seed(1..65, 129);
    panel.update(cx, |panel, cx| {
        assert!(panel.stage_adjacent_extension(&demand, &extension));
        assert!(std::sync::Arc::ptr_eq(&panel.snapshot(), &prior));
        cx.notify();
    });
    draw_frames(cx, window, 5);
    panel.read_with(cx, |panel, _| {
        let current = panel
            .last_frame_window
            .as_ref()
            .unwrap()
            .anchor
            .as_ref()
            .unwrap();
        assert_eq!(current.record_id, anchor.record_id);
        assert_eq!(current.viewport_y_px, anchor.viewport_y_px);
        assert_eq!(
            panel
                .last_frame_window
                .as_ref()
                .unwrap()
                .manual_scroll_total_px,
            -100_000.25
        );
        assert!(panel.snapshot().records.len() > 64);
        assert!(
            !panel
                .snapshot()
                .records
                .iter()
                .any(|record| record.id == prior.records.last().unwrap().id)
        );
        assert!(
            panel
                .snapshot()
                .records
                .iter()
                .any(|record| record.id == anchor.record_id)
        );
        assert_eq!(prior.records.len(), 64);
        assert!(panel.adjacent_demand().is_none());
    });
}

#[gpui::test]
fn input_supersedes_measured_adjacent_candidate(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, page_seed(65..129, 129));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, cx| {
        panel.manual_scroll_delta(180.0, -100_000.0, cx)
    });
    draw_frames(cx, window, 5);
    let demand = panel.read_with(cx, |panel, _| panel.adjacent_demand().unwrap());
    panel.update(cx, |panel, cx| {
        assert!(panel.stage_adjacent_extension(&demand, &page_seed(1..65, 129)));
        panel.manual_scroll_delta(180.0, 12.25, cx);
        assert!(panel.staged.is_none());
        assert!(panel.adjacent_demand().is_none());
        assert!(!panel.stage_adjacent_extension(&demand, &page_seed(1..65, 129)));
    });
    draw_frames(cx, window, 5);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.snapshot().records.len(), 64)
    });
}

fn seed(record_count: u64) -> PreparedTranscriptActivation {
    let view_id = TranscriptViewId("nested-panel".into());
    let revision = ProviderRevision(record_count);
    let mut views = Vec::new();
    let mut projections = Vec::new();
    for position in 0..record_count {
        let projection_id = ProjectionRecordId(format!("text-{position}"));
        let provenance = SyndicSourceProvenance {
            view_id: view_id.clone(),
            position: Some(TranscriptViewPosition(position)),
            turn_id: None,
            item_id: None,
            block_id: None,
            projection_id: Some(projection_id.clone()),
            resource_id: None,
            source_range: None,
            resource_range: None,
            copy_source_range: None,
        };
        views.push(TranscriptViewRecord {
            id: TranscriptViewRecordId(format!("entry-{position}")),
            position: TranscriptViewPosition(position),
            projection_id: projection_id.clone(),
            narrative_kind: TranscriptNarrativeKind::AssistantText,
            provenance: provenance.clone(),
        });
        projections.push(ProjectionRecord {
            id: projection_id,
            revision,
            kind: ProjectionRecordKind::TextChunk,
            payload: ProjectionPayload::Text {
                plain_text: None,
                text: format!("authored row {position}").into(),
            },
            provenance,
        });
    }
    PreparedTranscriptActivation::new(
        view_id.clone(),
        TranscriptActivationPlacement::Tail,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: view_id.clone(),
            revision,
            history_state: TranscriptProviderHistoryState::Complete,
            records: views,
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        Some(TranscriptProviderResponseKind::ProjectionRecords(
            ProjectionRecordSet {
                view_id,
                revision,
                records: projections,
                rejections: Vec::new(),
            },
        )),
    )
}

#[gpui::test]
fn adjacent_allocation_and_pin_changes_revoke_prepared_geometry(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, page_seed(65..129, 129));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, cx| {
        panel.manual_scroll_delta(180.0, -100_000.0, cx)
    });
    draw_frames(cx, window, 5);
    let demand = panel.read_with(cx, |panel, _| panel.adjacent_demand().unwrap());
    let extension = page_seed(1..65, 129);
    panel.update(cx, |panel, _| {
        assert!(panel.stage_adjacent_extension(&demand, &extension));
        panel.stage_activation(&extension, Some(gpui::size(px(640.0), px(240.0))));
        assert!(!panel.measurement_ready());
        assert!(panel.adjacent_candidate_current(&demand));
        let pinned = panel.snapshot().records.last().unwrap().id.clone();
        panel.host.push_demand_fact(DemandFact::new(
            demand.presentation_revision,
            DemandFactKind::OpenMenuPin { record_id: pinned },
        ));
        assert!(!panel.adjacent_candidate_current(&demand));
        panel.discard_staged_activation();
        assert!(!panel.adjacent_candidate_current(&demand));
        panel.retire();
        assert!(panel.adjacent_demand().is_none());
    });
}

#[gpui::test]
fn nested_body_paints_newest_tail_on_first_frame_and_preserves_manual_anchor(
    cx: &mut TestAppContext,
) {
    let window = cx.update(|app| {
        app.open_window(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                    None,
                    gpui::size(px(640.), px(640.)),
                    app,
                ))),
                ..Default::default()
            },
            |_, app| {
                let panel = app.new(|cx| {
                    let mut panel = SyndicTranscriptPanel::new(cx);
                    panel.stage_activation(&seed(24), None);
                    panel
                });
                app.new(|_| NestedPanel {
                    panel,
                    body_height: 180.,
                })
            },
        )
        .unwrap()
    });
    cx.update(|app| {
        for _ in 0..4 {
            app.update_window(window.into(), |_, window, app| window.draw(app).clear())
                .unwrap();
        }
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let body = visual.debug_bounds("nested-transcript-body").unwrap();
    assert_eq!(body.size.height, px(180.));
    let panel = window.update(cx, |view, _, _| view.panel.clone()).unwrap();
    let row = visual
        .debug_bounds("syndic-transcript-record:view:entry-23:projection:text-23")
        .unwrap();
    assert!(row.top() >= body.top());
    assert_eq!(row.bottom(), body.bottom());

    window
        .update(cx, |view, _, cx| {
            view.body_height = 144.;
            cx.notify();
        })
        .unwrap();
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| window.draw(app).clear())
            .unwrap();
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let body = visual.debug_bounds("nested-transcript-body").unwrap();
    let row = visual
        .debug_bounds("syndic-transcript-record:view:entry-23:projection:text-23")
        .unwrap();
    assert_eq!(row.bottom(), body.bottom());
    assert_eq!(body.size.height, px(144.));

    panel.update(cx, |panel, cx| {
        panel.manual_scroll_delta(144., -144., cx);
    });
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| window.draw(app).clear())
            .unwrap();
    });
    let anchor = panel.read_with(cx, |panel, _| {
        panel
            .last_frame_window
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap()
    });
    panel.update(cx, |panel, cx| {
        panel.stage_activation(&seed(25), None);
        cx.notify();
    });
    window
        .update(cx, |view, _, cx| {
            view.body_height = 216.;
            cx.notify();
        })
        .unwrap();
    cx.update(|app| {
        for _ in 0..4 {
            app.update_window(window.into(), |_, window, app| window.draw(app).clear())
                .unwrap();
        }
    });
    panel.read_with(cx, |panel, _| {
        let current = panel
            .last_frame_window
            .as_ref()
            .unwrap()
            .anchor
            .as_ref()
            .unwrap();
        assert_eq!(current.record_id, anchor.record_id);
        assert_eq!(current.viewport_y_px, anchor.viewport_y_px);
        assert!(matches!(
            panel.refresh_placement(),
            TranscriptActivationPlacement::Position(_)
        ));
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let body = visual.debug_bounds("nested-transcript-body").unwrap();
    assert_eq!(anchor.index, 16);
    let anchor_row = visual
        .debug_bounds("syndic-transcript-record:view:entry-16:projection:text-16")
        .unwrap();
    assert_eq!(anchor_row.top(), body.top() + px(anchor.viewport_y_px));
}
