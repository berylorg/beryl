use super::*;
#[path = "code_save.rs"]
mod code_save;
#[path = "transcript_keyboard_copy.rs"]
mod transcript_keyboard_copy;
#[path = "discussion_context_keyboard_copy.rs"]
mod discussion_context_keyboard_copy;
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use syndic_storage::{
    ProjectionSourceRange, ResourceBacking, ResourceMetadataRecord, ResourceStructure,
};

fn source_body() -> String {
    "ab\t🍒\r\n".to_owned()
        + "a"
        + &"\u{301}".repeat(600)
        + "\r\n"
        + &("x".repeat(512) + "\r\n").repeat(72)
        + "tail\r\n\r\n"
}

fn key(native: &mut gpui::Window, app: &mut gpui::App, value: &str) {
    native.dispatch_input_for_test(
        PlatformInput::KeyDown(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(value).unwrap(),
            is_held: false,
        }),
        app,
    );
}
fn drag(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    start: gpui::Point<gpui::Pixels>,
    end: gpui::Point<gpui::Pixels>,
) {
    code_interaction::press(native, app, start);
    native.dispatch_input_for_test(
        PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position: end,
            pressed_button: Some(gpui::MouseButton::Left),
            modifiers: Default::default(),
        }),
        app,
    );
    code_interaction::release(native, app, end);
}
fn shift_click(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    position: gpui::Point<gpui::Pixels>,
) {
    native.dispatch_input_for_test(
        PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: gpui::Modifiers {
                shift: true,
                ..Default::default()
            },
            click_count: 1,
            ..Default::default()
        }),
        app,
    );
    code_interaction::release(native, app, position);
}
fn wheel(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    panel: &Entity<SyndicTranscriptPanel>,
    delta: f32,
) {
    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
    native.dispatch_input_for_test(
        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
            position: bounds.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(delta))),
            touch_phase: gpui::TouchPhase::Moved,
            ..Default::default()
        }),
        app,
    );
}
fn header_copy(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    panel: &Entity<SyndicTranscriptPanel>,
) {
    let bounds = panel.read(app).code_command_geometry_for_test().unwrap().0;
    let position = bounds.center();
    native.dispatch_input_for_test(
        PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
        app,
    );
    native.dispatch_input_for_test(
        PlatformInput::MouseUp(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
        app,
    );
}

fn assert_command_geometry(panel: &SyndicTranscriptPanel) {
    let viewport = panel.code_viewport_for_test().unwrap().1;
    let (command, footer, mask) = panel.code_command_geometry_for_test().unwrap();
    assert_eq!(viewport.size.height, px(240.0));
    assert_eq!(command.size.height, px(32.0));
    assert_eq!(footer.size.height, px(29.0));
    assert_eq!(footer.intersect(&mask), footer);
    assert!(command.bottom() <= viewport.top());
    assert!(footer.top() >= viewport.bottom());
}

async fn ready(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    eprintln!("Code source fixture awaiting ready geometry");
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| {
                    panel.code_commands_current_for_test()
                        && panel
                            .diagnostic_snapshot()
                            .code_panels
                            .first()
                            .is_some_and(|code| {
                                code.layout_state == "ready" && code.layout_fragments > 0
                            })
                })
                .unwrap()
        },
        "Code source action fixture must paint admitted canonical geometry",
    )
    .await;
    panel
        .read_with(cx, |panel, _| assert_command_geometry(panel))
        .unwrap();
    eprintln!("Code source fixture ready geometry and visible feedback");
}

#[test]
fn native_code_selection_copy_uses_raw_offscreen_ranges_and_atomic_successor_endpoints() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(path, 1, Some(&format!("```text\n{}```", source_body())));
        },
        |owner, _, cx| {
            Box::pin(async move {
                let raw = source_body();
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let captured = Rc::new(RefCell::new(Vec::<(
                    gpui::ClipboardItem,
                    gpui::ClipboardLimits,
                )>::new()));
                let refuse = Rc::new(Cell::new(false));
                let sink = captured.clone();
                let fail = refuse.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_clipboard_writer_for_test(move |item, limits, _| {
                            assert!(sink.borrow().len() < 8);
                            sink.borrow_mut().push((item.clone(), limits));
                            if fail.get() {
                                Err(gpui::ClipboardError::Write(
                                    gpui::ClipboardRepresentation::Text,
                                ))
                            } else {
                                Ok(())
                            }
                        })
                    })
                    .unwrap();
                let original = panel
                    .read_with(cx, |panel, _| {
                        (
                            panel.code_presentation_for_test().unwrap().0,
                            panel.diagnostic_snapshot().code_panels[0].key,
                            panel
                                .measured_frame_for_test()
                                .unwrap()
                                .manual_scroll_total_px,
                            panel.code_viewport_for_test().unwrap().0,
                        )
                    })
                    .unwrap();
                let atoms = panel
                    .read_with(cx, |panel, _| panel.code_source_atoms_for_test())
                    .unwrap();
                assert_eq!(atoms.len(), 2);
                for (attempt, (source, bounds)) in atoms.iter().enumerate() {
                    refuse.set(attempt == 1);
                    with_native(window, cx, |native, app| {
                        drag(
                            native,
                            app,
                            bounds.origin
                                + point(bounds.size.width * 0.25, bounds.size.height * 0.5),
                            bounds.origin
                                + point(bounds.size.width * 0.75, bounds.size.height * 0.5),
                        );
                        assert_eq!(
                            panel.read(app).code_selection_for_test(),
                            Some(source.clone())
                        );
                        assert!(panel.read(app).code_selection_painted_quads_for_test() > 0);
                    })
                    .unwrap();
                    ready(window, &panel, cx).await;
                    with_native(window, cx, |native, app| {
                        key(native, app, "ctrl-c");
                        if attempt == 0 {
                            let before = panel.read(app).code_copy_progress_for_test();
                            assert!(panel.read(app).code_copy_pending_for_test());
                            native.dispatch_input_for_test(
                                PlatformInput::KeyDown(gpui::KeyDownEvent {
                                    keystroke: gpui::Keystroke::parse("ctrl-c").unwrap(),
                                    is_held: true,
                                }),
                                app,
                            );
                            assert_eq!(panel.read(app).code_copy_progress_for_test(), before);
                            key(native, app, "ctrl-c");
                            assert!(panel.read(app).code_copy_pending_for_test());
                        }
                        eprintln!(
                            "atom Copy admission: {:?}, {:?}",
                            panel.read(app).code_copy_feedback_for_test(),
                            panel.read(app).code_copy_progress_for_test()
                        );
                    })
                    .unwrap();
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    !panel.code_copy_pending_for_test()
                                        && captured.borrow().len() == attempt + 1
                                })
                                .unwrap()
                        },
                        "atom selection Copy must reach the checked terminal writer",
                    )
                    .await;
                    assert_eq!(captured.borrow().len(), attempt + 1);
                    assert_eq!(
                        captured.borrow()[attempt].0.text().as_deref(),
                        Some(
                            &raw[(source.start - BODY_START) as usize
                                ..(source.end - BODY_START) as usize]
                        )
                    );
                    assert!(captured.borrow()[attempt].0.metadata().is_none());
                    assert_eq!(
                        panel
                            .read_with(cx, |panel, _| panel.code_selection_for_test())
                            .unwrap(),
                        Some(source.clone())
                    );
                    if attempt == 1 {
                        assert!(
                            panel
                                .read_with(cx, |panel, _| panel
                                    .code_copy_feedback_for_test()
                                    .unwrap()
                                    .contains("unavailable"))
                                .unwrap()
                        );
                    }
                }
                refuse.set(false);
                let anchor = BODY_START + 1;
                with_native(window, cx, |native, app| {
                    let position = panel.read(app).code_source_point_for_test(anchor).unwrap();
                    code_interaction::press(native, app, position);
                    code_interaction::release(native, app, position);
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    let height = panel.read(app).code_line_height_for_test().unwrap();
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-52.0 * height))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                })
                .unwrap();
                ready(window, &panel, cx).await;
                let head = BODY_START + 1212 + 50 * 514 + 13;
                with_native(window, cx, |native, app| {
                    let position = panel.read(app).code_source_point_for_test(head).unwrap();
                    native.dispatch_input_for_test(
                        PlatformInput::MouseDown(gpui::MouseDownEvent {
                            position,
                            button: gpui::MouseButton::Left,
                            modifiers: gpui::Modifiers {
                                shift: true,
                                ..Default::default()
                            },
                            click_count: 1,
                            ..Default::default()
                        }),
                        app,
                    );
                    code_interaction::release(native, app, position);
                    assert_eq!(
                        panel.read(app).code_selection_for_test(),
                        Some(anchor..head)
                    );
                    key(native, app, "ctrl-c");
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.code_copy_feedback_for_test() == Some("Copied")
                            })
                            .unwrap()
                    },
                    "offscreen selection must copy through bounded Source pages",
                )
                .await;
                assert_eq!(
                    captured.borrow()[2].0.text().as_deref(),
                    Some(&raw[(anchor - BODY_START) as usize..(head - BODY_START) as usize])
                );
                assert!(
                    captured.borrow()[2].1.text_bytes
                        >= 3 * (head - anchor) as usize + 2 + 64 * 1024
                );
                with_native(window, cx, |native, app| {
                    wheel(native, app, &panel, -100_000.0)
                })
                .unwrap();
                ready(window, &panel, cx).await;
                with_native(window, cx, |native, app| {
                    let current = panel.read(app);
                    let bounds = current.code_viewport_for_test().unwrap().1;
                    let height = current.code_line_height_for_test().unwrap();
                    shift_click(
                        native,
                        app,
                        point(bounds.left() + px(60.0), bounds.bottom() - px(height * 0.5)),
                    );
                    assert_eq!(
                        panel.read(app).code_selection_for_test(),
                        Some(anchor..BODY_START + raw.len() as u64 - 2)
                    );
                    wheel(native, app, &panel, 100_000.0);
                    wheel(native, app, &panel, -52.0 * height);
                })
                .unwrap();
                ready(window, &panel, cx).await;
                with_native(window, cx, |native, app| {
                    let position = panel.read(app).code_source_point_for_test(head).unwrap();
                    shift_click(native, app, position);
                    assert_eq!(
                        panel.read(app).code_selection_for_test(),
                        Some(anchor..head)
                    );
                })
                .unwrap();
                let appearance = window
                    .read_with(cx, |root, _| root.test_appearance())
                    .unwrap();
                let prepared = {
                    let owner = owner.borrow();
                    native_appearance::system_font_appearance_with_code_size(
                        owner.test_services().graph().unwrap().state(),
                        48,
                    )
                };
                let large = Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                    appearance.number().checked_next().unwrap(),
                    prepared,
                    crate::theme_runtime::AppearancePublication::Durable,
                ));
                cx.update(|app| MainWindowShellRoot::test_apply_appearance(window, large, app))
                    .unwrap();
                ready(window, &panel, cx).await;
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_selection_for_test())
                        .unwrap(),
                    Some(anchor..head)
                );
                cx.update(|app| {
                    MainWindowShellRoot::test_apply_appearance(window, appearance.clone(), app)
                })
                .unwrap();
                ready(window, &panel, cx).await;
                let large_header = {
                    let owner = owner.borrow();
                    native_appearance::system_font_appearance_with_code_header_size(
                        owner.test_services().graph().unwrap().state(),
                        48,
                    )
                };
                let large_header = Arc::new(crate::theme_runtime::AppearanceGeneration::new(
                    appearance
                        .number()
                        .checked_next()
                        .unwrap()
                        .checked_next()
                        .unwrap(),
                    large_header,
                    crate::theme_runtime::AppearancePublication::Durable,
                ));
                cx.update(|app| {
                    MainWindowShellRoot::test_apply_appearance(window, large_header, app)
                })
                .unwrap();
                ready(window, &panel, cx).await;
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel.code_selection_for_test())
                        .unwrap(),
                    Some(anchor..head)
                );
                with_native(window, cx, |native, app| header_copy(native, app, &panel)).unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.code_copy_feedback_for_test() == Some("Copied")
                                    && captured.borrow().len() == 4
                            })
                            .unwrap()
                    },
                    "header Copy must read the full admitted raw resource",
                )
                .await;
                assert_eq!(captured.borrow()[3].0.text().as_deref(), Some(raw.as_str()));
                let metadata = {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    graph
                        .syndic()
                        .resource(
                            graph.home(),
                            original.3.0.parse().unwrap(),
                            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
                        )
                        .unwrap()
                        .unwrap()
                };
                let ResourceBacking::TextRange { source, range } = metadata.backing() else {
                    panic!();
                };
                with_native(window, cx, |native, app| {
                    key(native, app, "ctrl-c");
                    assert!(panel.read(app).code_copy_pending_for_test());
                })
                .unwrap();
                for (revision, bytes) in [
                    (metadata.revision().get() + 1, 6352_usize),
                    (metadata.revision().get() + 2, raw.len()),
                ] {
                    let successor = ResourceMetadataRecord::new(
                        metadata.id(),
                        beryl_model::ProjectionRevision::new(revision).unwrap(),
                        metadata.projection_id().unwrap(),
                        metadata.item_id(),
                        metadata.ordinal().unwrap(),
                        metadata.kind(),
                        metadata.media_type().unwrap(),
                        ResourceBacking::TextRange {
                            source,
                            range: ProjectionSourceRange::new(
                                range.start(),
                                range.start() + bytes as u64,
                            )
                            .unwrap(),
                        },
                        Sha256::digest(&raw.as_bytes()[..bytes]).into(),
                        None,
                        ResourceStructure::code(Some("text"), raw[..bytes].lines().count() as u64)
                            .unwrap(),
                    )
                    .unwrap();
                    let provider_revision =
                        history_storage::advance_history_revision(&owner, Some(successor));
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            panel
                                .read_with(cx, |panel, _| {
                                    panel.code_provider_revision_for_test()
                                        == Some(provider_revision)
                                        && panel.diagnostic_snapshot().code_panels[0].layout_state
                                            == "ready"
                                        && panel.code_selection_for_test()
                                            == Some(anchor..BODY_START + 6352)
                                })
                                .unwrap()
                        },
                        "both selection endpoints must reconcile to the successor together",
                    )
                    .await;
                    assert_eq!(
                        captured.borrow().len(),
                        4,
                        "a superseded source command must not reach the checked writer"
                    );
                }
                with_native(window, cx, |native, app| {
                    let current = panel.read(app);
                    assert_eq!(current.code_presentation_for_test().unwrap().0, original.0);
                    assert_eq!(current.diagnostic_snapshot().code_panels[0].key, original.1);
                    assert_eq!(
                        current
                            .measured_frame_for_test()
                            .unwrap()
                            .manual_scroll_total_px,
                        original.2
                    );
                    assert!(current.code_painted_body_glyphs_for_test() > 0);
                    assert!(
                        current.diagnostic_snapshot().code_panels[0].layout_charged_bytes
                            <= 8 * 1024 * 1024
                    );
                    assert!(native.focused(app).is_some());
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_code_copy_preflight_refusal_releases_owned_failure_and_retired_actions() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(
                path,
                1,
                Some(&format!("```text\nshort\r\n{}\r\n```", "x".repeat(262144))),
            );
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx).await;
                let writes = Rc::new(Cell::new(0));
                let last = Rc::new(RefCell::new(None::<String>));
                let output = last.clone();
                let sink = writes.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.code_set_clipboard_writer_for_test(move |item, _, _| {
                            sink.set(sink.get() + 1);
                            *output.borrow_mut() = item.text();
                            Ok(())
                        })
                    })
                    .unwrap();
                let selected = BODY_START + 1..BODY_START + 4;
                with_native(window, cx, |native, app| {
                    let viewport = panel.read(app).code_viewport_for_test().unwrap().1;
                    let blank = point(viewport.left() + px(2.0), viewport.bottom() - px(10.0));
                    code_interaction::press(native, app, blank);
                    code_interaction::release(native, app, blank);
                    assert_eq!(panel.read(app).code_selection_for_test(), None);
                    key(native, app, "ctrl-c");
                    assert!(!panel.read(app).code_copy_pending_for_test());
                    assert!(
                        panel
                            .read(app)
                            .code_copy_feedback_for_test()
                            .unwrap()
                            .contains("selection unavailable")
                    );
                    assert_eq!(writes.get(), 0);
                    let start = panel
                        .read(app)
                        .code_source_point_for_test(selected.start)
                        .unwrap();
                    let end = panel
                        .read(app)
                        .code_source_point_for_test(selected.end)
                        .unwrap();
                    drag(native, app, start, end);
                    let demand = panel.read(app).resource_demand();
                    header_copy(native, app, &panel);
                    assert_eq!(panel.read(app).resource_demand(), demand);
                    assert!(!panel.read(app).code_copy_pending_for_test());
                    assert_eq!(
                        panel.read(app).code_selection_for_test(),
                        Some(selected.clone())
                    );
                    assert!(
                        panel
                            .read(app)
                            .code_copy_unavailable_for_test()
                            .unwrap()
                            .contains("256 KiB")
                    );
                    key(native, app, "ctrl-c");
                    native.draw_and_present_for_test(app);
                    assert!(panel.read(app).code_copy_pending_for_test());
                    let demand = panel.read(app).resource_demand().unwrap();
                    let charge =
                        panel.read(app).diagnostic_snapshot().code_panels[0].layout_charged_bytes;
                    panel.update(app, |panel, cx| {
                        panel.reject_resource_demand(demand);
                        cx.notify();
                    });
                    let current = panel.read(app);
                    assert!(!current.code_copy_pending_for_test());
                    assert_eq!(current.code_selection_for_test(), Some(selected.clone()));
                    assert!(
                        current.diagnostic_snapshot().code_panels[0].layout_charged_bytes < charge
                    );
                    assert!(current.code_painted_body_glyphs_for_test() > 0);
                    key(native, app, "ctrl-c");
                    assert!(panel.read(app).code_copy_pending_for_test());
                    assert!(
                        panel.read(app).resource_demand().is_some(),
                        "explicit immediate retry must retire the exact rejected action tuple"
                    );
                })
                .unwrap();
                ready(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.code_copy_feedback_for_test() == Some("Copied")
                            })
                            .unwrap()
                    },
                    "manual same-range retry must complete",
                )
                .await;
                assert_eq!(writes.get(), 1);
                let target = panel
                    .read_with(cx, |panel, _| {
                        panel.code_layout_first_fragment_end_for_test(1).unwrap() * 16.0
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    native.dispatch_input_for_test(
                        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position: bounds.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-target), px(0.0))),
                            modifiers: Default::default(),
                            touch_phase: gpui::TouchPhase::Moved,
                        }),
                        app,
                    );
                })
                .unwrap();
                ready(window, &panel, cx).await;
                with_native(window, cx, |native, app| {
                    let bounds = panel.read(app).code_viewport_for_test().unwrap().1;
                    let height = panel.read(app).code_line_height_for_test().unwrap();
                    drag(
                        native,
                        app,
                        bounds.origin + point(px(30.0), px(height * 1.5)),
                        bounds.origin + point(px(90.0), px(height * 1.5)),
                    );
                    let selected = panel.read(app).code_selection_for_test().unwrap();
                    assert!(selected.start > BODY_START + 8192);
                    assert!(panel.read(app).code_selection_painted_quads_for_test() > 0);
                    key(native, app, "ctrl-c");
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.code_copy_feedback_for_test() == Some("Copied")
                                    && writes.get() == 2
                            })
                            .unwrap()
                    },
                    "later Display-fragment selection must copy its exact source range",
                )
                .await;
                let selected = panel
                    .read_with(cx, |panel, _| panel.code_selection_for_test().unwrap())
                    .unwrap();
                assert_eq!(
                    last.borrow().as_deref(),
                    Some(
                        "x".repeat((selected.end - selected.start) as usize)
                            .as_str()
                    )
                );
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel.diagnostic_snapshot().code_panels[0]
                                    .complete_inline_extent
                                    .is_some()
                            })
                            .unwrap()
                    },
                    "resource extent must resume after action and far foreground work",
                )
                .await;
                with_native(window, cx, |native, app| {
                    key(native, app, "ctrl-c");
                    assert!(panel.read(app).code_copy_pending_for_test());
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
                assert_eq!(
                    writes.get(),
                    2,
                    "retired accepted action must never reach clipboard effect"
                );
                assert_eq!(
                    panel
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .resident_data
                            .resource_bytes)
                        .unwrap(),
                    0
                );
            })
        },
    );
}
