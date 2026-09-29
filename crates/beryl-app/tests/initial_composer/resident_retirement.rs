use super::*;
use gpui::{AppContext, EntityInputHandler, Focusable};

#[gpui::test]
fn shutdown_retirement_retains_refused_service_and_preserves_exact_resident(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, shell) = startup_interaction::shell(cx, 101);
    verify_retirement(cx, fixture, shell, true);
}

#[gpui::test]
fn restored_shutdown_retirement_releases_construction_service(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, shell, _attempt, _service) = restored_native::shell_for_recovery(cx);
    verify_retirement(cx, fixture, shell, true);
}

#[gpui::test]
fn untouched_empty_acquired_resident_retires(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, shell) = startup_interaction::shell(cx, 101);
    verify_retirement(cx, fixture, shell, false);
}

#[gpui::test]
fn untouched_empty_restored_resident_retires(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, shell, _attempt, _service) = restored_native::shell_for_empty_recovery(cx);
    verify_retirement(cx, fixture, shell, false);
}

fn verify_retirement(
    cx: &mut gpui::TestAppContext,
    fixture: Fixture,
    shell: MainWindowShell,
    edit: bool,
) {
    let (_other_fixture, other) = startup_interaction::shell(cx, 111);
    startup_interaction::drive(&shell, cx);
    let original = shell
        .window()
        .read_with(cx, |root, _| {
            let controller = root.controller().unwrap();
            (controller.window_id(), controller.placement().clone())
        })
        .unwrap();
    let occupancy = fixture.process.main_window_occupancy();
    let mount = shell
        .window()
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap()
        })
        .unwrap();
    let resident = mount.read_with(cx, |mount, _| mount.contribution().unwrap());
    let input = resident.read_with(cx, |resident, _| resident.gpui_input());
    if !edit {
        assert_eq!(
            mount.read_with(cx, |mount, _| mount
                .selected_identity()
                .unwrap()
                .binding()
                .logical_extent()
                .logical_utf8_bytes()),
            0
        );
    }
    let mut draft = shell
        .window()
        .update(cx, |root, window, cx| {
            if edit {
                input.update(cx, |input, cx| {
                    input.replace_text_in_range(None, "preserved resident draft", window, cx);
                });
            }
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            input.focus_handle(cx).focus(window);
            root.test_begin_shutdown_draft(window, cx).unwrap()
        })
        .unwrap();
    let close = draft.test_ticket().unwrap();
    for pass in 0..512 {
        startup_interaction::drive(&shell, cx);
        cx.run_until_parked();
        let ready = shell
            .window()
            .update(cx, |root, window, cx| {
                let state = root
                    .test_advance_shutdown_draft(&draft, window, cx)
                    .unwrap();
                let quiescent = input.read(cx).is_quiescent();
                assert!(
                    pass < 511,
                    "close did not settle: {state:?}, quiescent={quiescent}, {:?}",
                    input.read(cx).realization_diagnostics().current
                );
                state
                    == MainWindowShutdownDraftAdvance::Resident(
                        MainWindowConversationComposerCloseAdvance::Ready,
                    )
                    && quiescent
            })
            .unwrap();
        if ready {
            break;
        }
    }
    other
        .window()
        .update(cx, |root, _, cx| {
            root.test_set_shutdown_interaction_gated(true, cx).unwrap();
            assert!(root.test_retire_shutdown_draft(&mut draft, cx).is_err());
        })
        .unwrap();
    let worker = mount.read_with(cx, |mount, _| {
        mount.test_window_close_worker(|| {}).unwrap()
    });
    shell
        .window()
        .update(cx, |root, _, cx| {
            assert!(!matches!(
                root.test_retire_shutdown_draft(&mut draft, cx),
                Ok(true)
            ));
            assert!(mount.read(cx).selected_identity().is_some());
            assert!(!root.test_shell_construction_retired());
        })
        .unwrap();
    drop(worker);
    let held = mount.update(cx, |mount, cx| {
        assert!(mount.fence_interrupted_exit_resident(close, cx).unwrap());
        resident
            .update(cx, |resident, cx| {
                resident.detach_recovery_service(close, cx)
            })
            .unwrap()
            .unwrap()
    });
    let store = fixture.store.clone();
    let faults = fixture.faults.clone();
    home_support::join(
        home_support::worker(move || {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }),
        cx,
    );
    shell
        .window()
        .update(cx, |root, window, cx| {
            let focus = window.focused(cx);
            assert!(!root.test_retire_shutdown_draft(&mut draft, cx).unwrap());
            assert!(!root.test_retire_shutdown_draft(&mut draft, cx).unwrap());
            assert!(!root.test_shell_construction_retired());
            assert_eq!(window.focused(cx), focus);
            assert_eq!(mount.read(cx).contribution().unwrap(), resident);
            assert!(mount.read(cx).selected_identity().is_none());
            assert!(!input.read(cx).is_enabled());
            assert!(
                resident
                    .read(cx)
                    .recovery_snapshot()
                    .unwrap()
                    .retired_close()
                    .is_none()
            );
        })
        .unwrap();
    drop(held);
    shell
        .window()
        .update(cx, |root, window, cx| {
            let focus = window.focused(cx);
            assert!(root.test_retire_shutdown_draft(&mut draft, cx).unwrap());
            assert!(root.test_retire_shutdown_draft(&mut draft, cx).unwrap());
            assert!(root.test_shell_construction_retired());
            let controller = root.controller().unwrap();
            assert_eq!(controller.window_id(), original.0);
            assert_eq!(controller.placement(), &original.1);
            assert!(!controller.is_threadless());
            assert_eq!(fixture.process.main_window_occupancy(), occupancy);
            assert!(root.test_set_shutdown_interaction_gated(false, cx).is_err());
            assert_eq!(window.focused(cx), focus);
            assert_eq!(resident.read(cx).gpui_input(), input);
            assert!(!input.read(cx).is_enabled());
            assert!(
                root.test_release_shutdown_draft(&draft, window, cx)
                    .is_err()
            );
            let retired = mount
                .update(cx, |mount, cx| {
                    mount.take_interrupted_exit_retirement(close, cx)
                })
                .unwrap()
                .unwrap();
            assert!(!root.test_retire_shutdown_draft(&mut draft, cx).unwrap());
            mount
                .update(cx, |mount, cx| {
                    mount.accept_interrupted_exit_retirement(close, retired, cx)
                })
                .ok()
                .unwrap();
            assert!(root.test_retire_shutdown_draft(&mut draft, cx).unwrap());
            assert!(
                input
                    .update(cx, |input, cx| input.dispose(window, cx))
                    .is_empty()
            );
            window.remove_window();
        })
        .unwrap();
    other
        .window()
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}
