use super::*;

#[test]
fn native_captured_branch_qualification_obeys_shared_worker_capacity() {
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
                let leases = window
                    .read_with(cx, |root, _| root.test_fill_transcript_worker_capacity())
                    .unwrap();
                assert!(!leases.is_empty());
                for _ in 0..4 {
                    open(window, &panel, cx);
                    wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| panel.transcript_menu_rows_for_test().iter().any(|row| row.0 == "Discuss in new branch" && !row.1 && row.2.as_deref().is_some_and(|reason| reason.contains("busy")))).unwrap(), "branch qualification must expose bounded busy state when shared workers are full").await;
                    assert!(
                        window
                            .read_with(cx, |root, _| root.test_transcript_worker_retained() <= 9
                                && root.test_captured_branch_state().is_none())
                            .unwrap()
                    );
                    with_native(window, cx, |native, app| {
                        key(native, app, "end");
                        key(native, app, "enter");
                        key(native, app, "escape");
                    })
                    .unwrap();
                }
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                drop(leases);
                invoke(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app)
                                    .is_some_and(|thread| thread != parent)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "qualification must become available after shared workers release",
                )
                .await;
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_transcript_worker_retained() <= 9)
                        .unwrap()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
