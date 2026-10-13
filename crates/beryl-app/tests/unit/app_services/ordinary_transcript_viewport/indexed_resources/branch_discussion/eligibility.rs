use super::*;

#[test]
fn native_captured_branch_incomplete_finalized_reply_stays_unavailable() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed_incomplete(path, SOURCE),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["branch prefix"]).await;
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let item = graph
                        .syndic()
                        .canonical_item(graph.home(), fixture::assistant(), fixture::limit())
                        .unwrap()
                        .unwrap();
                    let state = graph
                        .syndic()
                        .turn_state(graph.home(), item.turn_id(), fixture::limit())
                        .unwrap()
                        .unwrap();
                    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
                    assert!(state.finalized_item_count() >= item.ordinal().get());
                }
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                open(window, &panel, cx);
                wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| !panel.transcript_menu_rows_for_test().iter().any(|row| row.0 == "Discuss in new branch" && (row.1 || row.2.as_deref().is_none_or(|reason| !reason.contains("terminal reply"))))).unwrap(), "explicit incomplete reply must remain branch-unavailable despite finalized assistant text").await;
                assert!(
                    window
                        .read_with(cx, |root, _| root.test_captured_branch_state().is_none())
                        .unwrap()
                );
                assert_eq!(
                    window
                        .read_with(cx, |root, app| root.coherent_viewed_thread(app))
                        .unwrap(),
                    Some(parent)
                );
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
