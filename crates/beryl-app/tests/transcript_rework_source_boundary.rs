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
fn backend_source_exposes_no_cas_catalog_protocol() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend_src_dir = manifest_dir
        .parent()
        .expect("app crate should be under workspace crates")
        .join("beryl-backend")
        .join("src");
    let loaded_list_method = ["thread", "loaded", "list"].join("/");
    let loaded_list_type = ["Thread", "Loaded", "List"].join("");
    let known_thread_key = ["known", "threads"].join("_");
    let known_thread_constant = ["KNOWN", "THREADS"].join("_");
    let forbidden_needles = [
        "thread/list",
        "ThreadListOptions",
        "ThreadListResponse",
        "ThreadSortKey",
        "SortDirection",
        "CompatibilityProbe::ThreadList",
        "list_thread_page",
        "list_threads_with_options",
        "list_threads(",
        loaded_list_method.as_str(),
        loaded_list_type.as_str(),
        known_thread_key.as_str(),
        known_thread_constant.as_str(),
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&backend_src_dir) {
        let source = fs::read_to_string(&path).expect("backend source should be readable");
        for forbidden in forbidden_needles {
            if source.contains(forbidden) {
                offenders.push(format!(
                    "{} contains {forbidden}",
                    display_test_path(&backend_src_dir, &path)
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "CAS catalog protocol remains live: {offenders:?}"
    );
}

#[test]
fn live_app_source_avoids_obsolete_catalog_storage_and_activation_inputs() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let metadata_read_path = "shell/tool_activity_nickname.rs";
    let loaded_list_method = ["thread", "loaded", "list"].join("/");
    let loaded_list_type = ["Thread", "Loaded", "List"].join("");
    let known_thread_key = ["known", "threads"].join("_");
    let known_thread_constant = ["KNOWN", "THREADS"].join("_");
    let direct_store_open = ["SyndicStore", "open"].join("::");
    let store_open_options = ["Store", "Open", "Options"].join("");
    let storage_module = format!("{}::", ["syndic", "storage"].join("_"));
    let member_inventory = ["Member", "Thread", "Inventory"].join("");
    let selector_projection = ["Thread", "Selector", "Projection"].join("");
    let mut offenders = Vec::new();
    let mut metadata_read_sources = Vec::new();

    for path in rust_files_under(&src_dir) {
        let source = fs::read_to_string(&path).expect("app source should be readable");
        let relative_path = display_test_path(&src_dir, &path).replace('\\', "/");
        if source.contains("read_thread_metadata(")
            || source.contains("read_thread_metadata_details(")
        {
            metadata_read_sources.push(relative_path.clone());
            if relative_path != metadata_read_path {
                offenders.push(format!(
                    "{relative_path} contains metadata-only thread/read"
                ));
            }
        }
        for forbidden in [
            "ThreadActivationLoader",
            "ExistingThreadActivation",
            "thread_activation/loader",
            "thread/turns/list",
            "ThreadTurnsListOptions",
            "ThreadReadResponse",
            loaded_list_method.as_str(),
            loaded_list_type.as_str(),
            known_thread_key.as_str(),
            known_thread_constant.as_str(),
            direct_store_open.as_str(),
            store_open_options.as_str(),
            storage_module.as_str(),
            member_inventory.as_str(),
            selector_projection.as_str(),
        ] {
            if source.contains(forbidden) {
                offenders.push(format!("{relative_path} contains {forbidden}"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "obsolete app catalog, storage, or activation input remains: {offenders:?}"
    );
    metadata_read_sources.sort();
    metadata_read_sources.dedup();
    assert_eq!(metadata_read_sources, vec![metadata_read_path.to_string()]);
}

#[test]
fn cas_thread_name_is_not_thread_title_authority() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates_dir = manifest_dir
        .parent()
        .expect("app crate should be under workspace crates");
    let backend_src_dir = crates_dir.join("beryl-backend").join("src");
    let model_src_dir = crates_dir.join("beryl-model").join("src");
    let app_src_dir = manifest_dir.join("src");
    let backend_forbidden = [
        "thread/name/set",
        "ThreadNameSet",
        "ThreadSetNameParams",
        "set_thread_name(",
    ];
    let title_authority_forbidden = [
        "thread/name/set",
        "ThreadNameSet",
        "ThreadSetNameParams",
        "set_thread_name(",
        "apply_thread_name_update",
        "apply_authoritative_thread_name_update",
        "set_authoritative_thread_backend_name",
        "set_authoritative_backend_name",
        "set_thread_backend_name",
        "title_with_backend_name(",
        "backend_name",
        "ignored_backend_name_for_automatic_title",
        "ignores_backend_name_for_automatic_title",
        "with_ignored_backend_name_for_automatic_title",
    ];
    let mut offenders = Vec::new();

    for path in rust_files_under(&backend_src_dir) {
        let source = fs::read_to_string(&path).expect("backend source should be readable");
        for forbidden in backend_forbidden {
            if source.contains(forbidden) {
                offenders.push(format!(
                    "beryl-backend/{} contains {forbidden}",
                    display_test_path(&backend_src_dir, &path)
                ));
            }
        }
    }

    for (crate_label, src_dir) in [("beryl-app", app_src_dir), ("beryl-model", model_src_dir)] {
        for path in rust_files_under(&src_dir) {
            let source = fs::read_to_string(&path).expect("source should be readable");
            for forbidden in title_authority_forbidden {
                if source.contains(forbidden) {
                    offenders.push(format!(
                        "{crate_label}/{} contains {forbidden}",
                        display_test_path(&src_dir, &path)
                    ));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "CAS thread-name title authority remains live: {offenders:?}"
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
