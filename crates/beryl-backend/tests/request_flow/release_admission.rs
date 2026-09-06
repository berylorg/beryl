use std::path::Path;

use super::support::{
    CONFIG_CWD, TIMEOUT, assert_initialize, assert_initialized, connector, expect_close, read_json,
    send_config_response, send_initialize_response, send_json, spawn_server,
};

#[test]
fn request_only_lifecycle_release_admission_sends_exactly_one_config_read() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let config = read_json(socket).unwrap();
        assert_eq!(config["jsonrpc"], "2.0");
        assert_eq!(config["id"], 2);
        assert_eq!(config["method"], "config/read");
        assert_eq!(config["params"]["cwd"], CONFIG_CWD);
        assert_eq!(config["params"]["includeLayers"], false);
        assert!(config["params"].get("threadId").is_none());
        assert!(config["params"].get("turnId").is_none());
        assert!(config["params"].get("input").is_none());
        send_config_response(socket, 2);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();

    let defaults = session
        .admit_release_non_authorizing_for_lifecycle_test(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap();
    assert_eq!(defaults.model(), Some("gpt-5.6"));
    assert_eq!(defaults.model_reasoning_effort(), Some("high"));
    assert!(defaults.proves_release_admission());

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn release_admission_fails_closed_when_a_required_setting_is_false() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let config = read_json(socket).unwrap();
        assert_eq!(config["id"], 2);
        assert_eq!(config["method"], "config/read");
        assert!(config["params"].get("threadId").is_none());
        assert!(config["params"].get("input").is_none());
        send_json(
            socket,
            r#"{"id":2,"result":{"config":{"model":null,"model_reasoning_effort":null,"features":{"multi_agent_v2":{"enabled":false,"expose_spawn_agent_model_overrides":true}}},"origins":{"features.multi_agent_v2.enabled":{"name":{"type":"sessionFlags"},"version":"0"},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{"name":{"type":"sessionFlags"},"version":"0"}}}}"#,
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();

    let _error = session
        .admit_release_non_authorizing_for_lifecycle_test(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn release_admission_fails_closed_when_a_required_origin_is_not_session_flags() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let config = read_json(socket).unwrap();
        assert_eq!(config["id"], 2);
        assert_eq!(config["method"], "config/read");
        assert!(config["params"].get("threadId").is_none());
        assert!(config["params"].get("input").is_none());
        send_json(
            socket,
            r#"{"id":2,"result":{"config":{"model":null,"model_reasoning_effort":null,"features":{"multi_agent_v2":{"enabled":true,"expose_spawn_agent_model_overrides":true}}},"origins":{"features.multi_agent_v2.enabled":{"name":{"type":"userConfig"},"version":"0"},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{"name":{"type":"sessionFlags"},"version":"0"}}}}"#,
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();

    let _error = session
        .admit_release_non_authorizing_for_lifecycle_test(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}
