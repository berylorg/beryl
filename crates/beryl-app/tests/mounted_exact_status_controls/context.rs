use super::*;

fn wire(thread: &str, turn: &str, input: &str, window: &str) -> String {
    let breakdown = format!(
        r#"{{"totalTokens":99,"inputTokens":{input},"cachedInputTokens":1,"cacheWriteInputTokens":2,"outputTokens":3,"reasoningOutputTokens":4}}"#
    );
    format!(
        r#"{{"method":"thread/tokenUsage/updated","params":{{"threadId":"{thread}","turnId":"{turn}","tokenUsage":{{"total":{breakdown},"last":{breakdown},"modelContextWindow":{window}}}}},"emittedAtMs":1770000000123}}"#
    )
}

fn wait_context(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    expected: Option<u8>,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        support::draw(window, cx);
        if window
            .read_with(cx, |root, _| root.test_context_remaining_percent())
            .unwrap()
            == expected
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "context status failed to converge to {expected:?}"
        );
        cx.executor().advance_clock(Duration::from_millis(250));
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn exercise(
    server: &server::NormalTerminalServer,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::TestAppContext,
    worker: ExactStopWorker,
    thread: SyndicThreadId,
) {
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_context_remaining_percent())
            .unwrap(),
        None
    );
    server.send_observation(wire(
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID,
        "50",
        "200",
    ));
    wait_context(window, cx, Some(75));
    let held = support::join(
        support::worker(move || worker.selected_operation_snapshot(thread)),
        cx,
    );
    server.send_observation(wire(
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID,
        "-1",
        "200",
    ));
    wait_context(window, cx, None);
    server.send_observation(wire(
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID,
        "50",
        "200",
    ));
    wait_context(window, cx, Some(75));
    let stamp = window
        .read_with(cx, |root, app| {
            root.test_exact_status_observation_stamp(app)
        })
        .unwrap();
    window
        .update(cx, |root, window, cx| {
            root.test_apply_exact_status_observation(stamp, held, window, cx);
            assert_eq!(root.test_context_remaining_percent(), None);
        })
        .unwrap();
    wait_context(window, cx, Some(75));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    let bounds = visual
        .debug_bounds("main-window-status-context:75")
        .unwrap();
    assert_eq!(bounds.size.width, gpui::px(180.));
    drop(visual);
    for (thread, turn) in [
        ("another-thread", server::CAS_TURN_ID),
        (server::CAS_THREAD_ID, "another-turn"),
    ] {
        server.send_observation(wire(thread, turn, "0", "200"));
    }
    server.send_observation(wire(
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID,
        "100",
        "200",
    ));
    wait_context(window, cx, Some(50));
    server.send_observation(wire(
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID,
        "-1",
        "200",
    ));
    wait_context(window, cx, None);
    server.send_observation(wire(
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID,
        "50",
        "null",
    ));
    server.send_observation(wire(server::CAS_THREAD_ID, server::CAS_TURN_ID, "0", "200"));
    wait_context(window, cx, Some(100));
    server.send_observation(r#"{"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"gpt-5.6","limitName":"gpt-5.6","primary":{"usedPercent":20,"windowDurationMins":300,"resetsAt":null},"secondary":{"usedPercent":30,"windowDurationMins":10080,"resetsAt":0}}}}"#.to_owned());
    server.send_observation(wire(
        server::CAS_THREAD_ID,
        server::CAS_TURN_ID,
        "25",
        "200",
    ));
    wait_context(window, cx, Some(87));
}
