use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn live_tests_do_not_import_legacy_transcript_source_by_path() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = manifest_dir.join("tests");
    let legacy_transcript_path = ["src", "shell", "transcript"].join("/");
    let legacy_renderer_path = ["src", "shell", "render", "transcript"].join("/");
    let mut offenders = Vec::new();

    for path in rust_files_under(&tests_dir) {
        if path.file_name().and_then(|name| name.to_str())
            == Some("transcript_rework_source_boundary.rs")
        {
            continue;
        }

        let source = fs::read_to_string(&path).expect("test source should be readable");
        if source.contains(&legacy_transcript_path) || source.contains(&legacy_renderer_path) {
            offenders.push(display_test_path(&tests_dir, &path));
        }
    }

    assert!(
        offenders.is_empty(),
        "legacy transcript source imports remain in live tests: {offenders:?}"
    );
}

#[test]
fn syndic_transcript_sources_stay_inside_new_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let archived_crates = ["old", "crates"].join("-");
    let archived_rework_path = ["doc/rework/syndic-to-renderer", &archived_crates].join("/");
    let storage_crate_name = ["syndic", "storage"].join("-");
    let storage_module_name = ["syndic", "storage"].join("_");
    let forbidden_needles = [
        archived_crates.as_str(),
        archived_rework_path.as_str(),
        storage_crate_name.as_str(),
        storage_module_name.as_str(),
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&syndic_transcript_dir) {
        let source = fs::read_to_string(&path).expect("new transcript source should be readable");
        for forbidden in forbidden_needles {
            if source.contains(forbidden) {
                offenders.push(format!(
                    "{} contains {forbidden}",
                    display_test_path(&syndic_transcript_dir, &path)
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "new transcript boundary violations remain: {offenders:?}"
    );
}

#[test]
fn resident_core_sources_are_pure_rust_boundaries() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let checked_files = [
        "activation.rs",
        "context_menu.rs",
        "core.rs",
        "demand.rs",
        "media_action.rs",
        "renderer_context_menu.rs",
        "renderer_media_action.rs",
        "renderer_quote.rs",
        "renderer_selection.rs",
        "snapshot.rs",
        "selection.rs",
        "frame/mod.rs",
        "frame/geometry.rs",
        "frame/types.rs",
    ];
    let forbidden_needles = ["use gpui", "IntoElement", "impl Render", "gpui::Window"];
    let mut offenders = Vec::new();

    for file_name in checked_files {
        let path = syndic_transcript_dir.join(file_name);
        let source = fs::read_to_string(&path).expect("resident core source should be readable");
        for forbidden in forbidden_needles {
            if source.contains(forbidden) {
                offenders.push(format!("{file_name} contains {forbidden}"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "resident core sources crossed into rendering: {offenders:?}"
    );
}

#[test]
fn syndic_transcript_panel_renders_resident_snapshots_only() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let panel_path = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript")
        .join("panel.rs");
    let source = fs::read_to_string(panel_path).expect("panel source should be readable");
    let required_needles = [
        "RealizedFrameRequest",
        "realize_frame",
        "DemandFactKind::Viewport",
        "report_nested_resource_demands",
        "DemandFactKind::ResourceRange",
        "DemandFactKind::MediaPreviewPin",
        "ResidentResourceSlice",
        "snapshot.resources",
        "for frame_record in &frame_window.records",
        "snapshot.records.get(frame_record.index)",
        "record.id != frame_record.record_id",
        "render_realized_record(",
        "record_is_selected(&record.id, selected_record_ids)",
        "render_selected_record_ids",
        "render_selection_affordance",
        "ResidentPresentationRecordKind::TextChunk",
        "ResidentPresentationRecordKind::ResourceReference",
        "ResidentPresentationRecordKind::LocalUiFallback",
        "ResidentPresentationRecordKind::LocalAffordance",
        "syndic-transcript-record:",
    ];
    let storage_crate_name = ["syndic", "storage"].join("-");
    let storage_module_name = ["syndic", "storage"].join("_");
    let legacy_markdown_module = ["transcript", "markdown"].join("_");
    let parse_markdown_function = ["parse", "markdown"].join("_");
    let archived_crates = ["old", "crates"].join("-");
    let forbidden_needles = [
        "SyndicTranscriptProvider",
        "handle_request",
        "ReadViewPage",
        "ReadProjectionRecords",
        "ReadResourceMetadata",
        "ReadResourceRange",
        "request_resource_metadata",
        "request_resource_range",
        storage_crate_name.as_str(),
        storage_module_name.as_str(),
        legacy_markdown_module.as_str(),
        parse_markdown_function.as_str(),
        archived_crates.as_str(),
    ];

    for required in required_needles {
        assert!(
            source.contains(required),
            "panel source no longer contains expected resident-render boundary: {required}"
        );
    }

    for forbidden in forbidden_needles {
        assert!(
            !source.contains(forbidden),
            "panel source crossed renderer boundary with {forbidden}"
        );
    }
}

#[test]
fn syndic_transcript_host_forwards_frame_demand_facts_to_residency() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let host_source =
        fs::read_to_string(syndic_transcript_dir.join("host.rs")).expect("host source readable");
    let frame_dir = syndic_transcript_dir.join("frame");
    let frame_source = [
        fs::read_to_string(frame_dir.join("mod.rs")).expect("frame source readable"),
        fs::read_to_string(frame_dir.join("geometry.rs")).expect("frame geometry source readable"),
        fs::read_to_string(frame_dir.join("types.rs")).expect("frame types source readable"),
    ]
    .join("\n");
    let host_required = [
        "let window = self.scroll_controller.realize",
        "for fact in &window.demand_facts",
        "self.core.push_demand_fact(fact.clone())",
    ];
    let frame_required = [
        "DemandFactKind::VisibleRange",
        "DemandFactKind::OverscanRange",
        "DemandFactKind::MeasuredRecord",
        "DemandFactKind::AdjacentRange",
        "DemandFactKind::ObsoleteRange",
        "DemandFactKind::MissingBefore",
        "DemandFactKind::MissingAfter",
    ];

    for required in host_required {
        assert!(
            host_source.contains(required),
            "host no longer forwards frame demand fact boundary: {required}"
        );
    }
    for required in frame_required {
        assert!(
            frame_source.contains(required),
            "frame source no longer reports expected demand fact: {required}"
        );
    }
}

#[test]
fn syndic_transcript_manual_scroll_command_stays_on_host_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let command_source = fs::read_to_string(syndic_transcript_dir.join("command.rs"))
        .expect("command source readable");
    let host_source =
        fs::read_to_string(syndic_transcript_dir.join("host.rs")).expect("host source readable");
    let panel_source =
        fs::read_to_string(syndic_transcript_dir.join("panel.rs")).expect("panel source readable");
    let shell_source = fs::read_to_string(manifest_dir.join("src").join("shell.rs"))
        .expect("shell source readable");
    let shell_scroll_body = rust_function_body(&shell_source, "fn apply_transcript_scroll_command");

    assert!(command_source.contains("struct ManualTranscriptScrollCommand"));
    assert!(command_source.contains("pub(crate) fn frame_request(self) -> RealizedFrameRequest"));
    assert!(command_source.contains("manual_delta_px: self.delta_px"));
    assert!(host_source.contains("pub(crate) fn manual_scroll("));
    assert!(host_source.contains("self.realize_frame(command.frame_request())"));
    assert!(panel_source.contains("pub(crate) fn manual_scroll("));
    assert!(panel_source.contains("self.host.manual_scroll(command)"));
    assert!(panel_source.contains("pub(crate) fn manual_scroll_delta("));
    assert!(panel_source.contains("ManualTranscriptScrollCommand::new("));
    assert!(panel_source.contains("Some(snapshot.presentation_revision)"));
    assert!(shell_scroll_body.contains("ScrollTranscriptCommand::Wheel"));
    assert!(shell_scroll_body.contains("panel.manual_scroll_delta("));
    assert!(shell_scroll_body.contains("let manual_delta_px = -delta_y;"));
    assert!(shell_scroll_body.contains("window.viewport_size().height"));
    assert!(!shell_scroll_body.contains("apply_transcript_wheel_command"));
    assert!(!shell_scroll_body.contains("TranscriptViewportState"));
    assert!(!host_source.contains("TranscriptViewportState"));
    assert!(!panel_source.contains("TranscriptViewportState"));
}

#[test]
fn syndic_transcript_live_autoscroll_stays_on_resident_host_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let host_source =
        fs::read_to_string(syndic_transcript_dir.join("host.rs")).expect("host source readable");
    let frame_dir = syndic_transcript_dir.join("frame");
    let frame_source = [
        fs::read_to_string(frame_dir.join("mod.rs")).expect("frame source readable"),
        fs::read_to_string(frame_dir.join("geometry.rs")).expect("frame geometry source readable"),
        fs::read_to_string(frame_dir.join("types.rs")).expect("frame types source readable"),
    ]
    .join("\n");
    let required_frame_needles = [
        "RealizedFrameScrollMode",
        "LiveTailFollowing",
        "DetachedManual",
        "pending_tail_placement",
        "previous_snapshot_record_ids",
        "begin_live_tail_following",
        "detach_live_tail_following",
        "manual_delta_px != 0.0",
        "tail_anchor_for_snapshot",
        "has_coherent_tail_growth",
    ];
    let required_host_needles = [
        "TranscriptActivationPlacement::Tail",
        "self.scroll_controller.begin_live_tail_following()",
        "TranscriptActivationPlacement::Start | TranscriptActivationPlacement::Position(_)",
        "self.scroll_controller.detach_live_tail_following()",
        "scroll_snapshot.scroll_mode.diagnostic_label()",
    ];

    for required in required_frame_needles {
        assert!(
            frame_source.contains(required),
            "frame source lost resident live-autoscroll boundary: {required}"
        );
    }
    for required in required_host_needles {
        assert!(
            host_source.contains(required),
            "host source lost resident live-autoscroll boundary: {required}"
        );
    }

    assert!(!frame_source.contains("TranscriptViewportState"));
    assert!(!frame_source.contains("transcript_live_scroll"));
    assert!(!host_source.contains("TranscriptViewportState"));
    assert!(!host_source.contains("transcript_live_scroll"));
}

#[test]
fn syndic_transcript_status_facts_stay_on_resident_host_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let status_source = fs::read_to_string(syndic_transcript_dir.join("status_facts.rs"))
        .expect("status facts source readable");
    let host_source =
        fs::read_to_string(syndic_transcript_dir.join("host.rs")).expect("host source readable");
    let panel_source =
        fs::read_to_string(syndic_transcript_dir.join("panel.rs")).expect("panel source readable");
    let turn_view_source = fs::read_to_string(
        manifest_dir
            .join("src")
            .join("shell")
            .join("turn_view_status.rs"),
    )
    .expect("turn view status source readable");
    let combined_source = [
        status_source.as_str(),
        host_source.as_str(),
        panel_source.as_str(),
        turn_view_source.as_str(),
    ]
    .join("\n");

    for required in [
        "ResidentTranscriptStatusFacts",
        "ResidentTranscriptStatusState",
        "ResidentTranscriptStatusScrollMode",
        "ResidentTranscriptTurnViewFacts",
        "from_core_snapshot",
        "pending_demand_fact_count",
        "pending_provider_request_count",
        "rejected_demand_count",
    ] {
        assert!(
            status_source.contains(required),
            "status facts source lost host-owned fact: {required}"
        );
    }
    assert!(host_source.contains("pub(crate) fn status_facts"));
    assert!(host_source.contains("ResidentTranscriptStatusFacts::from_core_snapshot"));
    assert!(host_source.contains("self.scroll_controller.state_snapshot()"));
    assert!(panel_source.contains("pub(crate) fn status_facts"));
    assert!(panel_source.contains("self.host.status_facts()"));
    assert!(turn_view_source.contains("status_line_projection_with_transcript_facts"));
    assert!(turn_view_source.contains("ResidentTranscriptStatusFacts"));
    assert!(turn_view_source.contains("StatusLineTurnView::new("));
    assert!(turn_view_source.contains("transcript_status_facts.turn_view.current"));
    assert!(turn_view_source.contains("transcript_status_facts.turn_view.total"));

    for forbidden in [
        "syndic-storage",
        "syndic_storage",
        "ThreadStatus",
        "ThreadSessionMetadata",
        "ThreadTokenUsage",
        "SyndicTranscriptProvider",
        "handle_request",
        "rendered_text",
        "backend history",
        "TranscriptHistoryWindow",
        "TranscriptPresentationState",
        "TranscriptViewportState",
        "transcript_projection",
    ] {
        assert!(
            !combined_source.contains(forbidden),
            "status facts crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn rendered_status_line_consumes_panel_status_facts_only() {
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let turn_view_source = include_str!("../src/shell/turn_view_status.rs");
    let workspace_surface_body =
        rust_function_body(conversation_source, "fn render_workspace_surface");
    let turn_view_body = rust_function_body(turn_view_source, "fn status_line_turn_view");

    assert!(workspace_surface_body.contains("transcript_panel.read(cx).status_facts()"));
    assert!(workspace_surface_body.contains(
        "surface.status_line_projection_with_transcript_facts(&transcript_status_facts)"
    ));
    assert!(turn_view_body.contains("transcript_status_facts.turn_view.current"));
    assert!(turn_view_body.contains("transcript_status_facts.turn_view.total"));
    assert!(turn_view_body.contains("StatusLineTurnView::new("));

    for forbidden in [
        "diagnostic_snapshot",
        "snapshot().records",
        "presentation.records",
        "core_snapshot",
        "provider",
        "syndic-storage",
        "syndic_storage",
        "thread/turns/list",
        "ThreadInfo",
        "rendered_text",
        "backend history",
        "TranscriptHistoryWindow",
        "TranscriptPresentationState",
        "TranscriptViewportState",
        "transcript_projection",
    ] {
        assert!(
            !workspace_surface_body.contains(forbidden),
            "rendered status line crossed transcript fact boundary with {forbidden}"
        );
        assert!(
            !turn_view_body.contains(forbidden),
            "status-line turn-view mapping crossed transcript fact boundary with {forbidden}"
        );
    }
}

#[test]
fn selected_thread_activation_prepares_syndic_transcript_before_publication() {
    let shell_source = include_str!("../src/shell.rs");
    let worker_source = include_str!("../src/shell/turn_worker.rs");
    let workspace_open_source = include_str!("../src/shell/workspace_open.rs");
    let decision_resolution_source = include_str!("../src/shell/threaded_decision_resolution.rs");
    let finish_publication_body =
        rust_function_body(shell_source, "fn finish_published_thread_activation");
    let fallback_activation_body = rust_function_body(
        shell_source,
        "fn begin_transcript_host_activation_for_thread",
    );
    let selector_activation_body =
        rust_function_body(shell_source, "fn activate_thread_selector_target");
    let graph_activation_body = rust_function_body(shell_source, "fn select_graph_thread_ref");
    let decision_parent_activation_body = rust_function_body(
        decision_resolution_source,
        "fn begin_decision_resolution_parent_activation",
    );

    assert!(worker_source.contains("prepare_storage_backed_transcript_activation("));
    assert!(workspace_open_source.contains("prepare_storage_backed_transcript_activation("));
    assert!(finish_publication_body.contains("apply_prepared_activation("));
    assert!(fallback_activation_body.contains("prepare_storage_backed_transcript_activation("));
    assert!(fallback_activation_body.contains("apply_prepared_activation("));
    assert!(!fallback_activation_body.contains("begin_activation("));

    for body in [
        selector_activation_body,
        graph_activation_body,
        decision_parent_activation_body,
    ] {
        assert!(
            !body.contains("begin_transcript_host_activation_for_thread("),
            "selected activation path began transcript activation before prepared Syndic state"
        );
        for forbidden in [
            "thread/turns/list",
            "ThreadTurnsListOptions",
            "ThreadReadResponse",
            "include_turns",
            "load_selected_thread_history",
        ] {
            assert!(
                !body.contains(forbidden),
                "selected activation path referenced forbidden CAS history API {forbidden}"
            );
        }
    }
}

#[test]
fn active_turn_retained_text_bytes_are_per_turn_not_cumulative() {
    let active_turn_source = include_str!("../src/shell/active_turn_state.rs");
    let retained_counts_body = rust_function_body(active_turn_source, "fn retained_counts");
    let text_bytes_assignment_start = retained_counts_body
        .rfind("counts.text_bytes =")
        .expect("retained counts should assign text bytes");
    let text_bytes_assignment_tail = &retained_counts_body[text_bytes_assignment_start..];
    let text_bytes_assignment_end = text_bytes_assignment_tail
        .find(';')
        .expect("text bytes assignment should be a statement");
    let text_bytes_assignment = &text_bytes_assignment_tail[..=text_bytes_assignment_end];

    assert!(retained_counts_body.contains("let mut turn_text_bytes = 0usize;"));
    assert!(
        retained_counts_body
            .contains("turn_text_bytes = turn_text_bytes.saturating_add(fragment.text.len());")
    );
    assert!(
        retained_counts_body
            .contains("turn_text_bytes = turn_text_bytes.saturating_add(message.text.len());")
    );
    assert!(
        text_bytes_assignment.contains("turn_text_bytes"),
        "retained text bytes must add the current turn's text only: {text_bytes_assignment}"
    );
    assert!(
        !text_bytes_assignment.contains("user_fragment_text_bytes")
            && !text_bytes_assignment.contains("agent_text_bytes"),
        "retained text bytes must not re-add cumulative counters: {text_bytes_assignment}"
    );
}

#[test]
fn syndic_transcript_selection_copy_stays_on_resident_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let selection_source = fs::read_to_string(syndic_transcript_dir.join("selection.rs"))
        .expect("selection source readable");
    let core_source =
        fs::read_to_string(syndic_transcript_dir.join("core.rs")).expect("core source readable");
    let host_source =
        fs::read_to_string(syndic_transcript_dir.join("host.rs")).expect("host source readable");
    let renderer_selection_source =
        fs::read_to_string(syndic_transcript_dir.join("renderer_selection.rs"))
            .expect("renderer selection source readable");
    let panel_source =
        fs::read_to_string(syndic_transcript_dir.join("panel.rs")).expect("panel source readable");
    let combined_source = [
        selection_source.as_str(),
        core_source.as_str(),
        host_source.as_str(),
        renderer_selection_source.as_str(),
        panel_source.as_str(),
    ]
    .join("\n");
    let required_selection_needles = [
        "ResidentSelectionCommand",
        "ResidentSelectionRecordGeometry",
        "ResidentTranscriptSelection",
        "ResidentTranscriptCopyPayload",
        "ResidentSelectionUnavailable",
        "resident_selected_record",
        "resident_copy_markdown",
    ];
    let required_core_needles = [
        "apply_resident_selection",
        "resident_copy_payload",
        "active_selection_pins = selection.record_ids()",
        "validate_resident_selection",
        "copy_payload_for_selection",
        "clear_active_selection_state",
    ];
    let required_host_needles = [
        "pub(crate) fn apply_resident_selection",
        "pub(crate) fn clear_resident_selection",
        "pub(crate) fn resident_copy_payload",
        "pub(crate) fn resident_selection",
    ];
    let required_renderer_selection_needles = [
        "resident_selection_command_for_realized_record_ids",
        "realized_resident_selectable_record_ids",
        "resident_selection_frame_loss",
        "RealizedFrameWindow",
        "resident_selected_record(record)",
    ];
    let required_panel_needles = [
        "last_frame_window: Option<RealizedFrameWindow>",
        "apply_realized_selection_for_record_ids",
        "pub(crate) fn resident_copy_payload",
        "resident_selection_command_for_realized_record_ids",
        "reconcile_resident_selection_with_frame",
        "resident_selection_frame_loss",
        "clear_resident_selection",
        "render_selected_record_ids",
        "realized_resident_selectable_record_ids",
        "render_selection_affordance",
    ];
    let forbidden_needles = [
        "transcript_selection",
        "markdown_copy",
        "selection_context",
        "TranscriptLineCopyText",
        "selected_text_from_copy_lines",
    ];

    for required in required_selection_needles {
        assert!(
            selection_source.contains(required),
            "selection source lost resident selection/copy boundary: {required}"
        );
    }
    for required in required_core_needles {
        assert!(
            core_source.contains(required),
            "core source lost resident selection/copy boundary: {required}"
        );
    }
    for required in required_host_needles {
        assert!(
            host_source.contains(required),
            "host source lost resident selection/copy boundary: {required}"
        );
    }
    for required in required_renderer_selection_needles {
        assert!(
            renderer_selection_source.contains(required),
            "renderer selection source lost resident selection fact boundary: {required}"
        );
    }
    for required in required_panel_needles {
        assert!(
            panel_source.contains(required),
            "panel source lost resident selection capture boundary: {required}"
        );
    }
    for forbidden in forbidden_needles {
        assert!(
            !combined_source.contains(forbidden),
            "selection/copy source crossed into legacy API with {forbidden}"
        );
    }
}

#[test]
fn syndic_transcript_quote_domain_stays_on_resident_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let selection_source = fs::read_to_string(syndic_transcript_dir.join("selection.rs"))
        .expect("selection source readable");
    let core_source =
        fs::read_to_string(syndic_transcript_dir.join("core.rs")).expect("core source readable");
    let host_source =
        fs::read_to_string(syndic_transcript_dir.join("host.rs")).expect("host source readable");
    let panel_source =
        fs::read_to_string(syndic_transcript_dir.join("panel.rs")).expect("panel source readable");
    let combined_source = [
        selection_source.as_str(),
        core_source.as_str(),
        host_source.as_str(),
        panel_source.as_str(),
    ]
    .join("\n");
    let required_selection_needles = [
        "ResidentQuoteCommand",
        "ResidentTranscriptQuoteTarget",
        "ResidentTranscriptQuotePayload",
        "ResidentQuoteOutcome",
        "resident_quote_markdown",
    ];
    let required_core_needles = [
        "active_quote_target",
        "apply_resident_quote_target",
        "resident_quote_payload",
        "quote_payload_for_target",
        "active_quote_pins = target.record_ids()",
        "copy_payload_for_selection",
    ];
    let required_host_needles = [
        "pub(crate) fn apply_resident_quote_target",
        "pub(crate) fn clear_resident_quote_target",
        "pub(crate) fn resident_quote_payload",
        "pub(crate) fn resident_quote_target",
    ];

    for required in required_selection_needles {
        assert!(
            selection_source.contains(required),
            "selection source lost resident quote domain boundary: {required}"
        );
    }
    for required in required_core_needles {
        assert!(
            core_source.contains(required),
            "core source lost resident quote domain boundary: {required}"
        );
    }
    for required in required_host_needles {
        assert!(
            host_source.contains(required),
            "host source lost resident quote domain boundary: {required}"
        );
    }
    for forbidden in [
        "transcript_quote::quote_insertion_for_draft",
        "TranscriptQuotePopupState",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
    ] {
        assert!(
            !combined_source.contains(forbidden),
            "quote domain crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn syndic_transcript_context_menu_domain_stays_on_resident_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let context_menu_source = fs::read_to_string(syndic_transcript_dir.join("context_menu.rs"))
        .expect("context menu source readable");
    let core_source =
        fs::read_to_string(syndic_transcript_dir.join("core.rs")).expect("core source readable");
    let host_source =
        fs::read_to_string(syndic_transcript_dir.join("host.rs")).expect("host source readable");
    let combined_source = [
        context_menu_source.as_str(),
        core_source.as_str(),
        host_source.as_str(),
    ]
    .join("\n");
    let required_context_menu_needles = [
        "ResidentContextMenuCommand",
        "ResidentTranscriptContextMenuTarget",
        "ResidentContextMenuRecord",
        "ResidentContextMenuContentKind",
        "ResidentContextMenuOutcome",
        "ResidentContextMenuUnavailable",
        "resident_context_menu_record",
        "ResidentRecordSource::Syndic(source)",
        "source_has_stable_context_menu_provenance",
    ];
    let required_core_needles = [
        "active_context_menu_target",
        "apply_resident_context_menu_target",
        "clear_resident_context_menu_target",
        "resident_context_menu_target",
        "validate_resident_context_menu_target",
        "active_menu_pins = target.record_ids()",
        "clear_active_context_menu_target_state",
    ];
    let required_host_needles = [
        "pub(crate) fn apply_resident_context_menu_target",
        "pub(crate) fn clear_resident_context_menu_target",
        "pub(crate) fn resident_context_menu_target",
    ];

    for required in required_context_menu_needles {
        assert!(
            context_menu_source.contains(required),
            "context menu source lost resident domain boundary: {required}"
        );
    }
    for required in required_core_needles {
        assert!(
            core_source.contains(required),
            "core source lost resident context-menu boundary: {required}"
        );
    }
    for required in required_host_needles {
        assert!(
            host_source.contains(required),
            "host source lost resident context-menu boundary: {required}"
        );
    }
    for forbidden in [
        "transcript_branch",
        "transcript_edit",
        "TranscriptBranch",
        "TranscriptEdit",
        "TranscriptHistoryWindow",
        "transcript_projection",
        "selected_text_from_copy_lines",
        "syndic-storage",
        "syndic_storage",
    ] {
        assert!(
            !combined_source.contains(forbidden),
            "context-menu domain crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn syndic_transcript_renderer_quote_target_stays_on_resident_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let renderer_quote_source = fs::read_to_string(syndic_transcript_dir.join("renderer_quote.rs"))
        .expect("renderer quote source readable");
    let panel_source =
        fs::read_to_string(syndic_transcript_dir.join("panel.rs")).expect("panel source readable");
    let combined_source = [renderer_quote_source.as_str(), panel_source.as_str()].join("\n");
    let required_renderer_quote_needles = [
        "resident_quote_command_for_realized_record_ids",
        "realized_resident_quotable_record_ids",
        "resident_quote_frame_loss",
        "ResidentQuoteCommand::new",
        "resident_selection_command_for_realized_record_ids",
        "resident_selection_frame_loss",
        "ResidentTranscriptQuoteTarget",
    ];
    let required_panel_needles = [
        "apply_realized_quote_target_for_record_ids",
        "resident_quote_command_for_realized_record_ids",
        "reconcile_resident_quote_target_with_frame",
        "resident_quote_frame_loss",
        "clear_resident_quote_target",
        "resident_quote_target",
        "resident_quote_payload",
    ];

    for required in required_renderer_quote_needles {
        assert!(
            renderer_quote_source.contains(required),
            "renderer quote source lost resident fact boundary: {required}"
        );
    }
    for required in required_panel_needles {
        assert!(
            panel_source.contains(required),
            "panel source lost resident quote target capture boundary: {required}"
        );
    }
    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        "transcript_quote::quote_insertion_for_draft",
        "TranscriptQuotePopupState",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
    ] {
        assert!(
            !combined_source.contains(forbidden),
            "renderer quote target crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn syndic_transcript_renderer_context_menu_target_stays_on_resident_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let renderer_context_menu_source =
        fs::read_to_string(syndic_transcript_dir.join("renderer_context_menu.rs"))
            .expect("renderer context-menu source readable");
    let panel_source =
        fs::read_to_string(syndic_transcript_dir.join("panel.rs")).expect("panel source readable");
    let combined_source = [renderer_context_menu_source.as_str(), panel_source.as_str()].join("\n");
    let required_renderer_context_menu_needles = [
        "resident_context_menu_command_for_realized_record_id",
        "realized_resident_context_menu_record_ids",
        "resident_context_menu_frame_loss",
        "ResidentContextMenuCommand::new",
        "resident_context_menu_record(record",
        "ResidentTranscriptContextMenuTarget",
        "RealizedFrameWindow",
    ];
    let required_panel_needles = [
        "resident_context_menu_target",
        "resident_context_menu_command_target",
        "apply_realized_context_menu_target_for_record_id",
        "apply_realized_context_menu_target(&context_record_id",
        "OpenTranscriptContextMenu.boxed_clone()",
        "resident_context_menu_command_for_realized_record_id",
        "reconcile_resident_context_menu_target_with_frame",
        "resident_context_menu_frame_loss",
        "clear_resident_context_menu_target",
    ];

    for required in required_renderer_context_menu_needles {
        assert!(
            renderer_context_menu_source.contains(required),
            "renderer context-menu source lost resident fact boundary: {required}"
        );
    }
    for required in required_panel_needles {
        assert!(
            panel_source.contains(required),
            "panel source lost resident context-menu target capture boundary: {required}"
        );
    }
    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        "transcript_branch",
        "transcript_edit",
        "TranscriptBranch",
        "TranscriptEdit",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
    ] {
        assert!(
            !combined_source.contains(forbidden),
            "renderer context-menu target crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn syndic_transcript_renderer_media_action_target_stays_on_resident_boundary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let renderer_media_source =
        fs::read_to_string(syndic_transcript_dir.join("renderer_media_action.rs"))
            .expect("renderer media-action source readable");
    let panel_source =
        fs::read_to_string(syndic_transcript_dir.join("panel.rs")).expect("panel source readable");
    let combined_source = [renderer_media_source.as_str(), panel_source.as_str()].join("\n");
    let required_renderer_media_needles = [
        "resident_media_action_command_for_realized_record_id",
        "realized_resident_media_action_record_ids",
        "resident_media_action_frame_loss",
        "resident_media_reference(record)",
        "resident_media_action_record(reference",
        ".metadata_for(&reference.resource_id)",
        "ResidentMediaRangeAvailability::Demandable",
        "ResidentTranscriptMediaActionTarget",
        "RealizedFrameWindow",
    ];
    let required_panel_needles = [
        "resident_media_action_target",
        "apply_realized_media_action_target_for_record_id",
        "resident_media_action_command_for_realized_record_id",
        "reconcile_resident_media_action_target_with_frame",
        "resident_media_action_frame_loss",
        "clear_resident_media_action_target",
    ];

    for required in required_renderer_media_needles {
        assert!(
            renderer_media_source.contains(required),
            "renderer media-action source lost resident fact boundary: {required}"
        );
    }
    for required in required_panel_needles {
        assert!(
            panel_source.contains(required),
            "panel source lost resident media-action capture boundary: {required}"
        );
    }
    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        "transcript_media",
        "transcript_image",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
        "ReadResourceRange",
        "request_resource_range(",
    ] {
        assert!(
            !combined_source.contains(forbidden),
            "renderer media-action target crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn shell_transcript_copy_command_uses_resident_payload_only() {
    let shell_source = include_str!("../src/shell.rs");
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let panel_source = include_str!("../src/shell/syndic_transcript/panel.rs");
    let copy_body = rust_function_body(shell_source, "fn copy_transcript_selection_action");
    let payload_text_body = rust_function_body(
        shell_source,
        "fn transcript_clipboard_text_from_resident_payload",
    );
    let left_panel_body = rust_function_body(conversation_source, "fn render_left_panel");

    assert!(shell_source.contains("CopyTranscriptSelection"));
    assert!(shell_source.contains("Some(syndic_transcript::SYNDIC_TRANSCRIPT_KEY_CONTEXT)"));
    assert!(left_panel_body.contains("ShellView::copy_transcript_selection_action"));
    assert!(panel_source.contains("pub(crate) fn resident_copy_payload"));
    assert!(copy_body.contains(".resident_copy_payload()"));
    assert!(copy_body.contains("unavailable_command(\"copy_transcript_selection\")"));
    assert!(copy_body.contains("ClipboardItem::new_string(text)"));
    assert!(payload_text_body.contains("payload.markdown"));
    assert!(payload_text_body.contains("payload.plain_text"));

    for forbidden in [
        "conversation_input",
        "selection_export",
        "copy_composer_selection_action",
        "composer_clipboard_payload_from_selection",
        "selection.copy_text",
        "TranscriptHistoryWindow",
        "transcript_presentation",
        "transcript_quote",
        "selected_text_from_copy_lines",
    ] {
        assert!(
            !copy_body.contains(forbidden) && !payload_text_body.contains(forbidden),
            "shell transcript copy crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn shell_transcript_context_menu_command_uses_resident_target_only() {
    let shell_source = include_str!("../src/shell.rs");
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let command_source = include_str!("../src/shell/syndic_transcript/command.rs");
    let panel_source = include_str!("../src/shell/syndic_transcript/panel.rs");
    let left_panel_body = rust_function_body(conversation_source, "fn render_left_panel");
    let open_action_body =
        rust_function_body(shell_source, "fn open_transcript_context_menu_action");
    let open_body = rust_function_body(
        shell_source,
        "fn open_transcript_context_menu_from_resident_target",
    );
    let accept_body = rust_function_body(
        shell_source,
        "fn accept_resident_transcript_context_menu_target",
    );

    assert!(shell_source.contains("OpenTranscriptContextMenu"));
    assert!(left_panel_body.contains("ShellView::open_transcript_context_menu_action"));
    assert!(command_source.contains("ResidentContextMenuCommandTarget"));
    assert!(command_source.contains("from_active_target"));
    assert!(panel_source.contains("pub(crate) fn resident_context_menu_command_target"));
    assert!(panel_source.contains("apply_realized_context_menu_target(&context_record_id"));
    assert!(panel_source.contains("OpenTranscriptContextMenu.boxed_clone()"));
    assert!(open_action_body.contains("open_transcript_context_menu_from_resident_target"));
    assert!(open_body.contains(".resident_context_menu_command_target()"));
    assert!(open_body.contains("ResidentContextMenuCommandTarget::Targeted(target)"));
    assert!(open_body.contains("ResidentContextMenuCommandTarget::Unavailable(_)"));
    assert!(open_body.contains("unavailable_command(\"open_transcript_context_menu\")"));
    assert!(open_body.contains("accept_resident_transcript_context_menu_target(target)"));
    assert!(accept_body.contains("target.record_ids()"));
    assert!(accept_body.contains("TranscriptCommandResult::NoOp"));

    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        "transcript_branch",
        "transcript_edit",
        "TranscriptBranch",
        "TranscriptEdit",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
        "rendered_text",
        "backend history",
    ] {
        assert!(
            !open_body.contains(forbidden) && !accept_body.contains(forbidden),
            "shell transcript context-menu command crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn shell_resident_edit_and_branch_commands_use_context_menu_targets_only() {
    let shell_source = include_str!("../src/shell.rs");
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let command_source = include_str!("../src/shell/syndic_transcript/command.rs");
    let panel_source = include_str!("../src/shell/syndic_transcript/panel.rs");
    let left_panel_body = rust_function_body(conversation_source, "fn render_left_panel");
    let edit_action_body =
        rust_function_body(shell_source, "fn edit_resident_context_target_action");
    let edit_body = rust_function_body(shell_source, "fn edit_resident_context_target_from_panel");
    let accept_edit_body = rust_function_body(shell_source, "fn accept_resident_edit_target");
    let branch_action_body =
        rust_function_body(shell_source, "fn branch_resident_context_target_action");
    let branch_body =
        rust_function_body(shell_source, "fn branch_resident_context_target_from_panel");
    let accept_branch_body = rust_function_body(shell_source, "fn accept_resident_branch_target");
    let combined_command_bodies = [
        edit_action_body,
        edit_body,
        accept_edit_body,
        branch_action_body,
        branch_body,
        accept_branch_body,
    ]
    .join("\n");

    assert!(shell_source.contains("EditResidentContextTarget"));
    assert!(shell_source.contains("BranchResidentContextTarget"));
    assert!(left_panel_body.contains("ShellView::edit_resident_context_target_action"));
    assert!(left_panel_body.contains("ShellView::branch_resident_context_target_action"));
    assert!(panel_source.contains("pub(crate) fn resident_context_menu_command_target"));
    assert!(command_source.contains("ResidentActionTargetProvenance"));
    assert!(command_source.contains("from_context_menu_target"));
    assert!(command_source.contains("ResidentEditCommandTarget"));
    assert!(command_source.contains("ResidentBranchCommandTarget"));
    assert!(edit_action_body.contains("edit_resident_context_target_from_panel"));
    assert!(edit_body.contains(".resident_context_menu_command_target()"));
    assert!(edit_body.contains("ResidentEditCommandTarget::from_context_menu_command_target"));
    assert!(edit_body.contains("ResidentEditCommandTarget::Targeted(target)"));
    assert!(edit_body.contains("ResidentEditCommandTarget::Unavailable(_)"));
    assert!(edit_body.contains("unavailable_command(\"edit_resident_context_target\")"));
    assert!(edit_body.contains("accept_resident_edit_target(target, window, cx)"));
    assert!(accept_edit_body.contains("target.record_ids()"));
    assert!(accept_edit_body.contains("TranscriptCommandResult::NoOp"));
    assert!(branch_action_body.contains("branch_resident_context_target_from_panel"));
    assert!(branch_body.contains(".resident_context_menu_command_target()"));
    assert!(branch_body.contains("ResidentBranchCommandTarget::from_context_menu_command_target"));
    assert!(branch_body.contains("ResidentBranchCommandTarget::Targeted(target)"));
    assert!(branch_body.contains("ResidentBranchCommandTarget::Unavailable(_)"));
    assert!(branch_body.contains("unavailable_command(\"branch_resident_context_target\")"));
    assert!(branch_body.contains("accept_resident_branch_target(target, cx)"));
    assert!(accept_branch_body.contains("target.record_ids()"));
    assert!(accept_branch_body.contains("TranscriptCommandResult::NoOp"));

    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        "transcript_branch",
        "transcript_edit",
        "TranscriptBranch",
        "TranscriptEdit",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
        "rendered_text",
        "backend history",
    ] {
        assert!(
            !combined_command_bodies.contains(forbidden),
            "shell resident edit/branch command crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn shell_media_preview_command_uses_resident_payload_only() {
    let shell_source = include_str!("../src/shell.rs");
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let command_source = include_str!("../src/shell/syndic_transcript/command.rs");
    let host_source = include_str!("../src/shell/syndic_transcript/host.rs");
    let panel_source = include_str!("../src/shell/syndic_transcript/panel.rs");
    let left_panel_body = rust_function_body(conversation_source, "fn render_left_panel");
    let preview_action_body =
        rust_function_body(shell_source, "fn preview_resident_transcript_media_action");
    let preview_body = rust_function_body(
        shell_source,
        "fn preview_resident_transcript_media_from_panel",
    );
    let accept_body = rust_function_body(shell_source, "fn accept_resident_media_preview_payload");
    let combined_command_bodies = [preview_action_body, preview_body, accept_body].join("\n");

    assert!(shell_source.contains("PreviewResidentTranscriptMedia"));
    assert!(left_panel_body.contains("ShellView::preview_resident_transcript_media_action"));
    assert!(command_source.contains("ResidentMediaPreviewCommandPayload"));
    assert!(command_source.contains("ResidentMediaPreviewCommandTarget"));
    assert!(command_source.contains("from_resident_payload"));
    assert!(host_source.contains("pub(crate) fn resident_media_preview_command_target"));
    assert!(host_source.contains("self.core.resident_media_action_payload()"));
    assert!(panel_source.contains("pub(crate) fn resident_media_preview_command_target"));
    assert!(preview_action_body.contains("preview_resident_transcript_media_from_panel"));
    assert!(preview_body.contains(".resident_media_preview_command_target()"));
    assert!(preview_body.contains("ResidentMediaPreviewCommandTarget::Targeted(payload)"));
    assert!(preview_body.contains("ResidentMediaPreviewCommandTarget::Unavailable(_)"));
    assert!(preview_body.contains("unavailable_command(\"preview_resident_transcript_media\")"));
    assert!(preview_body.contains("accept_resident_media_preview_payload(payload)"));
    assert!(accept_body.contains("payload.record_ids()"));
    assert!(accept_body.contains("payload.range()"));
    assert!(accept_body.contains("payload.byte_len()"));
    assert!(accept_body.contains("TranscriptCommandResult::NoOp"));

    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        ".resident_media_action_payload()",
        "transcript_media::",
        "transcript_image",
        "TranscriptImage",
        "ClipboardItem",
        "write_to_clipboard",
        "read_from_clipboard",
        "std::fs",
        "File::",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
        "rendered_text",
        "backend history",
    ] {
        assert!(
            !combined_command_bodies.contains(forbidden),
            "shell media preview command crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn shell_media_copy_command_writes_resident_payload_to_image_clipboard() {
    let shell_source = include_str!("../src/shell.rs");
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let command_source = include_str!("../src/shell/syndic_transcript/command.rs");
    let host_source = include_str!("../src/shell/syndic_transcript/host.rs");
    let panel_source = include_str!("../src/shell/syndic_transcript/panel.rs");
    let left_panel_body = rust_function_body(conversation_source, "fn render_left_panel");
    let copy_action_body =
        rust_function_body(shell_source, "fn copy_resident_transcript_media_action");
    let copy_body =
        rust_function_body(shell_source, "fn copy_resident_transcript_media_from_panel");
    let write_body = rust_function_body(
        shell_source,
        "fn write_resident_media_copy_payload_to_clipboard",
    );
    let format_body = rust_function_body(shell_source, "fn resident_media_copy_image_format");
    let combined_command_bodies = [copy_action_body, copy_body, write_body, format_body].join("\n");

    assert!(shell_source.contains("CopyResidentTranscriptMedia"));
    assert!(left_panel_body.contains("ShellView::copy_resident_transcript_media_action"));
    assert!(command_source.contains("ResidentMediaCopyCommandPayload"));
    assert!(command_source.contains("ResidentMediaCopyCommandTarget"));
    assert!(command_source.contains("from_resident_payload"));
    assert!(host_source.contains("pub(crate) fn resident_media_copy_command_target"));
    assert!(host_source.contains("self.core.resident_media_action_payload()"));
    assert!(panel_source.contains("pub(crate) fn resident_media_copy_command_target"));
    assert!(copy_action_body.contains("copy_resident_transcript_media_from_panel"));
    assert!(copy_body.contains(".resident_media_copy_command_target()"));
    assert!(copy_body.contains("ResidentMediaCopyCommandTarget::Targeted(payload)"));
    assert!(copy_body.contains("ResidentMediaCopyCommandTarget::Unavailable(_)"));
    assert!(copy_body.contains("unavailable_command(\"copy_resident_transcript_media\")"));
    assert!(copy_body.contains("write_resident_media_copy_payload_to_clipboard(payload, cx)"));
    assert!(write_body.contains("resident_media_copy_image_format(&payload)"));
    assert!(write_body.contains("payload.complete()"));
    assert!(write_body.contains("payload.record_ids()"));
    assert!(write_body.contains("payload.range()"));
    assert!(write_body.contains("payload.byte_len()"));
    assert!(write_body.contains("Image::from_bytes(format, payload.bytes().to_vec())"));
    assert!(write_body.contains("ClipboardItem::new_image(&image)"));
    assert!(write_body.contains("cx.write_to_clipboard"));
    assert!(format_body.contains("payload.media_type()"));
    assert!(format_body.contains("ImageFormat::from_mime_type"));

    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        ".resident_media_action_payload()",
        "transcript_media::",
        "transcript_image",
        "TranscriptImage",
        "ClipboardItem::new_string",
        "read_from_clipboard",
        "std::fs",
        "File::",
        "PathBuf",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
        "rendered_text",
        "backend history",
    ] {
        assert!(
            !combined_command_bodies.contains(forbidden),
            "shell media copy command crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn shell_media_save_command_writes_resident_payload_to_explicit_destination() {
    let shell_source = include_str!("../src/shell.rs");
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let command_source = include_str!("../src/shell/syndic_transcript/command.rs");
    let host_source = include_str!("../src/shell/syndic_transcript/host.rs");
    let panel_source = include_str!("../src/shell/syndic_transcript/panel.rs");
    let left_panel_body = rust_function_body(conversation_source, "fn render_left_panel");
    let save_action_body =
        rust_function_body(shell_source, "fn save_resident_transcript_media_action");
    let no_destination_body = rust_function_body(
        shell_source,
        "fn save_resident_transcript_media_without_destination",
    );
    let save_body = rust_function_body(shell_source, "fn save_resident_transcript_media_to_path");
    let write_body =
        rust_function_body(shell_source, "fn write_resident_media_save_payload_to_path");
    let destination_body = rust_function_body(shell_source, "fn resident_media_save_destination");
    let safe_path_body = rust_function_body(
        shell_source,
        "fn resident_media_save_destination_path_is_safe",
    );
    let combined_command_bodies = [
        save_action_body,
        no_destination_body,
        save_body,
        write_body,
        destination_body,
        safe_path_body,
    ]
    .join("\n");

    assert!(shell_source.contains("SaveResidentTranscriptMedia"));
    assert!(left_panel_body.contains("ShellView::save_resident_transcript_media_action"));
    assert!(command_source.contains("ResidentMediaSaveCommandPayload"));
    assert!(command_source.contains("ResidentMediaSaveCommandTarget"));
    assert!(command_source.contains("ResidentMediaSaveDestination"));
    assert!(command_source.contains("ResidentMediaSaveDestinationUnavailable"));
    assert!(host_source.contains("pub(crate) fn resident_media_save_command_target"));
    assert!(host_source.contains("self.core.resident_media_action_payload()"));
    assert!(panel_source.contains("pub(crate) fn resident_media_save_command_target"));
    assert!(save_action_body.contains("save_resident_transcript_media_without_destination(cx)"));
    assert!(
        no_destination_body.contains("unavailable_command(\"save_resident_transcript_media\")")
    );
    assert!(save_body.contains("resident_media_save_destination(destination)"));
    assert!(save_body.contains(".resident_media_save_command_target()"));
    assert!(save_body.contains("ResidentMediaSaveCommandTarget::Targeted(payload)"));
    assert!(save_body.contains("ResidentMediaSaveCommandTarget::Unavailable(_)"));
    assert!(save_body.contains("unavailable_command(\"save_resident_transcript_media\")"));
    assert!(
        save_body.contains("write_resident_media_save_payload_to_path(payload, destination, cx)")
    );
    assert!(write_body.contains("payload.complete()"));
    assert!(write_body.contains("payload.record_ids()"));
    assert!(write_body.contains("payload.resource_id()"));
    assert!(write_body.contains("payload.range()"));
    assert!(write_body.contains("payload.byte_len()"));
    assert!(write_body.contains("payload.media_type()"));
    assert!(write_body.contains("fs::write(destination.path(), payload.bytes())"));
    assert!(destination_body.contains("ResidentMediaSaveDestination::new(destination).ok()?"));
    assert!(destination_body.contains("resident_media_save_destination_path_is_safe"));
    assert!(safe_path_body.contains("destination.exists()"));
    assert!(safe_path_body.contains("destination.is_dir()"));
    assert!(safe_path_body.contains("parent.is_dir()"));

    for forbidden in [
        "provider.read",
        "provider.request",
        "handle_provider_response",
        "drain_demand_facts",
        "syndic-storage",
        "syndic_storage",
        ".resident_media_action_payload()",
        "transcript_media::",
        "transcript_image",
        "TranscriptImage",
        "ClipboardItem",
        "write_to_clipboard",
        "read_from_clipboard",
        "File::",
        "Image::from_bytes",
        "ImageFormat",
        "selected_text()",
        "selected_text_from_copy_lines",
        "TranscriptHistoryWindow",
        "transcript_projection",
        "rendered_text",
        "backend history",
    ] {
        assert!(
            !combined_command_bodies.contains(forbidden),
            "shell media save command crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn shell_transcript_quote_command_uses_resident_payload_only() {
    let shell_source = include_str!("../src/shell.rs");
    let panel_source = include_str!("../src/shell/syndic_transcript/panel.rs");
    let quote_body = rust_function_body(shell_source, "fn insert_transcript_quote_into_draft");

    assert!(panel_source.contains("pub(crate) fn resident_quote_payload"));
    assert!(quote_body.contains(".resident_quote_payload()"));
    assert!(quote_body.contains("payload.quoted_markdown"));
    assert!(quote_body.contains("replace_selected_text(&quoted_markdown"));
    assert!(quote_body.contains("sync_composer_draft_from_input"));
    assert!(quote_body.contains("unavailable_command(\"quote_transcript_selection\")"));

    for forbidden in [
        "resident_copy_payload",
        "read_from_clipboard",
        "ClipboardItem",
        "selection_export",
        "selected_text()",
        "TranscriptHistoryWindow",
        "transcript_presentation",
        "transcript_projection",
        "transcript_quote",
        "selected_text_from_copy_lines",
    ] {
        assert!(
            !quote_body.contains(forbidden),
            "shell transcript quote crossed boundary with {forbidden}"
        );
    }
}

#[test]
fn syndic_transcript_sources_avoid_forbidden_legacy_apis() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let syndic_transcript_dir = manifest_dir
        .join("src")
        .join("shell")
        .join("syndic_transcript");
    let forbidden_needles = [
        "TranscriptHistoryWindow",
        "TranscriptPresentationState",
        "TranscriptViewportState",
        "TranscriptResidencyController",
        "TranscriptResidencyPageRequest",
        "transcript_markdown",
        "transcript_projection",
        "transcript_prepublication_preparation",
        "selected_thread_activation",
        "render::transcript",
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&syndic_transcript_dir) {
        let source = fs::read_to_string(&path).expect("new transcript source should be readable");
        for forbidden in forbidden_needles {
            if source.contains(forbidden) {
                offenders.push(format!(
                    "{} contains {forbidden}",
                    display_test_path(&syndic_transcript_dir, &path)
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "new transcript source references legacy APIs: {offenders:?}"
    );
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let signature_index = source
        .find(function_signature)
        .unwrap_or_else(|| panic!("missing function {function_signature}"));
    let after_signature = &source[signature_index..];
    let open_offset = after_signature
        .find('{')
        .unwrap_or_else(|| panic!("missing body for function {function_signature}"));
    let body_start = signature_index + open_offset;
    let mut depth = 0usize;

    for (offset, character) in source[body_start..].char_indices() {
        match character {
            '{' => depth = depth.saturating_add(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &source[body_start..body_start + offset + character.len_utf8()];
                }
            }
            _ => {}
        }
    }

    panic!("unterminated body for function {function_signature}");
}

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("test directory should be readable") {
            let entry = entry.expect("test directory entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    files
}

fn display_test_path(tests_dir: &Path, path: &Path) -> String {
    path.strip_prefix(tests_dir)
        .unwrap_or(path)
        .display()
        .to_string()
}
