use std::path::Path;

use beryl_backend::{
    DynamicToolFunctionSpec, ThreadApprovalPolicy, ThreadLoadOptions, ThreadSandboxMode,
    ThreadStartOptions, ThreadStatus,
};
use beryl_model::{CasThreadId, CasTurnId};
use serde_json::{Value, json};

use super::{
    send_lineage_response,
    support::{
        CONFIG_CWD, TIMEOUT, assert_initialize, assert_initialized, connector, expect_close,
        foreground_config, read_json, send_initialize_response, send_json, spawn_server,
    },
};

#[test]
fn all_lineage_methods_write_exact_params_and_publish_only_validated_identity() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let start = read_json(socket).unwrap();
        assert_eq!(start["jsonrpc"], "2.0");
        assert_eq!(start["id"], 2);
        assert_eq!(start["method"], "thread/start");
        assert_eq!(start["params"]["cwd"], CONFIG_CWD);
        assert_eq!(start["params"]["model"], "gpt-5.6");
        assert_eq!(start["params"]["modelProvider"], "openai");
        assert_eq!(start["params"]["approvalPolicy"], "on-request");
        assert_eq!(start["params"]["sandbox"], "workspace-write");
        assert_eq!(
            start["params"]["developerInstructions"],
            "start instructions"
        );
        assert_eq!(start["params"]["ephemeral"], true);
        assert_eq!(start["params"]["dynamicTools"][0]["type"], "function");
        assert_eq!(start["params"]["dynamicTools"][0]["name"], "lookup");
        assert_eq!(
            start["params"]["dynamicTools"][0]["inputSchema"]["type"],
            "object"
        );
        send_json(
            socket,
            r#"{"method":"unknown/progress","params":{"discard":["one","two"]}}"#,
        );
        send_lineage_response(socket, 2, "thread-started", r#"{"type":"idle"}"#, false);

        let resume = read_json(socket).unwrap();
        assert_load_request(&resume, 3, "thread/resume", "thread-source");
        assert!(resume["params"].get("ephemeral").is_none());
        assert!(resume["params"].get("lastTurnId").is_none());
        send_lineage_response(
            socket,
            3,
            "thread-source",
            r#"{"type":"active","activeFlags":["waitingOnUserInput"]}"#,
            true,
        );

        let fork = read_json(socket).unwrap();
        assert_load_request(&fork, 4, "thread/fork", "thread-source");
        assert_eq!(fork["params"]["ephemeral"], false);
        assert!(fork["params"].get("lastTurnId").is_none());
        send_lineage_response(
            socket,
            4,
            "thread-forked",
            r#"{"type":"systemError"}"#,
            false,
        );

        let through = read_json(socket).unwrap();
        assert_load_request(&through, 5, "thread/fork", "thread-source");
        assert_eq!(through["params"]["ephemeral"], false);
        assert_eq!(through["params"]["lastTurnId"], "turn-cut");
        send_lineage_response(socket, 5, "thread-through", r#"{"type":"idle"}"#, false);
        expect_close(socket);
    });

    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();

    let start_options = ThreadStartOptions::ephemeral()
        .with_model("gpt-5.6")
        .with_model_provider("openai")
        .with_approval_policy(ThreadApprovalPolicy::OnRequest)
        .with_sandbox(ThreadSandboxMode::WorkspaceWrite)
        .with_developer_instructions("start instructions")
        .with_dynamic_tool(
            DynamicToolFunctionSpec::new("lookup", "Lookup one value", json!({"type": "object"}))
                .into(),
        );
    let started = session
        .start_thread_with_options(Path::new(CONFIG_CWD), start_options, TIMEOUT)
        .unwrap();
    assert_eq!(started.thread_id().as_str(), "thread-started");
    assert_eq!(started.status(), &ThreadStatus::Idle);
    assert_eq!(started.metadata().model.as_deref(), Some("gpt-5.6"));
    assert_eq!(started.metadata().model_provider.as_deref(), Some("openai"));
    assert_eq!(started.metadata().reasoning_effort.as_deref(), Some("high"));

    let source = CasThreadId::new("thread-source").unwrap();
    let load_options = ThreadLoadOptions::for_root(CONFIG_CWD)
        .with_model("gpt-5.6-mini")
        .with_model_provider("openai")
        .with_approval_policy(ThreadApprovalPolicy::Never)
        .with_sandbox(ThreadSandboxMode::ReadOnly)
        .with_developer_instructions("load instructions");
    let resumed = session
        .resume_thread(&source, &load_options, TIMEOUT)
        .unwrap();
    assert_eq!(resumed.thread_id(), &source);
    assert!(resumed.status().waiting_on_user_input());

    let forked = session
        .fork_thread(&source, &load_options, TIMEOUT)
        .unwrap();
    assert_eq!(forked.thread_id().as_str(), "thread-forked");
    assert_eq!(forked.status(), &ThreadStatus::SystemError);

    let last_turn = CasTurnId::new("turn-cut").unwrap();
    let through = session
        .fork_thread_through_turn(&source, &last_turn, &load_options, TIMEOUT)
        .unwrap();
    assert_eq!(through.thread_id().as_str(), "thread-through");
    assert_eq!(through.status(), &ThreadStatus::Idle);
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn empty_start_options_omit_all_optional_fields() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "thread/start");
        assert_eq!(request["params"]["cwd"], CONFIG_CWD);
        assert_eq!(request["params"]["ephemeral"], false);
        for name in [
            "model",
            "modelProvider",
            "approvalPolicy",
            "sandbox",
            "developerInstructions",
            "dynamicTools",
        ] {
            assert!(request["params"].get(name).is_none(), "{name} was present");
        }
        send_lineage_response(socket, 2, "thread-default", r#"{"type":"idle"}"#, false);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let started = session
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap();
    assert_eq!(started.thread_id().as_str(), "thread-default");
    session.shutdown().unwrap();
    server.join().unwrap();
}

fn assert_load_request(request: &Value, id: u64, method: &str, source: &str) {
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], id);
    assert_eq!(request["method"], method);
    assert_eq!(request["params"]["threadId"], source);
    assert_eq!(request["params"]["model"], "gpt-5.6-mini");
    assert_eq!(request["params"]["modelProvider"], "openai");
    assert_eq!(request["params"]["cwd"], CONFIG_CWD);
    assert_eq!(request["params"]["approvalPolicy"], "never");
    assert_eq!(request["params"]["sandbox"], "read-only");
    assert_eq!(
        request["params"]["developerInstructions"],
        "load instructions"
    );
    assert_eq!(request["params"]["excludeTurns"], true);
}
