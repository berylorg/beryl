use super::*;

fn multiline_code(ending: &'static str) {
    require_fixture_ledger();
    let selected = format!("one\ttwo{ending}{ending}third");
    let end = 15 + selected.len() as u64;
    let source = format!("Intro\n\n```text\n{selected}{ending}```\n");
    run_mounted_with_prepared_home_retained(
        1,
        0,
        move |path| fixture::seed(path, &source, true),
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                ready(window, &panel, cx, &["Intro"]).await;
                wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| panel.code_source_point_for_test(15).is_some() && panel.code_source_point_for_test(end).is_some() && panel.diagnostic_snapshot().code_panels[0].layout_state == "ready").unwrap(), "the Code selection including the blank logical line must be genuinely realized").await;
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                with_native(window, cx, |native, app| {
                    drag(
                        native,
                        app,
                        panel.read(app).code_source_point_for_test(15).unwrap(),
                        panel.read(app).code_source_point_for_test(end).unwrap(),
                    );
                    assert_eq!(panel.read(app).code_selection_for_test(), Some(15..end));
                    key(native, app, "shift-f10");
                })
                .unwrap();
                wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| panel.transcript_menu_rows_for_test().iter().any(|row| row.0 == "Discuss in new branch" && row.1)).unwrap(), "authenticated separators and raw Tab/text must form one exact contiguous Code capture").await;
                with_native(window, cx, |native, app| {
                    key(native, app, "end");
                    key(native, app, "enter");
                })
                .unwrap();
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
                    "the exact multiline Code discussion must complete ordinary activation",
                )
                .await;
                let child = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                {
                    let owner = owner.borrow();
                    let graph = owner.test_services().graph().unwrap();
                    let child = graph
                        .syndic()
                        .thread(graph.home(), child, fixture::limit())
                        .unwrap()
                        .unwrap();
                    let context = graph
                        .syndic()
                        .context_envelope(
                            graph.home(),
                            child.context_owner_id().unwrap(),
                            fixture::limit(),
                        )
                        .unwrap()
                        .unwrap();
                    assert_eq!(context.envelope().text().as_str(), selected);
                    assert_eq!(context.envelope().descriptor().source().range().start(), 15);
                    assert_eq!(context.envelope().descriptor().source().range().end(), end);
                }
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}

#[test]
fn native_captured_branch_code_multiline_preserves_lf_blank_line_and_tab() {
    multiline_code("\n");
}

#[test]
fn native_captured_branch_code_multiline_preserves_crlf_blank_line_and_tab() {
    multiline_code("\r\n");
}
