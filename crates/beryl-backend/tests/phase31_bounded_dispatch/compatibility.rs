use std::path::Path;

use beryl_backend::{CompatibilityProbe, ManagedBackendError};
use beryl_model::CasThreadId;

use super::support::{
    CONFIG_CWD, TIMEOUT, assert_initialize, assert_initialized, connector, expect_close, read_json,
    send_config_response, send_empty_model_page, send_initialize_response, send_json,
    send_recognized_rejection, spawn_server,
};

const NIL_ID: &str = "00000000-0000-0000-0000-000000000000";

#[test]
fn request_only_profile_executes_the_exact_non_authorizing_compatibility_sequence() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        for (index, probe) in CompatibilityProbe::ALL.into_iter().enumerate() {
            let id = u64::try_from(index).unwrap() + 2;
            let request = read_json(socket).unwrap();
            assert_probe_request(&request, id, probe);
            match probe {
                CompatibilityProbe::ConfigRead => send_config_response(socket, id),
                CompatibilityProbe::ModelList => send_empty_model_page(socket, id),
                CompatibilityProbe::ThreadUnsubscribe => send_json(
                    socket,
                    &format!(r#"{{"id":{id},"result":{{"status":"notLoaded"}}}}"#),
                ),
                _ => send_recognized_rejection(socket, id),
            }
        }
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    assert!(!session.has_full_turn_stream());

    let (probe_successes, config_defaults, thread_branch_capabilities) = session
        .probe_non_authorizing_compatibility_for_lifecycle_test(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap();
    assert!(probe_successes.is_complete());
    assert_eq!(config_defaults.model(), Some("gpt-5.6"));
    assert_eq!(config_defaults.model_reasoning_effort(), Some("high"));
    assert!(thread_branch_capabilities.thread_fork());
    assert!(thread_branch_capabilities.thread_rollback());

    let before_wrong_profile = session.predispatch_state_for_lifecycle_test();
    let error = session.initialize_foreground(TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::InitializationProfileMismatch {
            profile: "foreground"
        }
    ));
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        before_wrong_profile
    );

    let target = CasThreadId::new("request-only-unsubscribe-target").unwrap();
    let before_unsubscribe = session.predispatch_state_for_lifecycle_test();
    let error = session.unsubscribe_thread(&target, TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestProfileMismatch {
            method: "thread/unsubscribe",
            required_profile: "foreground",
        }
    ));
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        before_unsubscribe
    );

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn unexpected_mutating_probe_success_fails_non_authorizing_sequence_and_retires_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let config = read_json(socket).unwrap();
        assert_probe_request(&config, 2, CompatibilityProbe::ConfigRead);
        send_config_response(socket, 2);
        let models = read_json(socket).unwrap();
        assert_probe_request(&models, 3, CompatibilityProbe::ModelList);
        send_empty_model_page(socket, 3);
        let compact = read_json(socket).unwrap();
        assert_probe_request(&compact, 4, CompatibilityProbe::ThreadCompactStart);
        send_json(socket, r#"{"id":4,"result":{}}"#);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();

    let error = session
        .probe_non_authorizing_compatibility_for_lifecycle_test(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::CompatibilityMutatingSuccess {
            probe: CompatibilityProbe::ThreadCompactStart
        }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

fn assert_probe_request(request: &serde_json::Value, id: u64, probe: CompatibilityProbe) {
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], id);
    assert_eq!(request["method"], probe.method());
    let params = &request["params"];
    match probe {
        CompatibilityProbe::ConfigRead => {
            assert_eq!(params["cwd"], CONFIG_CWD);
            assert_eq!(params["includeLayers"], false);
        }
        CompatibilityProbe::ModelList => {
            assert!(params.get("cursor").is_none());
            assert_eq!(params["limit"], 64);
            assert_eq!(params["includeHidden"], false);
        }
        CompatibilityProbe::ThreadFork => {
            assert_eq!(params["threadId"], NIL_ID);
            assert_eq!(params["cwd"], CONFIG_CWD);
            assert_eq!(params["excludeTurns"], true);
            assert_eq!(params["ephemeral"], false);
        }
        CompatibilityProbe::ThreadInjectItems => {
            assert_eq!(params["threadId"], NIL_ID);
            let item = &params["items"][0];
            assert_eq!(item["type"], "message");
            assert_eq!(item["role"], "user");
            assert_eq!(item["content"][0]["type"], "input_text");
            assert_eq!(item["content"][0]["text"], "Beryl compatibility probe");
        }
        CompatibilityProbe::ThreadResume => {
            assert_eq!(params["threadId"], NIL_ID);
            assert_eq!(params["cwd"], CONFIG_CWD);
            assert_eq!(params["excludeTurns"], true);
        }
        CompatibilityProbe::ThreadRollback => {
            assert_eq!(params["threadId"], NIL_ID);
            assert_eq!(params["numTurns"], 1);
        }
        CompatibilityProbe::TurnInterrupt => {
            assert_eq!(params["threadId"], NIL_ID);
            assert_eq!(params["turnId"], NIL_ID);
        }
        CompatibilityProbe::TurnStart => {
            assert_eq!(params["threadId"], NIL_ID);
            assert_eq!(params["input"][0]["type"], "text");
            assert_eq!(params["input"][0]["text"], "Beryl compatibility probe");
        }
        CompatibilityProbe::TurnSteer => {
            assert_eq!(params["threadId"], NIL_ID);
            assert_eq!(params["expectedTurnId"], NIL_ID);
            assert_eq!(params["input"][0]["type"], "text");
            assert_eq!(params["input"][0]["text"], "Beryl compatibility probe");
        }
        CompatibilityProbe::ThreadCompactStart | CompatibilityProbe::ThreadUnsubscribe => {
            assert_eq!(params["threadId"], NIL_ID);
        }
    }
}
