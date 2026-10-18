use super::*;
use crate::main_window::MainWindowConversationComposerSubmissionStatus;
use crate::syndic_transcript::{IndexedResourceValue, SyndicTranscriptPanel};
use gpui::{AsyncApp, Entity};
use syndic_storage::indexed_resource as index;

#[path = "transcript_code_history.rs"]
mod history;
#[path = "../unit/app_services/ordinary_transcript_viewport/indexed_resources/code_interaction.rs"]
mod interaction;
#[path = "native_window_geometry.rs"]
mod native_geometry;
pub(super) use history::{finish as finish_history, seed as prepare_history};

pub(super) fn draw(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    native_geometry::restore(window, cx);
    cx.update(|app| {
        app.update_window(window.into(), |_, native, app| {
            native.draw_and_present_for_test(app);
        })
        .unwrap();
    })
    .unwrap();
}

async fn wait_range(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    top: u64,
    cx: &mut AsyncApp,
) {
    for _ in 0..300 {
        draw(window, cx);
        let loaded = panel.read_with(cx, |panel, _| {
            let Some((resource, _, actual_top, _)) = panel.code_viewport_for_test() else { return false; };
            actual_top == top && panel.indexed_resource_response(&resource).is_some_and(|response| {
                matches!(&response.value, IndexedResourceValue::Range(range) if range.request_complete
                    && !range.items.is_empty() && range.items.len() <= 16
                    && matches!(range.cursor.request, index::Request::Code { lines } if lines.start == top.saturating_sub(2)))
            })
        }).unwrap();
        if loaded {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    panic!("replacement Code must load a provider-backed range at {top}");
}

pub(super) async fn qualify(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    wait_range(window, &panel, 0, cx).await;
    let (identity, key, revision) = panel
        .read_with(cx, |panel, _| {
            assert!(!panel.code_viewport_for_test().unwrap().3);
            let code = &panel.diagnostic_snapshot().code_panels[0];
            assert_eq!(code.logical_lines, 2000);
            (
                panel.code_presentation_for_test().unwrap().0,
                code.key,
                panel.code_provider_revision_for_test().unwrap(),
            )
        })
        .unwrap();
    cx.update(|app| {
        app.update_window(window.into(), |_, native, app| {
            let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
            interaction::press(native, app, bounds.center());
            interaction::release(native, app, bounds.center());
            let outer = panel
                .read(app)
                .measured_frame_for_test()
                .unwrap()
                .manual_scroll_total_px;
            native.dispatch_input_for_test(
                gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                    position: bounds.center(),
                    delta: gpui::ScrollDelta::Pixels(gpui::point(
                        gpui::px(0.0),
                        gpui::px(-18010.5),
                    )),
                    modifiers: Default::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                }),
                app,
            );
            assert_eq!(panel.read(app).code_viewport_for_test().unwrap().2, 900);
            assert!(panel.read(app).code_viewport_for_test().unwrap().3);
            assert_eq!(
                panel
                    .read(app)
                    .measured_frame_for_test()
                    .unwrap()
                    .manual_scroll_total_px,
                outer
            );
            native.draw_and_present_for_test(app);
        })
        .unwrap();
    })
    .unwrap();
    wait_range(window, &panel, 900, cx).await;
    for _ in 0..10 {
        draw(window, cx);
        panel
            .read_with(cx, |panel, _| {
                assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
                assert_eq!(panel.code_presentation_for_test().unwrap().2, 10.5);
                assert_eq!(panel.code_provider_revision_for_test(), Some(revision));
                let code = &panel.diagnostic_snapshot().code_panels[0];
                assert_eq!(code.key, key);
                assert!(code.selected_for_wheel);
                assert!(code.realized_visual_lines <= 16);
            })
            .unwrap();
    }
}

pub(super) async fn wait(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
    ready: impl FnMut(&mut AsyncApp) -> bool,
    message: &str,
) {
    wait_bounded(window, cx, Duration::from_secs(10), ready, message).await;
}

async fn wait_bounded(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
    duration: Duration,
    mut ready: impl FnMut(&mut AsyncApp) -> bool,
    message: &str,
) {
    let deadline = Instant::now() + duration;
    loop {
        draw(window, cx);
        if ready(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "{message}");
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
}

pub(super) async fn submit_code(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut AsyncApp,
) {
    let predecessor = composer(window, cx).await;
    let original = predecessor
        .read_with(cx, |composer, _| composer.selection_identity())
        .unwrap();
    let input = predecessor
        .read_with(cx, |composer, _| composer.gpui_input())
        .unwrap();
    wait(
        window,
        cx,
        |cx| {
            input
                .read_with(cx, |input, _| {
                    input.is_semantically_quiescent() && input.is_surface_current_and_interactive()
                })
                .unwrap()
        },
        "native recovered editor admission",
    )
    .await;
    let text = format!("```rust\n{}```", "let payload = 1;\n".repeat(2000));
    window
        .update(cx, |_, native, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(None, &text, native, cx)
            })
        })
        .unwrap();
    wait_bounded(
        window,
        cx,
        Duration::from_secs(30),
        |cx| {
            input
                .read_with(cx, |input, _| {
                    input.is_semantically_quiescent()
                        && input.is_surface_current_and_interactive()
                        && input.surface().is_some_and(|surface| {
                            surface.source_selection().head.byte_offset.get() == text.len() as u64
                        })
                })
                .unwrap()
        },
        "native Code edit settlement",
    )
    .await;
    let mount = window
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap().clone()
        })
        .unwrap();
    window
        .update(cx, |_, native, app| {
            mount.update(app, |mount, cx| {
                mount.test_use_storage_only_submission();
                let selected = mount.contribution().unwrap().read(cx).selection_identity();
                mount
                    .test_begin_submission_start(selected, native, cx)
                    .unwrap();
            })
        })
        .unwrap();
    let adoption_started = Instant::now();
    let mut reported = false;
    wait(
        window,
        cx,
        |cx| {
            if !reported && adoption_started.elapsed() > Duration::from_secs(3) {
                reported = true;
                eprintln!(
                    "native successor progress: {:?}",
                    mount
                        .read_with(cx, |mount, _| mount.test_submission_diagnostics())
                        .unwrap()
                );
            }
            mount
                .read_with(cx, |mount, app| {
                    let status = mount.submission_status();
                    assert!(
                        !matches!(
                            status,
                            MainWindowConversationComposerSubmissionStatus::Failed
                                | MainWindowConversationComposerSubmissionStatus::Unavailable
                                | MainWindowConversationComposerSubmissionStatus::NotCommitted
                        ),
                        "native submission: {status:?}"
                    );
                    status == MainWindowConversationComposerSubmissionStatus::Idle
                        && mount.contribution().is_some_and(|current| {
                            current.entity_id() != predecessor.entity_id()
                                && current.read(app).selection_identity().claim()
                                    == original.claim()
                        })
                })
                .unwrap()
        },
        "native Code successor adoption",
    )
    .await;
    let (home, storage) = {
        let retained = owner.borrow();
        let graph = retained.test_services().graph().unwrap();
        (graph.home().service_reference(), graph.syndic().clone())
    };
    cx.background_executor()
        .spawn(async move {
            let turn = original
                .binding()
                .candidate()
                .draft_id()
                .submitted_turn_id();
            let items = storage
                .turn_items(
                    &home,
                    turn,
                    None,
                    beryl_home_store::CursorReadLimits::new(1, 65536).unwrap(),
                )
                .unwrap();
            assert_eq!(items.records().len(), 1);
            finish_history(
                &home,
                &storage,
                original.claim().thread_id(),
                turn,
                items.records()[0].item_id(),
            );
        })
        .await;
}
