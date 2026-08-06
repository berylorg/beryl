#[path = "../src/diagnostic_child_protocol.rs"]
mod diagnostic_child_protocol;

use std::io::{BufReader, Cursor};

use diagnostic_child_protocol::{
    BoundedLineRead, CLOSE_POPUPS_COMMAND, CREATE_NEW_THREAD_COMMAND, DiagnosticChildCommand,
    DiagnosticProtocolResponse, HANDSHAKE_COMMAND, HARD_STOP_TURN_COMMAND,
    LIST_WORKSPACE_THREADS_COMMAND, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES,
    PREPARE_RENDERER_WINDOW_COMMAND, READ_RENDERER_COMMAND, READ_TRANSCRIPT_FRAME_METRICS_COMMAND,
    READ_UI_STATE_COMMAND, SOFT_STOP_TURN_COMMAND, START_TURN_COMMAND, SWITCH_WORKSPACE_COMMAND,
    parse_request_frame, parse_response_frame, read_bounded_line_bytes, request_frame,
    response_frame,
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
fn request_frame_limit_counts_the_trailing_newline_exactly() {
    let empty = request_frame(
        "boundary",
        DiagnosticChildCommand::StartTurn,
        json!({ "text": "" }),
    )
    .unwrap();
    let payload_bytes = MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES - empty.len();
    let exact = request_frame(
        "boundary",
        DiagnosticChildCommand::StartTurn,
        json!({ "text": "x".repeat(payload_bytes) }),
    )
    .unwrap();
    assert_eq!(exact.len(), MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES);
    assert!(exact.ends_with(b"\n"));

    let error = request_frame(
        "boundary",
        DiagnosticChildCommand::StartTurn,
        json!({ "text": "x".repeat(payload_bytes + 1) }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), "frame_too_large");
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
fn version_one_response_envelope_has_no_extension_fields() {
    let success = response_frame(DiagnosticProtocolResponse::success(
        "success",
        json!({ "value": true }),
    ));
    let error = response_frame(DiagnosticProtocolResponse::error(
        Some("error".to_string()),
        "shell_timeout",
        "timed out",
    ));

    let success: serde_json::Value = serde_json::from_slice(&success).unwrap();
    let error: serde_json::Value = serde_json::from_slice(&error).unwrap();
    assert_eq!(
        success
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["id", "ok", "result"]
    );
    assert_eq!(
        error
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["id", "ok", "error"]
    );
    assert!(success.get("liveness").is_none());
    assert!(error.get("liveness").is_none());
}

#[test]
fn version_one_response_parser_rejects_top_level_liveness_extensions() {
    let frame = br#"{"id":"req","ok":false,"error":{"kind":"shell_timeout","message":"timed out"},"liveness":{}}"#;

    assert_eq!(
        parse_response_frame(frame).unwrap_err().kind(),
        "invalid_json"
    );
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
        DiagnosticChildCommand::Handshake.as_str(),
        HANDSHAKE_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::ReadUiState.as_str(),
        READ_UI_STATE_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::ReadRenderer.as_str(),
        READ_RENDERER_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::ReadTranscriptFrameMetrics.as_str(),
        READ_TRANSCRIPT_FRAME_METRICS_COMMAND
    );
    assert_eq!(
        DiagnosticChildCommand::PrepareRendererWindow.as_str(),
        PREPARE_RENDERER_WINDOW_COMMAND
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
