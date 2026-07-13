#![allow(dead_code, unused_imports)]

#[path = "../src/memory_diagnostics.rs"]
mod memory_diagnostics;

mod dynamic_tool_namespace {
    pub const BERYL_DYNAMIC_TOOL_NAMESPACE: &str = "beryl";
}

#[path = "../src/diagnostic_dynamic_tools.rs"]
mod diagnostic_dynamic_tools;

#[path = "../src/gui_control_dynamic_tools.rs"]
mod gui_control_dynamic_tools;

use beryl_backend::{
    DynamicToolCallOutputContentItem, DynamicToolCallRequest, DynamicToolCallResponse,
    parse_dynamic_tool_call_request,
};
use diagnostic_dynamic_tools::VisibleMediaSnapshot;
use gui_control_dynamic_tools::{
    ActivityPanelUiState, BackendUnavailableUiState, BackgroundWorkUiState, CLOSE_POPUPS_TOOL,
    ClosePopupsResult, GuiControlToolRequest, MarkdownCacheUiState, PopupUiState,
    READ_UI_STATE_TOOL, SCROLL_TRANSCRIPT_TOOL, SETTINGS_WINDOW_POPUP_CLOSE_REASON,
    SWITCH_THREAD_TOOL, ScrollTranscriptCommand, TranscriptUiState, TurnViewUiState,
    UiStateSnapshot, close_popups_tool_response, parse_beryl_gui_control_dynamic_tool_request,
    ui_state_tool_response,
};
use serde_json::{Value, json};

#[test]
fn gui_control_parser_rejects_unknown_arguments() {
    let request = tool_request(
        SCROLL_TRANSCRIPT_TOOL,
        json!({
            "command": "bottom",
            "extra": true
        }),
    );

    let error = parse_beryl_gui_control_dynamic_tool_request(&request).unwrap_err();

    assert_eq!(error.kind(), "invalid_arguments");
}

#[test]
fn gui_control_parser_clamps_scroll_repeat_to_schema_max() {
    let request = tool_request(
        SCROLL_TRANSCRIPT_TOOL,
        json!({
            "command": "bottom",
            "repeat": 99
        }),
    );

    let parsed = parse_beryl_gui_control_dynamic_tool_request(&request).unwrap();

    assert_eq!(
        parsed,
        GuiControlToolRequest::ScrollTranscript(
            gui_control_dynamic_tools::ScrollTranscriptArguments {
                command: ScrollTranscriptCommand::Bottom,
                repeat: 8,
                delta_y: None,
                precise: false,
            }
        )
    );
}

#[test]
fn gui_control_parser_requires_bounded_delta_for_wheel_scroll() {
    let missing = parse_beryl_gui_control_dynamic_tool_request(&tool_request(
        SCROLL_TRANSCRIPT_TOOL,
        json!({ "command": "wheel" }),
    ))
    .unwrap_err();
    assert_eq!(missing.kind(), "invalid_arguments");

    let too_large = parse_beryl_gui_control_dynamic_tool_request(&tool_request(
        SCROLL_TRANSCRIPT_TOOL,
        json!({ "command": "wheel", "deltaY": 9999.0 }),
    ))
    .unwrap_err();
    assert_eq!(too_large.kind(), "invalid_arguments");

    let valid = parse_beryl_gui_control_dynamic_tool_request(&tool_request(
        SCROLL_TRANSCRIPT_TOOL,
        json!({ "command": "wheel", "deltaY": -42.5, "precise": true, "repeat": 3 }),
    ))
    .unwrap();

    assert_eq!(
        valid,
        GuiControlToolRequest::ScrollTranscript(
            gui_control_dynamic_tools::ScrollTranscriptArguments {
                command: ScrollTranscriptCommand::Wheel,
                repeat: 3,
                delta_y: Some(-42.5),
                precise: true,
            }
        )
    );
}

#[test]
fn switch_thread_requires_exact_non_empty_thread_id() {
    let empty = parse_beryl_gui_control_dynamic_tool_request(&tool_request(
        SWITCH_THREAD_TOOL,
        json!({ "threadId": "" }),
    ))
    .unwrap_err();
    assert_eq!(empty.kind(), "invalid_arguments");

    let valid = parse_beryl_gui_control_dynamic_tool_request(&tool_request(
        SWITCH_THREAD_TOOL,
        json!({ "threadId": "thread_123" }),
    ))
    .unwrap();

    assert_eq!(
        valid,
        GuiControlToolRequest::SwitchThread(gui_control_dynamic_tools::SwitchThreadArguments {
            thread_id: "thread_123".to_string(),
        })
    );
}

#[test]
fn close_popups_response_reports_settings_window_transient_popup_state() {
    let request = tool_request(CLOSE_POPUPS_TOOL, json!({}));
    let response = close_popups_tool_response(
        &request,
        ClosePopupsResult {
            closed_count: 1,
            closed: vec![SETTINGS_WINDOW_POPUP_CLOSE_REASON.to_string()],
            ui_state: UiStateSnapshot {
                shell_state: "ready".to_string(),
                selected_surface: "settings".to_string(),
                selected_thread_id: None,
                backend_unavailable: None,
                turn_state: gui_control_dynamic_tools::TurnUiState::default(),
                transcript: TranscriptUiState::default(),
                visible_media: VisibleMediaSnapshot::default(),
                markdown_cache: MarkdownCacheUiState::default(),
                activity_panel: ActivityPanelUiState::default(),
                popups: PopupUiState {
                    settings_window_visible: Some(true),
                    settings_window_transient_popup_open: Some(false),
                    ..PopupUiState::default()
                },
                background_work: BackgroundWorkUiState::default(),
            },
        },
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["ok"], true);
    assert_eq!(
        payload["result"]["closed"][0],
        SETTINGS_WINDOW_POPUP_CLOSE_REASON
    );
    assert_eq!(
        payload["result"]["uiState"]["popups"]["settingsWindowTransientPopupOpen"],
        false
    );
}

#[test]
fn ui_state_response_reports_backend_unavailable_reason() {
    let request = tool_request(READ_UI_STATE_TOOL, json!({ "limit": 1 }));
    let response = ui_state_tool_response(
        &request,
        UiStateSnapshot {
            shell_state: "backend_unavailable".to_string(),
            selected_surface: "conversation".to_string(),
            selected_thread_id: None,
            backend_unavailable: Some(BackendUnavailableUiState {
                kind: "missing_executable".to_string(),
                message: "Backend unavailable for Host Windows.".to_string(),
            }),
            turn_state: gui_control_dynamic_tools::TurnUiState::default(),
            transcript: TranscriptUiState::default(),
            visible_media: VisibleMediaSnapshot::default(),
            markdown_cache: MarkdownCacheUiState::default(),
            activity_panel: ActivityPanelUiState::default(),
            popups: PopupUiState::default(),
            background_work: BackgroundWorkUiState::default(),
        },
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["result"]["shellState"], "backend_unavailable");
    assert_eq!(
        payload["result"]["backendUnavailable"]["kind"],
        "missing_executable"
    );
}

#[test]
fn ui_state_response_reports_status_line_turn_view() {
    let request = tool_request(READ_UI_STATE_TOOL, json!({ "limit": 1 }));
    let response = ui_state_tool_response(
        &request,
        UiStateSnapshot {
            shell_state: "ready".to_string(),
            selected_surface: "conversation".to_string(),
            selected_thread_id: Some("thread_1".to_string()),
            backend_unavailable: None,
            turn_state: gui_control_dynamic_tools::TurnUiState {
                selected_thread_state: "idle".to_string(),
                last_turn_state: "ok".to_string(),
                view: TurnViewUiState {
                    current: Some(2),
                    total: Some(5),
                },
                ..gui_control_dynamic_tools::TurnUiState::default()
            },
            transcript: TranscriptUiState::default(),
            visible_media: VisibleMediaSnapshot::default(),
            markdown_cache: MarkdownCacheUiState::default(),
            activity_panel: ActivityPanelUiState::default(),
            popups: PopupUiState::default(),
            background_work: BackgroundWorkUiState::default(),
        },
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["result"]["turnState"]["view"]["current"], 2);
    assert_eq!(payload["result"]["turnState"]["view"]["total"], 5);
}

fn tool_request(tool: &str, arguments: Value) -> DynamicToolCallRequest {
    parse_dynamic_tool_call_request(
        json!("dynamic-request-1"),
        "item/tool/call",
        Some(json!({
            "threadId": "thread_1",
            "turnId": "turn_1",
            "callId": "call_1",
            "namespace": "beryl",
            "tool": tool,
            "arguments": arguments
        })),
    )
    .unwrap()
    .unwrap()
}

fn response_json(response: &DynamicToolCallResponse) -> Value {
    let Some(DynamicToolCallOutputContentItem::InputText { text }) = response.content_items.first()
    else {
        panic!("expected a single text content item")
    };
    serde_json::from_str(text).unwrap()
}
