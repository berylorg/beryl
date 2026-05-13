#[path = "../src/diagnostic_child_protocol.rs"]
mod diagnostic_child_protocol;

use std::io::{BufReader, Cursor};

use diagnostic_child_protocol::{
    BoundedLineRead, CLOSE_POPUPS_COMMAND, CREATE_NEW_THREAD_COMMAND, DiagnosticChildCommand,
    DiagnosticProtocolResponse, HARD_STOP_TURN_COMMAND, LIST_WORKSPACE_THREADS_COMMAND,
    MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES, READ_UI_STATE_COMMAND, SOFT_STOP_TURN_COMMAND,
    START_TURN_COMMAND, SWITCH_WORKSPACE_COMMAND, parse_request_frame, parse_response_frame,
    read_bounded_line_bytes, request_frame, response_frame,
};
use serde_json::json;

#[test]
fn request_frames_parse_and_preserve_request_identity() {
    let frame = br#"{"id":"req-1","command":"read_ui_state","params":{"limit":4}}"#;

    let request = parse_request_frame(frame).unwrap().unwrap();

    assert_eq!(request.id(), "req-1");
    assert_eq!(request.command(), DiagnosticChildCommand::ReadUiState);
    assert_eq!(request.params(), &json!({ "limit": 4 }));
}

#[test]
fn request_frames_reject_malformed_json_without_retaining_payload() {
    let error = parse_request_frame(br#"{"id":"req-1","command":"read_ui_state""#).unwrap_err();

    assert_eq!(error.kind(), "invalid_json");
    assert!(error.to_string().len() < 256);
}

#[test]
fn request_frames_reject_unknown_commands() {
    let frame = br#"{"id":"req-1","command":"unknown","params":{}}"#;

    let error = parse_request_frame(frame).unwrap_err();

    assert_eq!(error.kind(), "unsupported_command");
}

#[test]
fn bounded_line_reader_reports_oversized_frames_after_newline() {
    let bytes = vec![b'x'; MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES + 8];
    let mut input = bytes;
    input.push(b'\n');
    input.extend_from_slice(
        br#"{"id":"req-2","command":"close_popups","params":{}}
"#,
    );
    let mut reader = BufReader::new(Cursor::new(input));

    let first = read_bounded_line_bytes(&mut reader, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES).unwrap();
    let second = read_bounded_line_bytes(&mut reader, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES).unwrap();

    assert!(matches!(first, BoundedLineRead::LineTooLong { .. }));
    let second = match second {
        BoundedLineRead::Line(line) => line,
        _ => panic!("second frame should remain readable after oversized first frame"),
    };
    let request = parse_request_frame(&second).unwrap().unwrap();
    assert_eq!(request.command(), DiagnosticChildCommand::ClosePopups);
}

#[test]
fn request_frame_serialization_uses_newline_delimited_json() {
    let frame = request_frame(
        "req-3",
        DiagnosticChildCommand::ScrollTranscript,
        json!({ "command": "bottom" }),
    )
    .unwrap();

    assert!(frame.ends_with(b"\n"));
    let request = parse_request_frame(&frame).unwrap().unwrap();
    assert_eq!(request.id(), "req-3");
    assert_eq!(request.command().as_str(), "scroll_transcript");
}

#[test]
fn response_frames_keep_matching_ids_and_errors() {
    let response =
        DiagnosticProtocolResponse::error(Some("req-4".to_string()), "bad_request", "message");
    let frame = response_frame(response);

    let parsed = parse_response_frame(&frame).unwrap().unwrap();

    assert_eq!(parsed.id(), Some("req-4"));
    let error = parsed.into_result().unwrap_err();
    assert_eq!(error.kind(), "bad_request");
    assert_eq!(error.message(), "message");
}

#[test]
fn response_frames_replace_oversized_success_payload_with_bounded_error() {
    let response = DiagnosticProtocolResponse::success(
        "req-5",
        json!({ "payload": "x".repeat(MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES) }),
    );

    let frame = response_frame(response);
    let parsed = parse_response_frame(&frame).unwrap().unwrap();

    assert_eq!(parsed.id(), Some("req-5"));
    let error = parsed.into_result().unwrap_err();
    assert_eq!(error.kind(), "response_too_large");
    assert!(error.message().len() < 600);
}

#[test]
fn command_constants_match_protocol_command_names() {
    assert_eq!(
        DiagnosticChildCommand::ReadUiState.as_str(),
        READ_UI_STATE_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::SwitchWorkspace.as_str(),
        SWITCH_WORKSPACE_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::ClosePopups.as_str(),
        CLOSE_POPUPS_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::ListWorkspaceThreads.as_str(),
        LIST_WORKSPACE_THREADS_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::CreateNewThread.as_str(),
        CREATE_NEW_THREAD_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::StartTurn.as_str(),
        START_TURN_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::SoftStopTurn.as_str(),
        SOFT_STOP_TURN_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::HardStopTurn.as_str(),
        HARD_STOP_TURN_COMMAND
    );
}
