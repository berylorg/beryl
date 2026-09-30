use super::*;
use beryl_app::theme_runtime::GpuiAppearanceWindowSet;
use gpui::{AppContext, EntityInputHandler};
use std::num::NonZeroUsize;

pub(super) fn shell(cx: &mut gpui::TestAppContext, seed: u8) -> (Fixture, MainWindowShell) {
    let (fixture, prepared) = home_support::join(
        home_support::worker(move || {
            let fixture = Fixture::new(seed);
            let mut initial = fixture.begin(seed + 1);
            initial.advance(&CommandCancellation::new()).unwrap();
            let prepared = prepared_shell(&fixture, initial);
            (fixture, prepared)
        }),
        cx,
    );
    let shell = cx.update(|app| {
        let appearance = GpuiAppearanceWindowSet::new(
            prepared.appearance().clone(),
            NonZeroUsize::new(4).unwrap(),
            app,
        );
        GpuiMainWindowShellHost::new(app, appearance)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("hidden startup shell"))
    });
    (fixture, shell)
}

pub(super) fn drive(shell: &MainWindowShell, cx: &mut gpui::TestAppContext) {
    for _ in 0..32 {
        cx.run_until_parked();
        cx.update(|app| {
            app.update_window(shell.window().into(), |_, window, app| {
                window.draw(app).clear()
            })
            .unwrap()
        });
    }
}

#[gpui::test]
fn startup_gate_blocks_mutation_commands_and_lifecycle_reenable_then_restores_editing(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (_fixture, mut shell) = shell(cx, 31);
    cx.update(|app| shell.gate_startup_interaction(app))
        .unwrap();
    assert!(
        cx.update(|app| shell.gate_startup_interaction(app))
            .is_err()
    );
    drive(&shell, cx);
    assert!(cx.update(|app| shell.ready_to_publish(app)));
    let mount = shell
        .window()
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap()
        })
        .unwrap();
    let composer = mount.read_with(cx, |mount, _| mount.contribution().unwrap());
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    shell
        .window()
        .update(cx, |root, _, cx| {
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            root.test_set_shutdown_interaction_gated(false, cx).unwrap();
        })
        .unwrap();
    shell
        .window()
        .update(cx, |root, window, cx| {
            assert!(root.startup_interaction_gated());
            assert_eq!(
                root.test_exit_presentation(),
                ("Exit", "Beryl is preparing its windows.")
            );
            assert_eq!(
                root.new_window_disabled_reason(cx).as_deref(),
                Some("Beryl is preparing its windows.")
            );
            assert!(!input.read(cx).is_enabled());
            assert!(
                mount
                    .update(cx, |mount, cx| mount.begin_window_close(window, cx))
                    .is_err()
            );
            let selection = composer.read(cx).selection_identity();
            let generation = mount.read(cx).test_submission_start_generation();
            mount
                .update(cx, |mount, cx| {
                    mount.test_begin_submission_start(selection, window, cx)
                })
                .unwrap();
            assert_eq!(
                mount.read(cx).test_submission_start_generation(),
                generation
            );
            input.update(cx, |_, cx| {
                cx.emit(gpui_text_input::RangeTextInputEvent::CommandPropagated(
                    gpui_text_input::TextInputCommand::Enter,
                ));
                cx.emit(gpui_text_input::RangeTextInputEvent::CommandPropagated(
                    gpui_text_input::TextInputCommand::Paste,
                ));
                cx.emit(gpui_text_input::RangeTextInputEvent::CommandPropagated(
                    gpui_text_input::TextInputCommand::Cut,
                ));
            });
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "blocked", window, cx)
            });
            composer.update(cx, |composer, cx| {
                composer.begin_widget_release_fence(window, cx).unwrap();
                composer
                    .resume_after_widget_release_fence(window, cx)
                    .unwrap();
            });
            assert!(!input.read(cx).is_enabled());
            root.set_notices_inert(false, window, cx);
        })
        .unwrap();
    drive(&shell, cx);
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.test_submission_start_generation()),
        0
    );
    let ingress = shell
        .window()
        .update(cx, |root, window, cx| root.notice_ingress(window, cx))
        .unwrap();
    let window_id = shell
        .window()
        .read_with(cx, |root, _| root.controller().unwrap().window_id())
        .unwrap();
    let record = NoticeRecord {
        window_id,
        condition: NoticeConditionId::new(),
        revision: 1,
        kind: NoticeKind::Warning,
        content: NoticeContent::new(
            NoticeVariant::Warning,
            NoticeDismissal::Dismissible,
            "Startup",
            "Startup",
        ),
    };
    assert!(matches!(
        cx.update(|app| ingress.admit(record, app)),
        NoticeAdmission::Admitted(_)
    ));
    let visible = shell
        .window()
        .read_with(cx, |root, _| {
            root.notice_projection().unwrap().token.clone()
        })
        .unwrap();
    assert!(matches!(
        cx.update(
            |app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(visible.clone()), app)
        ),
        Err(MainWindowNoticeRouteRejection::Inert)
    ));
    assert!(input.read_with(cx, |input, _| {
        input
            .surface()
            .unwrap()
            .pages()
            .iter()
            .all(|page| page.text().is_empty())
    }));
    cx.update(|app| shell.publish(app)).unwrap();
    let shell = cx
        .update(|app| shell.release_published_handle(app))
        .err()
        .expect("gated shell retains startup custody");
    let window = shell.window();
    shell
        .window()
        .update(cx, |root, _, cx| {
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            assert_eq!(
                root.new_window_disabled_reason(cx).as_deref(),
                Some("Application Exit is waiting for active work and durable state.")
            );
        })
        .unwrap();
    cx.update(|app| {
        MainWindowShell::release_startup_interaction(std::slice::from_ref(&shell), app)
    })
    .unwrap();
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(visible), app))
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "blocked", window, cx)
            });
        })
        .unwrap();
    drive(&shell, cx);
    assert!(input.read_with(cx, |input, _| {
        input
            .surface()
            .unwrap()
            .pages()
            .iter()
            .all(|page| page.text().is_empty())
    }));
    shell
        .window()
        .update(cx, |root, window, cx| {
            composer.update(cx, |composer, cx| {
                composer.begin_widget_release_fence(window, cx).unwrap();
            });
            assert!(root.test_set_shutdown_interaction_gated(false, cx).is_err());
            assert!(root.test_set_shutdown_interaction_gated(true, cx).is_err());
            assert_eq!(
                root.test_exit_presentation(),
                (
                    "Exiting…",
                    "Application Exit is waiting for active work and durable state."
                )
            );
            assert_eq!(
                root.new_window_disabled_reason(cx).as_deref(),
                Some("Application Exit is waiting for active work and durable state.")
            );
            composer.update(cx, |composer, cx| {
                composer
                    .resume_after_widget_release_fence(window, cx)
                    .unwrap();
            });
            root.test_set_shutdown_interaction_gated(false, cx).unwrap();
            assert_ne!(
                root.new_window_disabled_reason(cx).as_deref(),
                Some("Application Exit is waiting for active work and durable state.")
            );
            assert_eq!(
                root.test_exit_presentation(),
                ("Exit", "Application Exit is not available.")
            );
        })
        .unwrap();
    window
        .update(cx, |root, window, cx| {
            assert!(!root.startup_interaction_gated());
            assert!(input.read(cx).is_enabled());
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "accepted", window, cx)
            });
        })
        .unwrap();
    drive(&shell, cx);
    assert!(input.read_with(cx, |input, _| {
        input
            .surface()
            .unwrap()
            .pages()
            .iter()
            .any(|page| page.text() == "accepted")
    }));
    cx.update(|app| shell.release_published_handle(app))
        .unwrap_or_else(|_| panic!("released ordinary shell"));
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}

#[gpui::test]
fn exit_toolbar_keeps_its_mount_and_explains_waiting_without_accepting_input(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (_fixture, mut shell) = shell(cx, 91);
    cx.update(|app| shell.gate_startup_interaction(app))
        .unwrap();
    drive(&shell, cx);
    cx.update(|app| shell.publish(app)).unwrap();
    let mut visual = gpui::VisualTestContext::from_window(shell.window().into(), cx);
    let mut previous_bounds = None;
    for waiting in [false, true, false] {
        shell
            .window()
            .update(cx, |root, _, cx| {
                root.test_set_shutdown_interaction_gated(waiting, cx)
                    .unwrap();
                let expected = if waiting {
                    (
                        "Exiting…",
                        "Application Exit is waiting for active work and durable state.",
                    )
                } else {
                    ("Exit", "Beryl is preparing its windows.")
                };
                assert_eq!(root.test_exit_presentation(), expected);
            })
            .unwrap();
        drive(&shell, cx);
        let bounds = visual
            .debug_bounds("main-window-exit")
            .expect("Exit toolbar mount");
        if let Some(previous) = previous_bounds {
            assert_eq!(
                bounds, previous,
                "Exit retains its toolbar position and geometry"
            );
        }
        previous_bounds = Some(bounds);
        visual.simulate_mouse_move(bounds.center(), None, gpui::Modifiers::none());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(501));
        drive(&shell, cx);
        assert!(visual.debug_bounds("main-window-exit-tooltip").is_some());
        visual.simulate_mouse_down(
            bounds.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        visual.simulate_mouse_up(
            bounds.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        visual.simulate_keystrokes("enter space");
        drive(&shell, cx);
        shell
            .window()
            .update(cx, |root, _, cx| {
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(cx).contribution().unwrap();
                assert!(!composer.read(cx).gpui_input().read(cx).is_enabled());
                assert_eq!(
                    root.test_exit_presentation().0,
                    if waiting { "Exiting…" } else { "Exit" }
                );
            })
            .unwrap();
        visual.simulate_mouse_move(
            gpui::point(gpui::px(0.), gpui::px(100.)),
            None,
            gpui::Modifiers::none(),
        );
    }
    cx.update(|app| {
        MainWindowShell::release_startup_interaction(std::slice::from_ref(&shell), app)
    })
    .unwrap();
    drive(&shell, cx);
    shell
        .window()
        .read_with(cx, |root, _| {
            assert_eq!(
                root.test_exit_presentation(),
                ("Exit", "Application Exit is not available.")
            );
        })
        .unwrap();
    let window = shell.window();
    cx.update(|app| shell.release_published_handle(app))
        .unwrap_or_else(|_| panic!("released shell"));
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}

#[gpui::test]
fn startup_release_failure_regates_prepared_prefix_without_handing_off_members(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (_first, mut first) = shell(cx, 51);
    let (_second, mut second) = shell(cx, 61);
    for shell in [&mut first, &mut second] {
        cx.update(|app| shell.gate_startup_interaction(app))
            .unwrap();
        drive(shell, cx);
        cx.update(|app| shell.publish(app)).unwrap();
    }
    second
        .window()
        .update(cx, |root, _, cx| {
            let mount = root.controller().unwrap().composer_mount().unwrap();
            let composer = mount.read(cx).contribution().unwrap();
            composer.update(cx, |composer, cx| {
                composer.test_set_terminal_error("late startup failure".to_owned(), cx)
            });
        })
        .unwrap();
    let shells = [first, second];
    assert!(
        cx.update(|app| MainWindowShell::release_startup_interaction(&shells, app))
            .is_err()
    );
    for shell in &shells {
        shell
            .window()
            .read_with(cx, |root, app| {
                assert!(root.startup_interaction_gated());
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(app).contribution().unwrap();
                assert!(!composer.read(app).gpui_input().read(app).is_enabled());
            })
            .unwrap();
    }
}

#[gpui::test]
fn shutdown_release_validates_all_windows_before_clearing_any_gate(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (_first, mut first) = shell(cx, 71);
    let (_second, mut second) = shell(cx, 81);
    for shell in [&mut first, &mut second] {
        cx.update(|app| shell.gate_startup_interaction(app))
            .unwrap();
        drive(shell, cx);
        cx.update(|app| shell.publish(app)).unwrap();
        shell
            .window()
            .update(cx, |root, _, cx| {
                root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            })
            .unwrap();
    }
    let windows = [first.window(), second.window()];
    second
        .window()
        .update(cx, |root, window, cx| {
            let mount = root.controller().unwrap().composer_mount().unwrap();
            let composer = mount.read(cx).contribution().unwrap();
            composer.update(cx, |composer, cx| {
                composer.begin_widget_release_fence(window, cx).unwrap();
            });
        })
        .unwrap();
    assert!(
        cx.update(
            |app| MainWindowShellRoot::test_release_shutdown_interaction_gates_after(
                &windows,
                app,
                || panic!("invalid windows must not settle process recovery")
            )
        )
        .is_err()
    );
    for window in windows {
        window
            .read_with(cx, |root, app| {
                assert_eq!(
                    root.new_window_disabled_reason(app).as_deref(),
                    Some("Application Exit is waiting for active work and durable state.")
                );
            })
            .unwrap();
    }
    second
        .window()
        .update(cx, |root, window, cx| {
            let mount = root.controller().unwrap().composer_mount().unwrap();
            let composer = mount.read(cx).contribution().unwrap();
            composer.update(cx, |composer, cx| {
                composer
                    .resume_after_widget_release_fence(window, cx)
                    .unwrap();
            });
        })
        .unwrap();
    let calls = std::cell::Cell::new(0);
    assert_eq!(
        cx.update(|app| {
            MainWindowShellRoot::test_release_shutdown_interaction_gates_after(
                &windows,
                app,
                || {
                    calls.set(calls.get() + 1);
                    Err("process settlement refused".to_owned())
                },
            )
        }),
        Err("process settlement refused".to_owned())
    );
    assert_eq!(calls.get(), 1);
    for window in windows {
        window
            .read_with(cx, |root, app| {
                assert_eq!(
                    root.new_window_disabled_reason(app).as_deref(),
                    Some("Application Exit is waiting for active work and durable state.")
                );
            })
            .unwrap();
    }
    cx.update(|app| {
        MainWindowShellRoot::test_release_shutdown_interaction_gates_after(&windows, app, || {
            calls.set(calls.get() + 1);
            Ok(())
        })
    })
    .unwrap();
    assert_eq!(calls.get(), 2);
    for window in windows {
        window
            .read_with(cx, |root, app| {
                assert_ne!(
                    root.new_window_disabled_reason(app).as_deref(),
                    Some("Application Exit is waiting for active work and durable state.")
                );
                assert!(root.startup_interaction_gated());
                let mount = root.controller().unwrap().composer_mount().unwrap();
                let composer = mount.read(app).contribution().unwrap();
                assert!(!composer.read(app).gpui_input().read(app).is_enabled());
            })
            .unwrap();
    }
    for shell in [&first, &second] {
        cx.update(|app| {
            MainWindowShell::release_startup_interaction(std::slice::from_ref(shell), app)
        })
        .unwrap();
    }
    first
        .window()
        .update(cx, |root, _, cx| {
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
        })
        .unwrap();
    cx.update(|app| second.release_published_handle(app))
        .unwrap_or_else(|_| panic!("released second shell"));
    windows[1]
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    assert!(
        cx.update(
            |app| MainWindowShellRoot::test_release_shutdown_interaction_gates_after(
                &windows,
                app,
                || panic!("invalid windows must not settle process recovery")
            )
        )
        .is_err()
    );
    first
        .window()
        .read_with(cx, |root, app| {
            assert_eq!(
                root.new_window_disabled_reason(app).as_deref(),
                Some("Application Exit is waiting for active work and durable state.")
            );
        })
        .unwrap();
    assert!(
        cx.update(
            |app| MainWindowShellRoot::test_release_shutdown_interaction_gates_after(
                &[],
                app,
                || panic!("empty windows must not settle process recovery")
            )
        )
        .is_err()
    );
    cx.update(|app| first.release_published_handle(app))
        .unwrap_or_else(|_| panic!("released first shell"));
    windows[0]
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}
