use std::time::Duration;

use beryl_backend::{
    DynamicToolCallResponse, DynamicToolCallResponseDisposition, ManagedBackendError,
    ManagedBackendSession, ResponseWorkError,
};
use tungstenite::Message;

use super::support::sink_harness;
use super::transport_support::{connect_foreground, expect_close_without_text, spawn_server};

const CALL: &str = r#"{"method":"item/tool/call","id":81,"params":{"threadId":"thread_1","turnId":"turn_1","callId":"call_1","tool":"lookup","arguments":null}}"#;

#[test]
fn dynamic_response_custody_survives_ingress_and_completes_only_after_response_write() {
    let (endpoint, server) = spawn_server(|socket| {
        socket.send(Message::Text(CALL.into())).unwrap();
        let Message::Text(response) = socket.read().unwrap() else {
            panic!("expected response")
        };
        serde_json::from_str::<serde_json::Value>(&response).unwrap()
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(false);
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();
    session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap();
    let call = harness.dynamic_calls.try_recv().unwrap();
    let observer = harness.observations.try_recv().unwrap();
    let completion = harness.completions.try_recv().unwrap();
    assert!(call.is_sealed());
    let before = observer.snapshot().unwrap();
    assert!(!before.response_written());
    assert_eq!(before.retained_capabilities(), 1);
    let response = DynamicToolCallResponse::success_text("done");
    let mut foreign =
        ManagedBackendSession::unsupported_streamed_input_gate_for_lifecycle_test().unwrap();
    assert!(matches!(
        foreign.respond_dynamic_tool_call(&call, &response),
        Err(ManagedBackendError::DynamicToolResponseAuthorityMismatch)
    ));
    assert_eq!(observer.snapshot().unwrap(), before);
    assert_eq!(completion.count(), 0);
    session.respond_dynamic_tool_call(&call, &response).unwrap();
    assert_eq!(
        call.response_disposition(),
        DynamicToolCallResponseDisposition::Responded
    );
    let written = observer.snapshot().unwrap();
    assert!(written.response_written());
    assert_eq!(written.retained_capabilities(), 1);
    assert_eq!(completion.count(), 1);
    assert_eq!(completion.snapshot(), written);
    assert_eq!(
        observer.validate_revision(before.revision()),
        Err(ResponseWorkError::StaleRevision)
    );
    assert!(matches!(
        session.respond_dynamic_tool_call(&call, &response),
        Err(ManagedBackendError::DynamicToolResponseAlreadySent)
    ));
    assert_eq!(observer.snapshot().unwrap(), written);
    drop(call);
    let released = observer.snapshot().unwrap();
    assert!(released.response_written());
    assert_eq!(released.retained_capabilities(), 0);
    assert_eq!(completion.count(), 1);
    assert_eq!(server.join().unwrap()["id"], 81);
}

#[test]
fn failed_dynamic_response_write_remains_unwritten_until_capability_release() {
    let (endpoint, server) = spawn_server(|socket| {
        socket.send(Message::Text(CALL.into())).unwrap();
        expect_close_without_text(socket);
    });
    let mut session = connect_foreground(endpoint, 2);
    session.enable_full_turn_stream_for_lifecycle_test();
    let harness = sink_harness(false);
    session.bind_ordered_turn_stream_sink(harness.sink).unwrap();
    session
        .poll_ordered_turn_stream_progress(Duration::from_secs(2))
        .unwrap();
    let call = harness.dynamic_calls.try_recv().unwrap();
    let observer = harness.observations.try_recv().unwrap();
    let completion = harness.completions.try_recv().unwrap();
    let before = observer.snapshot().unwrap();
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    assert!(
        session
            .respond_dynamic_tool_call(&call, &DynamicToolCallResponse::success_text("done"))
            .is_err()
    );
    assert_eq!(
        call.response_disposition(),
        DynamicToolCallResponseDisposition::ResponseRequired
    );
    assert_eq!(observer.snapshot().unwrap(), before);
    drop(session);
    assert_eq!(observer.snapshot().unwrap(), before);
    assert_eq!(completion.count(), 0);
    drop(call);
    let released = observer.snapshot().unwrap();
    assert!(!released.response_written());
    assert_eq!(released.retained_capabilities(), 0);
    assert_eq!(completion.count(), 1);
    assert_eq!(completion.snapshot(), released);
    server.join().unwrap();
}
