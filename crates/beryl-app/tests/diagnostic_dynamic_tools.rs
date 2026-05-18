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
    READ_RENDERER_DIAGNOSTICS_TOOL, READ_RETAINED_STATE_SUMMARY_TOOL,
    READ_SETTINGS_WINDOW_DIAGNOSTICS_TOOL, READ_TRANSCRIPT_FRAME_METRICS_TOOL,
    READ_VISIBLE_MEDIA_TOOL, RendererDiagnosticSnapshot, RuntimeTargetDiagnostic,
    SettingsWindowDiagnosticSnapshot, SettingsWindowPerformanceDiagnostic,
    SettingsWindowRowSurfaceDiagnostic, ShellWindowRendererDiagnostic, ThemeEditorModelDiagnostic,
    TranscriptFrameMetric, TranscriptFrameMetricsLog, TranscriptFrameMetricsSnapshot,
    VisibleMediaDiagnostics, VisibleMediaItemDiagnostic, VisibleMediaSnapshot,
    beryl_diagnostic_dynamic_tool_specs, dispatch_beryl_diagnostic_dynamic_tool_call,
    is_beryl_diagnostic_dynamic_tool, renderer_snapshot_with_shell_window,
};
use memory_diagnostics::RetainedStateSnapshot;
use serde_json::{Value, json};

#[test]
fn visible_media_diagnostics_caps_items_and_truncates_strings() {
    let mut diagnostics = VisibleMediaDiagnostics::default();
    diagnostics.begin_frame(Some("thread".repeat(200)), 10..20);
    diagnostics.begin_preload_frame(8..22);

    for index in 0..80 {
        let item = VisibleMediaItemDiagnostic {
            row_identity: Some(format!("row-{index}-{}", "x".repeat(700))),
            key: format!("key-{index}-{}", "x".repeat(700)),
            source_kind: "generated_image".to_string(),
            backing_kind: Some("source_backed_file".to_string()),
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
        };
        diagnostics.record_item(item.clone());
        diagnostics.record_preloaded_item(item);
    }

    let snapshot = diagnostics.snapshot();

    assert_eq!(snapshot.items.len(), 64);
    assert_eq!(snapshot.item_count, 64);
    assert!(snapshot.truncated);
    assert_eq!(snapshot.preloaded_items.len(), 64);
    assert_eq!(snapshot.preloaded_item_count, 64);
    assert!(snapshot.preloaded_truncated);
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
fn transcript_frame_metrics_are_content_free_bounded_and_dispatchable() {
    let mut log = TranscriptFrameMetricsLog::default();

    for index in 0..140 {
        log.record(TranscriptFrameMetric {
            sequence: 0,
            selected_thread_id: Some(format!("thread-{index}-{}", "x".repeat(700))),
            presentation_range: None,
            visible_range: None,
            total_loaded_turn_count: 12,
            total_item_count: Some(24),
            total_text_chars: Some(480),
            presentation_range_len: 4,
            visible_row_count: 2,
            panel_state_inspected_row_count: 4,
            frame_micros: index,
            style_snapshot_micros: 1,
            composer_measurement_micros: 2,
            row_build_total_micros: 3,
            row_prepaint_total_micros: 4,
            inline_text_construction_micros: 5,
            code_panel_render_micros: 6,
            media_run_render_micros: 7,
            media_preload_micros: 8,
            slowest_row_build_micros: 9,
            slowest_row_build_index: Some(1),
            slowest_row_build_identity: Some(format!("row-{index}-{}", "x".repeat(700))),
            slowest_row_prepaint_micros: 10,
            slowest_row_prepaint_index: Some(1),
            slowest_row_prepaint_identity: Some(format!("row-{index}-{}", "y".repeat(700))),
            largest_visible_row_text_chars: 300,
            largest_visible_row_text_chars_index: Some(1),
            largest_visible_row_item_count: 8,
            largest_visible_row_item_count_index: Some(1),
            dominant_cost_category: "inline_text_construction".repeat(40),
        });
    }

    let snapshot = log.snapshot();
    assert_eq!(snapshot.frames.len(), 128);
    assert_eq!(snapshot.frame_count, 128);
    assert_eq!(snapshot.frames.first().unwrap().sequence, 13);
    assert_eq!(snapshot.next_sequence, 141);
    assert!(
        snapshot
            .frames
            .iter()
            .all(|frame| frame.selected_thread_id.as_ref().unwrap().len() <= 512)
    );
    assert!(
        snapshot.frames.iter().all(|frame| frame
            .slowest_row_build_identity
            .as_ref()
            .unwrap()
            .len()
            <= 512)
    );

    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(
            READ_TRANSCRIPT_FRAME_METRICS_TOOL,
            json!({ "afterSequence": 20, "limit": 2 }),
        ),
        DiagnosticToolSnapshot {
            transcript_frame_metrics: snapshot,
            ..diagnostic_snapshot(VisibleMediaSnapshot::default(), event_snapshot(0))
        },
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["result"]["frames"].as_array().unwrap().len(), 2);
    assert_eq!(payload["result"]["frames"][0]["sequence"], 21);
    assert_eq!(payload["result"]["frameCount"], 2);
    assert_eq!(payload["result"]["truncated"], true);
    assert!(
        payload["result"]
            .to_string()
            .find("assistant text")
            .is_none()
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
            backing_kind: Some("source_backed_file".to_string()),
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
                preload_range: None,
                items: visible_items,
                item_count: 8,
                truncated: false,
                preloaded_items: Vec::new(),
                preloaded_item_count: 0,
                preloaded_truncated: false,
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
    assert_eq!(
        visible_payload["result"]["items"][0]["backingKind"],
        "source_backed_file"
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
fn settings_window_diagnostics_are_content_free_and_dispatchable() {
    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(READ_SETTINGS_WINDOW_DIAGNOSTICS_TOOL, json!({})),
        DiagnosticToolSnapshot {
            settings_window: SettingsWindowDiagnosticSnapshot {
                available: true,
                unavailable_reason: None,
                visible: true,
                selected_section_id: Some("themes".to_string()),
                selected_page_id: Some("theme_editor".to_string()),
                detail_rows: Some(SettingsWindowRowSurfaceDiagnostic {
                    surface_id: "selected_page_detail_rows".to_string(),
                    total_row_count: 22,
                    rendered_row_count: 22,
                    visible_range: None,
                    overscan_count: 0,
                    row_height_strategy: "full_selected_page".to_string(),
                }),
                split_list: Some(SettingsWindowRowSurfaceDiagnostic {
                    surface_id: "page_local_split_list".to_string(),
                    total_row_count: 176,
                    rendered_row_count: 10,
                    visible_range: Some(diagnostic_dynamic_tools::PresentationRangeDiagnostic {
                        start: 40,
                        end: 50,
                    }),
                    overscan_count: 3,
                    row_height_strategy: "fixed_height_windowed".to_string(),
                }),
                performance: SettingsWindowPerformanceDiagnostic {
                    render_count: 7,
                    last_render_tree_micros: 100,
                    model_sync_count: 3,
                    last_model_sync_micros: 40,
                    option_sync_count: 2,
                    last_option_sync_micros: 10,
                    input_sync_count: 12,
                    last_input_sync_entity_count: 4,
                    color_preview_lookup_count: 8,
                    last_render_color_preview_lookup_count: 2,
                    color_model_lookup_count: 6,
                    last_render_color_model_lookup_count: 1,
                    dominant_cost_category: "render_tree".to_string(),
                },
                theme_editor_model: Some(ThemeEditorModelDiagnostic {
                    candidate_definition_build_count: 1,
                    last_candidate_definition_build_micros: 21,
                    preview_projection_build_count: 1,
                    last_preview_projection_build_micros: 34,
                    role_preview_style_build_count: 176,
                    role_preview_row_count: 176,
                    selected_property_detail_row_count: 6,
                    modified_state_recompute_count: 4,
                    last_modified_state_recompute_micros: 55,
                }),
            },
            ..diagnostic_snapshot(VisibleMediaSnapshot::default(), event_snapshot(0))
        },
    );
    let payload = response_json(&response);
    let result_text = payload["result"].to_string();

    assert!(response.success);
    assert_eq!(payload["result"]["selectedPageId"], "theme_editor");
    assert_eq!(payload["result"]["detailRows"]["totalRowCount"], 22);
    assert_eq!(payload["result"]["splitList"]["totalRowCount"], 176);
    assert_eq!(payload["result"]["splitList"]["renderedRowCount"], 10);
    assert_eq!(
        payload["result"]["performance"]["dominantCostCategory"],
        "render_tree"
    );
    assert_eq!(
        payload["result"]["themeEditorModel"]["rolePreviewRowCount"],
        176
    );
    assert_eq!(
        payload["result"]["themeEditorModel"]["modifiedStateRecomputeCount"],
        4
    );
    assert!(!result_text.contains("End-turn sound"));
    assert!(!result_text.contains("developer instructions"));
    assert!(!result_text.contains("C:\\Users"));
    assert!(!result_text.contains("schema = 1"));
}

#[test]
fn settings_window_diagnostics_tool_is_registered() {
    let specs = beryl_diagnostic_dynamic_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == READ_SETTINGS_WINDOW_DIAGNOSTICS_TOOL)
        .expect("settings-window diagnostic tool should be registered");
    let request = diagnostic_tool_request(READ_SETTINGS_WINDOW_DIAGNOSTICS_TOOL, json!({}));

    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert!(is_beryl_diagnostic_dynamic_tool(&request));
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
            transcript_frame_metrics: TranscriptFrameMetricsSnapshot::default(),
            settings_window: SettingsWindowDiagnosticSnapshot::unavailable("not sampled in test"),
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
            transcript_frame_metrics: TranscriptFrameMetricsSnapshot::default(),
            settings_window: SettingsWindowDiagnosticSnapshot::unavailable("not sampled in test"),
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
fn renderer_diagnostics_serialize_source_backed_image_sections() {
    let process = ProcessDiagnosticSnapshot {
        pid: 42,
        executable_path: Some("beryl.exe".to_string()),
        beryl_home: Some("C:\\beryl-home".to_string()),
        selected_workspace_id: Some("workspace_1".to_string()),
        selected_thread_id: Some("thread_1".to_string()),
        selected_runtime_target: None,
        managed_backend_child_pids: Vec::new(),
    };
    let mut renderer_window = window_renderer_snapshot(9, true, 1200, 800);
    renderer_window.source_backed_images = gpui::SourceBackedImageDiagnosticSnapshot {
        request_count: 1,
        live_count: 1,
        requested_this_frame_count: 1,
        painted_resource_count: 1,
        live_gpu_bytes_estimate: 64,
        evicted_resource_count: 2,
        ..Default::default()
    };
    renderer_window.renderer.image_resources = gpui::ImageResourceDiagnosticSnapshot {
        resource_count: 1,
        gpu_bytes_estimate: 64,
        decoded_cpu_bytes_estimate: 0,
        upload_count: 1,
        upload_bytes: 64,
        ..Default::default()
    };

    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(READ_RENDERER_DIAGNOSTICS_TOOL, json!({})),
        DiagnosticToolSnapshot {
            renderer: RendererDiagnosticSnapshot {
                target: process.clone(),
                shell_window: ready_shell_window(9),
                renderer: gpui::RendererDiagnosticSnapshot {
                    window_count: 1,
                    windows: vec![renderer_window],
                    truncated: false,
                    loading_asset_count: 0,
                    decoded_image_assets: gpui::DecodedImageAssetDiagnosticSnapshot::default(),
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
            transcript_frame_metrics: TranscriptFrameMetricsSnapshot::default(),
            settings_window: SettingsWindowDiagnosticSnapshot::unavailable("not sampled in test"),
        },
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(
        payload["result"]["renderer"]["windows"][0]["sourceBackedImages"]["liveCount"],
        1
    );
    assert_eq!(
        payload["result"]["renderer"]["windows"][0]["sourceBackedImages"]["evictedResourceCount"],
        2
    );
    assert_eq!(
        payload["result"]["renderer"]["windows"][0]["renderer"]["imageResources"]["resourceCount"],
        1
    );
    assert_eq!(
        payload["result"]["renderer"]["windows"][0]["renderer"]["imageResources"]["decodedCpuBytesEstimate"],
        0
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
fn retained_state_summary_reports_source_backed_media_counters() {
    let mut snapshot = diagnostic_snapshot(VisibleMediaSnapshot::default(), event_snapshot(0));
    snapshot.retained_state.media_cache_loaded_entries = Some(10);
    snapshot
        .retained_state
        .media_cache_loaded_retained_byte_entries = Some(2);
    snapshot
        .retained_state
        .media_cache_loaded_source_backed_file_entries = Some(8);
    snapshot
        .retained_state
        .media_cache_loaded_native_generated_source_backed_file_entries = Some(7);
    snapshot
        .retained_state
        .media_cache_loaded_native_generated_retained_byte_entries = Some(1);
    snapshot.retained_state.media_cache_loaded_image_bytes = Some(123);

    let response = dispatch_beryl_diagnostic_dynamic_tool_call(
        &diagnostic_tool_request(READ_RETAINED_STATE_SUMMARY_TOOL, json!({})),
        snapshot,
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(
        payload["result"]["retainedState"]["mediaCacheLoadedRetainedByteEntries"],
        2
    );
    assert_eq!(
        payload["result"]["retainedState"]["mediaCacheLoadedSourceBackedFileEntries"],
        8
    );
    assert_eq!(
        payload["result"]["retainedState"]["mediaCacheLoadedNativeGeneratedSourceBackedFileEntries"],
        7
    );
    assert_eq!(
        payload["result"]["retainedState"]["mediaCacheLoadedNativeGeneratedRetainedByteEntries"],
        1
    );
    assert_eq!(
        payload["result"]["retainedState"]["mediaCacheLoadedImageBytes"],
        123
    );
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
        transcript_frame_metrics: TranscriptFrameMetricsSnapshot::default(),
        settings_window: SettingsWindowDiagnosticSnapshot::unavailable("not sampled in test"),
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
        source_backed_images: gpui::SourceBackedImageDiagnosticSnapshot::default(),
        renderer: gpui::PlatformRendererDiagnosticSnapshot {
            backend: "test".to_string(),
            resources: Vec::new(),
            image_resources: gpui::ImageResourceDiagnosticSnapshot::default(),
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
