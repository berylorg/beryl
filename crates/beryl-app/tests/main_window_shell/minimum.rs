use super::support::editor_snapshot;
use super::*;

#[gpui::test]
fn native_minimum_preserves_the_original_editor_with_large_line_metrics(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(93);
            let prepared = fixture.prepare_with(Box::new(|selection| {
                let mut widget = composer_support::widget_config(
                    selection.binding().range_binding(),
                    selection.binding().presentation_generation(),
                );
                widget.layout.line_height = gpui::px(80.);
                MainWindowConversationComposerConfig::new(selection, widget)
                    .map_err(|error| error.to_string())
            }));
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let mut shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("shell"))
    });
    draw(shell.window(), cx);
    let (minimum, editor_id) = shell
        .window()
        .read_with(cx, |root, app| {
            let controller = root.controller().unwrap();
            let mount = controller.composer_mount().unwrap();
            (
                controller.minimum_size(),
                mount.read(app).contribution().unwrap().entity_id(),
            )
        })
        .unwrap();
    cx.simulate_window_resize(shell.window().into(), minimum);
    draw(shell.window(), cx);
    let mut visual = gpui::VisualTestContext::from_window(shell.window().into(), cx);
    let editor = visual.debug_bounds("conversation-composer-root").unwrap();
    let panel = visual.debug_bounds("main-window-user-input-panel").unwrap();
    let status = visual.debug_bounds("main-window-status-line").unwrap();
    assert!(editor.size.height >= gpui::px(80.));
    assert!(panel.size.height <= minimum.height * 0.5);
    assert_eq!(panel.bottom(), status.top());
    assert_eq!(status.bottom(), minimum.height);
    shell
        .window()
        .read_with(cx, |root, app| {
            let mount = root.controller().unwrap().composer_mount().unwrap();
            assert_eq!(
                mount.read(app).contribution().unwrap().entity_id(),
                editor_id
            );
        })
        .unwrap();
    cx.update(|app| shell.publish(app)).unwrap();
    assert!(cx.window_visibility(shell.window().into()).is_visible);
}

#[gpui::test]
fn smaller_preparation_protects_prior_controls_and_failed_relaxation_keeps_stricter_tracking(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(94);
            let prepared = fixture.prepare();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let mut shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("shell"))
    });
    draw(shell.window(), cx);
    let original = editor_snapshot(&shell, cx);
    for _ in 0..32 {
        let ready = shell
            .window()
            .update(cx, |root, window, app| {
                root.test_prepare_panel_minimum(gpui::px(200.), window, app)
            })
            .unwrap()
            .unwrap();
        if ready {
            break;
        }
        draw(shell.window(), cx);
    }
    shell
        .window()
        .update(cx, |root, window, app| {
            root.test_commit_panel_minimum(window, app)
        })
        .unwrap()
        .unwrap();
    draw(shell.window(), cx);
    let protected = shell
        .window()
        .update(cx, |root, window, _| {
            assert_eq!(
                window.minimum_size_for_test(),
                Some(root.controller().unwrap().minimum_size())
            );
            window.minimum_size_for_test().unwrap()
        })
        .unwrap();
    assert!(
        shell
            .window()
            .update(cx, |root, window, app| root.test_prepare_panel_minimum(
                gpui::px(48.),
                window,
                app
            ))
            .unwrap()
            .unwrap()
    );
    shell
        .window()
        .update(cx, |root, window, _| {
            assert_eq!(window.minimum_size_for_test(), Some(protected));
            assert_eq!(root.controller().unwrap().minimum_size(), protected);
        })
        .unwrap();
    shell
        .window()
        .update(cx, |root, window, app| {
            window.fail_next_minimum_update_for_test(false);
            root.test_commit_panel_minimum(window, app)
        })
        .unwrap()
        .unwrap();
    draw(shell.window(), cx);
    shell
        .window()
        .update(cx, |root, window, _| {
            assert!(root.controller().unwrap().minimum_size().height < protected.height);
            assert_eq!(window.minimum_size_for_test(), Some(protected));
        })
        .unwrap();
    let after = editor_snapshot(&shell, cx);
    assert_eq!(after.selection, original.selection);
    assert_eq!(after.binding, original.binding);
    assert_eq!(after.source_selection, original.source_selection);
    cx.update(|app| shell.publish(app)).unwrap();
    assert!(cx.window_visibility(shell.window().into()).is_visible);
}

#[gpui::test]
fn unchanged_appearance_commit_preserves_original_pending_relaxation_until_settlement(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(96);
            let prepared = fixture.prepare();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("shell"))
    });
    draw(shell.window(), cx);
    let original = editor_snapshot(&shell, cx);
    for _ in 0..32 {
        if shell
            .window()
            .update(cx, |root, window, app| {
                root.test_prepare_panel_minimum(gpui::px(200.), window, app)
            })
            .unwrap()
            .unwrap()
        {
            break;
        }
        draw(shell.window(), cx);
    }
    shell
        .window()
        .update(cx, |root, window, app| {
            root.test_commit_panel_minimum(window, app)
        })
        .unwrap()
        .unwrap();
    draw(shell.window(), cx);
    let protected = shell
        .window()
        .read_with(cx, |root, _| root.controller().unwrap().minimum_size())
        .unwrap();
    cx.update(|app| {
        shell
            .window()
            .update(app, |root, window, app| {
                assert!(
                    root.test_prepare_panel_minimum(gpui::px(48.), window, app)
                        .unwrap()
                );
                root.test_commit_panel_minimum(window, app).unwrap();
                assert_eq!(root.test_minimum_update_status(), (protected, true));
            })
            .unwrap();
        beryl_app::main_window::MainWindowShellRoot::test_apply_appearance(
            shell.window(),
            fixture.appearance.clone(),
            app,
        );
        shell
            .window()
            .read_with(app, |root, _| {
                assert_eq!(root.test_minimum_update_status(), (protected, true));
            })
            .unwrap();
    });
    draw(shell.window(), cx);
    shell
        .window()
        .update(cx, |root, window, _| {
            let committed = root.controller().unwrap().minimum_size();
            assert!(committed.height < protected.height);
            assert_eq!(root.test_minimum_update_status(), (committed, false));
            assert_eq!(window.minimum_size_for_test(), Some(committed));
        })
        .unwrap();
    let after = editor_snapshot(&shell, cx);
    assert_eq!(after.selection, original.selection);
    assert_eq!(after.binding, original.binding);
    assert_eq!(after.source_selection, original.source_selection);
}

#[gpui::test]
fn minimized_candidate_waits_for_ordinary_restoration_of_the_original_window(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(95);
            let prepared = fixture.prepare();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let mut shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("shell"))
    });
    draw(shell.window(), cx);
    cx.update(|app| shell.publish(app)).unwrap();
    let original = shell.window().entity(cx).unwrap().entity_id();
    let committed = shell
        .window()
        .read_with(cx, |root, _| root.controller().unwrap().minimum_size())
        .unwrap();
    shell
        .window()
        .update(cx, |_, window, _| window.minimize_window())
        .unwrap();
    let minimized = cx.window_visibility(shell.window().into());
    assert!(minimized.is_minimized);
    assert!(
        !shell
            .window()
            .update(cx, |root, window, app| root.test_prepare_panel_minimum(
                gpui::px(200.),
                window,
                app
            ))
            .unwrap()
            .unwrap()
    );
    draw(shell.window(), cx);
    assert!(
        !shell
            .window()
            .update(cx, |root, window, app| root.test_prepare_panel_minimum(
                gpui::px(200.),
                window,
                app
            ))
            .unwrap()
            .unwrap()
    );
    let retained = cx.window_visibility(shell.window().into());
    assert!(retained.is_minimized);
    assert_eq!(
        retained.activation_change_count,
        minimized.activation_change_count
    );
    assert_eq!(
        retained.minimize_change_count,
        minimized.minimize_change_count
    );
    shell
        .window()
        .update(cx, |root, window, _| {
            assert_eq!(root.controller().unwrap().minimum_size(), committed);
            assert_eq!(window.minimum_size_for_test(), Some(committed));
            window.activate_window();
        })
        .unwrap();
    draw(shell.window(), cx);
    assert!(!cx.window_visibility(shell.window().into()).is_minimized);
    for _ in 0..32 {
        if shell
            .window()
            .update(cx, |root, window, app| {
                root.test_prepare_panel_minimum(gpui::px(200.), window, app)
            })
            .unwrap()
            .unwrap()
        {
            break;
        }
        draw(shell.window(), cx);
    }
    shell
        .window()
        .update(cx, |root, window, app| {
            root.test_commit_panel_minimum(window, app)
        })
        .unwrap()
        .unwrap();
    assert_eq!(shell.window().entity(cx).unwrap().entity_id(), original);
    assert!(
        shell
            .window()
            .read_with(cx, |root, _| root
                .controller()
                .unwrap()
                .minimum_size()
                .height
                > committed.height)
            .unwrap()
    );
}
