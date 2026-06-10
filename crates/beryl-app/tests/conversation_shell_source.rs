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
    assert!(toolbar_body.contains("graph_toolbar_button"));
    assert!(!toolbar_body.contains("activity_mode_button"));
    assert!(!toolbar_body.contains("thread_navigation_button"));
    assert!(!toolbar_body.contains("\"thread-navigation-backward"));
    assert!(!toolbar_body.contains("\"thread-navigation-forward"));
    assert!(thread_strip_body.contains("thread_navigation_button"));
    assert!(thread_strip_body.contains("\"thread-navigation-backward-thread-strip\""));
    assert!(thread_strip_body.contains("\"thread-navigation-forward-thread-strip\""));
    assert!(toolbar_body.contains("surface.graph_overlay().visible()"));
    assert!(toolbar_body.contains("\"settings-toolbar\""));
    assert!(render_source.contains("\"graph-toolbar\""));
    assert!(!toolbar_body.contains("\"toggle-graph-overlay\""));
    assert!(!toolbar_body.contains("\"toggle-checklist-sidebar\""));
    assert!(toolbar_body.contains("toolbar_branch_breadcrumb_segments("));
    assert!(render_source.contains("fn render_toolbar_branch_breadcrumbs("));
    assert!(thread_strip_body.contains("\"thread-strip-new-thread\""));
    assert!(!thread_strip_body.contains("thread_strip_breadcrumb_trail("));
    assert!(!thread_strip_body.contains("render_thread_strip_breadcrumbs("));
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
fn transcript_thread_link_activation_defers_shell_update_from_panel_handler() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let body = rust_function_body(transcript_source, "fn handle_transcript_mouse_down");

    assert!(body.contains("if let Some(thread_id) = self.thread_link_for_position"));
    assert!(body.contains("let shell = self.shell.clone();"));
    assert!(body.contains("window.defer(cx"));
    assert_order(body, "window.defer(cx", "shell.activate_beryl_thread_link");
    assert!(!body.contains(
        "self.shell.update(cx, |shell, cx| {\r\n                shell.activate_beryl_thread_link"
    ));
    assert!(!body.contains(
        "self.shell.update(cx, |shell, cx| {\n                shell.activate_beryl_thread_link"
    ));
}

#[test]
fn thread_navigation_history_is_source_aware_and_committed_after_success() {
    let shell_source = include_str!("../src/shell.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let thread_navigation_actions_source =
        include_str!("../src/shell/thread_navigation_actions.rs");
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let selector_selection_body =
        rust_function_body(shell_source, "fn activate_thread_selector_selection");
    let transcript_link_body =
        rust_function_body(shell_source, "pub(super) fn activate_beryl_thread_link");
    let breadcrumb_body = rust_function_body(render_source, "fn render_toolbar_parent_breadcrumb");
    let activation_body = rust_function_body(shell_source, "fn activate_thread_selector_target");
    let finish_thread_body = rust_function_body(
        lifecycle_source,
        "pub(super) fn finish_thread_activation_worker",
    );
    let finish_workspace_body =
        rust_function_body(lifecycle_source, "pub(super) fn finish_workspace_open");

    assert!(selector_selection_body.contains("ThreadNavigationActivationSource::ThreadSelector"));
    assert!(
        transcript_link_body.contains("ThreadNavigationActivationSource::TranscriptThreadLink")
    );
    assert!(breadcrumb_body.contains("activate_branch_breadcrumb_thread_target"));
    assert!(
        thread_navigation_actions_source
            .contains("ThreadNavigationActivationSource::BranchBreadcrumb")
    );
    assert!(
        activation_body
            .contains("pending_thread_navigation_activation_for_target(source, &target)")
    );
    assert_order(
        activation_body,
        "known_backend_unavailable_block_for_target(&execution_target)",
        "self.pending_thread_navigation_activation = pending_navigation",
    );
    assert!(activation_body.contains("WorkspaceOpenIntent::ThreadSelectorActivation"));
    assert!(finish_thread_body.contains("finish_pending_thread_navigation_activation"));
    assert!(finish_thread_body.contains("discard_pending_thread_navigation_activation"));
    assert!(finish_workspace_body.contains("finish_pending_thread_navigation_activation"));
    assert!(finish_workspace_body.contains("discard_pending_thread_navigation_activation"));
}

#[test]
fn thread_navigation_rejection_paths_do_not_consume_history_before_success() {
    let shell_source = include_str!("../src/shell.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let thread_navigation_actions_source =
        include_str!("../src/shell/thread_navigation_actions.rs");
    let activation_body = rust_function_body(shell_source, "fn activate_thread_selector_target");
    let activation_target_body = rust_function_body(
        thread_navigation_actions_source,
        "fn thread_navigation_activation_target",
    );
    let activate_navigation_body = rust_function_body(
        thread_navigation_actions_source,
        "fn activate_thread_navigation(",
    );
    let finish_pending_body = rust_function_body(
        thread_navigation_actions_source,
        "pub(super) fn finish_pending_thread_navigation_activation",
    );
    let finish_thread_body = rust_function_body(
        lifecycle_source,
        "pub(super) fn finish_thread_activation_worker",
    );
    let handle_thread_stopped_body = rust_function_body(
        lifecycle_source,
        "pub(super) fn handle_thread_activation_worker_stopped",
    );

    assert_order(
        activation_body,
        "if self.workspace_receiver.is_some()",
        "let pending_navigation =",
    );
    assert_order(
        activation_body,
        "known_backend_unavailable_block_for_target(&execution_target)",
        "self.pending_thread_navigation_activation = pending_navigation",
    );
    let already_selected_tail = activation_body
        .split("current_execution_target == execution_target")
        .nth(1)
        .expect("activation should check the already-selected thread");
    let already_selected_branch = &already_selected_tail[..already_selected_tail
        .find("let Some(connector)")
        .expect("already-selected branch should precede connector requirement")];
    assert!(already_selected_branch.contains("ThreadActivationStart::AlreadySelected"));
    assert!(!already_selected_branch.contains("pending_thread_navigation_activation"));
    assert!(activation_target_body.contains("activated_link_thread_target"));
    assert!(activation_target_body.contains("recorded navigation target no longer matches"));
    assert_order(
        activate_navigation_body,
        "thread_navigation_activation_target(&entry)",
        "self.activate_thread_selector_target(target, source, window, cx)",
    );
    assert!(activate_navigation_body.contains("navigation_target_unavailable"));
    assert_order(
        finish_pending_body,
        "target.thread_id().as_str() != activated_thread_id",
        ".thread_navigation_histories",
    );
    assert_order(
        finish_pending_body,
        "target.execution_target() != execution_target",
        ".thread_navigation_histories",
    );
    assert!(finish_pending_body.contains("ConversationSurfaceState::selected_thread_id"));
    assert!(finish_thread_body.contains("ThreadActivationOutcome::RequiresRebind"));
    assert!(finish_thread_body.contains("ThreadActivationOutcome::Failed"));
    let requires_rebind_tail = finish_thread_body
        .split("ThreadActivationOutcome::RequiresRebind")
        .nth(1)
        .expect("finish body should handle rebind failures");
    let requires_rebind_arm = &requires_rebind_tail[..requires_rebind_tail
        .find("ThreadActivationOutcome::Failed")
        .expect("rebind arm should precede failed arm")];
    let failed_tail = finish_thread_body
        .split("ThreadActivationOutcome::Failed")
        .nth(1)
        .expect("finish body should handle activation failures");
    assert!(requires_rebind_arm.contains("self.discard_pending_thread_navigation_activation();"));
    assert!(failed_tail.contains("self.discard_pending_thread_navigation_activation();"));
    assert!(handle_thread_stopped_body.contains("discard_pending_thread_navigation_activation"));
}

#[test]
fn pending_thread_activation_preserves_visible_transcript_until_history_apply() {
    let shell_source = include_str!("../src/shell.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let begin_activation_body = rust_function_body(shell_source, "fn begin_thread_activation");
    let load_history_body = rust_function_body(shell_source, "fn load_thread_history_window");
    let finish_activation_body = rust_function_body(
        lifecycle_source,
        "pub(super) fn finish_thread_activation_worker",
    );

    assert!(begin_activation_body.contains("self.pending_thread_activation = Some"));
    assert!(begin_activation_body.contains("self.notices.clear_all()"));
    assert!(begin_activation_body.contains("self.close_transcript_branch_menu()"));
    assert!(begin_activation_body.contains("self.cancel_transcript_edit_mode()"));
    assert!(!begin_activation_body.contains("self.execution_details"));
    assert!(!begin_activation_body.contains("self.transcript_presentation"));
    assert!(!begin_activation_body.contains("self.transcript_history_window"));
    assert!(!begin_activation_body.contains("self.transcript_list_state"));
    assert!(!begin_activation_body.contains("self.transcript_reset_generation"));
    assert!(!begin_activation_body.contains("load_thread_history_window"));
    assert!(finish_activation_body.contains("surface.load_thread_history_window"));
    assert_order(
        load_history_body,
        ".replace_from_turns(self.execution_details.turns())",
        "self.pending_thread_activation = None",
    );
    assert_order(
        load_history_body,
        ".replace_from_turns(self.execution_details.turns())",
        "self.transcript_list_state",
    );
}

#[test]
fn loaded_history_activation_does_not_schedule_deferred_submit_anchor_scroll() {
    let shell_source = include_str!("../src/shell.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let snapshot_source = include_str!("../src/shell/transcript_panel_snapshot.rs");
    let anchor_source = include_str!("../src/shell/transcript_anchor.rs");
    let load_history_body = rust_function_body(shell_source, "fn load_thread_history_window");
    let render_body = rust_function_body(transcript_source, "fn render(&mut self");

    for source in [
        shell_source,
        transcript_source,
        snapshot_source,
        anchor_source,
    ] {
        assert!(!source.contains("loaded_history_anchor_pending"));
        assert!(!source.contains("install_loaded_history_transcript_anchor"));
        assert!(!source.contains("TranscriptSubmitAnchor::passive"));
    }
    assert!(!anchor_source.contains("fn passive("));
    assert!(load_history_body.contains("self.transcript_submit_anchor = None;"));
    assert!(!load_history_body.contains("latest_user_prompt_anchor"));
    assert!(!load_history_body.contains("sync_live_transcript_rows"));
    assert!(!load_history_body.contains("scroll_to_reveal_item_end"));
    assert_eq!(load_history_body.matches(".reset(").count(), 1);
    assert_eq!(
        render_body
            .matches("transcript_list_state.scroll_to(ListOffset")
            .count(),
        1
    );
    assert_order(
        render_body,
        "let trailing_scroll_allowance = snapshot",
        "transcript_list_state.scroll_to(ListOffset",
    );
    assert_order(
        render_body,
        ".submit_anchor",
        "transcript_list_state.scroll_to(ListOffset",
    );
    assert_order(
        load_history_body,
        "self.transcript_submit_anchor = None;",
        ".reset(self.transcript_list_item_count())",
    );
}

#[test]
fn thread_navigation_history_is_pruned_when_backend_sessions_teardown() {
    let shell_source = include_str!("../src/shell.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let thread_navigation_source = include_str!("../src/shell/thread_navigation.rs");
    let thread_navigation_actions_source =
        include_str!("../src/shell/thread_navigation_actions.rs");
    let finish_workspace_body =
        rust_function_body(lifecycle_source, "pub(super) fn finish_workspace_open");
    let shutdown_target_body = rust_function_body(
        shell_source,
        "pub(super) fn shutdown_backend_server_for_target_in_background",
    );
    let shutdown_all_body = rust_function_body(
        shell_source,
        "pub(super) fn shutdown_all_backend_servers_in_background",
    );
    let application_shutdown_body =
        rust_function_body(shell_source, "fn begin_application_shutdown");
    let prune_history_body = rust_function_body(
        thread_navigation_source,
        "pub(crate) fn discard_entries_for_execution_target",
    );
    let prune_shell_body = rust_function_body(
        thread_navigation_actions_source,
        "pub(super) fn discard_thread_navigation_for_execution_target",
    );

    assert_order(
        finish_workspace_body,
        "self.backend_servers.contains_key(&opened.execution_target)",
        "spawn_managed_backend_shutdown",
    );
    assert_order(
        finish_workspace_body,
        "discard_thread_navigation_for_execution_target(&opened.execution_target)",
        "self.finish_pending_thread_navigation_activation",
    );
    assert_order(
        finish_workspace_body,
        "discard_thread_navigation_for_execution_target(\n                        &backend_unavailable.target",
        "ShellState::BackendUnavailable",
    );
    assert!(shutdown_target_body.contains("discard_thread_navigation_for_execution_target"));
    assert!(shutdown_all_body.contains("discard_all_thread_navigation_histories"));
    assert!(application_shutdown_body.contains("discard_all_thread_navigation_histories"));
    assert!(prune_history_body.contains("self.current"));
    assert!(prune_history_body.contains("self.backward"));
    assert!(prune_history_body.contains("self.forward"));
    assert_eq!(prune_history_body.matches(".retain(|entry|").count(), 2);
    assert!(prune_shell_body.contains("thread_navigation_histories.retain"));
    assert!(prune_shell_body.contains("history.discard_entries_for_execution_target"));
    assert!(prune_shell_body.contains("!history.is_empty()"));
}

#[test]
fn toolbar_uses_graph_toggle_and_activity_is_auto_only() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let common_source = include_str!("../src/shell/render/common.rs");
    let shell_source = include_str!("../src/shell.rs");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let graph_button_body = rust_function_body(render_source, "fn graph_toolbar_button");
    let secondary_active_button_body = rust_function_body(
        common_source,
        "pub(super) fn secondary_button_with_active_state",
    );
    let seeded_surface_body = rust_function_body(shell_source, "fn seeded");
    let shell_graph_toggle_body = rust_function_body(
        shell_source,
        "fn toggle_graph_overlay(\n        &mut self,\n        _: &gpui::ClickEvent",
    );
    let themed_button_base_body = rust_function_body(common_source, "fn themed_button_base");
    let themed_button_container_body =
        rust_function_body(common_source, "fn themed_button_container");

    assert!(toolbar_body.contains("graph_toolbar_button"));
    assert!(!toolbar_body.contains("activity_mode_button"));
    assert!(!toolbar_body.contains("surface.tool_activity_panel_mode()"));
    assert!(!toolbar_body.contains("surface.tool_activity_panel_visible()"));
    assert!(!render_source.contains("fn activity_mode_button"));
    assert!(!render_source.contains("cycle_tool_activity_panel_mode"));
    assert!(graph_button_body.contains("secondary_button_with_active_state"));
    assert!(graph_button_body.contains("\"graph-toolbar\""));
    assert!(graph_button_body.contains("\"Graph\""));
    assert!(graph_button_body.contains("graph_visible"));
    assert!(graph_button_body.contains("ShellView::toggle_graph_overlay"));
    assert!(shell_graph_toggle_body.contains("surface.toggle_graph_overlay()"));
    assert!(
        seeded_surface_body.contains("tool_activity_panel_mode: WorkspaceActivityPanelMode::Auto")
    );
    assert!(!seeded_surface_body.contains("workspace_ui_state.tool_activity_panel_mode()"));
    assert!(!toolbar_body.contains("secondary_fixed_label_button"));
    assert!(!toolbar_body.contains("GRAPH_TOGGLE_LABELS"));
    assert!(!render_source.contains("GRAPH_TOGGLE_LABELS"));
    assert!(!render_source.contains("CHECKLIST_TOGGLE_LABELS"));
    assert_eq!(toolbar_body.matches("toolbar_toggle_label").count(), 0);
    assert!(!toolbar_body.contains("\"Hide Graph\""));
    assert!(secondary_active_button_body.contains("shell.secondary_button_theme()"));
    assert!(secondary_active_button_body.contains("ChromeButtonVisualState::Active"));
    assert!(secondary_active_button_body.contains("ChromeButtonVisualState::Normal"));
    assert!(secondary_active_button_body.contains("themed_button(theme, visual_state"));
    assert!(themed_button_base_body.contains("themed_button_container"));
    assert!(themed_button_container_body.contains(".flex_none()"));
    assert!(themed_button_container_body.contains(".hover("));
    assert!(themed_button_container_body.contains(".active("));
    assert!(!themed_button_container_body.contains("theme.hover.foreground"));
    assert!(!themed_button_container_body.contains("theme.active.foreground"));
    assert!(themed_button_container_body.contains(".font_weight(theme.font_weight)"));
}

#[test]
fn toolbar_places_breadcrumbs_after_workspaces_and_thread_strip_places_navigation_before_selector()
{
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let workspace_picker_source = include_str!("../src/shell/render/workspace_picker.rs");
    let common_source = include_str!("../src/shell/render/common.rs");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let graph_button_body = rust_function_body(render_source, "fn graph_toolbar_button");
    let workspace_picker_button_body = rust_function_body(
        workspace_picker_source,
        "pub(super) fn render_workspace_picker_button",
    );
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let thread_strip_action_body = rust_function_body(render_source, "fn thread_strip_action");
    let navigation_button_body = rust_function_body(render_source, "fn thread_navigation_button");
    let themed_button_base_body = rust_function_body(common_source, "fn themed_button_base");
    let thread_navigation_actions_source =
        include_str!("../src/shell/thread_navigation_actions.rs");

    assert!(!toolbar_body.contains("thread_navigation_backward_disabled_reason"));
    assert!(!toolbar_body.contains("thread_navigation_forward_disabled_reason"));
    assert!(thread_strip_body.contains("thread_navigation_backward_disabled_reason"));
    assert!(thread_strip_body.contains("thread_navigation_forward_disabled_reason"));
    assert_order(
        toolbar_body,
        "render_workspace_picker_button",
        "render_toolbar_branch_breadcrumbs",
    );
    assert_order(
        toolbar_body,
        "render_toolbar_branch_breadcrumbs",
        ".flex_1()",
    );
    assert_order(
        thread_strip_body,
        "\"thread-strip-new-thread\"",
        "\"thread-navigation-backward-thread-strip\"",
    );
    assert_order(
        thread_strip_body,
        "\"thread-navigation-backward-thread-strip\"",
        "\"thread-navigation-forward-thread-strip\"",
    );
    assert_order(
        thread_strip_body,
        "\"thread-navigation-forward-thread-strip\"",
        "render_thread_strip_active_thread_title",
    );
    assert!(!thread_strip_body.contains("workspace.runtime_mode().display_name()"));
    assert!(!thread_strip_body.contains("RuntimeMode::WslLinux"));
    assert!(!thread_strip_body.contains("WslLinux {"));
    assert_order(toolbar_body, ".flex_1()", "graph_toolbar_button");
    assert_order(toolbar_body, "graph_toolbar_button", "\"settings-toolbar\"");
    assert!(!toolbar_body.contains("\"retry-backend-toolbar\""));
    assert!(toolbar_body.contains(".gap_3()"));
    assert!(toolbar_body.contains("surface.graph_overlay().visible()"));
    assert!(!toolbar_body.contains("activity_mode_button"));
    assert!(workspace_picker_button_body.contains("secondary_button("));
    assert!(!workspace_picker_button_body.contains("MAIN_CHROME_LEADING_CONTROL_WIDTH"));
    assert!(!workspace_picker_button_body.contains(".w(px("));
    assert!(thread_strip_action_body.contains("secondary_button(shell, id, label, on_click)"));
    assert!(!thread_strip_action_body.contains("MAIN_CHROME_LEADING_CONTROL_WIDTH"));
    assert!(!thread_strip_action_body.contains(".w(px("));
    assert!(thread_strip_body.contains("disabled_secondary_button"));
    assert!(!thread_strip_body.contains("MAIN_CHROME_LEADING_CONTROL_WIDTH"));
    assert!(themed_button_base_body.contains(".px(px(layout::BUTTON_HORIZONTAL_PADDING))"));
    assert!(graph_button_body.contains("ShellView::toggle_graph_overlay"));
    assert!(navigation_button_body.contains("disabled_secondary_button"));
    assert!(navigation_button_body.contains(".opacity(0.62)"));
    assert!(navigation_button_body.contains("build_thread_navigation_tooltip"));
    assert!(navigation_button_body.contains("secondary_button(shell, id, label, on_click)"));
    assert!(navigation_button_body.contains(".w(px(layout::BUTTON_ICON_OUTER_WIDTH))"));
    assert!(thread_navigation_actions_source.contains("No backward thread history."));
    assert!(thread_navigation_actions_source.contains("No forward thread history."));
    assert!(thread_navigation_actions_source.contains("thread_activation_busy_message"));
    assert!(
        thread_navigation_actions_source.contains("known_backend_unavailable_block_for_target")
    );
    assert!(thread_navigation_actions_source.contains("thread_navigation_activation_target"));
}

#[test]
fn thread_navigation_render_and_disabled_paths_do_not_refresh_inventory() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let thread_navigation_actions_source =
        include_str!("../src/shell/thread_navigation_actions.rs");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let disabled_reason_body = rust_function_body(
        thread_navigation_actions_source,
        "fn thread_navigation_disabled_reason",
    );

    for body in [toolbar_body, thread_strip_body, disabled_reason_body] {
        assert!(!body.contains("mark_member_thread_inventory_refresh_needed"));
        assert!(!body.contains("build_member_thread_inventory_snapshot"));
        assert!(!body.contains("begin_refresh"));
        assert!(!body.contains("finish_refresh"));
    }
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
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
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
    assert!(toolbar_body.contains("toolbar_branch_breadcrumb_segments"));
    assert!(!thread_strip_body.contains("thread_strip_breadcrumb_trail"));
    assert!(composer_body.contains("set_enabled(enabled"));
    assert!(composer_body.contains("backend_controls_disabled"));
}

#[test]
fn toolbar_branch_breadcrumbs_render_as_bounded_exact_parent_activation() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let transcript_text_blocks_source =
        include_str!("../src/shell/render/transcript/text_blocks.rs");
    let render_workspace_surface_body =
        rust_function_body(render_source, "fn render_workspace_surface");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let breadcrumb_source_body =
        rust_function_body(render_source, "fn toolbar_branch_breadcrumb_segments");
    let breadcrumb_body = rust_function_body(render_source, "fn render_toolbar_branch_breadcrumbs");
    let parent_breadcrumb_body =
        rust_function_body(render_source, "fn render_toolbar_parent_breadcrumb");

    assert!(render_workspace_surface_body.contains("&loaded_workspace.threaded_decision_state"));
    assert!(!render_source.contains("fn active_thread_title_label"));
    assert!(!render_source.contains("format!(\"Opening {label}\")"));
    assert!(toolbar_body.contains("toolbar_branch_breadcrumb_segments"));
    assert!(thread_strip_body.contains("render_thread_strip_active_thread_title"));
    assert!(thread_strip_body.contains("let active_label = selected_thread_title_label"));
    assert!(!thread_strip_body.contains("pending_thread_activation_label"));
    assert!(!thread_strip_body.contains("thread_strip_breadcrumb_trail"));
    assert!(!breadcrumb_source_body.contains("pending_thread_activation_label().is_some()"));
    assert!(toolbar_body.contains("selected_thread_title_label"));
    assert!(toolbar_body.contains("&selected_label"));
    assert!(breadcrumb_source_body.contains("thread_strip_breadcrumb_trail"));
    assert!(breadcrumb_source_body.contains("TransientBranchParent"));
    assert!(breadcrumb_source_body.contains("foreground_transcript_branch"));
    assert!(breadcrumb_source_body.contains("surface.selected_thread_id()"));
    assert!(breadcrumb_source_body.contains(".filter(|segment| !segment.active())"));
    assert!(breadcrumb_body.contains(".child(\">\")"));
    assert!(breadcrumb_body.contains(".gap(px(layout::TOOLBAR_BREADCRUMB_GAP))"));
    assert!(breadcrumb_body.contains(".px(px(layout::BUTTON_BORDER_WIDTH))"));
    assert!(breadcrumb_body.contains(".w(px(layout::TOOLBAR_BREADCRUMB_SEPARATOR_WIDTH))"));
    assert!(breadcrumb_body.contains(".text_center()"));
    assert!(!breadcrumb_body.contains("render_thread_strip_active_thread_title"));
    assert!(parent_breadcrumb_body.contains("\"toolbar-branch-parent-breadcrumb\""));
    assert!(parent_breadcrumb_body.contains("ThreadSelectorActivationTarget"));
    assert!(parent_breadcrumb_body.contains("breadcrumb.thread_id().clone()"));
    assert!(parent_breadcrumb_body.contains("label.clone()"));
    assert!(parent_breadcrumb_body.contains("view.activate_branch_breadcrumb_thread_target"));
    assert!(breadcrumb_body.contains(".max_w(px(layout::TOOLBAR_BREADCRUMB_TRAIL_MAX_WIDTH))"));
    assert!(!breadcrumb_body.contains(".max_w(relative(0.42))"));
    assert!(
        parent_breadcrumb_body.contains(".max_w(px(layout::TOOLBAR_BREADCRUMB_BUTTON_MAX_WIDTH))")
    );
    assert!(!parent_breadcrumb_body.contains(".max_w(relative(0.32))"));
    assert!(parent_breadcrumb_body.contains(".min_w(px(0.0))"));
    assert_eq!(
        parent_breadcrumb_body.matches(".overflow_hidden()").count(),
        1
    );
    assert!(parent_breadcrumb_body.contains(
        ".child(\r\n            div()\r\n                .min_w(px(0.0))\r\n                .overflow_hidden()"
    ) || parent_breadcrumb_body.contains(
        ".child(\n            div()\n                .min_w(px(0.0))\n                .overflow_hidden()"
    ));
    assert!(parent_breadcrumb_body.contains(".truncate()"));
    assert!(parent_breadcrumb_body.contains(".tooltip("));
    assert!(!transcript_source.contains("pending_thread_activation_state"));
    assert!(!transcript_source.contains("has_pending_thread_activation"));
    assert!(!transcript_text_blocks_source.contains("Opening {label}"));
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
fn transcript_prepaint_schedules_detail_loads_from_current_view() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");

    assert!(transcript_source.contains("let detail_shell = shell.clone();"));
    assert!(transcript_source.contains(
        "window.defer(cx, move |window, cx| {\n                                                    detail_shell.update(cx, |shell, cx| {\n                                                        shell.begin_transcript_turn_detail_loads_for_current_viewport"
    ) || transcript_source.contains(
        "window.defer(cx, move |window, cx| {\r\n                                                    detail_shell.update(cx, |shell, cx| {\r\n                                                        shell.begin_transcript_turn_detail_loads_for_current_viewport"
    ));
    assert!(!transcript_source.contains("shell.begin_transcript_turn_detail_loads_for_viewport"));
    assert!(!transcript_source.contains(
        "shell.update(cx, |shell, cx| {\n                                                    shell\n                                                        .begin_transcript_turn_detail_loads_for_viewport"
    ));
    assert!(!transcript_source.contains(
        "shell.update(cx, |shell, cx| {\r\n                                                    shell\r\n                                                        .begin_transcript_turn_detail_loads_for_viewport"
    ));
}

#[test]
fn transcript_scroll_schedules_detail_loads_without_waiting_for_prepaint() {
    let shell_source = include_str!("../src/shell.rs");
    let scroll_body = rust_function_body(shell_source, "fn apply_transcript_scroll_command");
    let scroll_event_body = rust_function_body(shell_source, "fn note_transcript_scroll_event");

    assert!(scroll_body.contains("self.notify_transcript_panel(cx);"));
    assert!(!scroll_body.contains("normalize_transcript_detail_placeholder_scroll_anchor"));
    assert!(
        scroll_body
            .contains("self.begin_transcript_turn_detail_loads_for_scroll_anchor(window, cx)")
    );
    assert!(!scroll_event_body.contains("normalize_transcript_detail_placeholder_scroll_anchor"));
    assert!(
        scroll_body
            .contains("self.begin_transcript_turn_detail_loads_for_current_viewport(window, cx);")
    );
    assert!(
        scroll_event_body
            .contains("self.begin_transcript_turn_detail_loads_for_scroll_anchor(window, cx)")
    );
    assert!(
        scroll_event_body
            .contains("self.begin_transcript_turn_detail_loads_for_current_viewport(window, cx);")
    );
}

#[test]
fn transcript_detail_scheduler_uses_newest_visible_turn_first() {
    let detail_source = include_str!("../src/shell/transcript_turn_detail.rs");
    let order_body = rust_function_body(detail_source, "fn transcript_turn_detail_viewport_order");

    assert!(order_body.contains("TranscriptTurnDetailViewportOrder::NewestFirst"));
    assert!(!order_body.contains("TranscriptTurnDetailViewportOrder::OldestFirst"));
}

#[test]
fn transcript_detail_tail_scheduling_targets_latest_row_before_broad_visible_range() {
    let detail_source = include_str!("../src/shell/transcript_turn_detail.rs");
    let viewport_body = rust_function_body(
        detail_source,
        "pub(super) fn begin_transcript_turn_detail_loads_for_current_viewport",
    );
    let latest_missing_body =
        rust_function_body(detail_source, "fn latest_source_turn_missing_detail_range");

    assert!(viewport_body.contains("ListScrollPosition::Bottom"));
    assert!(viewport_body.contains("ListScrollPosition::VirtualTail"));
    assert!(viewport_body.contains("latest_source_turn_missing_detail_range"));
    assert!(viewport_body.contains("let visible_range = list_state.visible_range();"));
    assert!(viewport_body.contains("let priority_range = match scroll_position"));
    assert!(viewport_body.contains("unwrap_or_else(|| visible_range.clone())"));
    assert!(viewport_body.contains("(priority_range, visible_range, order)"));
    assert!(detail_source.contains("from_priority_and_retained"));
    assert!(latest_missing_body.contains("is_missing_detail_requestable"));
    assert!(!latest_missing_body.contains("TranscriptTurnDetailStatus::Missing"));
}

#[test]
fn transcript_detail_apply_preserves_user_scrolled_loaded_row_anchor() {
    let detail_source = include_str!("../src/shell/transcript_turn_detail.rs");
    let apply_body = rust_function_body(
        detail_source,
        "pub(super) fn finish_loading_transcript_turn_details",
    );

    assert!(apply_body.contains("let visible_range_before_apply"));
    assert!(apply_body.contains("let content_anchor_before_apply"));
    assert!(apply_body.contains("let preserve_loaded_row_anchor"));
    assert!(apply_body.contains("anchor.item_ix == row_index"));
    assert!(apply_body.contains("self.transcript_list_state.scroll_to_position"));
}

#[test]
fn transcript_detail_poll_does_not_reschedule_before_layout_feedback() {
    let detail_source = include_str!("../src/shell/transcript_turn_detail.rs");
    let poll_body = rust_function_body(
        detail_source,
        "pub(super) fn poll_transcript_turn_detail_updates",
    );

    assert!(!poll_body.contains("begin_transcript_turn_detail_loads_for_current_viewport"));
    assert!(poll_body.contains("self.transcript_turn_detail_task = None;"));
}

#[test]
fn composer_image_label_sync_schedules_validation_before_minimal_scan() {
    let sync_source = include_str!("../src/shell/composer_image_label_sync.rs");
    let scheduler_body = rust_function_body(
        sync_source,
        "pub(super) fn begin_composer_image_label_sync_if_needed",
    );
    let validation_finish_body = rust_function_body(
        sync_source,
        "fn finish_composer_image_label_validation_worker",
    );
    let scan_finish_body =
        rust_function_body(sync_source, "fn finish_composer_image_label_scan_worker");

    assert!(scheduler_body.contains("composer_image_label_validation_receiver.is_some()"));
    assert!(scheduler_body.contains("TranscriptTurnDetailTask::has_active_tickets"));
    assert!(
        scheduler_body.contains(
            "ConversationSurfaceState::selected_thread_needing_composer_image_label_sync"
        )
    );
    assert!(scheduler_body.contains("ComposerImageLabelHistorySyncRequest::Validate"));
    assert!(scheduler_body.contains("spawn_composer_image_label_validation_worker"));
    assert!(scheduler_body.contains("ComposerImageLabelHistorySyncRequest::Scan"));
    assert!(scheduler_body.contains("spawn_composer_image_label_scan_worker_for_plan"));
    assert!(scheduler_body.contains("ComposerImageLabelScanPlan::FullCurrentHistory"));

    assert!(validation_finish_body.contains("composer_image_label_task_matches_selected"));
    assert!(
        validation_finish_body.contains("ComposerImageLabelFrontierValidationOutcome::CacheValid")
    );
    assert!(
        validation_finish_body.contains("ComposerImageLabelFrontierValidationOutcome::AppendOnly")
    );
    assert!(validation_finish_body.contains("ComposerImageLabelScanPlan::AppendOnlySuffix"));
    assert!(
        validation_finish_body
            .contains("ComposerImageLabelFrontierValidationOutcome::UnknownMutation")
    );
    assert!(validation_finish_body.contains("ComposerImageLabelScanPlan::FullCurrentHistory"));
    assert!(scan_finish_body.contains("composer_image_label_task_matches_selected"));
}

#[test]
fn composer_image_label_sync_stays_below_visible_transcript_detail_priority() {
    let shell_source = include_str!("../src/shell.rs");
    let sync_source = include_str!("../src/shell/composer_image_label_sync.rs");
    let poll_body = rust_function_body(shell_source, "fn poll(&mut self");
    let scheduler_body = rust_function_body(
        sync_source,
        "pub(super) fn begin_composer_image_label_sync_if_needed",
    );

    assert_order(
        poll_body,
        "poll_transcript_turn_detail_updates",
        "poll_composer_image_label_validation_updates",
    );
    assert_order(
        poll_body,
        "poll_composer_image_label_scan_updates",
        "begin_composer_image_label_sync_if_needed",
    );
    assert!(scheduler_body.contains("self.thread_history_page_receiver.is_some()"));
    assert!(scheduler_body.contains("TranscriptTurnDetailTask::has_active_tickets"));
}

#[test]
fn composer_image_label_sync_treats_not_loaded_thread_history_as_unscanned() {
    let shell_source = include_str!("../src/shell.rs");
    let load_body = rust_function_body(shell_source, "fn load_thread_history_window");

    assert!(load_body.contains("skeleton_partial_turns"));
    assert!(load_body.contains("history_window.has_older_pages() || skeleton_partial_turns"));
    assert!(!load_body.contains("history_window.has_older_pages() && !skeleton_partial_turns"));
}

#[test]
fn composer_image_paste_completion_rechecks_scope_and_readiness_before_allocating_label() {
    let shell_source = include_str!("../src/shell.rs");
    let begin_body = rust_function_body(shell_source, "fn begin_composer_image_asset_paste");
    let poll_body = rust_function_body(shell_source, "fn poll_composer_image_asset_updates");
    let finish_body = rust_function_body(shell_source, "fn finish_composer_image_asset_paste");

    assert!(begin_body.contains("composer_clipboard_label_scope"));
    assert!(begin_body.contains("label_scope"));
    assert!(poll_body.contains("finish_composer_image_asset_paste(result, window, cx)"));
    assert!(finish_body.contains("is_composer_clipboard_label_scope_current"));
    assert!(finish_body.contains("ensure_composer_image_paste_readiness(window, cx)"));
    assert!(finish_body.contains("self.composer_draft.image_labels()"));
    assert!(finish_body.contains("try_allocate_composer_image_label"));
    assert_order(
        finish_body,
        "is_composer_clipboard_label_scope_current",
        "ensure_composer_image_paste_readiness(window, cx)",
    );
    assert_order(
        finish_body,
        "ensure_composer_image_paste_readiness(window, cx)",
        "self.composer_draft.image_labels()",
    );
    assert_order(
        finish_body,
        "self.composer_draft.image_labels()",
        "try_allocate_composer_image_label",
    );
    assert_order(
        finish_body,
        "try_allocate_composer_image_label",
        "stage_image",
    );
}

#[test]
fn composer_cross_scope_marker_paste_uses_guarded_label_allocation() {
    let shell_source = include_str!("../src/shell.rs");
    let paste_body =
        rust_function_body(shell_source, "fn paste_resolved_composer_clipboard_payload");
    let mapping_body =
        rust_function_body(shell_source, "fn composer_clipboard_paste_label_mapping");

    assert_order(
        paste_body,
        "ensure_composer_image_paste_readiness(window, cx)",
        "composer_clipboard_paste_label_mapping",
    );
    assert!(mapping_body.contains("if same_scope"));
    assert!(mapping_body.contains("try_allocate_composer_image_label"));
    assert!(mapping_body.contains("reserved_labels.push(label.clone())"));
    assert!(!mapping_body.contains("surface.allocate_composer_image_label()"));
}

#[test]
fn pending_queued_image_labels_are_observed_only_after_queue_admission() {
    let shell_source = include_str!("../src/shell.rs");
    let queue_body = rust_function_body(shell_source, "fn queue_pending_turn_fragment");

    assert_order(
        queue_body,
        "match queue.try_append(user_input.clone())",
        "observe_composer_image_labels_in_thread_fragment",
    );
    assert_order(
        queue_body,
        "PendingTurnInputQueue::try_new",
        "observe_composer_image_labels_in_thread_fragment",
    );
    assert_order(
        queue_body,
        "let Some((turn_index, fragment_index)) = queued else",
        "observe_composer_image_labels_in_thread_fragment",
    );
}

#[test]
fn image_draft_submit_and_edit_start_recheck_label_readiness_before_mutating() {
    let shell_source = include_str!("../src/shell.rs");
    let edit_mode_source = include_str!("../src/shell/transcript_edit_mode.rs");
    let queue_body = rust_function_body(shell_source, "fn queue_turn_from_composer(");
    let edit_start_body = rust_function_body(
        edit_mode_source,
        "pub(crate) fn begin_transcript_edit_mode_from_request",
    );

    assert_order(
        queue_body,
        "ensure_composer_image_paste_readiness(window, cx)",
        "queue_transcript_edit_commit_from_composer",
    );
    assert_order(
        queue_body,
        "ensure_composer_image_paste_readiness(window, cx)",
        "begin_composer_image_delivery",
    );
    assert!(edit_start_body.contains("request.target().draft_seed().contains_images()"));
    assert_order(
        edit_start_body,
        "ensure_composer_image_paste_readiness(window, cx)",
        "begin_transcript_edit_mode(request)",
    );
    assert_order(
        edit_start_body,
        "ensure_composer_image_paste_readiness(window, cx)",
        "populate_composer_for_transcript_edit",
    );
}

#[test]
fn selected_thread_inventory_activity_change_invalidates_image_label_cache_for_validation() {
    let inventory_source = include_str!("../src/shell/member_thread_inventory.rs");
    let sync_source = include_str!("../src/shell/composer_image_label_sync.rs");
    let refresh_body = rust_function_body(
        inventory_source,
        "fn finish_member_thread_inventory_refresh",
    );
    let invalidation_body = rust_function_body(
        sync_source,
        "pub(super) fn mark_selected_thread_image_labels_need_validation_if_updated",
    );

    assert!(refresh_body.contains("selected_thread_updated_at"));
    assert!(refresh_body.contains("mark_selected_thread_image_labels_need_validation_if_updated"));
    assert_order(
        refresh_body,
        "selected_thread_updated_at",
        "finish_refresh_for_token",
    );
    assert!(invalidation_body.contains("thread.updated_at == updated_at"));
    assert!(invalidation_body.contains("mark_thread_history_needs_validation"));
}

#[test]
fn image_label_worker_completion_is_guarded_after_in_flight_invalidation() {
    let sync_source = include_str!("../src/shell/composer_image_label_sync.rs");
    let labels_source = include_str!("../src/shell/composer_image_labels.rs");
    let invalidation_body = rust_function_body(
        labels_source,
        "pub(super) fn mark_thread_history_needs_validation",
    );
    let scan_finish_body = rust_function_body(
        labels_source,
        "pub(super) fn finish_in_flight_thread_history_scan_with_frontier",
    );
    let worker_finish_body =
        rust_function_body(sync_source, "fn finish_composer_image_label_scan_worker");
    let begin_after_validation_body = rust_function_body(
        sync_source,
        "    fn begin_composer_image_label_scan_after_validation",
    );

    assert!(invalidation_body.contains("ComposerImageLabelHistoryState::Validating { frontier }"));
    assert!(invalidation_body.contains("ComposerImageLabelHistoryState::NeedsValidation"));
    assert!(invalidation_body.contains("ComposerImageLabelHistoryState::Scanning { .. }"));
    assert!(invalidation_body.contains("ComposerImageLabelHistoryState::NeedsScan"));
    assert!(scan_finish_body.contains("ComposerImageLabelHistoryState::Scanning { .. }"));
    assert!(worker_finish_body.contains("if surface.finish_composer_image_label_scan("));
    assert_order(
        worker_finish_body,
        "if surface.finish_composer_image_label_scan(",
        "surface.clear_notice_with_title",
    );
    assert!(worker_finish_body.contains("fail_in_flight_composer_image_label_scan"));
    assert_order(
        begin_after_validation_body,
        "surface.begin_composer_image_label_scan_after_validation",
        "backend_client_connector",
    );
}

#[test]
fn transcript_scroll_does_not_snap_history_detail_placeholders_to_row_top() {
    let shell_source = include_str!("../src/shell.rs");
    let detail_source = include_str!("../src/shell/transcript_turn_detail.rs");
    let scroll_body = rust_function_body(shell_source, "fn apply_transcript_scroll_command");
    let scroll_event_body = rust_function_body(shell_source, "fn note_transcript_scroll_event");

    assert!(!detail_source.contains("normalize_transcript_detail_placeholder_scroll_anchor"));
    assert!(!scroll_body.contains("has_history_detail_loading_placeholder"));
    assert!(!scroll_event_body.contains("has_history_detail_loading_placeholder"));
    assert!(!scroll_body.contains("TranscriptTurnDetailStatus::Missing"));
    assert!(!scroll_event_body.contains("TranscriptTurnDetailStatus::Missing"));
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
    assert!(older_history_page_body.contains("ShellState::Ready(ready)"));
    assert!(older_history_page_body.contains("ShellState::BackendUnavailable(_)"));
    assert!(!older_history_page_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(!older_history_page_body.contains("connector.launch_spec().runtime_mode().clone()"));
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

#[test]
fn transcript_detail_ui_pins_are_production_retention_only_hooks() {
    let detail_source = include_str!("../src/shell/transcript_turn_detail.rs");
    let cache_source = include_str!("../src/shell/transcript_history/detail_cache.rs");
    let branch_menu_source = include_str!("../src/shell/transcript_branch_menu.rs");
    let edit_mode_source = include_str!("../src/shell/transcript_edit_mode.rs");
    let transcript_live_rows_source = include_str!("../src/shell/transcript_live_rows.rs");

    let sync_body = rust_function_body(
        detail_source,
        "pub(super) fn sync_transcript_turn_detail_ui_pins",
    );
    assert!(sync_body.contains("TranscriptTurnDetailPinKind::ActiveContextMenu"));
    assert!(sync_body.contains("TranscriptTurnDetailPinKind::EditTarget"));
    assert!(sync_body.contains("TranscriptTurnDetailPinKind::MediaActionTarget"));
    assert!(sync_body.contains("TranscriptTurnDetailPinKind::ActiveTurn"));
    assert!(sync_body.contains("release_unpinned_transcript_turn_details_for_current_viewport"));
    assert!(!sync_body.contains("schedule_viewport_full_details"));
    assert!(!sync_body.contains("begin_loading"));

    assert!(!cache_source.contains("ActiveSelection"));
    assert!(!cache_source.contains("QuotePopup"));
    assert!(!cache_source.contains("VisibleRange"));
    assert!(!cache_source.contains("Overscan"));

    let close_menu_body = rust_function_body(
        branch_menu_source,
        "pub(crate) fn close_transcript_branch_menu",
    );
    assert!(close_menu_body.contains("sync_transcript_turn_detail_ui_pins"));
    let open_menu_body = rust_function_body(
        branch_menu_source,
        "pub(crate) fn open_transcript_branch_menu_for_row",
    );
    assert_order(
        open_menu_body,
        ".open_menu_with_title_update",
        "sync_transcript_turn_detail_ui_pins",
    );
    let accept_branch_body = rust_function_body(
        branch_menu_source,
        "fn accept_transcript_branch_menu_action",
    );
    assert_order(
        accept_branch_body,
        "dispatch_transcript_branch_request",
        "sync_transcript_turn_detail_ui_pins",
    );
    let accept_edit_body = rust_function_body(
        branch_menu_source,
        "pub(crate) fn edit_transcript_turn_from_menu",
    );
    assert_order(
        accept_edit_body,
        "begin_transcript_edit_mode_from_request",
        "sync_transcript_turn_detail_ui_pins",
    );

    let begin_edit_body =
        rust_function_body(edit_mode_source, "pub(crate) fn begin_transcript_edit_mode");
    let cancel_edit_body = rust_function_body(
        edit_mode_source,
        "pub(crate) fn cancel_transcript_edit_mode",
    );
    let reconcile_edit_body = rust_function_body(
        edit_mode_source,
        "pub(crate) fn reconcile_transcript_edit_mode",
    );
    assert!(begin_edit_body.contains("sync_transcript_turn_detail_ui_pins"));
    assert!(cancel_edit_body.contains("sync_transcript_turn_detail_ui_pins"));
    assert!(reconcile_edit_body.contains("sync_transcript_turn_detail_ui_pins"));

    let live_sync_body = rust_function_body(
        transcript_live_rows_source,
        "pub(super) fn sync_live_transcript_rows",
    );
    assert_order(
        live_sync_body,
        "sync_live_transcript_rows(",
        "sync_transcript_turn_detail_ui_pins",
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
