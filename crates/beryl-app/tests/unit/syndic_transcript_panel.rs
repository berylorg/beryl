use super::super::{
    ProjectionPayload, ProjectionRecord, ProjectionRecordId, ProjectionRecordKind,
    ProjectionRecordSet, ProviderRevision, SyndicSourceProvenance, TranscriptNarrativeKind,
    TranscriptProviderHistoryState, TranscriptProviderResponseKind, TranscriptViewId,
    TranscriptViewPage, TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId,
};
use super::*;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};

struct NestedPanel {
    panel: Entity<SyndicTranscriptPanel>,
    body_height: f32,
}

impl Render for NestedPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
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
                    panel.publish_coherent_activation(seed(8));
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
        app.update_window(window.into(), |_, window, app| window.draw(app).clear())
            .unwrap();
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let body = visual.debug_bounds("nested-transcript-body").unwrap();
    assert_eq!(body.size.height, px(180.));
    let panel = window.update(cx, |view, _, _| view.panel.clone()).unwrap();
    let row = visual
        .debug_bounds("syndic-transcript-record:view:entry-7:projection:text-7")
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
        .debug_bounds("syndic-transcript-record:view:entry-7:projection:text-7")
        .unwrap();
    assert_eq!(row.bottom(), body.bottom());
    assert_eq!(body.size.height, px(144.));

    let anchor = panel.update(cx, |panel, cx| {
        panel.manual_scroll_delta(144., -144., cx).anchor.unwrap()
    });
    panel.update(cx, |panel, cx| {
        panel.publish_coherent_activation(seed(9));
        cx.notify();
    });
    window
        .update(cx, |view, _, cx| {
            view.body_height = 216.;
            cx.notify();
        })
        .unwrap();
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| window.draw(app).clear())
            .unwrap();
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
    let selectors = [
        "syndic-transcript-record:view:entry-0:projection:text-0",
        "syndic-transcript-record:view:entry-1:projection:text-1",
        "syndic-transcript-record:view:entry-2:projection:text-2",
        "syndic-transcript-record:view:entry-3:projection:text-3",
        "syndic-transcript-record:view:entry-4:projection:text-4",
        "syndic-transcript-record:view:entry-5:projection:text-5",
        "syndic-transcript-record:view:entry-6:projection:text-6",
        "syndic-transcript-record:view:entry-7:projection:text-7",
    ];
    let anchor_row = visual.debug_bounds(selectors[anchor.index]).unwrap();
    assert_eq!(anchor_row.top(), body.top() + px(anchor.viewport_y_px));
}
