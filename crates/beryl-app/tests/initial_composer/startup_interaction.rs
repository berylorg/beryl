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
