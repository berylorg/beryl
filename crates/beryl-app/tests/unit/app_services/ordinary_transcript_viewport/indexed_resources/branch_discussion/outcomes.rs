use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand, test_faults::FaultBlock};
use gpui::Focusable;
use std::sync::{Arc, Mutex};

struct Held(FaultBlock);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[test]
fn native_captured_branch_activation_retry_opens_same_discussion_without_recreating() {
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
                window
                    .update(cx, |root, _, _| {
                        root.test_thread_confirmation_hooks(
                            Some(Box::new(|cancellation| cancellation.cancel())),
                            None,
                            None,
                        )
                    })
                    .unwrap();
                invoke(window, &panel, cx).await;
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
                    "a proven ordinary activation failure must expose same-discussion Retry",
                )
                .await;
                let (_, child, capture) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(capture.text.as_str(), SELECTED);
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                with_native(window, cx, |native, app| {
                key(native, app, "escape");
                assert!(panel.read(app).focus_handle(app).is_focused(native), "ordinary branch activation failure must restore transcript focus; selection={:?}", panel.read(app).narrative_selection_ranges_for_test());
                assert!(panel.read(app).narrative_selection_ranges_for_test().is_some(), "ordinary activation failure must preserve the original mapped selection");
                copy_both(native, app, &copied, SELECTED, "café 🌸 words");
                key(native, app, "shift-f10");
                assert_eq!(panel.read(app).transcript_menu_rows_for_test()[1], ("Retry".into(), true, None));
                key(native, app, "end"); key(native, app, "enter");
            }).unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app) == Some(child)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "Retry must activate the previously created discussion",
                )
                .await;
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                let owner = owner.borrow();
                let graph = owner.test_services().graph().unwrap();
                let thread = graph
                    .syndic()
                    .thread(graph.home(), child, fixture::limit())
                    .unwrap()
                    .unwrap();
                assert_eq!(thread.parent_thread_id(), Some(parent));
                let context = graph
                    .syndic()
                    .context_envelope(
                        graph.home(),
                        thread.context_owner_id().unwrap(),
                        fixture::limit(),
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(context.envelope().descriptor().source(), capture.source);
                assert_eq!(context.envelope().text(), &capture.text);
                drop(owner);
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_captured_branch_busy_and_reconciling_preserve_capture_and_reject_duplicate_input() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed(path, SOURCE, true),
        |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["branch prefix"]).await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                let held_slot = Arc::new(Mutex::new(None));
                let worker_slot = held_slot.clone();
                let create_faults = faults.clone();
                let reconcile = Held(faults.block_next(FaultPoint::BeforeReconciliationSnapshot));
                window
                    .update(cx, |root, _, _| {
                        root.test_captured_branch_hooks(
                            Some(Box::new(move |_| {
                                create_faults.fail_next(FaultPoint::AfterCommitBeforePersist);
                                *worker_slot.lock().unwrap() =
                                    Some(create_faults.block_next(FaultPoint::BeforeCommit));
                            })),
                            None,
                        )
                    })
                    .unwrap();
                let (copied, _) = writer(&panel, cx);
                invoke(window, &panel, cx).await;
                wait_until(
                    window,
                    cx,
                    |_| {
                        held_slot
                            .lock()
                            .unwrap()
                            .as_ref()
                            .is_some_and(|held: &FaultBlock| {
                                held.wait_until_reached(Duration::ZERO)
                            })
                    },
                    "captured branch must reach its exact commit cut",
                )
                .await;
                let creation = Held(held_slot.lock().unwrap().take().unwrap());
                let (_, child, original) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(original.text.as_str(), SELECTED);
                with_native(window, cx, |native, app| {
                    assert_eq!(
                        panel.read(app).transcript_menu_rows_for_test()[1].0,
                        "Creating discussion…"
                    );
                    assert!(!panel.read(app).transcript_menu_rows_for_test()[1].1);
                    key(native, app, "enter");
                    key(native, app, "enter");
                    key(native, app, "escape");
                    let (id, _, _) = record(panel.read(app), "branch prefix");
                    drag(
                        native,
                        app,
                        point_at(panel.read(app), &id, 0),
                        point_at(panel.read(app), &id, 6),
                    );
                    copy_both(native, app, &copied, SELECTED, "café 🌸 words");
                    key(native, app, "shift-f10");
                })
                .unwrap();
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                creation.0.release();
                wait_until(
                    window,
                    cx,
                    |_| reconcile.0.wait_until_reached(Duration::ZERO),
                    "the indeterminate branch must retain one reconciliation scope",
                )
                .await;
                let (state, same_child, capture) = window
                    .read_with(cx, |root, _| root.test_captured_branch_state().unwrap())
                    .unwrap();
                assert_eq!(state, "Reconciling");
                assert_eq!(child, same_child);
                assert_eq!(capture, original);
                with_native(window, cx, |native, app| {
                    assert_eq!(
                        panel.read(app).transcript_menu_rows_for_test()[1].0,
                        "Reconciling discussion…"
                    );
                    assert!(!panel.read(app).transcript_menu_rows_for_test()[1].1);
                    key(native, app, "end");
                    key(native, app, "enter");
                })
                .unwrap();
                reconcile.0.release();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        window
                            .read_with(cx, |root, app| {
                                root.coherent_viewed_thread(app) == Some(child)
                                    && !root.test_running_thread_activation_pending()
                            })
                            .unwrap()
                    },
                    "ExactNew must activate only the original captured discussion",
                )
                .await;
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
