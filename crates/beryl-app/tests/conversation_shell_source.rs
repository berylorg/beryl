#[test]
fn workspace_shell_rendering_uses_initialized_controls_and_shared_composer_frame() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let ready_shell_body = rust_function_body(render_source, "pub(super) fn render_ready_shell");
    let workspace_surface_body = rust_function_body(render_source, "fn render_workspace_surface");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let split_surface_body = rust_function_body(render_source, "fn render_split_surface");
    let measure_composer_body = rust_function_body(render_source, "fn measure_composer_input");
    let uncached_measure_composer_body =
        rust_function_body(render_source, "fn measure_uncached_composer_input");
    let composer_body = rust_function_body(render_source, "fn render_composer(");
    let composer_input_area_body =
        rust_function_body(render_source, "fn render_composer_input_area");
    let loaded_composer_body =
        rust_function_body(render_source, "fn render_loaded_workspace_composer");

    assert!(ready_shell_body.contains("render_workspace_surface"));
    assert!(workspace_surface_body.contains("render_toolbar("));
    assert!(workspace_surface_body.contains("render_thread_strip("));
    assert!(workspace_surface_body.contains("render_split_surface("));
    assert_eq!(
        workspace_surface_body
            .matches("measure_composer_input(")
            .count(),
        1
    );
    assert!(toolbar_body.contains("activity_mode_button"));
    assert!(toolbar_body.contains("\"toggle-graph-overlay\""));
    assert!(!toolbar_body.contains("\"toggle-checklist-sidebar\""));
    assert!(thread_strip_body.contains("\"thread-strip-new-thread\""));
    assert!(thread_strip_body.contains("thread_strip_breadcrumb_trail("));
    assert!(thread_strip_body.contains("render_thread_strip_breadcrumbs("));
    assert!(split_surface_body.contains("render_composer("));
    assert!(!split_surface_body.contains("measure_composer_input("));
    assert!(measure_composer_body.contains("ComposerInputMeasurementKey::new"));
    assert!(measure_composer_body.contains("cached_composer_input_measurement"));
    assert!(measure_composer_body.contains("let measurement_started = Instant::now();"));
    assert!(measure_composer_body.contains("record_composer_measurement_cost"));
    assert!(measure_composer_body.contains("composer_input_revision()"));
    assert!(measure_composer_body.contains("composer_image_atom_revision()"));
    assert!(measure_composer_body.contains("window.scale_factor()"));
    assert!(measure_composer_body.contains("shell.style().revision()"));
    assert!(measure_composer_body.contains("surface.transcript_edit_mode().is_some()"));
    assert!(uncached_measure_composer_body.contains("measure_geometry"));
    assert!(uncached_measure_composer_body.contains("composer_input_measurement"));
    assert!(
        uncached_measure_composer_body
            .contains("input_render_height >= initial_measurement.text_content_height")
    );
    assert!(composer_body.contains("render_composer_input_area"));
    assert!(!composer_body.contains("wrapped_visual_line_count_for_width"));
    assert!(!composer_body.contains("reveal_composer_cursor"));
    assert!(!composer_input_area_body.contains("overflow_y_scroll"));
    assert!(loaded_composer_body.contains("render_composer_input_area"));
    assert!(loaded_composer_body.contains("measure_geometry"));

    for body in [
        measure_composer_body,
        uncached_measure_composer_body,
        composer_body,
        composer_input_area_body,
        loaded_composer_body,
    ] {
        assert!(!body.contains("active_theme.lock"));
        assert!(!body.contains("ThemeResolver"));
        assert!(!body.contains("resolve_style("));
        assert!(!body.contains("resolve_property("));
        assert!(!body.contains("from_active_theme"));
    }
}

#[test]
fn activity_mode_uses_labeled_cycle_button_with_regular_button_theme() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let common_source = include_str!("../src/shell/render/common.rs");
    let activity_mode_body = rust_function_body(render_source, "fn activity_mode_button");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let labeled_cycle_button_body = rust_function_body(
        common_source,
        "pub(super) fn secondary_labeled_cycle_button_with_active_state",
    );
    let fixed_label_button_body =
        rust_function_body(common_source, "pub(super) fn secondary_fixed_label_button");
    let fixed_label_slot_body = rust_function_body(common_source, "fn fixed_label_slot");
    let themed_button_base_body = rust_function_body(common_source, "fn themed_button_base");
    let themed_button_container_body =
        rust_function_body(common_source, "fn themed_button_container");

    assert!(activity_mode_body.contains("secondary_labeled_cycle_button_with_active_state"));
    assert!(activity_mode_body.contains("\"Activity\""));
    assert!(activity_mode_body.contains("WorkspaceActivityPanelMode::cycle_value_labels()"));
    assert!(toolbar_body.contains("surface.tool_activity_panel_mode().value_label()"));
    assert!(toolbar_body.contains("secondary_fixed_label_button"));
    assert!(toolbar_body.contains("GRAPH_TOGGLE_LABELS"));
    assert!(
        render_source
            .contains("const GRAPH_TOGGLE_LABELS: [&str; 2] = [\"Graph\", \"Hide Graph\"];")
    );
    assert!(!render_source.contains("CHECKLIST_TOGGLE_LABELS"));
    assert_eq!(toolbar_body.matches("toolbar_toggle_label").count(), 1);
    assert!(!toolbar_body.contains("\"Graph\""));
    assert!(!toolbar_body.contains("\"Hide Graph\""));
    assert!(labeled_cycle_button_body.contains("shell.secondary_button_theme()"));
    assert!(labeled_cycle_button_body.contains("visual_state.theme_state(theme)"));
    assert!(labeled_cycle_button_body.contains("possible_value_labels"));
    assert!(
        labeled_cycle_button_body.contains("fixed_label_slot(value_label, possible_value_labels)")
    );
    assert!(fixed_label_button_body.contains("themed_fixed_label_button"));
    assert!(labeled_cycle_button_body.contains(".bg(button_state.border)"));
    assert!(labeled_cycle_button_body.contains(".group_hover("));
    assert!(labeled_cycle_button_body.contains("theme.hover.border"));
    assert!(labeled_cycle_button_body.contains(".group_active("));
    assert!(labeled_cycle_button_body.contains("theme.active.border"));
    assert!(!labeled_cycle_button_body.contains("shell.separator_color()"));
    assert!(labeled_cycle_button_body.contains(".on_click("));
    assert!(labeled_cycle_button_body.contains(".w(px(1.0))"));
    assert!(fixed_label_slot_body.contains(".opacity(0.0)"));
    assert!(fixed_label_slot_body.contains("for possible_label in possible_labels"));
    assert!(fixed_label_slot_body.contains(".absolute()"));
    assert!(fixed_label_slot_body.contains(".inset_0()"));
    assert!(fixed_label_slot_body.contains("layout::BUTTON_HORIZONTAL_PADDING"));
    assert!(themed_button_base_body.contains("themed_button_container"));
    assert!(themed_button_container_body.contains(".flex_none()"));
    assert!(themed_button_container_body.contains(".hover("));
    assert!(themed_button_container_body.contains(".active("));
    assert!(!themed_button_container_body.contains("theme.hover.foreground"));
    assert!(!themed_button_container_body.contains("theme.active.foreground"));
    assert!(themed_button_container_body.contains(".font_weight(theme.font_weight)"));
}

#[test]
fn startup_toolbar_leading_label_stays_single_line_in_shared_strip_height() {
    let common_source = include_str!("../src/shell/render/common.rs");
    let startup_frame_body = rust_function_body(common_source, "pub(super) fn startup_shell_frame");
    let toolbar_tail = startup_frame_body
        .split(".child(toolbar_strip(")
        .nth(1)
        .expect("startup frame should render the toolbar strip");
    let toolbar_leading = &toolbar_tail[..toolbar_tail
        .find("actions,")
        .expect("startup toolbar should pass trailing actions")];

    assert!(toolbar_leading.contains(".items_center()"));
    assert!(toolbar_leading.contains(".min_w(px(0.0))"));
    assert!(toolbar_leading.contains(".whitespace_nowrap()"));
    assert!(toolbar_leading.contains(".truncate()"));
    assert!(!toolbar_leading.contains(".flex_col()"));
    assert!(!toolbar_leading.contains(".text_lg()"));
}

#[test]
fn custom_button_renderers_use_themed_label_font_weight() {
    let code_panel_source = include_str!("../src/shell/render/code_panel.rs");
    let code_panel_controls_source =
        include_str!("../src/shell/render/transcript/code_panel_controls.rs");
    let startup_source = include_str!("../src/shell/render/startup.rs");
    let graph_rows_source = include_str!("../src/shell/render/graph_overlay/rows.rs");
    let code_panel_button_body =
        rust_function_body(code_panel_source, "fn code_panel_header_button");
    let code_panel_header_body =
        rust_function_body(code_panel_controls_source, "pub(super) fn header");
    let render_picker_body = rust_function_body(startup_source, "fn render_picker");
    let distro_chip_body = rust_function_body(startup_source, "fn distro_chip");
    let invalid_thread_ref_actions_body =
        rust_function_body(graph_rows_source, "fn render_invalid_thread_ref_actions");
    let rebind_button_tail = invalid_thread_ref_actions_body
        .split("\"graph-thread-ref-rebind-row\"")
        .nth(1)
        .expect("missing graph thread-ref rebind button");
    let rebind_button_body = &rebind_button_tail[..rebind_button_tail
        .find(".on_mouse_down")
        .expect("missing rebind action")];

    assert!(code_panel_button_body.contains(".font_weight(button_font_weight)"));
    assert!(!code_panel_button_body.contains("FontWeight(500.0)"));
    assert!(code_panel_header_body.contains("button_font_weight: self.state.button_font_weight"));
    assert!(render_picker_body.contains("distro_chip("));
    assert!(render_picker_body.contains("shell,"));
    assert!(distro_chip_body.contains(".font_weight(secondary.font_weight)"));
    assert!(distro_chip_body.contains(".flex_none()"));
    assert!(!distro_chip_body.contains("FontWeight(500.0)"));
    assert!(rebind_button_body.contains("layout::BUTTON_OUTER_HEIGHT"));
    assert!(rebind_button_body.contains(".flex_none()"));
    assert!(rebind_button_body.contains("layout::BUTTON_HORIZONTAL_PADDING"));
    assert!(rebind_button_body.contains("layout::BUTTON_VERTICAL_PADDING"));
    assert!(rebind_button_body.contains("layout::BUTTON_LABEL_FONT_SIZE"));
    assert!(rebind_button_body.contains("layout::BUTTON_LABEL_LINE_HEIGHT"));
    assert!(rebind_button_body.contains(".font_weight(button_theme.font_weight)"));
    assert!(!rebind_button_body.contains(".h(px(24.0))"));
    assert!(!rebind_button_body.contains(".px_2()"));
    assert!(!rebind_button_body.contains(".text_xs()"));
}

#[test]
fn transient_button_feedback_does_not_change_label_color_or_geometry() {
    let common_source = include_str!("../src/shell/render/common.rs");
    let workspace_picker_source = include_str!("../src/shell/render/workspace_picker.rs");
    let workspace_row_menu_source =
        include_str!("../src/shell/render/workspace_picker_row_menu.rs");
    let graph_rows_source = include_str!("../src/shell/render/graph_overlay/rows.rs");
    let themed_button_container_body =
        rust_function_body(common_source, "fn themed_button_container");
    let member_action_trigger_body = rust_function_body(
        workspace_picker_source,
        "fn render_member_row_action_trigger",
    );
    let attach_member_row_body =
        rust_function_body(workspace_picker_source, "fn render_attach_member_row");
    let create_workspace_row_body =
        rust_function_body(workspace_picker_source, "fn render_create_workspace_row");
    let workspace_row_action_trigger_body = rust_function_body(
        workspace_row_menu_source,
        "pub(super) fn render_workspace_row_action_trigger",
    );
    let invalid_thread_ref_actions_body =
        rust_function_body(graph_rows_source, "fn render_invalid_thread_ref_actions");

    for body in [
        themed_button_container_body,
        member_action_trigger_body,
        attach_member_row_body,
        create_workspace_row_body,
        workspace_row_action_trigger_body,
        invalid_thread_ref_actions_body,
    ] {
        assert!(!body.contains("hover.foreground"));
        assert!(!body.contains("active.foreground"));
        assert!(!body.contains("hover_foreground"));
        assert!(!body.contains("active_foreground"));
        assert!(!body.contains(".shadow_"));
        assert!(!body.contains(".scale("));
        assert!(!body.contains(".translate("));
    }
}

#[test]
fn conversation_input_changes_notify_shell_for_composer_remeasurement() {
    let shell_source = include_str!("../src/shell.rs");
    let handler_body = rust_function_body(shell_source, "fn handle_conversation_input_event");
    let note_measurement_body =
        rust_function_body(shell_source, "fn note_composer_input_measurement_changed");

    assert!(handler_body.contains("TextInputEvent::Changed(_)"));

    let changed_arm_tail = handler_body
        .split("TextInputEvent::Changed(_)")
        .nth(1)
        .expect("missing changed event arm");
    let changed_arm_end = changed_arm_tail
        .find("TextInputEvent::InlineAtomClicked")
        .or_else(|| changed_arm_tail.find("_ =>"))
        .unwrap_or(changed_arm_tail.len());
    let changed_arm_body = &changed_arm_tail[..changed_arm_end];

    assert!(changed_arm_body.contains("cx.notify()"));
    assert!(changed_arm_body.contains("note_composer_input_measurement_changed"));
    assert!(handler_body.contains("TextInputEvent::SelectionChanged(_)"));
    assert!(note_measurement_body.contains("composer_input_revision.wrapping_add(1)"));
    assert!(note_measurement_body.contains("composer_image_atom_revision.wrapping_add(1)"));
    assert!(handler_body.contains("TextInputEvent::InlineAtomClicked"));
    assert!(handler_body.contains("open_composer_image_marker_menu"));
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
    assert!(thread_strip_body.contains("thread_strip_breadcrumb_trail"));
    assert!(composer_body.contains("set_enabled(enabled"));
    assert!(composer_body.contains("backend_controls_disabled"));
}

#[test]
fn thread_strip_branch_breadcrumbs_render_as_bounded_exact_parent_activation() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let render_workspace_surface_body =
        rust_function_body(render_source, "fn render_workspace_surface");
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let breadcrumb_body = rust_function_body(render_source, "fn render_thread_strip_breadcrumbs");
    let parent_breadcrumb_body =
        rust_function_body(render_source, "fn render_thread_strip_parent_breadcrumb");

    assert!(render_workspace_surface_body.contains("&loaded_workspace.threaded_decision_state"));
    assert!(thread_strip_body.contains("pending_thread_activation_label"));
    assert!(thread_strip_body.contains(".is_none()"));
    assert!(thread_strip_body.contains("thread_strip_breadcrumb_trail"));
    assert!(thread_strip_body.contains("TransientBranchParent"));
    assert!(thread_strip_body.contains("foreground_transcript_branch"));
    assert!(thread_strip_body.contains("surface.selected_thread_id()"));
    assert!(breadcrumb_body.contains(".child(\">\")"));
    assert!(breadcrumb_body.contains("render_thread_strip_active_thread_title"));
    assert!(parent_breadcrumb_body.contains("\"thread-strip-branch-parent-breadcrumb\""));
    assert!(parent_breadcrumb_body.contains("ThreadSelectorActivationTarget"));
    assert!(parent_breadcrumb_body.contains("breadcrumb.thread_id().clone()"));
    assert!(parent_breadcrumb_body.contains("label.clone()"));
    assert!(parent_breadcrumb_body.contains("view.activate_thread_selector_target"));
    assert!(parent_breadcrumb_body.contains(".max_w(relative(0.32))"));
    assert!(parent_breadcrumb_body.contains(".min_w(px(0.0))"));
    assert!(parent_breadcrumb_body.contains(".overflow_hidden()"));
    assert!(parent_breadcrumb_body.contains(".truncate()"));
    assert!(parent_breadcrumb_body.contains(".tooltip("));
}

#[test]
fn new_thread_control_state_uses_cached_workspace_target() {
    let shell_source = include_str!("../src/shell.rs");
    let current_new_thread_target_body =
        rust_function_body(shell_source, "fn current_new_thread_target");
    let cached_target_body =
        rust_function_body(shell_source, "fn cached_new_thread_execution_target");
    let cached_implicit_home_body =
        rust_function_body(shell_source, "fn cached_implicit_home_execution_target");

    assert!(!shell_source.contains("resolve_new_thread_execution_target"));
    assert!(current_new_thread_target_body.contains("cached_new_thread_execution_target"));
    assert!(!current_new_thread_target_body.contains("resolve_new_thread_execution_target"));
    assert!(cached_target_body.contains("PrimaryWorkspaceMember::Explicit"));
    assert!(cached_target_body.contains("PrimaryWorkspaceMember::ImplicitHome"));
    assert!(cached_implicit_home_body.contains("ImplicitHomePathResolutionStatus::Resolved"));
    assert!(cached_implicit_home_body.contains("ImplicitHomePathResolutionStatus::Pending"));
    assert!(cached_implicit_home_body.contains("ImplicitHomePathResolutionStatus::Failed"));
    assert!(!cached_implicit_home_body.contains("resolve_runtime_home_directory"));
    assert!(!cached_implicit_home_body.contains("canonicalize_wsl"));
    assert!(!cached_implicit_home_body.contains("wsl.exe"));
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
    assert!(!inventory_refresh_body.contains("thread_selector().is_open()"));
    assert!(!shell_source.contains("MemberThreadInventoryEvent::SelectorFreshnessRequested"));
}

#[test]
fn hidden_developer_instructions_route_only_user_facing_turn_starts() {
    let shell_source = include_str!("../src/shell.rs");
    let edit_commit_source = include_str!("../src/shell/transcript_edit_commit.rs");
    let title_worker_source = include_str!("../src/shell/thread_title/worker.rs");
    let status_operation_source = include_str!("../src/shell/status_operation.rs");
    let inventory_source = include_str!("../src/shell/member_thread_inventory.rs");
    let thread_activation_source = include_str!("../src/shell/thread_activation.rs");

    let direct_turn_body = rust_function_body(shell_source, "fn queue_accepted_composer_fragment");
    let pending_queue_body =
        rust_function_body(shell_source, "fn begin_pending_turn_input_queue_for_thread");
    let replacement_turn_body = rust_function_body(
        edit_commit_source,
        "fn begin_transcript_edit_replacement_turn",
    );
    let compaction_queue_body = rust_function_body(
        shell_source,
        "fn queue_context_compaction_turn_from_composer",
    );
    let lifecycle_continue_body =
        rust_function_body(shell_source, "fn begin_lifecycle_phase_continue");
    let steering_body = rust_function_body(shell_source, "fn begin_turn_steering");

    assert_order(
        direct_turn_body,
        "turn_options_with_current_developer_instructions(",
        "spawn_turn_worker",
    );
    assert_order(
        pending_queue_body,
        "turn_options_with_current_developer_instructions(",
        "spawn_turn_worker",
    );
    assert_order(
        replacement_turn_body,
        "turn_options_with_current_developer_instructions_defaults(",
        "spawn_turn_worker",
    );

    assert!(!compaction_queue_body.contains("turn_options_with_current_developer_instructions"));
    assert!(!lifecycle_continue_body.contains("turn_options_with_current_developer_instructions"));
    assert!(!steering_body.contains("turn_options_with_current_developer_instructions"));
    assert!(!title_worker_source.contains("current_hidden_developer_instructions"));
    assert!(!status_operation_source.contains("current_hidden_developer_instructions"));
    assert!(!inventory_source.contains("current_hidden_developer_instructions"));
    assert!(!thread_activation_source.contains("current_hidden_developer_instructions"));
}

#[test]
fn threaded_decision_context_is_not_hidden_or_pinned_for_child_threads() {
    let shell_source = include_str!("../src/shell.rs");
    let developer_instructions_source = include_str!("../src/shell/developer_instructions.rs");
    let transcript_panel_snapshot_source =
        include_str!("../src/shell/transcript_panel_snapshot.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let text_blocks_source = include_str!("../src/shell/render/transcript/text_blocks.rs");
    let current_hidden_body = rust_function_body(
        developer_instructions_source,
        "fn current_hidden_developer_instructions",
    );
    let snapshot_body = rust_function_body(
        transcript_panel_snapshot_source,
        "fn transcript_panel_snapshot",
    );
    let transcript_render_body = rust_function_body(transcript_source, "fn render(&mut self");

    assert!(!shell_source.contains("fn threaded_decision_child_context"));
    assert!(!current_hidden_body.contains("decision_context"));
    assert!(!current_hidden_body.contains("compose_hidden_developer_instructions_with_contexts"));
    assert!(!snapshot_body.contains("decision_context"));
    assert!(!transcript_render_body.contains("snapshot.decision_context"));
    assert!(!transcript_render_body.contains("decision_context_state"));
    assert!(!text_blocks_source.contains("pub(super) fn decision_context_state"));
}

#[test]
fn user_initiated_thread_title_updates_bypass_automatic_title_eligibility_guards() {
    let shell_source = include_str!("../src/shell.rs");
    let branch_menu_source = include_str!("../src/shell/transcript_branch_menu.rs");
    let automatic_start_body = rust_function_body(shell_source, "fn begin_thread_title_generation");
    let manual_start_body = rust_function_body(shell_source, "fn begin_thread_title_update");
    let manual_dispatch_body = rust_function_body(
        branch_menu_source,
        "fn dispatch_transcript_thread_title_update_request",
    );

    assert!(automatic_start_body.contains("thread_title_generation_can_start"));
    assert!(manual_start_body.contains("thread_title_task_active_for_thread"));
    assert!(manual_start_body.contains("thread_title_worker_capacity_available"));
    assert!(!manual_start_body.contains("thread_title_generation_can_start"));
    assert!(!manual_start_body.contains("thread_automatic_title_generation_eligible"));
    assert!(!manual_start_body.contains("thread_ignores_backend_name_for_automatic_title"));
    assert!(!manual_start_body.contains("normalized_thread_name"));

    assert!(manual_dispatch_body.contains("ThreadTitleCandidate::for_selected_user_turn"));
    assert!(manual_dispatch_body.contains("thread_has_manual_gui_title"));
    assert!(!manual_dispatch_body.contains("thread_title_generation_can_start"));
    assert!(!manual_dispatch_body.contains("thread_automatic_title_generation_eligible"));
    assert!(!manual_dispatch_body.contains("thread_ignores_backend_name_for_automatic_title"));
    assert!(!manual_dispatch_body.contains("normalized_thread_name"));
    assert_order(
        manual_dispatch_body,
        "thread_title_task_active_for_thread",
        "ThreadTitleCandidate::for_selected_user_turn",
    );
}

#[test]
fn branch_retitle_scheduling_waits_for_first_real_turn_and_respects_manual_titles() {
    let shell_source = include_str!("../src/shell.rs");
    let helper_source = include_str!("../src/shell/thread_helpers.rs");
    let mode_body = rust_function_body(
        shell_source,
        "fn thread_title_mode_for_user_submission_state(",
    );
    let pending_history_body = rust_function_body(
        shell_source,
        "fn begin_pending_branch_retitle_from_known_history(",
    );
    let earliest_input_body =
        rust_function_body(shell_source, "fn earliest_known_user_input_for_thread(");
    let begin_retitle_body = rust_function_body(
        shell_source,
        "fn begin_branch_thread_retitle_from_candidate(",
    );

    assert!(helper_source.contains("branch_bootstrap_turn_id()"));
    assert_order(
        helper_source,
        "branch_bootstrap_turn_id()",
        ".saturating_add(1)",
    );
    assert!(helper_source.contains("find_map(|turn| turn.first_user_input_fragment_text())"));
    assert!(pending_history_body.contains("first_real_branch_user_input_fragment_text"));
    assert!(earliest_input_body.contains("first_real_branch_user_input_fragment_text"));
    assert!(mode_body.contains("thread_branch_title_retitle_pending"));
    assert!(mode_body.contains("TurnThreadTitleMode::BranchRetitleAfterFirstUserTurn"));
    assert_order(
        mode_body,
        "thread_branch_title_retitle_pending",
        "TurnThreadTitleMode::BranchRetitleAfterFirstUserTurn",
    );
    assert!(begin_retitle_body.contains("thread_has_manual_gui_title"));
    assert_order(
        begin_retitle_body,
        "thread_has_manual_gui_title",
        "mark_branch_thread_title_retitle_finished",
    );
    assert_order(
        begin_retitle_body,
        "begin_thread_title_update",
        "mark_branch_thread_title_retitle_started",
    );
    assert!(!begin_retitle_body.contains("thread_title_generation_can_start"));
    assert!(!begin_retitle_body.contains("thread_automatic_title_generation_eligible"));
}

#[test]
fn backend_unavailable_target_gates_are_target_scoped() {
    let shell_source = include_str!("../src/shell.rs");
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let graph_thread_start_source = include_str!("../src/shell/graph_thread_start.rs");
    let graph_link_menu_render_source = include_str!("../src/shell/render/graph_link_menu.rs");
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
    let graph_node_action_menu_body =
        rust_function_body(graph_link_menu_render_source, "fn render_node_action_menu");

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
    assert!(graph_node_action_menu_body.contains("new_thread_controls_disabled"));
    assert!(graph_node_action_menu_body.contains("Start New Codex Thread"));
}

#[test]
fn context_compaction_uses_configured_completion_timeout_only_for_stream_wait() {
    let shell_source = include_str!("../src/shell.rs");
    let status_operation_source = include_str!("../src/shell/status_operation.rs");

    let manual_compaction_body = rust_function_body(
        status_operation_source,
        "pub(crate) fn compact_selected_thread_from_status_popup",
    );
    let lifecycle_continue_body =
        rust_function_body(shell_source, "fn begin_lifecycle_phase_continue");
    let worker_body =
        rust_function_body(status_operation_source, "fn run_context_compaction_worker");

    assert!(status_operation_source.contains("request_timeout: Duration"));
    assert!(status_operation_source.contains("stream_timeout: Duration"));
    assert!(manual_compaction_body.contains("self.bootstrap.probe_timeout()"));
    assert!(manual_compaction_body.contains("self.current_context_compaction_timeout()"));
    assert!(lifecycle_continue_body.contains("self.bootstrap.probe_timeout()"));
    assert!(lifecycle_continue_body.contains("self.current_context_compaction_timeout()"));
    assert!(worker_body.contains("connector.connect_client(request_timeout)"));
    assert!(worker_body.contains("session.resume_thread_metadata(&thread_id, request_timeout)"));
    assert!(worker_body.contains("session.compact_thread(&thread_id, request_timeout)"));
    assert!(worker_body.contains("let event_timeout = remaining.min"));
    assert!(!status_operation_source.contains("CONTEXT_COMPACTION_MIN_STREAM_TIMEOUT"));
}

#[test]
fn active_theme_refresh_notifies_open_surfaces_without_reconstructing_workspace_state() {
    let shell_source = include_str!("../src/shell.rs");
    let dynamic_theme_source = include_str!("../src/shell/dynamic_theme.rs");
    let apply_body = rust_function_body(shell_source, "fn apply_settings_window_changes");
    let settings_event_body = rust_function_body(shell_source, "fn handle_settings_window_event");
    let publish_body = rust_function_body(shell_source, "fn publish_active_theme_projection");
    let refresh_body = rust_function_body(shell_source, "fn refresh_active_theme_surfaces");
    let transcript_preview_body = rust_function_body(
        shell_source,
        "pub(super) fn preview_transcript_theme_candidate",
    );
    let transcript_stop_preview_body = rust_function_body(
        shell_source,
        "pub(super) fn stop_transcript_theme_candidate_preview",
    );
    let install_finish_body =
        rust_function_body(shell_source, "fn finish_theme_candidate_install_update");
    let restore_candidate_body = rust_function_body(
        shell_source,
        "fn restore_active_theme_candidate_preview_if_needed",
    );
    let reconcile_candidate_body =
        rust_function_body(shell_source, "fn reconcile_theme_candidate_preview_scope");
    let dynamic_preview_body =
        rust_function_body(dynamic_theme_source, "fn handle_dynamic_theme_preview");
    let dynamic_stop_preview_body =
        rust_function_body(dynamic_theme_source, "fn stop_dynamic_theme_preview");
    let dynamic_repository_snapshot_body = rust_function_body(
        dynamic_theme_source,
        "fn apply_dynamic_theme_repository_snapshot",
    );

    assert!(apply_body.contains("self.refresh_active_theme_surfaces(cx)"));
    assert!(publish_body.contains("self.active_theme.lock()"));
    assert!(publish_body.contains("ShellRenderThemeCache::new(projection)"));
    assert!(!publish_body.contains("cx.notify"));
    assert_eq!(
        settings_event_body
            .matches("self.publish_settings_active_theme_projection()")
            .count(),
        2
    );
    for body in [
        transcript_preview_body,
        transcript_stop_preview_body,
        install_finish_body,
        restore_candidate_body,
        reconcile_candidate_body,
        dynamic_preview_body,
        dynamic_stop_preview_body,
        dynamic_repository_snapshot_body,
    ] {
        assert!(body.contains("publish_active_theme_projection"));
        assert!(!body.contains("active_theme.lock"));
    }
    assert!(dynamic_preview_body.contains("self.refresh_theme_candidate_surfaces(cx)"));
    assert!(dynamic_stop_preview_body.contains("self.refresh_theme_candidate_surfaces(cx)"));
    assert!(dynamic_repository_snapshot_body.contains("self.refresh_theme_candidate_surfaces(cx)"));
    assert!(!dynamic_repository_snapshot_body.contains("cx.notify()"));
    assert!(refresh_body.contains("self.notify_transcript_panel(cx)"));
    assert!(!refresh_body.contains("self.notify_checklist_sidebar_panel(cx)"));
    assert!(refresh_body.contains("cx.refresh_windows()"));
    assert!(refresh_body.contains("cx.notify()"));
    assert!(!refresh_body.contains("LoadedWorkspaceState::new"));
    assert!(!refresh_body.contains("ConversationSurfaceState::new"));
    assert!(!refresh_body.contains("refresh_after_backend_reopen"));
    assert!(!refresh_body.contains("SemanticGraph"));
}

#[test]
fn settings_window_model_sync_does_not_force_option_sync() {
    let shell_source = include_str!("../src/shell.rs");
    let model_sync_body = rust_function_body(shell_source, "fn sync_settings_window_model");
    let options_sync_body = rust_function_body(shell_source, "fn sync_settings_window_options");
    let options_sync_value_body =
        rust_function_body(shell_source, "fn sync_settings_window_options_value");

    assert!(model_sync_body.contains(".update_model("));
    assert!(!model_sync_body.contains("sync_settings_window_options"));
    assert!(!model_sync_body.contains(".update_options("));
    assert!(options_sync_body.contains("window_options_for_sync"));
    assert!(options_sync_body.contains("sync_settings_window_options_value"));
    assert!(options_sync_value_body.contains(".update_options("));
    assert!(options_sync_value_body.contains("options_with_renderer"));
    assert!(options_sync_value_body.contains("record_window_options_synced"));
}

#[test]
fn dynamic_theme_durable_tools_run_repository_io_on_worker() {
    let shell_source = include_str!("../src/shell.rs");
    let dynamic_theme_source = include_str!("../src/shell/dynamic_theme.rs");
    let dynamic_theme_worker_source = include_str!("../src/shell/dynamic_theme_worker.rs");
    let poll_body = rust_function_body(shell_source, "fn poll(");
    let frame_work_body = rust_function_body(shell_source, "fn has_frame_poll_work");
    let begin_body = rust_function_body(
        dynamic_theme_source,
        "fn begin_dynamic_theme_durable_tool_request",
    );
    let validate_body = rust_function_body(
        dynamic_theme_source,
        "fn validate_dynamic_theme_durable_operation",
    );
    let worker_body = rust_function_body(
        dynamic_theme_worker_source,
        "fn run_dynamic_theme_durable_operation",
    );

    assert!(poll_body.contains("self.poll_dynamic_theme_durable_updates(cx)"));
    assert!(frame_work_body.contains("self.dynamic_theme_durable_receiver.is_some()"));
    assert!(begin_body.contains("spawn_dynamic_theme_durable_worker(operation, store)"));
    assert!(!begin_body.contains(".install_theme("));
    assert!(!begin_body.contains(".update_theme("));
    assert!(!begin_body.contains(".save_as_theme("));
    assert!(!begin_body.contains(".activate_theme("));
    assert!(!begin_body.contains(".load_theme_definition("));
    assert!(!begin_body.contains(".load_or_default("));
    assert!(validate_body.contains("BUILT_IN_INSTALLED_THEME_ID"));
    assert!(worker_body.contains(".install_theme("));
    assert!(worker_body.contains(".update_theme("));
    assert!(worker_body.contains(".save_as_theme("));
    assert!(worker_body.contains(".activate_theme("));
    assert!(worker_body.contains(".load_theme_definition("));
    assert!(worker_body.contains(".load_or_default("));
    assert!(worker_body.contains("BUILT_IN_INSTALLED_THEME_ID"));
}

#[test]
fn phase28_shell_splits_final_review_blocks_into_focused_modules() {
    let shell_source = include_str!("../src/shell.rs");
    let render_theme_source = include_str!("../src/shell/render_theme.rs");
    let dynamic_theme_source = include_str!("../src/shell/dynamic_theme.rs");
    let dynamic_settings_source = include_str!("../src/shell/dynamic_settings.rs");
    let dynamic_theme_worker_source = include_str!("../src/shell/dynamic_theme_worker.rs");
    let diagnostics_source = include_str!("../src/shell/diagnostics.rs");

    assert!(shell_source.lines().count() < 15_000);
    for module in [
        "mod render_theme;",
        "mod dynamic_theme;",
        "mod dynamic_theme_worker;",
        "mod dynamic_settings;",
        "mod diagnostics;",
    ] {
        assert!(shell_source.contains(module), "missing {module}");
    }

    for removed in [
        "struct ShellRenderThemeCache",
        "enum DynamicThemeDurableOperation",
        "fn handle_beryl_theme_immediate_tool_result",
        "fn handle_beryl_settings_dynamic_tool_request",
        "fn diagnostic_tool_snapshot",
    ] {
        assert!(
            !shell_source.contains(removed),
            "shell.rs still contains {removed}"
        );
    }

    assert!(render_theme_source.contains("struct ShellRenderThemeCache"));
    assert!(render_theme_source.contains("pub(super) struct ShellRenderStyleSnapshot"));
    assert!(dynamic_theme_source.contains("fn handle_beryl_theme_immediate_tool_result"));
    assert!(dynamic_theme_worker_source.contains("fn run_dynamic_theme_durable_operation"));
    assert!(dynamic_settings_source.contains("fn handle_beryl_settings_dynamic_tool_request"));
    assert!(diagnostics_source.contains("fn diagnostic_tool_snapshot"));
}

#[test]
fn phase29_theme_settings_modules_are_split_into_focused_sources() {
    let render_theme_source = include_str!("../src/shell/render_theme.rs");
    let render_theme_button_source = include_str!("../src/shell/render_theme/button.rs");
    let render_theme_frame_source = include_str!("../src/shell/render_theme/frame.rs");
    let render_theme_role_style_source = include_str!("../src/shell/render_theme/role_style.rs");
    let theme_editor_source = include_str!("../src/shell/settings/theme_editor.rs");
    let theme_editor_draft_source = include_str!("../src/shell/settings/theme_editor/draft.rs");
    let theme_editor_rows_source = include_str!("../src/shell/settings/theme_editor/rows.rs");
    let theme_editor_helpers_source = include_str!("../src/shell/settings/theme_editor/helpers.rs");
    let theme_dynamic_source = include_str!("../src/theme_dynamic_tools.rs");
    let theme_dynamic_parser_source = include_str!("../src/theme_dynamic_tools/parser.rs");
    let theme_dynamic_response_source = include_str!("../src/theme_dynamic_tools/response.rs");
    let theme_dynamic_schema_output_source =
        include_str!("../src/theme_dynamic_tools/schema_output.rs");
    let settings_dynamic_source = include_str!("../src/settings_dynamic_tools.rs");
    let settings_dynamic_parser_source = include_str!("../src/settings_dynamic_tools/parser.rs");
    let settings_dynamic_response_source =
        include_str!("../src/settings_dynamic_tools/response.rs");
    let theme_store_source = include_str!("../src/appearance/theme/repository/store.rs");
    let theme_store_io_source = include_str!("../src/appearance/theme/repository/store/io.rs");
    let theme_store_snapshot_source =
        include_str!("../src/appearance/theme/repository/store/snapshot.rs");

    for (path, source) in [
        ("shell/render_theme.rs", render_theme_source),
        ("shell/render_theme/button.rs", render_theme_button_source),
        ("shell/render_theme/frame.rs", render_theme_frame_source),
        (
            "shell/render_theme/role_style.rs",
            render_theme_role_style_source,
        ),
        ("shell/settings/theme_editor.rs", theme_editor_source),
        (
            "shell/settings/theme_editor/draft.rs",
            theme_editor_draft_source,
        ),
        (
            "shell/settings/theme_editor/rows.rs",
            theme_editor_rows_source,
        ),
        (
            "shell/settings/theme_editor/helpers.rs",
            theme_editor_helpers_source,
        ),
        ("theme_dynamic_tools.rs", theme_dynamic_source),
        ("theme_dynamic_tools/parser.rs", theme_dynamic_parser_source),
        (
            "theme_dynamic_tools/response.rs",
            theme_dynamic_response_source,
        ),
        (
            "theme_dynamic_tools/schema_output.rs",
            theme_dynamic_schema_output_source,
        ),
        ("settings_dynamic_tools.rs", settings_dynamic_source),
        (
            "settings_dynamic_tools/parser.rs",
            settings_dynamic_parser_source,
        ),
        (
            "settings_dynamic_tools/response.rs",
            settings_dynamic_response_source,
        ),
        ("appearance/theme/repository/store.rs", theme_store_source),
        (
            "appearance/theme/repository/store/io.rs",
            theme_store_io_source,
        ),
        (
            "appearance/theme/repository/store/snapshot.rs",
            theme_store_snapshot_source,
        ),
    ] {
        assert!(
            source.lines().count() < 500,
            "{path} should stay below the rough split threshold"
        );
    }

    assert!(!render_theme_source.contains("pub(super) struct ShellRenderFrame<'a>"));
    assert!(!render_theme_source.contains("struct ShellRoleStyle"));
    assert!(!render_theme_source.contains("struct ChromeButtonTheme"));
    assert!(render_theme_frame_source.contains("pub(in crate::shell) struct ShellRenderFrame<'a>"));
    assert!(render_theme_role_style_source.contains("struct ShellRoleStyle"));
    assert!(render_theme_button_source.contains("struct ChromeButtonTheme"));

    assert!(!theme_editor_source.contains("fn candidate_property_source"));
    assert!(!theme_editor_source.contains("fn property_row("));
    assert!(!theme_editor_source.contains("enum PropertySourceChoice"));
    assert!(theme_editor_draft_source.contains("fn candidate_property_source"));
    assert!(theme_editor_rows_source.contains("fn property_row("));
    assert!(theme_editor_helpers_source.contains("enum PropertySourceChoice"));

    assert!(!theme_dynamic_source.contains("struct SaveThemeAsArguments"));
    assert!(!theme_dynamic_source.contains("fn theme_repository_value"));
    assert!(theme_dynamic_parser_source.contains("struct SaveThemeAsArguments"));
    assert!(theme_dynamic_response_source.contains("fn theme_repository_value"));
    assert!(theme_dynamic_schema_output_source.contains("fn theme_schema_value"));

    assert!(!settings_dynamic_source.contains("struct SettingsUpdateArguments"));
    assert!(!settings_dynamic_source.contains("fn gui_settings_snapshot_value"));
    assert!(settings_dynamic_parser_source.contains("struct SettingsUpdateArguments"));
    assert!(settings_dynamic_response_source.contains("fn gui_settings_snapshot_value"));

    assert!(!theme_store_source.contains("fn read_manifest"));
    assert!(!theme_store_source.contains("fn snapshot_from_loaded"));
    assert!(theme_store_io_source.contains("fn read_manifest"));
    assert!(theme_store_snapshot_source.contains("fn snapshot_from_loaded"));
}

#[test]
fn threaded_decision_dynamic_tool_routes_through_live_shell_bridge() {
    let turn_worker_source = include_str!("../src/shell/turn_worker.rs");
    let shell_source = include_str!("../src/shell.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let member_thread_inventory_source = include_str!("../src/shell/member_thread_inventory.rs");
    let archive_source = include_str!("../src/shell/threaded_decision_archive.rs");
    let resolution_source = include_str!("../src/shell/threaded_decision_resolution.rs");
    let resolution_tool_source = include_str!("../src/shell/threaded_decision_resolution/tool.rs");
    let resolution_graph_source =
        include_str!("../src/shell/threaded_decision_resolution/graph_update.rs");
    let routed_tools_body = rust_function_body(
        turn_worker_source,
        "pub(crate) fn handle_beryl_dynamic_tool_call_with_shell_tools",
    );
    let shell_poll_body = rust_function_body(shell_source, "fn poll_shell_dynamic_tool_requests");

    assert!(routed_tools_body.contains("is_beryl_threaded_decision_dynamic_tool"));
    assert!(shell_poll_body.contains("is_beryl_threaded_decision_dynamic_tool"));
    assert!(shell_poll_body.contains("handle_beryl_threaded_decision_dynamic_tool_request"));
    assert!(shell_source.contains("poll_decision_resolution_graph_updates"));
    assert!(shell_source.contains("begin_next_ready_decision_resolution_handoff"));
    assert!(shell_source.contains("note_decision_handoff_turn_started"));
    assert!(resolution_tool_source.contains("mark_pending_resolution"));
    assert!(resolution_source.contains("mark_handoff_started"));
    assert!(resolution_graph_source.contains("mark_checklist_updated"));
    assert!(resolution_graph_source.contains("spawn_threaded_decision_graph_patch_worker"));
    assert!(resolution_graph_source.contains("queue_decision_archive_job"));
    assert!(shell_source.contains("poll_decision_archive_updates"));
    assert!(shell_source.contains("begin_next_ready_decision_archive_cleanup"));
    assert!(shell_source.contains("closed_decision_branch_submission_block"));
    assert!(shell_source.contains("normal_selector_hidden_decision_child_thread_ids"));
    assert!(lifecycle_source.contains("thread_is_read_only_decision_branch"));
    assert!(lifecycle_source.contains("remember_thread_summary"));
    assert!(member_thread_inventory_source.contains("hidden_thread_ids"));
    assert!(
        member_thread_inventory_source.contains("normal_selector_hidden_decision_child_thread_ids")
    );
    assert!(archive_source.contains("mark_archive_pending"));
    assert!(archive_source.contains("archive_thread"));
    assert!(archive_source.contains("ThreadListOptions::page(100)"));
    assert!(archive_source.contains(".archived()"));
    assert!(archive_source.contains("mark_closed"));
    assert!(archive_source.contains("mark_archive_failed"));
    assert!(!archive_source.contains("unarchive_thread"));
}

#[test]
fn thread_inventory_refresh_is_scheduled_from_thread_lifecycle_events() {
    let shell_source = include_str!("../src/shell.rs");
    let transcript_branch_source = concat!(
        include_str!("../src/shell/transcript_branch_worker.rs"),
        include_str!("../src/shell/transcript_branch_worker/handlers.rs"),
    );
    let decision_branch_source = concat!(
        include_str!("../src/shell/threaded_decision_branch.rs"),
        include_str!("../src/shell/threaded_decision_branch/completion.rs"),
    );
    let decision_archive_source = include_str!("../src/shell/threaded_decision_archive.rs");
    let poll_turn_body = rust_function_body(shell_source, "fn poll_turn_updates");

    assert!(poll_turn_body.contains("activated_new_thread"));
    assert_order(
        poll_turn_body,
        "if activated_new_thread",
        "repair_selected_thread_title_if_needed",
    );
    assert!(poll_turn_body.contains("refresh_inventory_for_event"));
    assert!(poll_turn_body.contains("ThreadArchived"));
    assert!(poll_turn_body.contains("ThreadUnarchived"));
    assert!(transcript_branch_source.contains("mark_member_thread_inventory_refresh_needed"));
    assert!(decision_branch_source.contains("mark_member_thread_inventory_refresh_needed"));
    assert!(decision_archive_source.contains("mark_member_thread_inventory_refresh_needed"));
}

#[test]
fn branch_and_switch_uses_foreground_owned_bootstrap_stream() {
    let shell_source = include_str!("../src/shell.rs");
    let branch_menu_source = include_str!("../src/shell/transcript_branch_menu.rs");
    let branch_worker_source = concat!(
        include_str!("../src/shell/transcript_branch_worker.rs"),
        include_str!("../src/shell/transcript_branch_worker/foreground.rs"),
        include_str!("../src/shell/transcript_branch_worker/handlers.rs"),
    );
    let branch_core_source = include_str!("../src/shell/transcript_branch_core.rs");
    let poll_turn_body = rust_function_body(shell_source, "fn poll_turn_updates");
    let dispatch_body =
        rust_function_body(branch_menu_source, "fn dispatch_transcript_branch_request");
    let begin_body = rust_function_body(
        branch_worker_source,
        "fn begin_foreground_transcript_branch(",
    );

    assert!(branch_core_source.contains("prepare_transcript_branch"));
    assert!(branch_worker_source.contains("start_branch_bootstrap_turn_only"));
    assert!(branch_worker_source.contains("stream_foreground_branch_bootstrap_events"));
    assert!(branch_worker_source.contains("prove_branch_thread_completed_bootstrap_from_history"));
    assert!(dispatch_body.contains("TranscriptBranchAction::SwitchTo"));
    assert!(dispatch_body.contains("spawn_foreground_transcript_branch_worker"));
    assert!(dispatch_body.contains("self.turn_receiver = Some"));
    assert!(dispatch_body.contains("TranscriptBranchAction::Background"));
    assert!(dispatch_body.contains("spawn_transcript_branch_worker"));
    assert!(begin_body.contains("surface.load_thread_history(start.thread())"));
    assert!(begin_body.contains("surface.begin_turn_for_thread"));
    assert!(begin_body.contains("TurnStreamEvent::TurnStarted"));
    assert!(poll_turn_body.contains("ForegroundTranscriptBranchStarted"));
    assert!(poll_turn_body.contains("ForegroundTranscriptBranchPublicationFinished"));
    assert!(poll_turn_body.contains("foreground_transcript_branch_event_is_bootstrap_terminal"));
    assert!(poll_turn_body.contains("applied_stream_event.title_candidate = None"));
    assert!(branch_worker_source.contains("saw_target_idle_before_completion = true"));
    assert!(branch_worker_source.contains("pending_idle_event"));
    assert!(branch_worker_source.contains("TurnStreamEvent::TurnCompleted"));
    assert_order(
        dispatch_body,
        "spawn_foreground_transcript_branch_worker",
        "self.schedule_poll_if_needed",
    );
    assert_order(
        begin_body,
        "surface.load_thread_history(start.thread())",
        "surface.begin_turn_for_thread",
    );
    assert_order(
        begin_body,
        "surface.begin_turn_for_thread",
        "TurnStreamEvent::TurnStarted",
    );
}

#[test]
fn decision_branch_publication_validates_registration_and_binding_before_graph_persistence() {
    let decision_branch_source = concat!(
        include_str!("../src/shell/threaded_decision_branch.rs"),
        include_str!("../src/shell/threaded_decision_branch/worker.rs"),
        include_str!("../src/shell/threaded_decision_branch/completion.rs"),
    );
    let run_body = rust_function_body(decision_branch_source, "fn run_decision_branch_start(");
    let finish_body = rust_function_body(
        decision_branch_source,
        "fn finish_successful_decision_branch(",
    );

    assert!(!decision_branch_source.contains("GraphRefPersistenceStarted"));
    assert!(!run_body.contains("apply_graph_patch"));
    assert!(!run_body.contains("apply_threaded_decision_graph_patch"));
    assert!(finish_body.contains("let mut candidate_workspace_state"));
    assert!(finish_body.contains("let mut candidate_threaded_decision_state"));
    assert!(finish_body.contains("register_transcript_branch_thread"));
    assert!(finish_body.contains("activate_branch_with_bootstrap_turn"));
    assert!(finish_body.contains("apply_threaded_decision_graph_patch"));
    assert_order(
        finish_body,
        "register_transcript_branch_thread",
        "activate_branch_with_bootstrap_turn",
    );
    assert_order(
        finish_body,
        "activate_branch_with_bootstrap_turn",
        "apply_threaded_decision_graph_patch",
    );
}

#[test]
fn decision_branch_bootstrap_uses_visible_parent_context_source_content() {
    let decision_branch_source = concat!(
        include_str!("../src/shell/threaded_decision_branch.rs"),
        include_str!("../src/shell/threaded_decision_branch/worker.rs"),
        include_str!("../src/shell/threaded_decision_branch/queue.rs"),
        include_str!("../src/shell/threaded_decision_branch/support.rs"),
    );
    let context_source = include_str!("../src/threaded_decision_context.rs");
    let run_body = rust_function_body(decision_branch_source, "fn run_decision_branch_start(");
    let resolve_body =
        rust_function_body(decision_branch_source, "fn resolve_branch_point_for_job");

    assert!(decision_branch_source.contains("parent_context_source: Option<String>"));
    assert!(decision_branch_source.contains("fn parent_context_source_for_turn("));
    assert!(resolve_body.contains("parent_context_source_for_turn(turn)"));
    assert!(run_body.contains("branch_context: Some(context.text())"));
    assert!(run_body.contains("parent_context_source: job.parent_context_source.as_deref()"));
    assert!(context_source.contains("parent_context_source: Option<&'a str>"));
    assert!(context_source.contains("Parent context source content:"));
    assert!(context_source.contains("bootstrap turn records context"));
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
