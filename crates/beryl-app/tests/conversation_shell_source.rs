#[test]
fn workspace_shell_rendering_uses_initialized_controls_and_shared_composer_frame() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let ready_shell_body = rust_function_body(render_source, "pub(super) fn render_ready_shell");
    let workspace_surface_body = rust_function_body(render_source, "fn render_workspace_surface");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let split_surface_body = rust_function_body(render_source, "fn render_split_surface");
    let measure_composer_body = rust_function_body(render_source, "fn measure_composer_input");
    let composer_body = rust_function_body(render_source, "fn render_composer(");
    let composer_input_area_body =
        rust_function_body(render_source, "fn render_composer_input_area");
    let loaded_composer_body =
        rust_function_body(render_source, "fn render_loaded_workspace_composer");

    assert!(ready_shell_body.contains("render_workspace_surface"));
    assert!(workspace_surface_body.contains("render_toolbar("));
    assert!(workspace_surface_body.contains("render_thread_strip("));
    assert!(workspace_surface_body.contains("render_split_surface("));
    assert!(toolbar_body.contains("activity_mode_button"));
    assert!(toolbar_body.contains("\"toggle-graph-overlay\""));
    assert!(toolbar_body.contains("\"toggle-checklist-sidebar\""));
    assert!(thread_strip_body.contains("\"thread-strip-new-thread\""));
    assert!(split_surface_body.contains("render_composer("));
    assert!(measure_composer_body.contains("measure_geometry"));
    assert!(measure_composer_body.contains("composer_input_measurement"));
    assert!(composer_body.contains("render_composer_input_area"));
    assert!(!composer_body.contains("wrapped_visual_line_count_for_width"));
    assert!(!composer_body.contains("reveal_composer_cursor"));
    assert!(!composer_input_area_body.contains("overflow_y_scroll"));
    assert!(loaded_composer_body.contains("render_composer_input_area"));
    assert!(loaded_composer_body.contains("measure_geometry"));
}

#[test]
fn workspace_shell_rendering_omits_legacy_no_member_composer_affordances() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let idle_shell_body =
        rust_function_body(render_source, "pub(super) fn render_idle_workspace_shell");

    assert!(idle_shell_body.contains("Runtime environment recovery required"));
    assert!(idle_shell_body.contains("No runtime environment selected"));
    assert!(!idle_shell_body.contains("has_selected_runtime"));
    assert!(!render_source.contains("\"Workspace Member Required\""));
    assert!(!render_source.contains("\"workspace-member-required\""));
    assert!(!render_source.contains("\"No primary workspace member selected\""));
    assert!(!render_source.contains("\"No managed backend is active\""));
    assert!(!idle_shell_body.contains("disabled_secondary_button"));
}

#[test]
fn backend_unavailable_workspace_surface_disables_backend_controls() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let shell_source = include_str!("../src/shell.rs");
    let backend_unavailable_body = rust_function_body(
        render_source,
        "pub(super) fn render_backend_unavailable_shell",
    );
    let workspace_surface_body = rust_function_body(render_source, "fn render_workspace_surface");
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let composer_body = rust_function_body(render_source, "fn render_composer(");
    let backend_controls_body = rust_function_body(
        shell_source,
        "pub(crate) fn backend_controls_disabled_message",
    );

    assert!(backend_unavailable_body.contains("execution_target.display_label()"));
    assert!(backend_unavailable_body.contains("backend_controls_disabled_message()"));
    assert!(backend_controls_body.contains("current_conversation_submission_block()"));
    assert!(workspace_surface_body.contains("StatusLineProjection::unknown()"));
    assert!(workspace_surface_body.contains("backend_controls_disabled.is_none()"));
    assert!(workspace_surface_body.contains("new_thread_controls_disabled_message()"));
    assert!(workspace_surface_body.contains("thread_selector_controls_disabled_message()"));
    assert!(workspace_surface_body.contains("thread_selector_controls_disabled.is_none()"));
    assert!(thread_strip_body.contains("disabled_secondary_button"));
    assert!(thread_strip_body.contains("new_thread_enabled"));
    assert!(thread_strip_body.contains("thread_selector_enabled"));
    assert!(composer_body.contains("set_enabled(enabled"));
    assert!(composer_body.contains("backend_controls_disabled"));
}

#[test]
fn backend_unavailable_commands_gate_before_mutating_drafts_or_threads() {
    let shell_source = include_str!("../src/shell.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let inventory_source = include_str!("../src/shell/member_thread_inventory.rs");
    let queue_body = rust_function_body(shell_source, "fn queue_turn_from_composer(");
    let queue_fragment_body =
        rust_function_body(shell_source, "fn queue_accepted_composer_fragment");
    let submission_target_body =
        rust_function_body(shell_source, "fn current_conversation_submission_target");
    let start_new_thread_body = rust_function_body(
        shell_source,
        "fn start_new_thread(&mut self, _: &gpui::ClickEvent",
    );
    let diagnostic_start_turn_body =
        rust_function_body(shell_source, "fn handle_start_turn_tool_result");
    let diagnostic_ui_state_body = rust_function_body(shell_source, "fn ui_state_snapshot(");
    let diagnostic_backend_unavailable_body =
        rust_function_body(shell_source, "fn backend_unavailable_ui_state");
    let diagnostic_list_threads_body =
        rust_function_body(shell_source, "fn handle_list_workspace_threads_tool_result");
    let title_generation_body =
        rust_function_body(shell_source, "fn begin_thread_title_generation");
    let finish_workspace_open_body =
        rust_function_body(lifecycle_source, "fn finish_workspace_open");
    let inventory_refresh_body = rust_function_body(
        inventory_source,
        "fn begin_member_thread_inventory_refresh_if_needed",
    );

    assert_order(
        queue_body,
        "current_conversation_submission_block()",
        "sync_composer_draft_from_input",
    );
    assert_order(
        queue_fragment_body,
        "backend_client_connector_for_execution_target(&workspace)",
        "ready.surface.begin_turn",
    );
    assert!(queue_fragment_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(queue_fragment_body.contains("unavailable.surface.begin_turn"));
    assert!(!queue_fragment_body.contains("ShellState::BackendUnavailable(_) => return false"));
    assert!(submission_target_body.contains("selected_thread_registered_execution_target"));
    assert_order(
        queue_fragment_body,
        "backend_client_connector_for_execution_target(&workspace)",
        "clear_composer_draft",
    );
    assert_order(
        start_new_thread_body,
        "current_new_thread_block()",
        "clear_active_thread",
    );
    assert_order(
        diagnostic_start_turn_body,
        "current_conversation_submission_block()",
        "input.set_text",
    );
    assert!(diagnostic_ui_state_body.contains("backend_unavailable_ui_state()"));
    assert!(diagnostic_ui_state_body.contains("backend_unavailable,"));
    assert!(diagnostic_backend_unavailable_body.contains("diagnostic_label()"));
    assert!(diagnostic_backend_unavailable_body.contains("runtime_target_diagnostic"));
    assert!(diagnostic_list_threads_body.contains("\"status\": block.kind"));
    assert!(
        title_generation_body
            .contains("backend_client_connector_for_execution_target(&execution_target)")
    );
    assert!(!title_generation_body.contains("backend_client_connector()"));
    assert_order(
        finish_workspace_open_body,
        "record_backend_unavailable",
        "ShellState::BackendUnavailable",
    );
    assert_order(
        finish_workspace_open_body,
        "ShellState::BackendUnavailable",
        "ShellState::Blocked",
    );
    assert!(finish_workspace_open_body.contains("finish_loaded_for_target"));
    assert!(finish_workspace_open_body.contains("opened.execution_target.clone()"));
    assert!(inventory_refresh_body.contains("backend_client_connectors()"));
    assert!(inventory_refresh_body.contains("if connectors.is_empty()"));
    assert!(inventory_refresh_body.contains("spawn_member_thread_inventory_worker"));
    assert!(!inventory_refresh_body.contains("begin_open_target"));
}

#[test]
fn backend_unavailable_target_gates_are_target_scoped() {
    let shell_source = include_str!("../src/shell.rs");
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let graph_thread_start_source = include_str!("../src/shell/graph_thread_start.rs");
    let checklist_thread_menu_source = include_str!("../src/shell/checklist_thread_menu.rs");
    let status_operation_source = include_str!("../src/shell/status_operation.rs");
    let status_operation_state_source = include_str!("../src/shell/status_operation_state.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");

    let backend_required_target_block_body =
        rust_function_body(shell_source, "fn backend_required_target_block");
    let backend_connector_body = rust_function_body(
        shell_source,
        "pub(super) fn backend_client_connector_for_execution_target",
    );
    let backend_connectors_body =
        rust_function_body(shell_source, "pub(super) fn backend_client_connectors");
    let backend_current_connector_body =
        rust_function_body(shell_source, "pub(super) fn backend_client_connector");
    let composer_image_runtime_body =
        rust_function_body(shell_source, "fn composer_image_delivery_runtime_mode");
    let selector_activation_body =
        rust_function_body(shell_source, "fn activate_thread_selector_target");
    let graph_thread_ref_body = rust_function_body(shell_source, "fn select_graph_thread_ref");
    let new_thread_controls_body = rust_function_body(
        shell_source,
        "pub(crate) fn new_thread_controls_disabled_message",
    );
    let thread_selector_controls_body = rust_function_body(
        shell_source,
        "pub(crate) fn thread_selector_controls_disabled_message",
    );
    let backend_controls_body = rust_function_body(
        shell_source,
        "pub(crate) fn backend_controls_disabled_message",
    );
    let queue_fragment_body =
        rust_function_body(shell_source, "fn queue_accepted_composer_fragment");
    let queue_steering_body =
        rust_function_body(shell_source, "fn queue_active_turn_steering_from_composer");
    let queue_steering_fallback_body =
        rust_function_body(shell_source, "fn queue_steering_fragments_for_next_turn");
    let context_compaction_queue_body = rust_function_body(
        shell_source,
        "fn queue_context_compaction_turn_from_composer",
    );
    let older_history_page_body =
        rust_function_body(shell_source, "fn begin_older_thread_history_page_if_needed");
    let lifecycle_continue_body =
        rust_function_body(shell_source, "fn begin_lifecycle_phase_continue");
    let status_model_config_body = rust_function_body(
        status_operation_source,
        "fn status_model_list_config_cwd_for_connector",
    );
    let status_model_target_body = rust_function_body(
        status_operation_source,
        "fn status_model_list_target_for_connector",
    );
    let status_model_load_body = rust_function_body(
        status_operation_source,
        "fn begin_status_model_list_load_if_needed",
    );
    let status_operation_event_body =
        rust_function_body(status_operation_source, "fn apply_status_operation_event");
    let status_backend_available_body = rust_function_body(
        status_operation_source,
        "pub(crate) fn status_line_backend_operation_available",
    );
    let activation_finish_body = rust_function_body(
        lifecycle_source,
        "pub(super) fn finish_thread_activation_worker",
    );
    let render_workspace_surface_body =
        rust_function_body(render_source, "fn render_workspace_surface");
    let graph_thread_start_body = rust_function_body(
        graph_thread_start_source,
        "fn start_thread_from_semantic_node",
    );
    let prepare_semantic_thread_start_body = rust_function_body(
        graph_thread_start_source,
        "fn prepare_semantic_thread_start",
    );
    let checklist_menu_body = rust_function_body(
        checklist_thread_menu_source,
        "pub(crate) fn start_checklist_item_thread_from_menu",
    );

    assert_order(
        backend_required_target_block_body,
        "known_backend_unavailable_block_for_target(target)",
        "backend_client_connector_for_execution_target(target)",
    );
    assert_order(
        backend_connector_body,
        "BackendAvailabilityRecord::unavailable_reason",
        "self.backend_servers",
    );
    assert!(backend_connectors_body.contains("BackendAvailabilityRecord::unavailable_reason"));
    assert!(backend_current_connector_body.contains(
        "ShellState::BackendUnavailable(_) => self.current_conversation_submission_target().ok()"
    ));
    assert!(
        backend_current_connector_body
            .contains("ShellState::Ready(_) => self.current_new_thread_target().ok()")
    );
    assert!(composer_image_runtime_body.contains("current_conversation_submission_target()"));
    assert!(!composer_image_runtime_body.contains("let ShellState::Ready"));
    assert!(selector_activation_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert_order(
        selector_activation_body,
        "known_backend_unavailable_block_for_target(&execution_target)",
        "backend_client_connector_for_execution_target(&execution_target)",
    );
    assert!(graph_thread_ref_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(graph_thread_ref_body.contains("thread_ref(&thread_ref_id)"));
    assert_order(
        graph_thread_ref_body,
        "known_backend_unavailable_block_for_target(&execution_target)",
        "backend_client_connector_for_execution_target(&execution_target)",
    );
    assert!(shell_source.contains("self.thread_selector_controls_disabled_message()"));
    assert!(new_thread_controls_body.contains("ShellState::Blocked(blocked)"));
    assert!(thread_selector_controls_body.contains("ShellState::BackendUnavailable(_)"));
    assert!(backend_controls_body.contains("current_conversation_submission_block()"));
    assert!(!backend_controls_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(queue_fragment_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(queue_fragment_body.contains("unavailable.surface.begin_turn"));
    assert!(queue_steering_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(!queue_steering_body.contains("| ShellState::BackendUnavailable(_)"));
    assert!(queue_steering_fallback_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(queue_steering_fallback_body.contains("registered_thread_execution_target"));
    assert!(context_compaction_queue_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(older_history_page_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(older_history_page_body.contains("connector.launch_spec().runtime_mode().clone()"));
    assert!(!older_history_page_body.contains("| ShellState::BackendUnavailable(_)"));
    assert!(status_operation_event_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(status_operation_event_body.contains("selected_thread_registered_execution_target"));
    assert!(status_operation_event_body.contains("surface.apply_stream_event"));
    assert!(
        status_model_load_body.contains("status_model_list_config_cwd_for_connector(&connector)")
    );
    assert!(status_model_load_body.contains("should_load_for(&target)"));
    assert!(status_model_load_body.contains("begin_loading_for(target)"));
    assert!(status_model_config_body.contains("status_model_list_target_for_connector(connector)"));
    assert!(status_model_target_body.contains("current_conversation_submission_target()"));
    assert!(
        status_model_target_body
            .contains("target.runtime_mode() == connector.launch_spec().runtime_mode()")
    );
    assert!(
        status_model_target_body
            .contains("target.canonical_path() == connector.launch_spec().cwd()")
    );
    assert!(status_model_target_body.contains("WorkspaceId::from_parts"));
    assert!(status_model_target_body.contains("connector.launch_spec().cwd()"));
    assert!(!status_operation_source.contains("fn pending_new_thread_config_cwd"));
    assert!(status_operation_source.contains("self.status_model_cache.target()"));
    assert!(status_operation_source.contains("let Some(cache_target)"));
    assert!(status_operation_state_source.contains("finish_loaded_for_target"));
    assert!(
        status_operation_state_source
            .contains("Beryl discarded a model list loaded without a runtime target.")
    );
    assert!(status_backend_available_body.contains("backend_client_connector().is_some()"));
    assert!(lifecycle_continue_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(lifecycle_continue_body.contains("registered_thread_execution_target"));
    assert!(lifecycle_continue_body.contains("unavailable.surface.queue_pending_turn_fragment"));
    assert!(!lifecycle_continue_body.contains("| ShellState::BackendUnavailable(_)"));
    assert!(
        activation_finish_body
            .contains("ShellState::BackendUnavailable(_) => Some(execution_target.clone())")
    );
    assert_order(
        render_workspace_surface_body,
        "new_thread_controls_disabled_message()",
        "render_thread_strip(",
    );
    assert_order(
        render_workspace_surface_body,
        "thread_selector_controls_disabled_message()",
        "render_thread_strip(",
    );
    assert!(graph_thread_start_body.contains("backend_required_target_block(&execution_target)"));
    assert!(
        prepare_semantic_thread_start_body.contains("ShellState::BackendUnavailable(unavailable)")
    );
    assert!(
        prepare_semantic_thread_start_body
            .contains("&unavailable.loaded_workspace.workspace_state")
    );
    assert!(prepare_semantic_thread_start_body.contains("&unavailable.execution_target"));
    assert!(checklist_menu_body.contains("new_thread_controls_disabled_message()"));
    assert!(!checklist_menu_body.contains("backend_controls_disabled_message()"));
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

fn assert_order(source: &str, before: &str, after: &str) {
    let before_index = source
        .find(before)
        .unwrap_or_else(|| panic!("missing {before:?}"));
    let after_index = source
        .find(after)
        .unwrap_or_else(|| panic!("missing {after:?}"));
    assert!(
        before_index < after_index,
        "expected {before:?} to appear before {after:?}"
    );
}
