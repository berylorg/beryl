use super::*;
use crate::syndic_transcript::{ProviderRevision, SyndicTranscriptPanel};
use gpui::{AsyncApp, Entity, WindowHandle};
use std::{cell::RefCell, rc::Rc};

const LINE_BYTES: u64 = ("let payload = 1;".len() * 16 + 1) as u64;

#[test]
fn native_history_revision_reloads_code_and_preserves_source_viewport() {
    history_revision(RevisionTiming::Admitted);
}

#[test]
fn native_history_revision_preserves_code_position_before_current_line_admission() {
    history_revision(RevisionTiming::BeforeScheduling);
}

#[test]
fn native_history_revision_refuses_running_code_read_and_preserves_position() {
    history_revision(RevisionTiming::DuringRead);
}

#[derive(Clone, Copy, PartialEq)]
enum RevisionTiming {
    Admitted,
    BeforeScheduling,
    DuringRead,
}

fn history_revision(timing: RevisionTiming) {
    let before_admission = timing != RevisionTiming::Admitted;
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some(&format!(
                    "```rust\n{}\n```",
                    ("let payload = 1;".repeat(16) + "\n").repeat(2000)
                )),
            )
        },
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_range(window, &panel, None, 0, cx).await;
                cx.update(|app| {
                    app.update_window(window.into(), |_, native, app| {
                        let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                        code_interaction::press(native, app, bounds.center());
                        code_interaction::release(native, app, bounds.center());
                        native.dispatch_input_for_test(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position: bounds.center(),
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    gpui::px(-45.5),
                                    gpui::px(-90.5),
                                )),
                                modifiers: Default::default(),
                                touch_phase: gpui::TouchPhase::Moved,
                            }),
                            app,
                        );
                        native.draw_and_present_for_test(app);
                    })
                    .unwrap();
                })
                .unwrap();
                wait_range(window, &panel, None, 4, cx).await;
                let (identity, key, activation, prior_revision, outer, resource) = panel
                    .read_with(cx, |panel, _| {
                        assert_eq!(panel.code_presentation_for_test().unwrap().2, 10.5);
                        assert_eq!(panel.code_horizontal_offset_for_test(), Some(45.5));
                        assert!(panel.code_viewport_for_test().unwrap().3);
                        (
                            panel.code_presentation_for_test().unwrap().0,
                            panel.diagnostic_snapshot().code_panels[0].key,
                            panel.snapshot().activation_revision,
                            panel.code_provider_revision_for_test().unwrap(),
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            panel.code_viewport_for_test().unwrap().0,
                        )
                    })
                    .unwrap();
                let selected = snapshot(&owner).windows()[0].selected_thread();
                let gate = (timing == RevisionTiming::DuringRead).then(|| {
                    window
                        .update(cx, |root, _, _| {
                            root.test_hold_indexed_code_read(prior_revision)
                        })
                        .unwrap()
                });
                let top = if before_admission {
                    panel
                        .update(cx, |panel, _| {
                            if timing == RevisionTiming::BeforeScheduling {
                                panel.pause_code_reads_through_revision_for_test(prior_revision);
                            }
                        })
                        .unwrap();
                    cx.update(|app| {
                        app.update_window(window.into(), |_, native, app| {
                            let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                            native.dispatch_input_for_test(
                                gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                    position: bounds.center(),
                                    delta: gpui::ScrollDelta::Pixels(gpui::point(
                                        gpui::px(0.0),
                                        gpui::px(-18000.0),
                                    )),
                                    modifiers: Default::default(),
                                    touch_phase: gpui::TouchPhase::Moved,
                                }),
                                app,
                            );
                            native.draw_and_present_for_test(app);
                        })
                        .unwrap();
                    })
                    .unwrap();
                    if let Some(gate) = &gate {
                        for _ in 0..300 {
                            draw(window, cx);
                            if gate.entered() {
                                break;
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(20))
                                .await;
                        }
                        assert!(
                            gate.entered(),
                            "ordinary Code worker must finish the predecessor range read"
                        );
                        assert!(!gate.refused());
                    }
                    for _ in 0..10 {
                        draw(window, cx);
                        panel
                            .read_with(cx, |panel, _| {
                                assert_eq!(
                                    panel.code_provider_revision_for_test(),
                                    Some(prior_revision)
                                );
                                assert_eq!(panel.code_viewport_for_test().unwrap().2, 904);
                                assert!(panel.code_foreground_response_for_test(&resource).is_none());
                                if timing == RevisionTiming::DuringRead {
                                    let demand = panel.resource_demand().unwrap();
                                    assert_eq!(demand.revision, prior_revision);
                                    assert!(matches!(demand.indexed.unwrap().query,
                                        IndexedResourceQuery::Range { request: index::Request::Code { lines }, .. }
                                        if lines.start == 902 && lines.end == 918));
                                }
                                if timing == RevisionTiming::BeforeScheduling {
                                    assert!(panel.resource_demand().is_none());
                                }
                                assert_eq!(
                                    panel.code_anchor_for_test().unwrap().position,
                                    "```rust\n".len() as u64 + 4 * LINE_BYTES
                                );
                            })
                            .unwrap();
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                    904
                } else {
                    4
                };
                let revision = history_storage::advance_history_revision(&owner, None);
                assert!(revision > prior_revision);
                if let Some(gate) = &gate {
                    assert!(!gate.refused());
                    gate.release();
                }
                wait_range(window, &panel, Some(revision), top, cx).await;
                if let Some(gate) = &gate {
                    assert!(
                        gate.refused(),
                        "predecessor worker must fail current-source validation"
                    );
                }
                assert_eq!(snapshot(&owner).windows()[0].selected_thread(), selected);
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_transcript_panel().entity_id())
                        .unwrap(),
                    panel.entity_id()
                );
                panel
                    .read_with(cx, |panel, _| {
                        assert!(panel.snapshot().activation_revision > activation);
                        assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
                        assert_eq!(panel.code_presentation_for_test().unwrap().2, 10.5);
                        assert_eq!(panel.code_horizontal_offset_for_test(), Some(45.5));
                        assert_eq!(panel.code_viewport_for_test().unwrap().0, resource);
                        assert!(panel.code_viewport_for_test().unwrap().3);
                        let anchor = panel.code_anchor_for_test().unwrap();
                        assert_eq!(anchor.position, "```rust\n".len() as u64 + top * LINE_BYTES);
                        assert_eq!(anchor.visual_offset, -11);
                        let diagnostics = panel.diagnostic_snapshot();
                        let code = &diagnostics.code_panels[0];
                        assert_eq!(code.key, key);
                        assert_eq!(code.source_continuity, "same");
                        assert_eq!(
                            code.anchor_remap_result,
                            if before_admission { "none" } else { "resolved" }
                        );
                        assert_eq!(code.anchor_visual_offset, Some(-10.5));
                        assert!(code.realized_visual_lines <= 16);
                        assert_eq!(
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            outer
                        );
                    })
                    .unwrap();
                if !before_admission {
                    code_anchor::scroll_beyond_resident_lines(window, &panel, LINE_BYTES, cx).await;
                }
                window
                    .update(cx, |root, native, app| {
                        root.set_ordinary_close_interaction_gated(true, app);
                        root.suspend_running_thread_reads(native, app);
                    })
                    .unwrap();
                let mut drained = false;
                for _ in 0..100 {
                    drained = window
                        .update(cx, |root, _, _| {
                            root.release_suspended_running_thread_sources()
                        })
                        .unwrap();
                    if drained {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                assert!(drained);
                window
                    .update(cx, |root, native, app| {
                        root.retire_running_thread_views(native, app).unwrap()
                    })
                    .unwrap();
                panel.update(cx, |panel, _| panel.retire()).unwrap();
                draw(window, cx);
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .code_panels
                            .is_empty())
                        .unwrap()
                );
                cx.update(|app| app.quit()).unwrap();
            })
        },
    );
}

pub(super) fn draw(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    cx.update(|app| {
        app.update_window(window.into(), |_, native, app| {
            native.draw_and_present_for_test(app);
            native.minimize_window();
        })
        .unwrap();
    })
    .unwrap();
}

pub(super) async fn wait_range(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    revision: Option<ProviderRevision>,
    top: u64,
    cx: &mut AsyncApp,
) {
    wait_range_ready(window, panel, revision, top, false, cx).await;
}

pub(super) async fn wait_painted_range(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    revision: Option<ProviderRevision>,
    top: u64,
    cx: &mut AsyncApp,
) {
    wait_range_ready(window, panel, revision, top, true, cx).await;
}

async fn wait_range_ready(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    revision: Option<ProviderRevision>,
    top: u64,
    require_paint: bool,
    cx: &mut AsyncApp,
) {
    for _ in 0..300 {
        draw(window, cx);
        if panel.read_with(cx, |panel, _| {
            let Some((resource, _, actual_top, _)) = panel.code_viewport_for_test() else { return false; };
            actual_top == top && (!require_paint || panel.painted_glyphs_for_test() > 50)
                && revision.is_none_or(|revision| panel.code_provider_revision_for_test() == Some(revision))
                && panel.code_foreground_response_for_test(&resource).is_some_and(|response| {
                    matches!(&response.value, IndexedResourceValue::Range(range) if range.request_complete
                        && !range.items.is_empty() && range.items.len() <= 16
                        && matches!(range.cursor.request, index::Request::Code { lines } if lines.start == top.saturating_sub(2)))
                })
        }).unwrap() { return; }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    let facts = panel
        .read_with(cx, |panel, _| {
            (
                panel.code_provider_revision_for_test(),
                panel
                    .code_viewport_for_test()
                    .map(|(_, _, top, selected)| (top, selected)),
                panel.diagnostic_snapshot().code_panels,
                panel
                    .code_viewport_for_test()
                    .and_then(|(resource, _, _, _)| {
                        panel
                            .code_foreground_response_for_test(&resource)
                            .and_then(|response| {
                                if let IndexedResourceValue::Range(range) = &response.value {
                                    Some((range.cursor, range.request_complete, range.items.len()))
                                } else {
                                    None
                                }
                            })
                    }),
            )
        })
        .unwrap();
    panic!("Code revision {revision:?} must load its range at {top}: {facts:?}");
}
