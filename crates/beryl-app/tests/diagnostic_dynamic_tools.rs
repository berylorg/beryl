#[path = "../src/memory_diagnostics.rs"]
mod memory_diagnostics;

mod dynamic_tools {
    pub const BERYL_DYNAMIC_TOOL_NAMESPACE: &str = "beryl";
}

#[path = "../src/diagnostic_dynamic_tools.rs"]
mod diagnostic_dynamic_tools;

use beryl_backend::{
    DynamicToolCallOutputContentItem, DynamicToolCallRequest, DynamicToolCallResponse,
    parse_dynamic_tool_call_request,
};
use diagnostic_dynamic_tools::{
    DiagnosticToolSnapshot, MediaDiagnosticEvent, MediaDiagnosticLog, MediaEventSnapshot,
    MemoryDiagnosticSnapshot, MemoryDiagnosticUiCorrelation, PreviewDiagnostic,
    ProcessDiagnosticSnapshot, READ_MEDIA_EVENTS_TOOL, READ_MEMORY_DIAGNOSTICS_TOOL,
    READ_RENDERER_DIAGNOSTICS_TOOL, READ_RETAINED_STATE_SUMMARY_TOOL, READ_VISIBLE_MEDIA_TOOL,
    RendererDiagnosticSnapshot, RuntimeTargetDiagnostic, ShellWindowRendererDiagnostic,
    VisibleMediaDiagnostics, VisibleMediaItemDiagnostic, VisibleMediaSnapshot,
    dispatch_beryl_diagnostic_dynamic_tool_call, renderer_snapshot_with_shell_window,
};
use memory_diagnostics::RetainedStateSnapshot;
use serde_json::{Value, json};

#[test]
fn visible_media_diagnostics_caps_items_and_truncates_strings() {
    let mut diagnostics = VisibleMediaDiagnostics::default();
    diagnostics.begin_frame(Some("thread".repeat(200)), 10..20);

    for index in 0..80 {
        diagnostics.record_item(VisibleMediaItemDiagnostic {
            row_identity: Some(format!("row-{index}-{}", "x".repeat(700))),
            key: format!("key-{index}-{}", "x".repeat(700)),
            source_kind: "generated_image".to_string(),
            outcome: "loaded".to_string(),
            format: Some("png".to_string()),
            compressed_bytes: Some(12),
            decoded_bytes_estimate: Some(48),
            natural_width: Some(2),
            natural_height: Some(6),
            displayed_width: 2.0,
            displayed_height: 6.0,
            image_id: Some(index),
            image_asset_key_hash: Some(index + 1),
        });
    }

    let snapshot = diagnostics.snapshot();

    assert_eq!(snapshot.items.len(), 64);
    assert_eq!(snapshot.item_count, 64);
    assert!(snapshot.truncated);
    assert_eq!(snapshot.selected_thread_id.unwrap().len(), 512);
    assert!(snapshot.items.iter().all(|item| item.key.len() <= 512));
    assert!(
        snapshot
            .items
            .iter()
            .all(|item| item.row_identity.as_ref().unwrap().len() <= 512)
    );
}

#[test]
fn media_event_log_is_a_metadata_only_bounded_ring() {
    let mut log = MediaDiagnosticLog::default();

    for index in 0..300 {
        let mut event = MediaDiagnosticEvent::new(format!("event-{index}-{}", "x".repeat(700)));
        event.key = Some(format!("key-{index}-{}", "x".repeat(700)));
        event.detail = Some("detail".repeat(200));
        event.compressed_bytes = Some(index);
        log.record(event);
    }

    let snapshot = log.snapshot();

    assert_eq!(snapshot.events.len(), 256);
    assert_eq!(snapshot.event_count, 256);
    assert_eq!(snapshot.events.first().unwrap().sequence, 45);
    assert_eq!(snapshot.events.last().unwrap().sequence, 300);
    assert_eq!(snapshot.next_sequence, 301);
    assert!(snapshot.events.iter().all(|event| event.kind.len() <= 512));
    assert!(
        snapshot
            .events
            .iter()
            .all(|event| event.key.as_ref().unwrap().len() <= 512)
    );
    assert!(
        snapshot
            .events
            .iter()
            .all(|event| event.detail.as_ref().unwrap().len() <= 512)
    );
}

#[test]
fn diagnostic_dispatch_caps_visible_media_and_media_events() {
    let mut visible_items = Vec::new();
    for index in 0..8 {
        visible_items.push(VisibleMediaItemDiagnostic {
            row_identity: Some(format!("row-{index}")),
            key: format!("key-{index}"),
            source_kind: "generated_image".to_string(),
            outcome: "loaded".to_string(),
            format: Some("png".to_string()),
            compressed_bytes: Some(10),
            decoded_bytes_estimate: Some(40),
            natural_width: Some(1),
            natural_height: Some(10),
            displayed_width: 1.0,
            displayed_height: 10.0,
            image_id: Some(index),
            image_asset_key_hash: Some(index + 1),
        });
    }

    let visible_response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(READ_VISIBLE_MEDIA_TOOL, json!({ "limit": 3 })),
        diagnostic_snapshot(
            VisibleMediaSnapshot {
                frame_generation: 7,
                selected_thread_id: Some("thread".to_string()),
                presentation_range: None,
                items: visible_items,
                item_count: 8,
                truncated: false,
                stale: false,
                preview: PreviewDiagnostic::default(),
            },
            event_snapshot(0),
        ),
    );
    let visible_payload = response_json(&visible_response);

    assert!(visible_response.success);
    assert_eq!(visible_payload["ok"], true);
    assert_eq!(
        visible_payload["result"]["items"].as_array().unwrap().len(),
        3
    );
    assert_eq!(visible_payload["result"]["itemCount"], 3);
    assert_eq!(visible_payload["result"]["truncated"], true);

    let event_response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(
            READ_MEDIA_EVENTS_TOOL,
            json!({
                "afterSequence": 4,
                "limit": 2
            }),
        ),
        diagnostic_snapshot(VisibleMediaSnapshot::default(), event_snapshot(8)),
    );
    let event_payload = response_json(&event_response);

    assert!(event_response.success);
    assert_eq!(event_payload["ok"], true);
    assert_eq!(
        event_payload["result"]["events"].as_array().unwrap().len(),
        2
    );
    assert_eq!(event_payload["result"]["events"][0]["sequence"], 5);
    assert_eq!(event_payload["result"]["events"][1]["sequence"], 6);
    assert_eq!(event_payload["result"]["eventCount"], 2);
    assert_eq!(event_payload["result"]["truncated"], true);
}

#[test]
fn memory_diagnostics_include_same_snapshot_ui_correlation_labels() {
    let runtime = RuntimeTargetDiagnostic {
        runtime: "host-windows".to_string(),
        canonical_path: "C:\\work\\beryl".to_string(),
        display_label: "C:\\work\\beryl".to_string(),
    };
    let process = ProcessDiagnosticSnapshot {
        pid: 7,
        executable_path: None,
        beryl_home: None,
        selected_workspace_id: Some("workspace_1".to_string()),
        selected_thread_id: Some("thread_1".to_string()),
        selected_runtime_target: Some(runtime),
        managed_backend_child_pids: Vec::new(),
    };
    let memory = MemoryDiagnosticSnapshot {
        counters: None,
        unavailable_reason: Some("not sampled in test".to_string()),
        ui: MemoryDiagnosticUiCorrelation::from_process(&process),
    };

    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(READ_MEMORY_DIAGNOSTICS_TOOL, json!({})),
        DiagnosticToolSnapshot {
            renderer: renderer_snapshot(&process),
            process,
            memory,
            retained_state: RetainedStateSnapshot::default(),
            visible_media: VisibleMediaSnapshot::default(),
            media_events: event_snapshot(0),
        },
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["ok"], true);
    assert_eq!(
        payload["result"]["ui"]["selectedWorkspaceId"],
        "workspace_1"
    );
    assert_eq!(payload["result"]["ui"]["selectedThreadId"], "thread_1");
    assert_eq!(
        payload["result"]["ui"]["selectedRuntimeTarget"]["runtime"],
        "host-windows"
    );
}

#[test]
fn renderer_diagnostics_include_target_identity_and_bounded_snapshot() {
    let process = ProcessDiagnosticSnapshot {
        pid: 42,
        executable_path: Some("beryl.exe".to_string()),
        beryl_home: Some("C:\\beryl-home".to_string()),
        selected_workspace_id: Some("workspace_1".to_string()),
        selected_thread_id: Some("thread_1".to_string()),
        selected_runtime_target: None,
        managed_backend_child_pids: Vec::new(),
    };
    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(READ_RENDERER_DIAGNOSTICS_TOOL, json!({})),
        DiagnosticToolSnapshot {
            renderer: RendererDiagnosticSnapshot {
                target: process.clone(),
                shell_window: ready_shell_window(9),
                renderer: gpui::RendererDiagnosticSnapshot {
                    window_count: 0,
                    windows: Vec::new(),
                    truncated: false,
                    loading_asset_count: 3,
                    decoded_image_assets: gpui::DecodedImageAssetDiagnosticSnapshot {
                        asset_count: 2,
                        loading_count: 1,
                        completed_count: 1,
                        failed_count: 0,
                        decoded_bytes_estimate: 128,
                        frame_count: 1,
                        removed_count: 4,
                        removed_completed_count: 3,
                        items: Vec::new(),
                        truncated: false,
                    },
                },
            },
            process,
            memory: MemoryDiagnosticSnapshot {
                counters: None,
                unavailable_reason: Some("not sampled in test".to_string()),
                ui: MemoryDiagnosticUiCorrelation::default(),
            },
            retained_state: RetainedStateSnapshot::default(),
            visible_media: VisibleMediaSnapshot::default(),
            media_events: event_snapshot(0),
        },
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["result"]["target"]["pid"], 42);
    assert_eq!(payload["result"]["target"]["selectedThreadId"], "thread_1");
    assert_eq!(payload["result"]["renderer"]["windowCount"], 0);
    assert_eq!(payload["result"]["renderer"]["loadingAssetCount"], 3);
    assert_eq!(
        payload["result"]["renderer"]["decodedImageAssets"]["completedCount"],
        1
    );
    assert_eq!(
        payload["result"]["shellWindow"]["rendererAttributionReady"],
        true
    );
}

#[test]
fn renderer_snapshot_merges_current_shell_window_when_app_snapshot_omits_it() {
    let process = ProcessDiagnosticSnapshot {
        pid: 42,
        executable_path: Some("beryl.exe".to_string()),
        beryl_home: Some("C:\\beryl-home".to_string()),
        selected_workspace_id: Some("workspace_1".to_string()),
        selected_thread_id: Some("thread_1".to_string()),
        selected_runtime_target: None,
        managed_backend_child_pids: Vec::new(),
    };
    let snapshot = renderer_snapshot_with_shell_window(
        process,
        gpui::RendererDiagnosticSnapshot {
            window_count: 1,
            windows: vec![window_renderer_snapshot(7, false, 0, 0)],
            truncated: false,
            loading_asset_count: 0,
            decoded_image_assets: gpui::DecodedImageAssetDiagnosticSnapshot::default(),
        },
        window_renderer_snapshot(9, true, 1200, 800),
    );

    assert_eq!(snapshot.shell_window.window_id, 9);
    assert!(snapshot.shell_window.matched_renderer_window);
    assert!(snapshot.shell_window.renderer_attribution_ready);
    assert_eq!(snapshot.renderer.window_count, 2);
    assert_eq!(snapshot.renderer.windows[0].window_id, 9);
    assert!(
        snapshot
            .renderer
            .windows
            .iter()
            .any(|window| window.window_id == 7)
    );
}

#[test]
fn diagnostic_dispatch_rejects_unknown_arguments_without_state_mutation() {
    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(
            READ_VISIBLE_MEDIA_TOOL,
            json!({ "limit": 1, "extra": true }),
        ),
        diagnostic_snapshot(VisibleMediaSnapshot::default(), event_snapshot(0)),
    );
    let payload = response_json(&response);

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "invalid_arguments");
}

#[test]
fn retained_state_summary_rejects_caller_limits() {
    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(READ_RETAINED_STATE_SUMMARY_TOOL, json!({ "limit": 1 })),
        diagnostic_snapshot(VisibleMediaSnapshot::default(), event_snapshot(0)),
    );
    let payload = response_json(&response);

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "invalid_arguments");
}

#[cfg(target_os = "windows")]
#[test]
fn process_memory_snapshot_samples_thread_count_on_windows() {
    let snapshot = memory_diagnostics::current_process_memory_snapshot()
        .expect("Windows process memory snapshot should be available");

    assert!(snapshot.thread_count.is_some_and(|count| count > 0));
}

fn diagnostic_snapshot(
    visible_media: VisibleMediaSnapshot,
    media_events: MediaEventSnapshot,
) -> DiagnosticToolSnapshot {
    let process = ProcessDiagnosticSnapshot {
        pid: 7,
        executable_path: None,
        beryl_home: None,
        selected_workspace_id: None,
        selected_thread_id: None,
        selected_runtime_target: None,
        managed_backend_child_pids: Vec::new(),
    };
    DiagnosticToolSnapshot {
        renderer: renderer_snapshot(&process),
        process,
        memory: MemoryDiagnosticSnapshot {
            counters: None,
            unavailable_reason: Some("not sampled in test".to_string()),
            ui: MemoryDiagnosticUiCorrelation::default(),
        },
        retained_state: RetainedStateSnapshot::default(),
        visible_media,
        media_events,
    }
}

fn event_snapshot(count: u64) -> MediaEventSnapshot {
    let events = (1..=count)
        .map(|sequence| {
            let mut event = MediaDiagnosticEvent::new("event");
            event.sequence = sequence;
            event.key = Some(format!("key-{sequence}"));
            event
        })
        .collect::<Vec<_>>();
    MediaEventSnapshot {
        event_count: events.len(),
        events,
        truncated: false,
        next_sequence: count + 1,
    }
}

fn renderer_snapshot(process: &ProcessDiagnosticSnapshot) -> RendererDiagnosticSnapshot {
    RendererDiagnosticSnapshot {
        target: process.clone(),
        shell_window: ready_shell_window(1),
        renderer: gpui::RendererDiagnosticSnapshot {
            window_count: 0,
            windows: Vec::new(),
            truncated: false,
            loading_asset_count: 0,
            decoded_image_assets: gpui::DecodedImageAssetDiagnosticSnapshot::default(),
        },
    }
}

fn ready_shell_window(window_id: u64) -> ShellWindowRendererDiagnostic {
    ShellWindowRendererDiagnostic {
        window_id,
        matched_renderer_window: true,
        active: Some(true),
        logical_width: Some(100.0),
        logical_height: Some(100.0),
        device_width: Some(100),
        device_height: Some(100),
        scale_factor: Some(1.0),
        surface_usable: Some(true),
        renderer_attribution_ready: true,
        unready_reason: None,
    }
}

fn window_renderer_snapshot(
    window_id: u64,
    active: bool,
    device_width: u32,
    device_height: u32,
) -> gpui::WindowRendererDiagnosticSnapshot {
    gpui::WindowRendererDiagnosticSnapshot {
        window_id,
        active,
        logical_width: f64::from(device_width),
        logical_height: f64::from(device_height),
        device_width,
        device_height,
        scale_factor: 1.0,
        surface_usable: device_width > 0 && device_height > 0,
        surface_unusable_reason: (device_width == 0 || device_height == 0)
            .then(|| "zero_device_size".to_string()),
        renderer: gpui::PlatformRendererDiagnosticSnapshot {
            backend: "test".to_string(),
            resources: Vec::new(),
            atlas: gpui::AtlasDiagnosticSnapshot::default(),
            pipeline_buffers: Vec::new(),
            unavailable_reason: None,
        },
    }
}

fn diagnostic_tool_request(tool: &str, arguments: Value) -> DynamicToolCallRequest {
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
