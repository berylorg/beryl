use super::*;
use gpui::Entity;
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Wake, Waker},
};

#[derive(Default)]
struct CompletionWake(AtomicUsize);

impl Wake for CompletionWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn poll<F: Future>(future: Pin<&mut F>, wake: &Arc<CompletionWake>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(&Waker::from(wake.clone())))
}

fn composer(
    shell: &MainWindowShell,
    cx: &mut gpui::TestAppContext,
) -> Entity<MainWindowConversationComposer> {
    shell
        .window()
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
        })
        .unwrap()
}

fn assert_terminal_gate(
    shell: &MainWindowShell,
    composer: &Entity<MainWindowConversationComposer>,
    cx: &mut gpui::TestAppContext,
) {
    shell
        .window()
        .update(cx, |root, window, cx| {
            assert!(root.startup_interaction_gated());
            composer.update(cx, |composer, cx| {
                assert!(composer.release_startup_widget(window, cx).is_err());
                assert!(
                    composer
                        .resume_after_widget_release_fence(window, cx)
                        .is_err()
                );
                assert!(!composer.gpui_input().read(cx).is_enabled());
            });
        })
        .unwrap();
}

#[gpui::test]
fn startup_widget_release_idle_is_exact_and_one_shot(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, mut shell) = startup_interaction::shell(cx, 101);
    let composer = composer(&shell, cx);
    shell
        .window()
        .update(cx, |_, window, cx| {
            assert!(
                composer
                    .update(cx, |composer, cx| {
                        composer.release_startup_widget(window, cx)
                    })
                    .is_err()
            );
        })
        .unwrap();
    cx.update(|app| shell.gate_startup_interaction(app))
        .unwrap();
    startup_interaction::drive(&shell, cx);
    assert!(cx.update(|app| shell.ready_to_publish(app)));
    let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
    let completion = shell
        .window()
        .update(cx, |_, window, cx| {
            composer.update(cx, |composer, cx| {
                composer.release_startup_widget(window, cx)
            })
        })
        .unwrap()
        .unwrap();
    let mut completion = Box::pin(completion);
    let Poll::Ready(Ok(released)) = poll(completion.as_mut(), &Arc::default()) else {
        panic!("idle widget release completes without another event-loop turn")
    };
    assert_eq!(released.selection(), selection);
    assert!(composer.read_with(cx, |composer, _| composer.test_widget_released()));
    assert_terminal_gate(&shell, &composer, cx);
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert!(matches!(
        fixture.session(selection.binding().candidate().draft_id(), 102),
        DraftEditorCandidateSessionReadOutcomeV1::Active(_)
    ));
}

#[gpui::test]
fn startup_widget_release_waits_for_admitted_dispatch_and_survives_observer_drop(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    for drop_observer in [false, true] {
        let (fixture, mut shell) =
            startup_interaction::shell(cx, if drop_observer { 121 } else { 111 });
        let composer = composer(&shell, cx);
        let gate = composer.read_with(cx, |composer, _| {
            composer.test_block_next_selected_dispatch()
        });
        cx.update(|app| shell.gate_startup_interaction(app))
            .unwrap();
        startup_interaction::drive(&shell, cx);
        assert!(
            gate.is_blocked(),
            "initial selected dispatch reaches the real worker gate"
        );
        assert!(composer.read_with(cx, |composer, _| composer.test_has_active_flight()));
        let selection = composer.read_with(cx, |composer, _| composer.selection_identity());
        let completion = shell
            .window()
            .update(cx, |_, window, cx| {
                composer.update(cx, |composer, cx| {
                    composer.release_startup_widget(window, cx)
                })
            })
            .unwrap()
            .unwrap();
        let wake = Arc::new(CompletionWake::default());
        let mut completion = Some(Box::pin(completion));
        assert!(poll(completion.as_mut().unwrap().as_mut(), &wake).is_pending());
        assert!(!composer.read_with(cx, |composer, _| composer.test_widget_released()));
        assert_terminal_gate(&shell, &composer, cx);
        if drop_observer {
            drop(completion.take());
        }
        gate.release();
        startup_interaction::drive(&shell, cx);
        assert!(composer.read_with(cx, |composer, _| composer.test_widget_released()));
        assert!(!composer.read_with(cx, |composer, _| composer.test_has_active_flight()));
        if let Some(mut completion) = completion {
            assert!(
                wake.0.load(Ordering::SeqCst) > 0,
                "settlement wakes its completion observer"
            );
            let Poll::Ready(Ok(released)) = poll(completion.as_mut(), &wake) else {
                panic!("admitted dispatch settles before the release receipt succeeds")
            };
            assert_eq!(released.selection(), selection);
        }
        assert_terminal_gate(&shell, &composer, cx);
        assert_eq!(fixture.process.main_window_occupancy(), 1);
        assert!(
            shell
                .window()
                .read_with(cx, |root, _| root.controller().is_some())
                .unwrap()
        );
    }
}

#[gpui::test]
fn startup_widget_release_pending_failure_wakes_observer_and_retains_shell(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, mut shell) = startup_interaction::shell(cx, 141);
    let composer = composer(&shell, cx);
    let gate = composer.read_with(cx, |composer, _| {
        composer.test_block_next_selected_dispatch()
    });
    cx.update(|app| shell.gate_startup_interaction(app))
        .unwrap();
    startup_interaction::drive(&shell, cx);
    assert!(gate.is_blocked());
    assert!(composer.read_with(cx, |composer, _| composer.test_has_active_flight()));
    let completion = shell
        .window()
        .update(cx, |_, window, cx| {
            composer.update(cx, |composer, cx| {
                composer.release_startup_widget(window, cx)
            })
        })
        .unwrap()
        .unwrap();
    let wake = Arc::new(CompletionWake::default());
    let mut completion = Box::pin(completion);
    assert!(poll(completion.as_mut(), &wake).is_pending());
    assert!(!composer.read_with(cx, |composer, _| composer.test_widget_released()));
    composer.update(cx, |composer, cx| {
        composer.test_set_terminal_error("failure while dispatch was pending".to_owned(), cx);
    });
    gate.release();
    startup_interaction::drive(&shell, cx);
    assert!(
        wake.0.load(Ordering::SeqCst) > 0,
        "dispatch settlement wakes the failed release observer"
    );
    let Poll::Ready(Err(error)) = poll(completion.as_mut(), &wake) else {
        panic!("pending terminal failure must settle the release observer with an error")
    };
    assert!(
        error.contains("failure while dispatch was pending"),
        "{error}"
    );
    assert!(!composer.read_with(cx, |composer, _| composer.test_has_active_flight()));
    assert!(!composer.read_with(cx, |composer, _| composer.test_widget_released()));
    assert_terminal_gate(&shell, &composer, cx);
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert!(
        shell
            .window()
            .read_with(cx, |root, _| root.controller().is_some())
            .unwrap()
    );
}

#[gpui::test]
fn startup_widget_release_terminal_failure_never_proves_release(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, mut shell) = startup_interaction::shell(cx, 131);
    cx.update(|app| shell.gate_startup_interaction(app))
        .unwrap();
    startup_interaction::drive(&shell, cx);
    let composer = composer(&shell, cx);
    let completion = shell
        .window()
        .update(cx, |_, window, cx| {
            composer.update(cx, |composer, cx| {
                composer.test_set_terminal_error("selected dispatch failed".to_owned(), cx);
                composer.release_startup_widget(window, cx)
            })
        })
        .unwrap()
        .unwrap();
    let mut completion = Box::pin(completion);
    let Poll::Ready(Err(error)) = poll(completion.as_mut(), &Arc::default()) else {
        panic!("terminal failure must complete with an error")
    };
    assert!(error.contains("selected dispatch failed"), "{error}");
    assert!(!composer.read_with(cx, |composer, _| composer.test_widget_released()));
    assert_terminal_gate(&shell, &composer, cx);
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert!(
        shell
            .window()
            .read_with(cx, |root, _| root.controller().is_some())
            .unwrap()
    );
}
