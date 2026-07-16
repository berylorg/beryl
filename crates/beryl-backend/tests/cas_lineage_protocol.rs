use std::{path::Path, thread, time::Duration};

use beryl_backend::{
    FreshIdleThread, ManagedBackendError, ManagedBackendSession, THREAD_INJECTION_MAX_ITEMS,
    THREAD_INJECTION_MAX_TEXT_BYTES, ThreadApprovalPolicy, ThreadInjectionBatch,
    ThreadInjectionBatchError, ThreadInjectionItem, ThreadInjectionMessageText,
    ThreadInjectionMessageTextError, ThreadInjectionOutcome, ThreadLoadOptions, ThreadSandboxMode,
    ThreadStatus,
};
use beryl_model::{CasThreadId, CasTurnId};
use serde_json::json;

#[path = "support/cas_lineage.rs"]
mod cas_lineage_support;

use cas_lineage_support::*;

#[test]
fn thread_start_normalizes_loaded_state_and_metadata_without_turn_bodies() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/start"));
        assert_eq!(
            request["params"],
            json!({
                "cwd": EXECUTION_ROOT,
                "ephemeral": false
            })
        );
        send_result(
            &mut socket,
            2,
            lineage_result("thread_fresh", json!({ "type": "idle" })),
        );
    });

    let mut client = connect(endpoint);
    let fresh = client
        .start_thread(Path::new(EXECUTION_ROOT), REQUEST_TIMEOUT)
        .unwrap();

    assert_eq!(fresh.thread_id().as_str(), "thread_fresh");
    assert_eq!(fresh.status(), &ThreadStatus::Idle);
    assert_lineage_metadata(fresh.metadata());
    let idle = fresh.into_idle().unwrap();
    assert_eq!(idle.thread_id().as_str(), "thread_fresh");

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn resume_forks_and_rollback_use_exact_lineage_params_and_metadata_only_results() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);

        let common = json!({
            "threadId": "thread_source",
            "cwd": EXECUTION_ROOT,
            "excludeTurns": true,
            "model": "gpt-5.5",
            "modelProvider": "openai",
            "developerInstructions": "Preserve exact native lineage.",
            "approvalPolicy": "on-request",
            "sandbox": "workspace-write"
        });

        let resume = read_json(&mut socket);
        assert_eq!(resume["id"], json!(2));
        assert_eq!(resume["method"], json!("thread/resume"));
        assert_eq!(resume["params"], common);
        send_result(
            &mut socket,
            2,
            lineage_result("thread_source", json!({ "type": "idle" })),
        );

        let full_fork = read_json(&mut socket);
        assert_eq!(full_fork["id"], json!(3));
        assert_eq!(full_fork["method"], json!("thread/fork"));
        let mut expected_full_fork = common.clone();
        expected_full_fork["ephemeral"] = json!(false);
        assert_eq!(full_fork["params"], expected_full_fork);
        send_result(
            &mut socket,
            3,
            lineage_result("thread_full_fork", json!({ "type": "idle" })),
        );

        let prefix_fork = read_json(&mut socket);
        assert_eq!(prefix_fork["id"], json!(4));
        assert_eq!(prefix_fork["method"], json!("thread/fork"));
        let mut expected_prefix_fork = expected_full_fork;
        expected_prefix_fork["lastTurnId"] = json!("turn_terminal");
        assert_eq!(prefix_fork["params"], expected_prefix_fork);
        send_result(
            &mut socket,
            4,
            lineage_result("thread_prefix_fork", json!({ "type": "idle" })),
        );

        let rollback = read_json(&mut socket);
        assert_eq!(rollback["id"], json!(5));
        assert_eq!(rollback["method"], json!("thread/rollback"));
        assert_eq!(
            rollback["params"],
            json!({
                "threadId": "thread_prefix_fork",
                "numTurns": 2
            })
        );
        send_result(
            &mut socket,
            5,
            lineage_result("thread_prefix_fork", json!({ "type": "idle" })),
        );
    });

    let mut client = connect(endpoint);
    let source = CasThreadId::new("thread_source").unwrap();
    let prefix = CasTurnId::new("turn_terminal").unwrap();
    let options = ThreadLoadOptions::for_root(EXECUTION_ROOT)
        .with_model("gpt-5.5")
        .with_model_provider("openai")
        .with_developer_instructions("Preserve exact native lineage.")
        .with_approval_policy(ThreadApprovalPolicy::OnRequest)
        .with_sandbox(ThreadSandboxMode::WorkspaceWrite);

    let resumed = client
        .resume_thread(&source, &options, REQUEST_TIMEOUT)
        .unwrap();
    assert_eq!(resumed.thread_id().as_str(), "thread_source");
    assert_lineage_metadata(resumed.metadata());

    let full_fork = client
        .fork_thread(&source, &options, REQUEST_TIMEOUT)
        .unwrap();
    assert_eq!(full_fork.thread_id().as_str(), "thread_full_fork");
    assert_lineage_metadata(full_fork.metadata());

    let prefix_fork = client
        .fork_thread_through_turn(&source, &prefix, &options, REQUEST_TIMEOUT)
        .unwrap();
    assert_eq!(prefix_fork.thread_id().as_str(), "thread_prefix_fork");
    assert_lineage_metadata(prefix_fork.metadata());

    let prefix_thread = CasThreadId::new("thread_prefix_fork").unwrap();
    let rolled_back = client
        .rollback_thread(&prefix_thread, 2, REQUEST_TIMEOUT)
        .unwrap();
    assert_eq!(rolled_back.thread_id(), &prefix_thread);
    assert_lineage_metadata(rolled_back.metadata());

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn fresh_active_and_not_loaded_threads_cannot_become_injection_targets() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);

        for (request_id, thread_id, status) in [
            (
                2,
                "thread_active",
                json!({ "type": "active", "activeFlags": [] }),
            ),
            (3, "thread_not_loaded", json!({ "type": "notLoaded" })),
        ] {
            let request = read_json(&mut socket);
            assert_eq!(request["id"], json!(request_id));
            assert_eq!(request["method"], json!("thread/start"));
            send_result(&mut socket, request_id, lineage_result(thread_id, status));
        }
    });

    let mut client = connect(endpoint);
    let active = client
        .start_thread(Path::new(EXECUTION_ROOT), REQUEST_TIMEOUT)
        .unwrap()
        .into_idle()
        .unwrap_err();
    assert_eq!(active.thread_id().as_str(), "thread_active");
    assert!(matches!(active.status(), ThreadStatus::Active { .. }));

    let not_loaded = client
        .start_thread(Path::new(EXECUTION_ROOT), REQUEST_TIMEOUT)
        .unwrap()
        .into_idle()
        .unwrap_err();
    assert_eq!(not_loaded.thread_id().as_str(), "thread_not_loaded");
    assert_eq!(not_loaded.status(), &ThreadStatus::NotLoaded);

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn injection_sends_exact_ordered_closed_envelope_with_escaping_and_utf8() {
    let user_text = "quote: \"exact\"; path: C:\\root; newline:\n; nul:\0; \u{017e}lu\u{0165}ou\u{010d}k\u{00fd} \u{1f9ea}";
    let assistant_text = "assistant says: \u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\nsecond line";
    let batch = ThreadInjectionBatch::new(vec![
        ThreadInjectionItem::user_input_text(user_text).unwrap(),
        ThreadInjectionItem::assistant_output_text(assistant_text).unwrap(),
        ThreadInjectionItem::user_input_text("final user item").unwrap(),
    ])
    .unwrap();

    let outcome = run_injection_case(batch, REQUEST_TIMEOUT, move |socket, raw, request| {
        assert!(raw.contains(r#"\"exact\""#));
        assert!(raw.contains(r#"C:\\root"#));
        assert!(raw.contains(r#"\n"#));
        assert!(raw.contains(r#"\u0000"#));
        assert!(raw.contains("\u{017e}lu\u{0165}ou\u{010d}k\u{00fd} \u{1f9ea}"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_fresh",
                "items": [
                    {
                        "type": "message",
                        "role": "user",
                        "content": [{ "type": "input_text", "text": user_text }]
                    },
                    {
                        "type": "message",
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": assistant_text }]
                    },
                    {
                        "type": "message",
                        "role": "user",
                        "content": [{ "type": "input_text", "text": "final user item" }]
                    }
                ]
            })
        );
        send_result(socket, 3, json!({}));
    });

    match outcome {
        ThreadInjectionOutcome::Succeeded { thread } => {
            assert_eq!(thread.thread_id().as_str(), "thread_fresh");
            assert_eq!(thread.status(), &ThreadStatus::Idle);
            assert_lineage_metadata(thread.metadata());
        }
        other => panic!("expected exact injection success, got {other:?}"),
    }
}

#[test]
fn injection_preserves_one_65703_byte_assistant_item() {
    const FRAMED_ASSISTANT_BYTES: usize = 65_703;

    let assistant_text = "A".repeat(FRAMED_ASSISTANT_BYTES);
    let expected_text = assistant_text.clone();
    let batch = ThreadInjectionBatch::new(vec![
        ThreadInjectionItem::assistant_output_text(assistant_text).unwrap(),
    ])
    .unwrap();
    assert_eq!(batch.canonical_text_bytes(), FRAMED_ASSISTANT_BYTES);

    let outcome = run_injection_case(batch, REQUEST_TIMEOUT, move |socket, _, request| {
        let text = request["params"]["items"][0]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert_eq!(text.len(), FRAMED_ASSISTANT_BYTES);
        assert_eq!(text, expected_text);
        assert_eq!(request["params"]["items"][0]["role"], json!("assistant"));
        assert_eq!(
            request["params"]["items"][0]["content"][0]["type"],
            json!("output_text")
        );
        send_result(socket, 3, json!({}));
    });

    assert!(matches!(outcome, ThreadInjectionOutcome::Succeeded { .. }));
}

#[test]
fn injection_preserves_structured_rejection_without_a_retry_capability() {
    let outcome = run_injection_case(single_item_batch(), REQUEST_TIMEOUT, |socket, _, _| {
        send_error(
            socket,
            3,
            -32602,
            "thread must be loaded and idle",
            Some(json!({ "reason": "notLoaded" })),
        );
    });

    match outcome {
        ThreadInjectionOutcome::Rejected {
            thread_id,
            rejection,
        } => {
            assert_eq!(thread_id.as_str(), "thread_fresh");
            assert_eq!(rejection.code(), -32602);
            assert_eq!(rejection.message(), "thread must be loaded and idle");
            assert!(rejection.data_was_present());
        }
        other => panic!("expected structured injection rejection, got {other:?}"),
    }
}

#[test]
fn injection_classifies_wrong_response_id_timeout_as_unknown_completion() {
    let timeout = Duration::from_millis(30);
    let outcome = run_injection_case(single_item_batch(), timeout, move |socket, _, _| {
        send_result(socket, 300, json!({}));
        thread::sleep(Duration::from_millis(90));
    });

    match outcome {
        ThreadInjectionOutcome::CompletionUnknown { thread_id, error } => {
            assert_eq!(thread_id.as_str(), "thread_fresh");
            assert!(matches!(
                *error,
                ManagedBackendError::RequestTimeout { ref method, timeout: actual }
                    if method == "thread/inject_items" && actual == timeout
            ));
        }
        other => panic!("expected unknown completion after wrong-id timeout, got {other:?}"),
    }
}

#[test]
fn injection_classifies_transport_disconnect_without_completion_as_lost() {
    let outcome = run_injection_case(single_item_batch(), REQUEST_TIMEOUT, |_, _, _| {});

    match outcome {
        ThreadInjectionOutcome::TransportLost { thread_id, error } => {
            assert_eq!(thread_id.as_str(), "thread_fresh");
            assert!(matches!(
                *error,
                ManagedBackendError::TransportClosed { .. }
                    | ManagedBackendError::WebSocketTransport { .. }
                    | ManagedBackendError::ReadTransport { .. }
            ));
        }
        other => panic!("expected transport-lost injection outcome, got {other:?}"),
    }
}

#[test]
fn injection_classifies_nonempty_success_result_as_unknown_completion() {
    let outcome = run_injection_case(single_item_batch(), REQUEST_TIMEOUT, |socket, _, _| {
        send_result(socket, 3, json!({ "unexpected": true }));
    });

    match outcome {
        ThreadInjectionOutcome::CompletionUnknown { thread_id, error } => {
            assert_eq!(thread_id.as_str(), "thread_fresh");
            assert!(matches!(
                *error,
                ManagedBackendError::DeserializeResponse { ref method, .. }
                    if method == "thread/inject_items"
            ));
        }
        other => panic!("expected unknown completion for invalid result, got {other:?}"),
    }
}

#[test]
fn injection_validation_enforces_empty_byte_and_item_count_boundaries() {
    assert_eq!(
        ThreadInjectionMessageText::new("").unwrap_err(),
        ThreadInjectionMessageTextError::Empty
    );
    assert_eq!(
        ThreadInjectionBatch::new(Vec::new()).unwrap_err(),
        ThreadInjectionBatchError::Empty
    );

    let exact_text =
        ThreadInjectionMessageText::new("x".repeat(THREAD_INJECTION_MAX_TEXT_BYTES)).unwrap();
    assert_eq!(exact_text.byte_count(), THREAD_INJECTION_MAX_TEXT_BYTES);
    assert_eq!(
        ThreadInjectionMessageText::new("x".repeat(THREAD_INJECTION_MAX_TEXT_BYTES + 1))
            .unwrap_err(),
        ThreadInjectionMessageTextError::TooManyBytes {
            byte_count: THREAD_INJECTION_MAX_TEXT_BYTES + 1,
            max_bytes: THREAD_INJECTION_MAX_TEXT_BYTES,
        }
    );

    let exact_bytes = ThreadInjectionBatch::new(vec![
        ThreadInjectionItem::user_input_text("x".repeat(THREAD_INJECTION_MAX_TEXT_BYTES - 4))
            .unwrap(),
        ThreadInjectionItem::assistant_output_text("\u{1f9ea}").unwrap(),
    ])
    .unwrap();
    assert_eq!(
        exact_bytes.canonical_text_bytes(),
        THREAD_INJECTION_MAX_TEXT_BYTES
    );
    assert_eq!(
        ThreadInjectionBatch::new(vec![
            ThreadInjectionItem::user_input_text("x".repeat(THREAD_INJECTION_MAX_TEXT_BYTES))
                .unwrap(),
            ThreadInjectionItem::assistant_output_text("y").unwrap(),
        ])
        .unwrap_err(),
        ThreadInjectionBatchError::TooManyTextBytes {
            canonical_text_bytes: THREAD_INJECTION_MAX_TEXT_BYTES + 1,
            max_bytes: THREAD_INJECTION_MAX_TEXT_BYTES,
        }
    );

    let one_byte_item = ThreadInjectionItem::user_input_text("x").unwrap();
    let exact_count =
        ThreadInjectionBatch::new(vec![one_byte_item.clone(); THREAD_INJECTION_MAX_ITEMS]).unwrap();
    assert_eq!(exact_count.item_count(), THREAD_INJECTION_MAX_ITEMS);
    let mut too_many_items = exact_count.into_items().into_vec();
    too_many_items.push(one_byte_item);
    assert_eq!(
        ThreadInjectionBatch::new(too_many_items).unwrap_err(),
        ThreadInjectionBatchError::TooManyItems {
            item_count: THREAD_INJECTION_MAX_ITEMS + 1,
            max_items: THREAD_INJECTION_MAX_ITEMS,
        }
    );

    let _: fn(
        &mut ManagedBackendSession,
        FreshIdleThread,
        &ThreadInjectionBatch,
        Duration,
    ) -> ThreadInjectionOutcome = ManagedBackendSession::inject_thread_items;
}
