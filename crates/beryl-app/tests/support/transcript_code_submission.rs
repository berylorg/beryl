use super::*;
use crate::syndic_transcript::{IndexedResourceValue, SyndicTranscriptPanel};
use gpui::{Entity, EntityInputHandler, PlatformInput, WindowHandle, point, px};
use syndic_storage::indexed_resource as index;

#[path = "transcript_code_history.rs"]
mod history;

#[path = "../unit/app_services/ordinary_transcript_viewport/indexed_resources/code_interaction.rs"]
mod interaction;

pub(super) fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    thread: SyndicThreadId,
    cx: &mut TestAppContext,
) -> Arc<crate::app_services::PublishedRunningThreadsReader> {
    let panel = window
        .read_with(cx, |root, _| root.test_transcript_panel())
        .unwrap();
    assert!(panel.read_with(cx, |panel, _| panel.snapshot().records.is_empty()));
    let mount = window
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap().clone()
        })
        .unwrap();
    let predecessor = mount.read_with(cx, |mount, _| mount.contribution().unwrap());
    let original = predecessor.read_with(cx, |composer, _| composer.selection_identity());
    let turn = original
        .binding()
        .candidate()
        .draft_id()
        .submitted_turn_id();
    let input = predecessor.read_with(cx, |composer, _| composer.gpui_input());
    wait(
        cx,
        |cx| {
            input.read_with(cx, |input, _| {
                input.is_semantically_quiescent() && input.is_surface_current_and_interactive()
            })
        },
        "recovered editor is not interactive before submission",
    );
    let text = format!("```rust\n{}```", "let payload = 1;\n".repeat(2000));
    window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(None, &text, window, cx)
            })
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            input.read_with(cx, |input, _| {
                input.is_semantically_quiescent()
                    && input.is_surface_current_and_interactive()
                    && input.surface().is_some_and(|surface| {
                        surface.source_selection().head.byte_offset.get() == text.len() as u64
                    })
            })
        },
        "Code edit did not settle before mounted submission",
    );
    window
        .update(cx, |_, window, app| {
            mount.update(app, |mount, cx| {
                mount.test_use_storage_only_submission();
                let selection = mount.contribution().unwrap().read(cx).selection_identity();
                mount
                    .test_begin_submission_start(selection, window, cx)
                    .unwrap();
            })
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            mount.read_with(cx, |mount, app| {
                let status = mount.submission_status();
                assert!(
                    !matches!(
                        status,
                        MainWindowConversationComposerSubmissionStatus::Failed
                            | MainWindowConversationComposerSubmissionStatus::Unavailable
                            | MainWindowConversationComposerSubmissionStatus::NotCommitted
                    ),
                    "mounted Code submission failed: {status:?}"
                );
                status == MainWindowConversationComposerSubmissionStatus::Idle
                    && mount.contribution().is_some_and(|current| {
                        current.entity_id() != predecessor.entity_id()
                            && current.read(app).selection_identity().claim() == original.claim()
                    })
            })
        },
        "mounted Code submission did not adopt its successor editor",
    );
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let storage = graph.syndic();
        let items = storage
            .turn_items(
                graph.home(),
                turn,
                None,
                beryl_home_store::CursorReadLimits::new(1, 65536).unwrap(),
            )
            .unwrap();
        assert_eq!(items.records().len(), 1);
        history::finish(
            graph.home(),
            storage,
            thread,
            turn,
            items.records()[0].item_id(),
        );
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
    let reader = Arc::new(owner.borrow().running_threads_reader().unwrap());
    window
        .update(cx, |root, window, cx| {
            root.test_running_thread_reader(&reader, window, cx)
        })
        .unwrap()
        .unwrap();
    wait_range(&panel, 0, cx);
    let (identity, key, revision) = panel.read_with(cx, |panel, _| {
        assert!(!panel.code_viewport_for_test().unwrap().3);
        (
            panel.code_presentation_for_test().unwrap().0,
            panel.diagnostic_snapshot().code_panels[0].key,
            panel.code_provider_revision_for_test().unwrap(),
        )
    });
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| {
            let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
            interaction::press(window, app, bounds.center());
            interaction::release(window, app, bounds.center());
            let outer = panel
                .read(app)
                .measured_frame_for_test()
                .unwrap()
                .manual_scroll_total_px;
            window.dispatch_input_for_test(
                PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                    position: bounds.center(),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-18010.5))),
                    modifiers: Default::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                }),
                app,
            );
            window.draw_and_present_for_test(app);
            assert_eq!(panel.read(app).code_viewport_for_test().unwrap().2, 900);
            assert_eq!(
                panel
                    .read(app)
                    .measured_frame_for_test()
                    .unwrap()
                    .manual_scroll_total_px,
                outer
            );
        })
        .unwrap();
    });
    wait_range(&panel, 900, cx);
    for _ in 0..10 {
        cx.run_until_parked();
        cx.update(|app| {
            app.update_window(window.into(), |_, window, app| window.draw(app).clear())
                .unwrap();
        });
        window
            .read_with(cx, |root, app| {
                assert_eq!(root.test_transcript_panel().entity_id(), panel.entity_id());
                let panel = panel.read(app);
                assert_eq!(panel.code_presentation_for_test().unwrap().0, identity);
                assert_eq!(panel.code_provider_revision_for_test(), Some(revision));
                assert_eq!(panel.code_presentation_for_test().unwrap().2, 10.5);
                let code = &panel.diagnostic_snapshot().code_panels[0];
                assert_eq!(code.key, key);
                assert_eq!(code.logical_lines, 2000);
                assert!(code.realized_visual_lines <= 16);
                assert!(code.selected_for_wheel);
                assert_eq!(panel.code_viewport_for_test().unwrap().2, 900);
            })
            .unwrap();
    }
    reader
}

fn wait_range(panel: &Entity<SyndicTranscriptPanel>, top: u64, cx: &mut TestAppContext) {
    wait(
        cx,
        |cx| {
            panel.read_with(cx, |panel, _| {
        let Some((resource, _, actual_top, _)) = panel.code_viewport_for_test() else { return false; };
        actual_top == top && panel.indexed_resource_response(&resource).is_some_and(|response| {
            matches!(&response.value, IndexedResourceValue::Range(range)
                if range.request_complete && !range.items.is_empty() && range.items.len() <= 16
                && matches!(range.cursor.request, index::Request::Code { lines } if lines.start == top.saturating_sub(2)))
        })
    })
        },
        "recovered conversation Code range did not load",
    );
}
