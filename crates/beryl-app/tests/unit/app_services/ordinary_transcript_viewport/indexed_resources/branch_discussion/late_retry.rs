use super::*;

fn cancel_next_activation(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    window
        .update(cx, |root, _, _| {
            root.test_thread_confirmation_hooks(
                Some(Box::new(|cancellation| cancellation.cancel())),
                None,
                None,
            )
        })
        .unwrap();
}

async fn settled_retry(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    wait_until(
        window,
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_captured_branch_state()
                        .is_some_and(|state| state.0 == "Retry")
                        && !root.test_running_thread_activation_pending()
                })
                .unwrap()
        },
        "ordinary activation must settle before checking the invoking selection",
    )
    .await;
}

#[test]
fn native_captured_branch_late_retry_failure_preserves_its_current_origin_selection() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed(path, SOURCE, true),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["branch prefix"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                let (copied, _) = writer(&panel, cx);
                cancel_next_activation(window, cx);
                invoke(window, &panel, cx).await;
                settled_retry(window, cx).await;
                let (_, first, original) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "escape");
                    let (id, _, _) = record(panel.read(app), "branch prefix");
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, 0),
                        point_at(panel.read(app), &id, "branch prefix".len()),
                    );
                    key(native, app, "shift-f10");
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel
                                    .transcript_menu_rows_for_test()
                                    .iter()
                                    .any(|row| row.0 == "Discuss in new branch" && row.1)
                            })
                            .unwrap()
                    },
                    "another exact source selection must qualify independently",
                )
                .await;
                cancel_next_activation(window, cx);
                with_native(window, cx, |native, app| {
                    key(native, app, "end");
                    key(native, app, "enter");
                })
                .unwrap();
                settled_retry(window, cx).await;
                let (_, second, later) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_ne!(first, second);
                assert_eq!(later.text.as_str(), "branch prefix");
                with_native(window, cx, |native, app| {
                    key(native, app, "escape");
                })
                .unwrap();
                open(window, &panel, cx);
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel
                                    .transcript_menu_rows_for_test()
                                    .iter()
                                    .any(|row| row.0 == "Retry" && row.1)
                            })
                            .unwrap()
                    },
                    "the original retained discussion must expose its own Retry",
                )
                .await;
                cancel_next_activation(window, cx);
                window
                    .update(cx, |root, _, _| {
                        root.test_captured_branch_hooks(
                            Some(Box::new(|_| {
                                panic!("late Retry must not create another discussion")
                            })),
                            None,
                        )
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "end");
                    key(native, app, "enter");
                })
                .unwrap();
                settled_retry(window, cx).await;
                let (_, retried, restored) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(retried, first);
                assert_eq!(restored, original);
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                with_native(window, cx, |native, app| {
                    key(native, app, "escape");
                    copy_both(native, app, &copied, SELECTED, "café 🌸 words");
                })
                .unwrap();
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
