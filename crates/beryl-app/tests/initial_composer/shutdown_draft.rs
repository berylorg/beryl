use super::*;
use gpui::{AppContext, EntityInputHandler};

#[gpui::test]
fn shutdown_draft_preserves_exact_resident_ticket_through_flush_and_release(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (_fixture, shell) = startup_interaction::shell(cx, 61);
    let (_other_fixture, other) = startup_interaction::shell(cx, 71);
    startup_interaction::drive(&shell, cx);
    let mount = shell
        .window()
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap()
        })
        .unwrap();
    let composer = mount.read_with(cx, |mount, _| mount.contribution().unwrap());
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let draft = shell
        .window()
        .update(cx, |root, window, cx| {
            assert!(root.test_begin_shutdown_draft(window, cx).is_err());
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "retained shutdown draft", window, cx)
            });
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            root.test_begin_shutdown_draft(window, cx).unwrap()
        })
        .unwrap();
    let ticket = draft.test_ticket().unwrap();
    other
        .window()
        .update(cx, |root, window, cx| {
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            assert!(
                root.test_advance_shutdown_draft(&draft, window, cx)
                    .is_err()
            );
            assert!(
                root.test_release_shutdown_draft(&draft, window, cx)
                    .is_err()
            );
        })
        .unwrap();
    let mut ready = false;
    for _ in 0..512 {
        startup_interaction::drive(&shell, cx);
        let state = shell
            .window()
            .update(cx, |root, window, cx| {
                let joined = root.test_begin_shutdown_draft(window, cx).unwrap();
                assert_eq!(joined.test_ticket(), Some(ticket));
                root.test_advance_shutdown_draft(&draft, window, cx)
                    .unwrap()
            })
            .unwrap();
        if state
            == MainWindowShutdownDraftAdvance::Resident(
                MainWindowConversationComposerCloseAdvance::Ready,
            )
        {
            ready = true;
            break;
        }
        assert!(matches!(
            state,
            MainWindowShutdownDraftAdvance::Resident(
                MainWindowConversationComposerCloseAdvance::Preparing
                    | MainWindowConversationComposerCloseAdvance::Progress(_)
                    | MainWindowConversationComposerCloseAdvance::ReconciliationPending
            )
        ));
    }
    assert!(ready, "resident shutdown draft did not become ready");
    assert!(!composer.read_with(cx, |composer, _| composer.test_widget_released()));
    assert!(input.read_with(cx, |input, _| {
        input
            .surface()
            .unwrap()
            .pages()
            .iter()
            .any(|page| page.text() == "retained shutdown draft")
    }));
    let mut released = false;
    for _ in 0..512 {
        startup_interaction::drive(&shell, cx);
        let state = shell
            .window()
            .update(cx, |root, window, cx| {
                root.test_release_shutdown_draft(&draft, window, cx)
                    .unwrap()
            })
            .unwrap();
        if state == MainWindowShutdownDraftRelease::Released {
            released = true;
            break;
        }
    }
    assert!(released, "resident close ticket did not release");
    shell
        .window()
        .update(cx, |_, window, cx| {
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "must remain rejected", window, cx);
            });
        })
        .unwrap();
    startup_interaction::drive(&shell, cx);
    assert!(input.read_with(cx, |input, _| {
        input
            .surface()
            .unwrap()
            .pages()
            .iter()
            .any(|page| page.text() == "retained shutdown draft")
    }));
    let other_resident = other
        .window()
        .read_with(cx, |root, cx| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(cx)
                .contribution()
                .unwrap()
        })
        .unwrap();
    shell
        .window()
        .update(cx, |root, window, cx| {
            assert_eq!(
                root.test_release_shutdown_draft(&draft, window, cx)
                    .unwrap(),
                MainWindowShutdownDraftRelease::Released
            );
            assert!(!composer.read(cx).test_widget_released());
            let original = mount.update(cx, |mount, _| {
                mount
                    .test_replace_window_close_resident(other_resident)
                    .unwrap()
            });
            assert!(
                root.test_release_shutdown_draft(&draft, window, cx)
                    .is_err()
            );
            assert!(
                root.test_advance_shutdown_draft(&draft, window, cx)
                    .is_err()
            );
            mount.update(cx, |mount, _| {
                mount.test_replace_window_close_resident(original);
            });
            assert_eq!(
                root.test_release_shutdown_draft(&draft, window, cx)
                    .unwrap(),
                MainWindowShutdownDraftRelease::Released
            );
            let successor = root.test_begin_shutdown_draft(window, cx).unwrap();
            assert_ne!(successor.test_ticket(), Some(ticket));
            assert!(
                root.test_release_shutdown_draft(&draft, window, cx)
                    .is_err()
            );
            root.test_release_shutdown_draft(&successor, window, cx)
                .unwrap();
        })
        .unwrap();
    for shell in [&shell, &other] {
        shell
            .window()
            .update(cx, |root, window, cx| {
                root.test_set_shutdown_interaction_gated(false, cx).unwrap();
                window.remove_window();
            })
            .unwrap();
    }
}

#[gpui::test]
fn shutdown_draft_rejects_startup_custody(cx: &mut gpui::TestAppContext) {
    let (_fixture, mut shell) = startup_interaction::shell(cx, 81);
    cx.update(|app| shell.gate_startup_interaction(app))
        .unwrap();
    shell
        .window()
        .update(cx, |root, window, cx| {
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            assert!(root.test_begin_shutdown_draft(window, cx).is_err());
            window.remove_window();
        })
        .unwrap();
}
