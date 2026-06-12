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
fn thread_selector_activation_defers_shell_work_out_of_selector_callbacks() {
    let shell_source = include_str!("../src/shell.rs");
    let selection_body = rust_function_body(shell_source, "fn activate_thread_selector_selection");
    let defer_body = rust_function_body(shell_source, "fn defer_thread_selector_activation_target");
    let click_handler_tail = shell_source
        .split("event: &gpui::ClickEvent")
        .nth(1)
        .expect("thread selector click handler should accept a click event");
    let click_handler_body = &click_handler_tail[..click_handler_tail
        .find("fn activate_thread_selector_selection")
        .expect("click handler should precede activation helper")];
    let key_handler_body = rust_function_body(shell_source, "fn handle_thread_selector_key_down");

    assert!(selection_body.contains("ThreadNavigationActivationSource::ThreadSelector"));
    assert!(selection_body.contains("self.defer_thread_selector_activation_target("));
    assert!(!selection_body.contains("self.activate_thread_selector_target("));
    assert!(defer_body.contains("window.defer(cx, move |window, cx|"));
    assert_order(defer_body, "window.defer(cx", "shell.update(cx");
    assert_order(
        defer_body,
        "shell.update(cx",
        "shell.activate_thread_selector_target",
    );
    assert!(click_handler_body.contains("self.activate_thread_selector_selection(window, cx)"));
    assert!(key_handler_body.contains("self.activate_thread_selector_selection(window, cx)"));
}

#[test]
fn thread_selector_activation_duplicate_pending_target_is_stable() {
    let shell_source = include_str!("../src/shell.rs");
    let selected_activation_source = include_str!("../src/shell/selected_thread_activation.rs");
    let activation_body = rust_function_body(shell_source, "fn activate_thread_selector_target");

    assert!(selected_activation_source.contains("pub(super) fn pending_thread_activation_matches"));
    assert!(activation_body.contains("surface.pending_thread_activation_matches("));
    assert_order(
        activation_body,
        "surface.pending_thread_activation_matches(",
        "return ThreadActivationStart::Started;",
    );
    assert_order(
        activation_body,
        "return ThreadActivationStart::Started;",
        "Thread activation unavailable",
    );
    assert!(activation_body.contains("kind: \"busy\""));
}

#[test]
fn transcript_block_window_renderer_follows_multi_block_user_prompts_and_reasoning() {
    let turn_blocks_source = include_str!("../src/shell/render/transcript/turn_blocks.rs");
    let body = rust_function_body(turn_blocks_source, "fn render_turn_card_block_window");

    assert!(body.contains("render_user_prompt_fragment_markdown_source_slice"));
    assert!(!body.contains("render_user_prompt_units("));
    assert!(body.contains("ExecutionItem::Reasoning(reasoning)"));
    assert!(body.contains("reasoning_source_text("));
    assert!(body.contains("TranscriptTextRole::AssistantReasoning"));
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
    let publication_finish_body = rust_function_body(
        shell_source,
        "fn finish_published_selected_thread_activation",
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
    assert!(finish_thread_body.contains("finish_published_selected_thread_activation"));
    assert!(publication_finish_body.contains("finish_pending_thread_navigation_activation"));
    assert!(finish_workspace_body.contains("synchronous_publication"));
    assert_order(
        finish_workspace_body,
        "self.state = ShellState::Ready",
        "self.finish_published_selected_thread_activation(publication)",
    );
    assert!(finish_thread_body.contains("discard_pending_thread_navigation_activation"));
    assert!(finish_workspace_body.contains("discard_pending_thread_navigation_activation"));
    assert!(!finish_workspace_body.contains("finish_pending_thread_navigation_activation"));
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
        "self.pending_thread_navigation_activation.as_ref()",
        "self.pending_thread_navigation_activation.take()",
    );
    assert_order(
        finish_pending_body,
        "target.thread_id().as_str() != activated_thread_id",
        "self.pending_thread_navigation_activation.take()",
    );
    assert_order(
        finish_pending_body,
        "target.execution_target() != execution_target",
        "self.pending_thread_navigation_activation.take()",
    );
    assert!(finish_pending_body.contains("ConversationSurfaceState::selected_thread_id"));
    assert_order(
        finish_pending_body,
        "ConversationSurfaceState::selected_thread_id",
        "self.pending_thread_navigation_activation.take()",
    );
    assert_order(
        finish_pending_body,
        "self.pending_thread_navigation_activation.take()",
        ".thread_navigation_histories",
    );
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
fn pending_thread_activation_preserves_visible_transcript_until_staged_publication() {
    let shell_source = include_str!("../src/shell.rs");
    let selected_activation_source = include_str!("../src/shell/selected_thread_activation.rs");
    let selected_activation_preparation_source =
        include_str!("../src/shell/selected_thread_activation/preparation.rs");
    let selected_activation_publisher_source =
        include_str!("../src/shell/selected_thread_activation/publisher.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let shell_poll_body = rust_function_body(shell_source, "fn poll(");
    let selected_activation_pending_body = rust_function_body(
        selected_activation_source,
        "fn selected_thread_activation_pending",
    );
    let poll_activation_updates_body =
        rust_function_body(shell_source, "fn poll_thread_activation_updates");
    let current_submission_block_body =
        rust_function_body(shell_source, "fn current_conversation_submission_block");
    let current_new_thread_block_body =
        rust_function_body(shell_source, "fn current_new_thread_block");
    let graph_thread_ref_body = rust_function_body(shell_source, "fn select_graph_thread_ref");
    let begin_activation_body =
        rust_function_body(selected_activation_source, "fn begin_thread_activation");
    let progress_poll_body = rust_function_body(
        selected_activation_source,
        "fn poll_pending_thread_activation_progress",
    );
    let progress_cap_body = rust_function_body(
        selected_activation_source,
        "fn pending_thread_activation_progress_cap",
    );
    let stage_activation_body = rust_function_body(
        selected_activation_source,
        "fn stage_selected_thread_activation",
    );
    let publish_staged_body = rust_function_body(
        selected_activation_source,
        "fn publish_staged_selected_thread_activation",
    );
    let publish_history_body = rust_function_body(
        selected_activation_publisher_source,
        "fn publish_history_window",
    );
    let publisher_try_body = rust_function_body(
        selected_activation_publisher_source,
        "pub(super) fn try_publish",
    );
    let finish_publication_body = rust_function_body(
        shell_source,
        "fn finish_published_selected_thread_activation",
    );
    let seeded_surface_body = rust_function_body(shell_source, "fn seeded");
    let refresh_reopen_body = rust_function_body(shell_source, "fn refresh_after_backend_reopen");
    let finish_activation_body = rust_function_body(
        lifecycle_source,
        "pub(super) fn finish_thread_activation_worker",
    );

    assert!(begin_activation_body.contains("self.pending_thread_activation = Some"));
    assert!(begin_activation_body.contains("thread_id: thread_id.into()"));
    assert!(begin_activation_body.contains("execution_target"));
    assert!(begin_activation_body.contains("source"));
    assert!(begin_activation_body.contains("started_at: Instant::now()"));
    assert!(begin_activation_body.contains("PENDING_THREAD_ACTIVATION_INITIAL_PROGRESS"));
    assert!(begin_activation_body.contains("self.notices.clear_all()"));
    assert!(begin_activation_body.contains("self.close_transcript_branch_menu()"));
    assert!(begin_activation_body.contains("self.cancel_transcript_edit_mode()"));
    assert!(!begin_activation_body.contains("self.execution_details"));
    assert!(!begin_activation_body.contains("self.transcript_presentation"));
    assert!(!begin_activation_body.contains("self.transcript_history_window"));
    assert!(!begin_activation_body.contains("self.transcript_list_state"));
    assert!(!begin_activation_body.contains("self.transcript_reset_generation"));
    assert!(!begin_activation_body.contains("load_thread_history_window"));
    assert!(!begin_activation_body.contains("publish_history_window"));
    assert!(!begin_activation_body.contains("sync_thread_selector_active_thread"));
    assert!(progress_poll_body.contains("pending_thread_activation_progress_cap"));
    assert!(progress_poll_body.contains("advance_progress_to"));
    assert!(!progress_poll_body.contains("publish_history_window"));
    assert!(progress_cap_body.contains("PENDING_THREAD_ACTIVATION_WORKER_PROGRESS_CAP"));
    assert!(progress_cap_body.contains("StagedSelectedThreadActivation::progress_cap"));
    assert!(
        stage_activation_body.contains("self.staged_selected_thread_activation = Some(activation)")
    );
    assert!(!stage_activation_body.contains("self.execution_details"));
    assert!(!stage_activation_body.contains("self.transcript_presentation"));
    assert!(!stage_activation_body.contains("self.transcript_history_window"));
    assert!(!stage_activation_body.contains("self.transcript_list_state"));
    assert!(!stage_activation_body.contains("sync_thread_selector_active_thread"));
    assert!(!stage_activation_body.contains("upsert_selected_thread"));
    assert!(publish_staged_body.contains("SelectedThreadPublisher::try_publish(self)"));
    assert!(
        publisher_try_body
            .contains("let staged = surface.staged_selected_thread_activation.as_ref()?")
    );
    assert!(publisher_try_body.contains("if !staged.is_ready_for_publication()"));
    assert!(publisher_try_body.contains("surface.staged_selected_thread_activation.take()?"));
    assert!(selected_activation_publisher_source.contains("publish_history_window("));
    assert!(
        selected_activation_publisher_source
            .contains("surface.set_thread_session_metadata(metadata)")
    );
    assert_order(
        publisher_try_body,
        "if !staged.is_ready_for_publication()",
        "surface.staged_selected_thread_activation.take()?",
    );
    assert_order(
        selected_activation_publisher_source,
        "surface.staged_selected_thread_activation.take()?",
        "publish_history_window",
    );
    assert_order(
        publish_history_body,
        ".replace_from_turns(surface.execution_details.turns())",
        "surface.pending_thread_activation = None",
    );
    assert_order(
        publish_history_body,
        ".replace_from_turns(surface.execution_details.turns())",
        ".transcript_list_state",
    );
    assert!(
        seeded_surface_body
            .contains("state.stage_selected_thread_activation(ActivationPreparer::prepare")
    );
    assert_order(
        seeded_surface_body,
        "state.stage_selected_thread_activation",
        "state.publish_staged_selected_thread_activation",
    );
    assert!(seeded_surface_body.contains("let published_activation = if let Some(thread)"));
    assert!(seeded_surface_body.contains("(state, published_activation)"));
    assert!(!seeded_surface_body.contains("load_thread_history_window"));
    assert!(
        refresh_reopen_body
            .contains("self.stage_selected_thread_activation(ActivationPreparer::prepare")
    );
    assert_order(
        refresh_reopen_body,
        "self.stage_selected_thread_activation",
        "self.publish_staged_selected_thread_activation",
    );
    assert!(
        refresh_reopen_body
            .contains("let (preserve_staged_selected_thread_activation, published_activation) =")
    );
    assert!(
        refresh_reopen_body.contains(
            "let published_activation = self.publish_staged_selected_thread_activation()"
        )
    );
    assert!(refresh_reopen_body.contains("(published_activation.is_none(), published_activation)"));
    assert!(refresh_reopen_body.contains("published_activation"));
    assert_order(
        refresh_reopen_body,
        "let (preserve_staged_selected_thread_activation, published_activation) =",
        "if !preserve_staged_selected_thread_activation",
    );
    assert!(!refresh_reopen_body.contains("load_thread_history_window"));
    assert!(finish_activation_body.contains("pending_selected_thread_activation_source"));
    assert!(finish_activation_body.contains("ActivationPreparer::prepare"));
    assert!(finish_activation_body.contains("surface.stage_selected_thread_activation"));
    assert!(finish_activation_body.contains("surface.publish_staged_selected_thread_activation"));
    assert!(!finish_activation_body.contains("surface.load_thread_history_window"));
    assert!(!finish_activation_body.contains("ready.execution_target = execution_target.clone()"));
    assert!(finish_activation_body.contains("ThreadActivationFinish::Staged"));
    assert!(finish_activation_body.contains("ThreadActivationFinish::Published"));
    assert!(finish_publication_body.contains("ready.execution_target = execution_target.clone()"));
    assert!(selected_activation_pending_body.contains("self.thread_activation_receiver.is_some()"));
    assert!(
        selected_activation_pending_body.contains("pending_thread_activation_label().is_some()")
    );
    assert!(
        selected_activation_pending_body.contains("staged_selected_thread_activation.is_some()")
    );
    assert!(current_submission_block_body.contains("self.selected_thread_activation_pending()"));
    assert!(current_submission_block_body.contains("kind: \"pending_thread_activation\""));
    assert!(current_new_thread_block_body.contains("self.selected_thread_activation_pending()"));
    assert!(poll_activation_updates_body.contains("ThreadActivationFinish::Staged => {}"));
    assert_order(
        poll_activation_updates_body,
        "ThreadActivationFinish::Published",
        "finish_decision_resolution_parent_activation(true",
    );
    assert!(
        !poll_activation_updates_body
            .contains("matches!(outcome, ThreadActivationOutcome::Activated")
    );
    assert!(!transcript_source.contains("finish_delayed_selected_thread_activation_publication"));
    assert_order(
        graph_thread_ref_body,
        "self.workspace_persistence_for_worker()",
        "surface.begin_thread_activation",
    );
    assert!(shell_poll_body.contains("poll_pending_thread_activation_progress"));
    assert_order(
        shell_poll_body,
        "poll_pending_thread_activation_progress",
        "poll_thread_activation_updates",
    );
    assert!(
        finish_activation_body
            .contains("surface.stage_selected_thread_activation(ActivationPreparer::prepare")
    );
    assert_order(
        finish_activation_body,
        "surface.stage_selected_thread_activation",
        "surface.publish_staged_selected_thread_activation",
    );
    assert!(
        selected_activation_preparation_source.contains("struct StagedSelectedThreadActivation")
    );
}

#[test]
fn selected_thread_activation_worker_uses_assets_only_image_resolver_before_ui_result() {
    let worker_source = include_str!("../src/shell/turn_worker.rs");
    let activation_worker_body =
        rust_function_body(worker_source, "fn run_thread_activation_worker");

    assert!(activation_worker_body.contains("transcript_image_path_resolver_for_workspace_assets"));
    assert!(!activation_worker_body.contains("transcript_image_path_resolver_for_turns"));
    assert_order(
        activation_worker_body,
        "transcript_image_path_resolver_for_workspace_assets",
        "ThreadActivationOutcome::Activated",
    );
}

#[test]
fn existing_thread_activation_prepares_resident_history_before_shell_publish() {
    let activation_source = include_str!("../src/shell/thread_activation.rs");
    let discovery_source = include_str!("../src/shell/discovery.rs");
    let worker_source = include_str!("../src/shell/turn_worker.rs");
    let activation_body = rust_function_body(activation_source, "fn load_existing_thread_direct");
    let loader_body = rust_function_body(activation_source, "pub(crate) fn load_existing_thread");
    let apply_body = rust_function_body(activation_source, "fn apply_initial_thread_resident_page");
    let selected_history_body =
        rust_function_body(discovery_source, "fn load_selected_thread_history");
    let selected_image_resolver_body =
        rust_function_body(discovery_source, "fn selected_thread_image_resolver");
    let worker_body = rust_function_body(worker_source, "fn run_thread_activation_worker");

    assert!(activation_source.contains("struct ThreadActivationLoader"));
    assert!(loader_body.contains("load_existing_thread_direct"));
    assert!(activation_body.contains("initial_thread_resident_page_options()"));
    assert!(!activation_body.contains("initial_thread_history_page_options()"));
    assert!(activation_body.contains("apply_initial_thread_resident_page"));
    assert_order(
        activation_body,
        "backend\n        .list_thread_turns",
        "apply_initial_thread_resident_page",
    );
    assert!(apply_body.contains("loaded_full_page_from_desc_response"));
    assert!(apply_body.contains("validate_initial_resident_page"));
    assert!(!apply_body.contains("loaded_page_from_desc_response"));
    assert_order(
        worker_body,
        "ThreadActivationLoader::load_existing_thread",
        "transcript_image_path_resolver_for_workspace_assets",
    );
    assert_order(
        worker_body,
        "transcript_image_path_resolver_for_workspace_assets",
        "ThreadActivationOutcome::Activated",
    );
    assert!(selected_history_body.contains("ThreadActivationLoader::load_existing_thread"));
    assert_order(
        selected_history_body,
        "ThreadActivationLoader::load_existing_thread",
        "selected_thread_image_resolver",
    );
    assert!(
        selected_image_resolver_body
            .contains("transcript_image_path_resolver_for_workspace_assets")
    );
    assert!(!selected_image_resolver_body.contains("transcript_image_path_resolver_for_turns"));
}

#[test]
fn loaded_history_activation_does_not_schedule_deferred_submit_anchor_scroll() {
    let shell_source = include_str!("../src/shell.rs");
    let selected_activation_publisher_source =
        include_str!("../src/shell/selected_thread_activation/publisher.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let transcript_theme_source = include_str!("../src/shell/render/transcript/theme.rs");
    let item_blocks_source = include_str!("../src/shell/render/transcript/item_blocks.rs");
    let snapshot_source = include_str!("../src/shell/transcript_panel_snapshot.rs");
    let anchor_source = include_str!("../src/shell/transcript_anchor.rs");
    let live_scroll_source = include_str!("../src/shell/transcript_live_scroll.rs");
    let residency_pages_source = include_str!("../src/shell/transcript_residency_pages.rs");
    let live_scroll_detection_source =
        include_str!("../src/shell/transcript_live_scroll_detection.rs");
    let load_history_body = rust_function_body(
        selected_activation_publisher_source,
        "fn publish_history_window",
    );
    let finish_history_page_body = rust_function_body(
        residency_pages_source,
        "fn publish_loaded_thread_history_page",
    );
    let render_body = rust_function_body(transcript_source, "fn render(&mut self");
    let live_effect_body = rust_function_body(transcript_source, "fn apply_live_scroll_effect");
    let prompt_effect_body = rust_function_body(transcript_source, "fn apply_prompt_scroll_effect");
    let prompt_pending_commentary_body = rust_function_body(
        transcript_source,
        "fn apply_prompt_with_pending_commentary_effect",
    );
    let pending_commentary_body = rust_function_body(
        transcript_source,
        "fn apply_pending_commentary_follow_effect",
    );
    let final_effect_body = rust_function_body(transcript_source, "fn apply_final_start_effect");
    let final_read_body = rust_function_body(transcript_source, "fn final_read_trailing_allowance");
    let final_runway_body =
        rust_function_body(transcript_source, "fn final_start_trailing_allowance");
    let final_runway_placement_body =
        rust_function_body(transcript_source, "fn final_start_trailing_placement");
    let narrative_geometry_body =
        rust_function_body(anchor_source, "pub(crate) fn narrative_item_geometries");
    let loaded_history_live_scroll_body = rust_function_body(
        live_scroll_detection_source,
        "pub(super) fn reset_loaded_history_live_scroll",
    );
    let final_detection_body =
        rust_function_body(live_scroll_detection_source, "fn live_scroll_final_message");
    let markdown_style_body = rust_function_body(
        item_blocks_source,
        "pub(super) fn agent_message_markdown_style",
    );

    for source in [
        shell_source,
        transcript_source,
        snapshot_source,
        anchor_source,
        live_scroll_source,
    ] {
        assert!(!source.contains("loaded_history_anchor_pending"));
        assert!(!source.contains("install_loaded_history_transcript_anchor"));
        assert!(!source.contains("TranscriptSubmitAnchor::passive"));
    }
    assert!(!anchor_source.contains("fn passive("));
    assert!(load_history_body.contains("surface.reset_loaded_history_live_scroll();"));
    assert!(loaded_history_live_scroll_body.contains("clear_for_tail_activation"));
    assert!(loaded_history_live_scroll_body.contains("set_loaded_history_final_runway"));
    assert!(
        finish_history_page_body
            .contains("restore_history_page_with_image_resolver_and_partial_mode")
    );
    assert!(live_scroll_source.contains("fn refresh_loaded_history_final_runway"));
    assert!(
        live_scroll_source
            .contains("previous_phase: TranscriptLiveScrollDetachedPhase::TailActivation")
    );
    assert!(!load_history_body.contains("self.transcript_submit_anchor = None;"));
    assert!(!load_history_body.contains("latest_user_prompt_anchor"));
    assert!(!load_history_body.contains("sync_live_transcript_rows"));
    assert!(!load_history_body.contains("scroll_to_reveal_item_end"));
    assert_eq!(load_history_body.matches(".reset(").count(), 1);
    assert!(render_body.contains("apply_live_scroll_effect("));
    assert!(render_body.contains("set_content_anchor_resize_policy"));
    assert!(render_body.contains("snapshot.live_scroll_preserves_anchor_offset"));
    assert!(transcript_source.contains("TranscriptMarkdownRenderUnit::Media"));
    assert!(transcript_source.contains("ExecutionItem::Reasoning(reasoning)"));
    assert!(transcript_source.contains("ExecutionItem::GeneratedImage(_)"));
    assert!(transcript_source.contains("flush_pending_media_geometry"));
    assert!(live_scroll_detection_source.contains("turn.narrative_entries()"));
    assert!(live_scroll_detection_source.contains(".rev()"));
    assert!(final_detection_body.contains("Some(ProtocolPhase::FinalAnswer) | None"));
    assert!(
        markdown_style_body
            .contains("None => InlineMarkdownStyle::base(TranscriptTextRole::AssistantFinal)")
    );
    assert_eq!(
        render_body
            .matches("transcript_list_state.scroll_to(ListOffset")
            .count(),
        0
    );
    assert_order(
        render_body,
        "let trailing_scroll_allowance = apply_live_scroll_effect(",
        "transcript_list_state.set_virtual_trailing_scroll_allowance(trailing_scroll_allowance)",
    );
    assert_order(
        render_body,
        "snapshot.live_scroll.as_ref()",
        "transcript_list_state.set_virtual_trailing_scroll_allowance(trailing_scroll_allowance)",
    );
    assert_order(
        load_history_body,
        "surface.reset_loaded_history_live_scroll();",
        ".reset(surface.transcript_list_item_count())",
    );
    assert_order(
        finish_history_page_body,
        "restore_history_page_with_image_resolver_and_partial_mode",
        "self.replace_transcript_presentation_turn(replacement.index, replacement.turn)",
    );
    assert_order(
        prompt_effect_body,
        "transcript_list_state.scroll_to(ListOffset",
        "cx.defer(move |cx|",
    );
    assert_order(
        prompt_effect_body,
        "cx.defer(move |cx|",
        "view.mark_prompt_reread_applied(&applied_anchor, cx)",
    );
    assert!(prompt_effect_body.contains("let applied_anchor = anchor.clone();"));
    assert!(live_effect_body.contains(
        "TranscriptLiveScrollEffectSnapshot::PromptWithPendingCommentary { prompt, commentary }"
    ));
    assert!(live_effect_body.contains("apply_prompt_with_pending_commentary_effect("));
    assert_order(
        prompt_pending_commentary_body,
        "let prompt_runway = apply_prompt_scroll_effect(",
        "if apply_pending_commentary_follow_effect(",
    );
    assert!(prompt_pending_commentary_body.contains("prompt_runway"));
    assert!(pending_commentary_body.contains("commentary_follow_should_scroll("));
    assert_order(
        pending_commentary_body,
        "commentary_follow_should_scroll(",
        "transcript_list_state.scroll_to(ListOffset",
    );
    assert!(
        pending_commentary_body
            .contains("view.mark_commentary_follow_applied(&applied_anchor, cx)")
    );
    assert_order(
        final_effect_body,
        "transcript_list_state.scroll_to(ListOffset",
        "cx.defer(move |cx|",
    );
    assert_order(
        final_effect_body,
        "cx.defer(move |cx|",
        "view.mark_final_start_applied(&applied_anchor, applied_scroll_offset, cx)",
    );
    assert!(final_effect_body.contains("let applied_anchor = anchor.clone();"));
    assert!(final_effect_body.contains("let applied_scroll_offset = final_start.scroll_offset;"));
    assert!(!anchor_source.contains("const FINAL_START_TOP_PAINT_GUARD"));
    assert!(anchor_source.contains("const FINAL_START_MIN_TOP_GUARD: f32 = 8.0;"));
    assert!(anchor_source.contains("const FINAL_START_LINE_HEIGHT_GUARD_RATIO: f32 = 0.25;"));
    assert!(anchor_source.contains("fn final_start_top_paint_guard("));
    assert!(anchor_source.contains("final_start_top_paint_guard(geometry.first_line_height)"));
    assert!(anchor_source.contains("first_line_height: item.first_line_height"));
    assert!(anchor_source.contains("let prompt_width = prompt_text_width(transcript_width);"));
    assert!(
        anchor_source
            .contains("let assistant_width = assistant_narrative_text_width(transcript_width);")
    );
    assert!(narrative_geometry_body.contains(
        "TranscriptNarrativeBlockPlan::UserPrompt { plan } => {\n                let mut measurer = WindowPromptMeasurer::new(themes.prompt, window);\n                let layout = prompt_markdown_layout_from_plan(\n                    plan,\n                    prompt_width,"
    ));
    assert!(narrative_geometry_body.contains(
        "TranscriptNarrativeBlockPlan::AssistantMarkdown {\n                item_id,\n                plan,\n                role,\n            } => {\n                let mut measurer = WindowPromptMeasurer::new(themes.theme_for(*role), window);\n                let layout = prompt_markdown_layout_from_plan(\n                    plan,\n                    assistant_width,"
    ));
    assert!(transcript_source.contains("&theme.prompt_anchor_theme()"));
    assert!(
        transcript_source.contains("theme.text_anchor_theme(TranscriptTextRole::AssistantFinal)")
    );
    assert!(transcript_source.contains("agent_message_narrative_anchor_role(message.phase)"));
    assert!(transcript_theme_source.contains("pub(crate) fn prompt_anchor_theme(&self)"));
    assert!(
        transcript_theme_source
            .contains("pub(crate) fn text_anchor_theme(&self, role: TranscriptTextRole)")
    );
    assert!(transcript_theme_source.contains("TranscriptTextRole::AssistantFinal =>"));
    assert!(
        transcript_theme_source
            .contains("&self.paragraph, TranscriptInlineCodeHost::AssistantFinal")
    );
    assert!(final_read_body.contains("placement.scroll_offset != anchor.applied_scroll_offset"));
    assert!(final_read_body.contains(
        "transcript_list_state.scroll_position() == ListScrollPosition::Content(previous_target)"
    ));
    assert!(live_scroll_source.contains("last_final_anchor"));
    assert!(live_scroll_source.contains("set_passive_final_runway"));
    assert!(live_scroll_source.contains("refresh_loaded_history_final_runway"));
    assert!(live_scroll_source.contains("TranscriptLiveScrollEffectSnapshot::FinalRunway"));
    assert!(live_scroll_detection_source.contains("fn set_loaded_history_final_runway"));
    assert!(live_scroll_detection_source.contains("set_passive_final_runway"));
    assert!(
        live_scroll_detection_source.contains("self.transcript_presentation.len().checked_sub(1)")
    );
    assert!(live_effect_body.contains(
        "TranscriptLiveScrollEffectSnapshot::FinalRunway(anchor) => final_start_trailing_allowance("
    ));
    assert_order(
        live_scroll_source,
        "if let Some(anchor) = &self.last_final_anchor",
        "self.last_prompt_anchor.as_ref()",
    );
    assert!(final_runway_body.contains("final_start_trailing_placement("));
    assert!(!final_runway_body.contains("scroll_to("));
    assert!(!final_runway_body.contains("mark_final_start_applied"));
    assert!(!final_runway_placement_body.contains("scroll_to("));
    assert!(!final_runway_placement_body.contains("mark_final_start_applied"));
    assert_order(
        final_read_body,
        "if placement.scroll_offset != anchor.applied_scroll_offset",
        "transcript_list_state.scroll_to(target)",
    );
    assert_order(
        final_read_body,
        "transcript_list_state.scroll_to(target)",
        "view.mark_final_start_applied(&applied_anchor, applied_scroll_offset, cx)",
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
        "self\n                    .backend_servers\n                    .insert(opened.execution_target.clone(), opened.server)",
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
    let active_title_body =
        rust_function_body(render_source, "fn render_thread_strip_active_thread_title");
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
    assert!(thread_strip_body.contains("pending_thread_activation_progress"));
    assert!(thread_strip_body.contains("pending_thread_activation_label"));
    assert!(thread_strip_body.contains("selected_thread_title_label"));
    assert!(thread_strip_body.contains("&& pending_activation_progress.is_none()"));
    assert!(active_title_body.contains("pending_activation_progress.is_some()"));
    assert!(active_title_body.contains(".when_some(pending_activation_progress"));
    assert!(active_title_body.contains("BerylThemeRole::PrimitiveAccentMarker"));
    assert!(active_title_body.contains(".w(relative(progress.clamp(0.0, 1.0)))"));
    assert!(!thread_strip_body.contains("thread_strip_breadcrumb_trail"));
    assert!(!breadcrumb_source_body.contains("pending_thread_activation_label().is_some()"));
    assert!(!breadcrumb_source_body.contains("pending_thread_activation_progress"));
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
fn transcript_prepaint_reports_render_facts_without_residency_requests() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");

    assert!(!transcript_source.contains("begin_transcript_residency_update_for_scroll_event"));
    assert!(!transcript_source.contains("view.preload_transcript_media_range"));
    assert!(transcript_source.contains("view.report_transcript_media_preload_facts(request);"));
    assert!(transcript_source.contains("window.defer(cx, move |window, cx|"));
    assert!(transcript_source.contains("view.drain_transcript_media_preload_coordinator"));
    assert!(transcript_source.contains("view.finish_text_span_frame"));
    assert!(transcript_source.contains("!visible_range.contains(index)"));
}

#[test]
fn transcript_media_render_consumes_displayed_media_without_scheduling_loads() {
    let media_cache_source = include_str!("../src/shell/render/transcript/media_cache.rs");
    let media_blocks_source = include_str!("../src/shell/render/transcript/media_blocks.rs");

    let media_for_body = rust_function_body(media_cache_source, "pub(super) fn media_for");
    let preload_media_for_body =
        rust_function_body(media_cache_source, "pub(super) fn preload_media_for");
    let lookup_media_body = rust_function_body(media_cache_source, "fn lookup_media");
    let render_media_run_body =
        rust_function_body(media_blocks_source, "pub(super) fn render_media_run");
    let preload_media_run_body =
        rust_function_body(media_blocks_source, "pub(super) fn preload_media_run");
    let preload_media_item_body = rust_function_body(media_blocks_source, "fn preload_media_item");

    assert!(render_media_run_body.contains("context.media_for"));
    assert!(!render_media_run_body.contains("preload_media_for"));
    assert!(media_for_body.contains("displayed_outcome"));
    assert!(!media_for_body.contains("lookup_media("));
    assert!(!media_for_body.contains("schedule_media_load"));

    assert!(preload_media_for_body.contains("lookup_media"));
    assert!(lookup_media_body.contains("schedule_media_load"));
    assert!(preload_media_run_body.contains("context.preload_media_for"));
    assert!(preload_media_run_body.contains("context.media_for"));
    assert!(preload_media_item_body.contains("source_backed_image_request_status"));
}

#[test]
fn huge_turn_render_uses_block_window_slices() {
    let turn_blocks_source = include_str!("../src/shell/render/transcript/turn_blocks.rs");
    let block_markdown_source = include_str!("../src/shell/render/transcript/block_markdown.rs");

    let card_body = rust_function_body(turn_blocks_source, "pub(super) fn render_turn_card");
    let block_window_body =
        rust_function_body(turn_blocks_source, "fn render_turn_card_block_window");

    assert!(card_body.contains("render_window(row_scroll_offset, viewport_height)"));
    assert!(card_body.contains("render_turn_card_block_window"));
    assert!(card_body.contains("render_turn_card_full"));
    assert!(block_window_body.contains("intersect_local_range"));
    assert!(block_window_body.contains("render_block_spacer"));
    assert!(turn_blocks_source.contains("markdown_render_units(&markdown_key"));
    assert!(turn_blocks_source.contains("TranscriptMarkdownRenderUnit::Media"));
    assert!(turn_blocks_source.contains("TranscriptMediaRenderIdentity::new"));
    assert!(turn_blocks_source.contains("render_user_prompt_markdown_source_slice"));
    assert!(turn_blocks_source.contains("render_item_markdown_source_slice"));
    assert!(block_markdown_source.contains("fn render_markdown_plan_slice"));
    assert!(block_markdown_source.contains("render_block_sequence_range"));
}

#[test]
fn completed_media_admission_is_window_backed_for_residency_pages_only() {
    let shell_source = include_str!("../src/shell.rs");
    let selected_activation_preparation_source =
        include_str!("../src/shell/selected_thread_activation/preparation.rs");
    let residency_pages_source = include_str!("../src/shell/transcript_residency_pages.rs");
    let admission_source = include_str!("../src/shell/transcript_media_admission.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let admission_driver_source = include_str!("../src/shell/render/transcript/media_admission.rs");
    let media_blocks_source = include_str!("../src/shell/render/transcript/media_blocks.rs");
    let diagnostics_source = include_str!("../src/shell/diagnostics.rs");

    let stage_residency_body = rust_function_body(
        residency_pages_source,
        "fn stage_loading_thread_history_page",
    );
    let drain_body = rust_function_body(
        transcript_source,
        "fn drain_staged_transcript_media_admission",
    );

    assert!(shell_source.contains("mod transcript_media_admission;"));
    assert!(
        !selected_activation_preparation_source
            .contains("media_admission: TranscriptMediaAdmissionWindow")
    );
    assert!(
        !selected_activation_preparation_source
            .contains("TranscriptMediaAdmissionWindow::from_selected_thread_activation")
    );
    assert!(stage_residency_body.contains("TranscriptMediaAdmissionWindow::from_history_page"));
    assert!(admission_source.contains("staged_transcript_media_admission_request"));
    assert!(admission_source.contains("note_staged_transcript_media_admission_summary"));
    assert!(admission_source.contains("requires_completed_media_admission"));
    assert!(admission_source.contains("pub(crate) fn requires_retry"));
    assert!(admission_source.contains("estimated_required_media_item_count"));

    assert!(transcript_source.contains("mod media_admission;"));
    assert!(transcript_source.contains("TranscriptWindowMediaAdmissionDriver"));
    assert!(drain_body.contains("surface.staged_transcript_media_admission_request()"));
    assert!(drain_body.contains("self.media_admission.drain_pending("));
    assert!(drain_body.contains("markdown_context"));
    assert!(drain_body.contains("stream_projection_context"));
    assert!(drain_body.contains("note_staged_transcript_media_admission_summary"));
    assert!(!admission_source.contains("staged_selected_thread_activation"));
    assert!(drain_body.contains("publish_staged_thread_history_page"));
    assert!(!drain_body.contains("publish_staged_selected_thread_activation"));
    assert!(drain_body.contains("publish_staged_thread_history_page"));
    assert!(!drain_body.contains("finish_delayed_selected_thread_activation_publication"));
    assert!(transcript_source.contains("empty_state_staged_admission_entity"));
    assert_order(
        transcript_source,
        "empty_state_staged_admission_entity",
        "view.drain_staged_transcript_media_admission",
    );
    assert_order(
        transcript_source,
        "empty_state_staged_admission_entity",
        ".when(has_turns",
    );
    assert_order(
        transcript_source,
        "view.drain_staged_transcript_media_admission",
        "view.drain_transcript_media_preload_coordinator",
    );
    assert!(drain_body.contains("let mut media_requires_retry = false;"));
    assert!(drain_body.contains("media_requires_retry = summary.requires_retry();"));
    assert_order(
        drain_body,
        "surface.note_staged_transcript_media_admission_summary",
        "media_requires_retry = summary.requires_retry();",
    );
    assert!(drain_body.contains("preparation_requires_retry || media_requires_retry"));

    assert!(admission_driver_source.contains("source_backed_image_request_status"));
    assert!(admission_driver_source.contains("preload_source_backed_image"));
    assert!(admission_driver_source.contains("AdmissionMediaItemDrain::RetryCurrent"));
    assert!(admission_driver_source.contains("note_source_backed_upload_admission"));
    assert!(admission_driver_source.contains("SourceBackedUploadAdmissionDecision"));
    assert!(admission_source.contains("requested_upload_bytes > max_upload_bytes"));
    assert!(admission_source.contains("SourceBackedUploadAdmissionDecision::TerminalFallback"));
    assert!(admission_source.contains("SourceBackedUploadAdmissionDecision::RetryCurrent"));
    assert!(admission_driver_source.contains("TranscriptMediaReadinessKey"));
    assert!(admission_driver_source.contains("leased_source_backed"));
    assert!(admission_driver_source.contains("SourceBackedImageRequestStatus::Live"));
    assert!(admission_driver_source.contains("SourceBackedImageRequestStatus::ReadyForUpload"));
    assert!(admission_driver_source.contains("SourceBackedImageRequestStatus::BudgetDeferred"));
    assert!(admission_driver_source.contains("TranscriptMediaAdmissionSummary"));
    assert!(admission_driver_source.contains("markdown_render_units"));
    assert!(admission_driver_source.contains("used_parser_fallback"));
    assert!(admission_driver_source.contains("AdmissionRowPlan::Pending"));
    assert!(admission_driver_source.contains("note_unprocessed_estimated_items"));

    assert!(!media_blocks_source.contains("TranscriptWindowMediaAdmissionDriver"));
    assert!(!diagnostics_source.contains("TranscriptWindowMediaAdmissionDriver"));
}

#[test]
fn selected_thread_publication_is_not_gated_by_renderer_media_admission() {
    let selected_activation_preparation_source =
        include_str!("../src/shell/selected_thread_activation/preparation.rs");
    let publish_activation_body = rust_function_body(
        selected_activation_preparation_source,
        "fn is_ready_for_publication",
    );
    let progress_cap_body =
        rust_function_body(selected_activation_preparation_source, "fn progress_cap");

    assert!(publish_activation_body.contains("presentability.structural_readiness_settled()"));
    assert!(!publish_activation_body.contains("prepublication_preparation"));
    assert!(!publish_activation_body.contains("media_admission.is_settled_for_publication()"));
    assert!(!progress_cap_body.contains("preparation_progress"));
    assert!(!progress_cap_body.contains("media_progress"));
}

#[test]
fn prepublication_preparation_is_bounded_and_staged_for_residency_pages_only() {
    let shell_source = include_str!("../src/shell.rs");
    let selected_activation_preparation_source =
        include_str!("../src/shell/selected_thread_activation/preparation.rs");
    let residency_pages_source = include_str!("../src/shell/transcript_residency_pages.rs");
    let preparation_source = include_str!("../src/shell/transcript_prepublication_preparation.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");

    let stage_residency_body = rust_function_body(
        residency_pages_source,
        "fn stage_loading_thread_history_page",
    );
    let drain_body = rust_function_body(
        transcript_source,
        "fn drain_staged_transcript_media_admission",
    );
    let publish_activation_body = rust_function_body(
        selected_activation_preparation_source,
        "fn is_ready_for_publication",
    );
    let publish_page_body =
        rust_function_body(residency_pages_source, "fn is_ready_for_publication");

    assert!(shell_source.contains("mod transcript_prepublication_preparation;"));
    assert!(
        !selected_activation_preparation_source
            .contains("prepublication_preparation: TranscriptPrepublicationPreparationWindow")
    );
    assert!(
        residency_pages_source
            .contains("prepublication_preparation: TranscriptPrepublicationPreparationWindow")
    );
    assert!(
        !selected_activation_preparation_source
            .contains("TranscriptPrepublicationPreparationWindow::from_selected_thread_activation")
    );
    assert!(
        stage_residency_body
            .contains("TranscriptPrepublicationPreparationWindow::from_history_page")
    );
    assert!(preparation_source.contains("TranscriptPrepublicationPreparationLayout"));
    assert!(preparation_source.contains("TranscriptPrepublicationPreparedRow"));
    assert!(preparation_source.contains("max_rows_per_drain"));
    assert!(preparation_source.contains("max_block_units_per_drain"));
    assert!(preparation_source.contains("max_media_items_per_drain"));
    assert!(preparation_source.contains("max_preparation_bytes_per_drain"));
    assert!(preparation_source.contains("max_in_flight_preparation_passes"));
    assert!(preparation_source.contains("prepared_row_count_for_layout"));
    assert!(preparation_source.contains("can_start_row"));

    assert!(transcript_source.contains("TranscriptPrepublicationPreparationDriver"));
    assert!(transcript_source.contains("TranscriptPrepublicationPreparationLayout::new"));
    assert!(drain_body.contains("staged_transcript_prepublication_preparation_request"));
    assert!(drain_body.contains("self.prepublication_preparation.drain_pending"));
    assert!(drain_body.contains("note_staged_transcript_prepublication_preparation_summary"));
    assert!(!preparation_source.contains("staged_selected_thread_activation"));
    assert!(preparation_source.contains("Option<TranscriptPrepublicationPreparationSummary>"));
    assert!(drain_body.contains("let mut preparation_requires_retry = false;"));
    assert!(drain_body.contains("preparation_requires_retry = summary.requires_retry();"));
    assert_order(
        drain_body,
        "note_staged_transcript_prepublication_preparation_summary",
        "preparation_requires_retry = summary.requires_retry();",
    );
    assert_order(
        drain_body,
        "note_staged_transcript_prepublication_preparation_summary",
        "publish_staged_thread_history_page",
    );
    assert!(!publish_activation_body.contains("prepublication_preparation"));
    assert!(
        publish_page_body.contains("prepublication_preparation")
            && publish_page_body.contains("is_settled_for_publication")
    );
}

#[test]
fn staged_history_page_publication_defers_residency_followup_from_transcript_panel_update() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let drain_body = rust_function_body(
        transcript_source,
        "fn drain_staged_transcript_media_admission",
    );

    let defer_index = drain_body
        .find("window.defer(cx, move |window, cx|")
        .expect("staged publication follow-up should defer shell work");
    let residency_update_index = drain_body
        .find("view.begin_transcript_residency_update_for_current_view")
        .expect("staged publication should still schedule residency follow-up");
    let synchronous_shell_update = &drain_body[..defer_index];

    assert!(
        drain_body
            .contains("let (defer_residency_update, retry_staged_admission) = self.shell.update")
    );
    assert!(drain_body.contains("defer_residency_update = true;"));
    assert!(drain_body.contains("if retry_staged_admission"));
    assert!(defer_index < residency_update_index);
    assert!(
        !synchronous_shell_update
            .contains("view.begin_transcript_residency_update_for_current_view")
    );
    assert_order(
        drain_body,
        "surface.publish_staged_thread_history_page",
        "defer_residency_update = true;",
    );
    assert!(!drain_body.contains("finish_delayed_selected_thread_activation_publication"));
    assert_order(drain_body, "if defer_residency_update", "window.defer(cx");
    assert_order(
        drain_body,
        "let (defer_residency_update, retry_staged_admission) = self.shell.update",
        "if retry_staged_admission",
    );
    assert_order(
        drain_body,
        "if retry_staged_admission",
        "if defer_residency_update",
    );
    assert_order(
        drain_body,
        "window.defer(cx",
        "view.begin_transcript_residency_update_for_current_view",
    );
}

#[test]
fn transcript_focus_routes_turn_jump_keybindings() {
    let shell_source = include_str!("../src/shell.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");

    assert!(shell_source.contains("KeyBinding::new(\"ctrl-up\", JumpTranscriptTurnUp"));
    assert!(shell_source.contains("\"ctrl-down\""));
    assert!(transcript_source.contains("KeyBinding::new(\n            \"ctrl-up\""));
    assert!(transcript_source.contains("JumpTranscriptTurnUp"));
    assert!(transcript_source.contains("KeyBinding::new(\n            \"ctrl-down\""));
    assert!(transcript_source.contains("JumpTranscriptTurnDown"));
    assert!(transcript_source.contains("Self::jump_transcript_turn_up_action"));
    assert!(transcript_source.contains("Self::jump_transcript_turn_down_action"));
    assert!(transcript_source.contains("shell.jump_transcript_turn_up_action"));
    assert!(transcript_source.contains("shell.jump_transcript_turn_down_action"));
}

#[test]
fn transcript_history_load_and_unload_paths_log_turn_counts() {
    let shell_source = include_str!("../src/shell.rs");
    let selected_activation_publisher_source =
        include_str!("../src/shell/selected_thread_activation/publisher.rs");
    let logging_source = include_str!("../src/shell/transcript_residency_logging.rs");
    let residency_pages_source = include_str!("../src/shell/transcript_residency_pages.rs");

    let load_window_body = rust_function_body(
        selected_activation_publisher_source,
        "fn publish_history_window",
    );
    let finish_page_body = rust_function_body(
        residency_pages_source,
        "fn publish_loaded_thread_history_page",
    );
    let release_body = rust_function_body(
        residency_pages_source,
        "fn release_resident_turn_payloads_for_plan",
    );

    assert!(shell_source.contains("mod transcript_residency_logging;"));
    assert!(load_window_body.contains("log_transcript_resident_turns_admitted"));
    assert!(load_window_body.contains("note_transcript_residency_admission"));
    assert!(load_window_body.contains("\"initial\""));
    assert!(finish_page_body.contains("log_transcript_resident_turns_admitted"));
    assert!(finish_page_body.contains("note_transcript_residency_admission"));
    assert!(residency_pages_source.contains("\"older\""));
    assert!(residency_pages_source.contains("\"released\""));
    assert!(release_body.contains("log_transcript_resident_turns_released"));
    assert!(release_body.contains("release_history_turns_by_id"));
    assert!(release_body.contains("note_transcript_content_release"));
    assert!(logging_source.contains("use tracing::info;"));
    assert!(logging_source.contains("\"Fetched transcript transport page\""));
    assert!(logging_source.contains("\"Admitted transcript resident turns\""));
    assert!(logging_source.contains("\"Released transcript resident turns\""));
    assert!(logging_source.contains("\"Planned transcript residency target\""));
    assert!(logging_source.contains("transport_turns"));
    assert!(logging_source.contains("admitted_turns"));
    assert!(logging_source.contains("released_turns"));
}

#[test]
fn transcript_scroll_routes_boundary_work_through_residency_controller() {
    let shell_source = include_str!("../src/shell.rs");
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let conversation_render_source = include_str!("../src/shell/render/conversation.rs");
    let scroll_body = rust_function_body(shell_source, "fn apply_transcript_scroll_command");
    let scroll_event_body = rust_function_body(shell_source, "fn note_transcript_scroll_event");
    let scrollbar_activity_body = rust_function_body(shell_source, "fn note_scrollbar_activity");
    let notify_scrollbar_region_body =
        rust_function_body(shell_source, "fn notify_scrollbar_region");
    let surface_scroll_body = rust_function_body(shell_source, "fn set_transcript_user_scrolled");

    assert!(scroll_body.contains("self.note_transcript_scroll_event(&event, window, cx);"));
    assert!(!scroll_body.contains("self.notify_transcript_panel(cx);"));
    assert!(
        !scroll_body.contains("self.begin_transcript_residency_update_for_scroll_event(&event")
    );
    assert!(
        scroll_event_body.contains(
            "self.begin_transcript_residency_update_for_scroll_event(event, window, cx);"
        )
    );
    assert!(
        scroll_event_body.contains("self.note_transcript_scroll(event.is_scrolled, window, cx);")
    );
    assert!(scroll_event_body.contains("note_transcript_content_scroll_signature(event)"));
    assert!(scroll_event_body.contains("self.notify_transcript_panel(cx);"));
    assert!(!scroll_event_body.contains("self.note_transcript_scroll(true, window, cx);"));
    assert!(shell_source.contains("last_transcript_content_scroll_signature"));
    assert!(shell_source.contains("boundary_state_for_visible_range"));
    assert!(conversation_render_source.contains("render_interactive_vertical_scrollbar("));
    assert!(conversation_render_source.contains("note_transcript_scrollbar_owner_update"));
    assert!(!transcript_source.contains("render_interactive_vertical_scrollbar("));
    assert!(scrollbar_activity_body.contains("record_viewport_activity"));
    assert!(!scrollbar_activity_body.contains("notify_scrollbar_region(&region"));
    assert!(!notify_scrollbar_region_body.contains("notify_transcript_panel"));
    assert!(surface_scroll_body.contains("if is_scrolled {"));
    assert!(surface_scroll_body.contains("self.release_transcript_submit_anchor()"));
    assert!(transcript_source.contains(".on_scroll_wheel({"));
    assert!(transcript_source.contains(
        "view.release_transcript_submit_anchor(cx);\n                                                view.note_scrollbar_activity("
    ) || transcript_source.contains(
        "view.release_transcript_submit_anchor(cx);\r\n                                                view.note_scrollbar_activity("
    ));
}

#[test]
fn transcript_row_renderer_uses_layout_context_without_reentrant_list_state_reads() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let render_body = rust_function_body(transcript_source, "fn render(&mut self");
    let list_start = render_body
        .find("list(transcript_list_state.clone(), move |index, row_context, _, cx| {")
        .expect("missing transcript list row renderer");
    let list_region = &render_body[list_start..];
    let list_end = list_region
        .find("\n                                        })\n                                        .size_full()")
        .expect("missing transcript list row renderer end");
    let row_renderer_body = &list_region[..list_end];

    assert!(!render_body.contains("list_row_scroll_top"));
    assert!(!render_body.contains("list_row_viewport_height"));
    assert!(!row_renderer_body.contains("logical_scroll_top()"));
    assert!(!row_renderer_body.contains("viewport_bounds()"));
    assert!(row_renderer_body.contains("row_context.scroll_offset_in_item"));
    assert!(row_renderer_body.contains("row_context.viewport_height"));
}

#[test]
fn transcript_row_renderer_uses_row_local_selection_and_copy_counters() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let render_body = rust_function_body(transcript_source, "fn render(&mut self");
    let list_start = render_body
        .find("list(transcript_list_state.clone(), move |index, row_context, _, cx| {")
        .expect("missing transcript list row renderer");
    let list_region = &render_body[list_start..];
    let list_end = list_region
        .find("\n                                        })\n                                        .size_full()")
        .expect("missing transcript list row renderer end");
    let row_renderer_body = &list_region[..list_end];

    assert!(!render_body.contains("let selection_order = Rc::new(Cell::new(0usize));"));
    assert!(!render_body.contains("let narrative_copy_block_count = Rc::new(Cell::new(0usize));"));
    assert!(row_renderer_body.contains("TranscriptTextLineOrder::row_start(index)"));
    assert!(row_renderer_body.contains("transcript_row_initial_narrative_copy_block_count"));
}

#[test]
fn transcript_scroll_residency_worker_loads_full_pages_before_range_extension() {
    let shell_source = include_str!("../src/shell.rs");
    let history_worker_source = include_str!("../src/shell/thread_history_worker.rs");
    let history_source = include_str!("../src/shell/transcript_history.rs");

    let residency_body = rust_function_body(
        shell_source,
        "fn begin_transcript_residency_update_for_scroll_event",
    );

    assert!(residency_body.contains("spawn_thread_residency_page_worker"));
    assert!(!residency_body.contains("spawn_older_thread_history_page_worker"));
    assert!(history_worker_source.contains("load_thread_resident_history_page"));
    assert!(!history_worker_source.contains("load_thread_history_page("));
    assert!(history_source.contains("pub(crate) fn thread_resident_history_page_options"));
    assert!(history_source.contains(".with_items_view(TurnItemsView::Full)"));
}

#[test]
fn transcript_residency_page_worker_stages_before_publication() {
    let shell_source = include_str!("../src/shell.rs");
    let lifecycle_source = include_str!("../src/shell/lifecycle.rs");
    let controller_source = include_str!("../src/shell/transcript_residency_controller.rs");
    let residency_pages_source = include_str!("../src/shell/transcript_residency_pages.rs");

    let scroll_body = rust_function_body(
        shell_source,
        "fn begin_transcript_residency_update_for_scroll_event",
    );
    let controller_body = rust_function_body(
        controller_source,
        "pub(super) fn begin_transcript_residency_controller_update",
    );
    let controller_request_body = rust_function_body(
        controller_source,
        "fn begin_loading_thread_history_page_for_residency_plan",
    );
    let controller_facts_body = rust_function_body(
        controller_source,
        "fn transcript_residency_controller_facts",
    );
    let poll_body = rust_function_body(shell_source, "fn poll_thread_history_page_updates");
    let worker_finish_body =
        rust_function_body(lifecycle_source, "fn finish_thread_history_page_worker");
    let stage_body = rust_function_body(
        residency_pages_source,
        "fn stage_loading_thread_history_page",
    );
    let publish_staged_body = rust_function_body(
        residency_pages_source,
        "fn publish_staged_thread_history_page",
    );
    let publish_loaded_body = rust_function_body(
        residency_pages_source,
        "fn publish_loaded_thread_history_page",
    );
    let current_body = rust_function_body(
        residency_pages_source,
        "fn transcript_residency_page_request_is_current",
    );

    assert!(shell_source.contains("pending_thread_history_page_request"));
    assert!(shell_source.contains("staged_transcript_residency_page"));
    assert!(scroll_body.contains("begin_transcript_residency_controller_update"));
    assert!(scroll_body.contains("let request = request_ticket.request().clone();"));
    assert!(controller_body.contains("residency_target_plan"));
    assert!(controller_body.contains("release_resident_turn_payloads_for_plan"));
    assert!(
        controller_body.contains(
            "let released_content = self.release_resident_turn_payloads_for_plan(&plan);"
        )
    );
    assert!(!controller_body.contains("release_cold_history_pages"));
    assert!(controller_body.contains("begin_loading_thread_history_page_for_residency_plan"));
    assert_order(
        controller_body,
        "let released_content = self.release_resident_turn_payloads_for_plan(&plan);",
        "let request = allow_request",
    );
    assert!(controller_facts_body.contains("request_allowed"));
    assert!(controller_request_body.contains("begin_loading_page_for_residency_target_plan"));
    assert!(controller_request_body.contains("note_transcript_residency_request_started"));
    assert_order(
        scroll_body,
        "self.pending_thread_history_page_request = Some(request_ticket);",
        "spawn_thread_residency_page_worker",
    );
    assert!(poll_body.contains("let request = self.pending_thread_history_page_request.take();"));
    assert!(poll_body.contains("self.finish_thread_history_page_worker(outcome, request);"));

    assert!(worker_finish_body.contains("stage_loading_thread_history_page"));
    assert!(worker_finish_body.contains("publish_staged_thread_history_page"));
    assert!(worker_finish_body.contains("request_matches_thread"));
    assert_order(
        worker_finish_body,
        "surface.stage_loading_thread_history_page",
        "surface.publish_staged_thread_history_page();",
    );
    assert!(!worker_finish_body.contains("publish_loaded_thread_history_page"));

    assert!(stage_body.contains("transcript_residency_page_request_is_current"));
    assert!(stage_body.contains("latest_transcript_residency_controller_facts"));
    assert!(stage_body.contains("staged_transcript_residency_page = Some"));
    assert!(!stage_body.contains("finish_loading_older_with_turn_ids"));
    assert!(!stage_body.contains("finish_loading_released_page"));
    assert!(!stage_body.contains("fail_loading_older"));
    assert!(!stage_body.contains("prepend_thread_history_page_with_image_resolver"));
    assert!(!stage_body.contains("restore_history_page_with_image_resolver"));
    assert!(!stage_body.contains("log_transcript_turns_loaded"));
    assert!(stage_body.contains("log_transcript_transport_page_received"));
    assert!(stage_body.contains("note_transcript_residency_staged_admission"));
    assert!(publish_staged_body.contains("staged_transcript_residency_page.as_ref()"));
    assert!(publish_staged_body.contains("if !staged.is_ready_for_publication()"));
    assert!(publish_staged_body.contains("staged_transcript_residency_page.take()"));
    assert_order(
        publish_staged_body,
        "if !staged.is_ready_for_publication()",
        "staged_transcript_residency_page.take()",
    );
    assert!(publish_staged_body.contains("self.publish_loaded_thread_history_page(staged)"));
    assert!(publish_loaded_body.contains("prepend_thread_history_page_with_image_resolver"));
    assert!(publish_loaded_body.contains("restore_history_page_with_image_resolver"));
    assert!(publish_loaded_body.contains("log_transcript_resident_turns_admitted"));
    assert!(publish_loaded_body.contains("note_transcript_residency_admission"));
    assert!(current_body.contains("selected_thread_id()"));
    assert!(current_body.contains("cancellation_generation"));
    assert!(current_body.contains("loading_page_matches_request"));
}

#[test]
fn transcript_residency_controller_gathers_bounded_scroll_facts() {
    let controller_source = include_str!("../src/shell/transcript_residency_controller.rs");
    let history_source = include_str!("../src/shell/transcript_history.rs");
    let begin_body = rust_function_body(
        controller_source,
        "pub(super) fn begin_transcript_residency_controller_update",
    );
    let facts_body = rust_function_body(
        controller_source,
        "fn transcript_residency_controller_facts",
    );
    let planning_range_body = rust_function_body(
        controller_source,
        "fn transcript_residency_planning_presentation_range",
    );
    let bounded_plan_body = rust_function_body(
        history_source,
        "pub(crate) fn residency_target_plan_for_source_window",
    );

    assert!(begin_body.contains("transcript_residency_planning_presentation_range"));
    assert!(begin_body.contains("sync_transcript_residency_derived_byte_estimates"));
    assert!(begin_body.contains("residency_target_plan_for_source_window"));
    assert!(planning_range_body.contains("range_with_vertical_margin"));
    assert!(facts_body.contains("presentation_range_for_source_range"));
    assert!(controller_source.contains("derived_byte_estimates_by_turn_id_for_range"));
    assert!(!controller_source.contains("derived_byte_estimates_by_turn_id()"));
    assert!(!facts_body.contains("0..self.transcript_presentation.len()"));
    assert!(!facts_body.contains("resident_turn_ids()"));
    assert!(!facts_body.contains("pinned_turn_ids()"));
    assert!(!facts_body.contains("indexed_turns().len()"));
    assert!(bounded_plan_body.contains("indexed_turns_for_source_range_and_required"));
    assert!(bounded_plan_body.contains("unpinned_resident_turns_outside_source_range"));
    assert!(bounded_plan_body.contains("cached_pinned_turn_ids"));
    assert!(!bounded_plan_body.contains(".indexed_turns()"));
    assert!(!bounded_plan_body.contains(".pinned_turn_ids()"));
}

#[test]
fn transcript_residency_controller_defers_until_activation_viewport_is_authoritative() {
    let controller_source = include_str!("../src/shell/transcript_residency_controller.rs");
    let begin_body = rust_function_body(
        controller_source,
        "pub(super) fn begin_transcript_residency_controller_update",
    );
    let deferred_body = rust_function_body(
        controller_source,
        "fn transcript_residency_controller_update_deferred",
    );
    let viewport_body = rust_function_body(
        controller_source,
        "fn transcript_residency_controller_viewport_unready",
    );

    assert!(begin_body.contains("transcript_residency_controller_update_deferred"));
    assert!(begin_body.contains("return TranscriptResidencyControllerUpdate::default();"));
    assert_order(
        begin_body,
        "transcript_residency_controller_update_deferred",
        "transcript_residency_planning_presentation_range",
    );
    assert_order(
        begin_body,
        "transcript_residency_controller_update_deferred",
        "sync_transcript_residency_derived_byte_estimates",
    );
    assert_order(
        begin_body,
        "transcript_residency_controller_update_deferred",
        "residency_target_plan_for_source_window",
    );
    assert_order(
        begin_body,
        "transcript_residency_controller_update_deferred",
        "release_resident_turn_payloads_for_plan",
    );
    assert!(deferred_body.contains("self.staged_selected_thread_activation.is_some()"));
    assert!(deferred_body.contains("transcript_residency_controller_viewport_unready"));
    assert!(viewport_body.contains("self.transcript_presentation.len() > 0"));
    assert!(viewport_body.contains("presentation_visible_range.is_empty()"));
    assert!(viewport_body.contains("viewport_bounds().size.height"));
}

#[test]
fn row_presentability_model_is_staged_outside_render_paths() {
    let shell_source = include_str!("../src/shell.rs");
    let selected_activation_preparation_source =
        include_str!("../src/shell/selected_thread_activation/preparation.rs");
    let residency_pages_source = include_str!("../src/shell/transcript_residency_pages.rs");
    let presentability_source = include_str!("../src/shell/transcript_presentability.rs");
    let transcript_render_source = include_str!("../src/shell/render/transcript.rs");
    let transcript_snapshot_source = include_str!("../src/shell/transcript_panel_snapshot.rs");
    let status_line_source = include_str!("../src/shell/status_line.rs");
    let scrollbars_source = include_str!("../src/shell/render/scrollbars.rs");

    let staged_activation_new_body = rust_function_body(
        selected_activation_preparation_source,
        "pub(in crate::shell) fn prepare",
    );
    let stage_residency_body = rust_function_body(
        residency_pages_source,
        "fn stage_loading_thread_history_page",
    );

    assert!(shell_source.contains("mod transcript_presentability;"));
    assert!(presentability_source.contains("TranscriptRowPresentabilityState"));
    assert!(presentability_source.contains("TranscriptFullDetailReadiness"));
    assert!(presentability_source.contains("TranscriptRowPresentationReadiness"));
    assert!(presentability_source.contains("TranscriptMarkdownMediaPlanReadiness"));
    assert!(presentability_source.contains("TranscriptCompletedMediaReadiness"));
    assert!(presentability_source.contains("TranscriptMediaReadinessKey"));
    assert!(presentability_source.contains("TranscriptMediaRequestedRenderSize"));
    assert!(presentability_source.contains("TranscriptMediaPathIdentity"));
    assert!(presentability_source.contains("TranscriptMediaTerminalFallback"));
    assert!(presentability_source.contains("LivePendingGeneratedImage"));
    assert!(
        staged_activation_new_body
            .contains("TranscriptPresentabilityWindow::from_selected_thread_activation")
    );
    assert!(stage_residency_body.contains("TranscriptPresentabilityWindow::from_history_page"));
    assert!(!transcript_render_source.contains("transcript_presentability"));
    assert!(!transcript_render_source.contains("TranscriptPresentabilityWindow"));
    assert!(!transcript_snapshot_source.contains("TranscriptPresentabilityWindow"));
    assert!(!status_line_source.contains("TranscriptPresentabilityWindow"));
    assert!(!scrollbars_source.contains("TranscriptPresentabilityWindow"));
}

#[test]
fn transcript_release_removes_rows_without_rendered_placeholders() {
    let residency_pages_source = include_str!("../src/shell/transcript_residency_pages.rs");
    let projection_source = include_str!("../src/shell/transcript_projection.rs");

    let release_body = rust_function_body(
        residency_pages_source,
        "fn release_resident_turn_payloads_for_plan",
    );

    assert!(release_body.contains("self.replace_transcript_presentation_turn("));
    assert!(projection_source.contains("if !has_user_prompt && items.is_empty()"));
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
    assert!(scheduler_body.contains("self.thread_history_page_receiver.is_some()"));
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
fn composer_image_label_sync_waits_for_residency_page_loading() {
    let shell_source = include_str!("../src/shell.rs");
    let sync_source = include_str!("../src/shell/composer_image_label_sync.rs");
    let poll_body = rust_function_body(shell_source, "fn poll(&mut self");
    let scheduler_body = rust_function_body(
        sync_source,
        "pub(super) fn begin_composer_image_label_sync_if_needed",
    );

    assert_order(
        poll_body,
        "poll_thread_history_page_updates",
        "poll_composer_image_label_validation_updates",
    );
    assert_order(
        poll_body,
        "poll_composer_image_label_scan_updates",
        "begin_composer_image_label_sync_if_needed",
    );
    assert!(scheduler_body.contains("self.thread_history_page_receiver.is_some()"));
    assert!(scheduler_body.contains("self.thread_history_page_receiver.is_some()"));
}

#[test]
fn composer_image_label_sync_treats_not_loaded_thread_history_as_unscanned() {
    let selected_activation_publisher_source =
        include_str!("../src/shell/selected_thread_activation/publisher.rs");
    let load_body = rust_function_body(
        selected_activation_publisher_source,
        "fn publish_history_window",
    );

    assert!(load_body.contains("history_window.has_older_pages()"));
    assert!(load_body.contains("turn.items_view != beryl_backend::TurnItemsView::Full"));
    assert!(!load_body.contains("history_window.has_older_pages() &&"));
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
fn transcript_scroll_routes_through_residency_state() {
    let shell_source = include_str!("../src/shell.rs");
    let scroll_body = rust_function_body(shell_source, "fn apply_transcript_scroll_command");
    let scroll_event_body = rust_function_body(shell_source, "fn note_transcript_scroll_event");

    assert!(scroll_body.contains("note_transcript_scroll_event"));
    assert!(!scroll_body.contains("begin_transcript_residency_update_for_scroll_event"));
    assert!(scroll_event_body.contains("begin_transcript_residency_update_for_scroll_event"));
}

#[test]
fn status_line_turn_view_projection_stays_render_path_free() {
    let shell_source = include_str!("../src/shell.rs");
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let status_operation_source = include_str!("../src/shell/status_operation.rs");
    let status_operation_render_source = include_str!("../src/shell/render/status_operation.rs");
    let turn_view_status_source = include_str!("../src/shell/turn_view_status.rs");
    let turn_view_source = include_str!("../src/shell/turn_view.rs");
    let status_projection_body =
        rust_function_body(turn_view_status_source, "fn status_line_projection(");
    let status_turn_view_body =
        rust_function_body(turn_view_status_source, "fn status_line_turn_view(");
    let turn_operation_target_body = rust_function_body(
        turn_view_status_source,
        "fn status_line_turn_operation_target(",
    );
    let hard_stop_targets_body = rust_function_body(
        turn_view_status_source,
        "fn status_line_hard_stop_targets_for(",
    );
    let render_status_line_body = rust_function_body(render_source, "fn render_status_line(");
    let render_turn_operations_menu_body = rust_function_body(
        status_operation_render_source,
        "fn render_turn_operations_menu(",
    );
    let turn_ui_state_body = rust_function_body(shell_source, "fn turn_ui_state(");
    let ui_state_snapshot_body = rust_function_body(shell_source, "fn ui_state_snapshot(");

    assert!(status_projection_body.contains(".with_turn_view(self.status_line_turn_view())"));
    assert!(status_projection_body.contains("self.status_line_turn_operation_target()"));
    assert!(status_projection_body.contains("self.status_line_hard_stop_targets_for("));
    assert!(status_turn_view_body.contains("self.transcript_turn_numbering_snapshot()"));
    assert!(status_turn_view_body.contains("StatusLineTurnView::new("));
    assert!(turn_ui_state_body.contains("turn_view_ui_state(projection.turn_view)"));
    assert!(render_status_line_body.contains("status_line::status_line_cell_specs("));
    assert!(render_status_line_body.contains("status.cancellable_active_turn.is_some()"));
    assert!(render_turn_operations_menu_body.contains("status_line_turn_operation_target()"));
    assert!(render_turn_operations_menu_body.contains("status_line_hard_stop_targets_for("));
    assert!(!render_turn_operations_menu_body.contains("status_line_projection()"));

    for body in [
        status_projection_body,
        status_turn_view_body,
        turn_operation_target_body,
        hard_stop_targets_body,
        render_status_line_body,
        render_turn_operations_menu_body,
        turn_ui_state_body,
        ui_state_snapshot_body,
    ] {
        assert!(!body.contains("backend_client_connector"));
        assert!(!body.contains("spawn_turn_view_count_worker"));
        assert!(!body.contains("execution_details.turns()"));
        assert!(!body.contains("begin_loading"));
        assert!(!body.contains("thread/turns/list"));
        assert!(!body.contains("list_thread_turns"));
        assert!(!body.contains("thread_history_page_receiver"));
        assert!(!body.contains(".splice("));
        assert!(!body.contains("replace_transcript_presentation"));
        assert!(!body.contains(".replace_turn("));
        assert!(!body.contains(".append_turn("));
        assert!(!body.contains(".prepend_from_turns("));
    }

    for function_signature in [
        "fn open_status_turn_operations_popup(",
        "fn begin_soft_stop_selected_turn_from_control(",
        "fn begin_hard_stop_hold_from_status_popup_source(",
        "fn poll_status_operation_hold(",
        "fn complete_hard_stop_hold_from_status_popup(",
        "fn begin_hard_stop_selected_turn_from_control(",
    ] {
        let body = rust_function_body(status_operation_source, function_signature);
        assert!(!body.contains("status_line_projection()"));
        assert!(!body.contains("turn_view"));
        assert!(!body.contains("transcript_turn_numbering_snapshot"));
    }

    for function_signature in [
        "fn handle_soft_stop_turn_tool_result(",
        "fn handle_hard_stop_turn_tool_result(",
    ] {
        let body = rust_function_body(shell_source, function_signature);
        assert!(!body.contains("status_line_projection()"));
        assert!(!body.contains("turn_view"));
        assert!(!body.contains("transcript_turn_numbering_snapshot"));
    }

    assert!(turn_view_source.contains("viewport_ends_in_virtual_trailing_space"));
    assert!(turn_view_source.contains("source_turn_index_at"));
    assert!(turn_view_source.contains("selected_thread_turn_total_is_exact"));
    assert!(turn_view_source.contains("oldest_source_position_known"));
    assert!(turn_view_source.contains("backend_turn_count_for_thread"));
    assert!(!turn_view_source.contains("StatusLineTurnView"));
    assert!(!turn_view_source.contains("turn_at(viewport_bottom_row_index)"));
    assert!(!turn_view_source.contains("backend_client_connector"));
    assert!(!turn_view_source.contains("begin_loading"));
    assert!(!turn_view_source.contains("execution_details.turns()"));
    assert!(shell_source.contains("mod turn_view_status;"));
    assert!(!shell_source.contains("mod turn_view_numbering;"));
    assert!(!shell_source.contains("mod turn_view_count;"));
    assert!(!shell_source.contains("turn_numbering_state"));
    assert!(!shell_source.contains("reset_turn_numbering"));
    assert!(!shell_source.contains("observe_selected_history_loaded_for_turn_numbering"));
    assert!(!shell_source.contains("observe_selected_live_turn_started_for_numbering"));
    assert!(!shell_source.contains("turn_view_count_receiver"));
    assert!(!shell_source.contains("poll_turn_view_count_updates"));
    assert!(!shell_source.contains("begin_selected_thread_turn_count_if_needed"));

    for source in [turn_view_status_source, turn_view_source] {
        assert!(!source.contains("ManagedBackendClientConnector"));
        assert!(!source.contains("ThreadTurnsListOptions"));
        assert!(!source.contains("TurnItemsView"));
        assert!(!source.contains("list_thread_turns"));
        assert!(!source.contains("thread/turns/list"));
        assert!(!source.contains("spawn_turn_view_count_worker"));
        assert!(!source.contains("thread::spawn"));
        assert!(!source.contains("mpsc"));
        assert!(!source.contains("backend_client_connector"));
    }
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
    let residency_scroll_body = rust_function_body(
        shell_source,
        "fn begin_transcript_residency_update_for_scroll_event",
    );
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
    let publication_finish_body = rust_function_body(
        shell_source,
        "fn finish_published_selected_thread_activation",
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
    assert!(residency_scroll_body.contains("ShellState::Ready(ready)"));
    assert!(residency_scroll_body.contains("ShellState::BackendUnavailable(_)"));
    assert!(!residency_scroll_body.contains("ShellState::BackendUnavailable(unavailable)"));
    assert!(!residency_scroll_body.contains("connector.launch_spec().runtime_mode().clone()"));
    assert!(!residency_scroll_body.contains("| ShellState::BackendUnavailable(_)"));
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
    assert!(activation_finish_body.contains("published_activation"));
    assert!(publication_finish_body.contains("&execution_target"));
    assert!(publication_finish_body.contains("remember_thread_summary"));
    assert!(publication_finish_body.contains("finish_pending_thread_navigation_activation"));
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
    let selected_activation_source = include_str!("../src/shell/selected_thread_activation.rs");
    let selected_activation_preparation_source =
        include_str!("../src/shell/selected_thread_activation/preparation.rs");

    assert!(shell_source.lines().count() < 15_000);
    for module in [
        "mod render_theme;",
        "mod dynamic_theme;",
        "mod dynamic_theme_worker;",
        "mod dynamic_settings;",
        "mod diagnostics;",
        "mod selected_thread_activation;",
        "mod turn_view_status;",
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
    assert!(selected_activation_source.contains("mod preparation;"));
    assert!(
        selected_activation_preparation_source.contains("struct StagedSelectedThreadActivation")
    );
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
    assert!(shell_source.contains("thread_is_read_only_decision_branch"));
    assert!(shell_source.contains("remember_thread_summary"));
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
fn transcript_residency_ui_pins_invalidate_controller_without_direct_loads() {
    let pins_source = include_str!("../src/shell/transcript_residency_pins.rs");
    let residency_source = include_str!("../src/shell/transcript_history/residency.rs");
    let branch_menu_source = include_str!("../src/shell/transcript_branch_menu.rs");
    let edit_mode_source = include_str!("../src/shell/transcript_edit_mode.rs");
    let presentation_reconcile_source =
        include_str!("../src/shell/transcript_presentation_reconcile.rs");

    let sync_body = rust_function_body(
        pins_source,
        "pub(super) fn sync_transcript_residency_ui_pins",
    );
    assert!(sync_body.contains("TranscriptResidencyPinKind::ActiveContextMenu"));
    assert!(sync_body.contains("TranscriptResidencyPinKind::EditTarget"));
    assert!(sync_body.contains("TranscriptResidencyPinKind::MediaActionTarget"));
    assert!(sync_body.contains("TranscriptResidencyPinKind::ActiveTurn"));
    assert!(sync_body.contains("invalidate_transcript_residency_controller"));
    assert!(!sync_body.contains("release_unpinned_resident_turns_for_current_viewport"));
    assert!(!sync_body.contains("begin_loading"));

    assert!(!residency_source.contains("ActiveSelection"));
    assert!(!residency_source.contains("QuotePopup"));
    assert!(!residency_source.contains("VisibleRange"));
    assert!(!residency_source.contains("Overscan"));

    let close_menu_body = rust_function_body(
        branch_menu_source,
        "pub(crate) fn close_transcript_branch_menu",
    );
    assert!(close_menu_body.contains("sync_transcript_residency_ui_pins"));
    let open_menu_body = rust_function_body(
        branch_menu_source,
        "pub(crate) fn open_transcript_branch_menu_for_row",
    );
    assert_order(
        open_menu_body,
        ".open_menu_with_title_update",
        "sync_transcript_residency_ui_pins",
    );
    let accept_branch_body = rust_function_body(
        branch_menu_source,
        "fn accept_transcript_branch_menu_action",
    );
    assert_order(
        accept_branch_body,
        "dispatch_transcript_branch_request",
        "sync_transcript_residency_ui_pins",
    );
    let accept_edit_body = rust_function_body(
        branch_menu_source,
        "pub(crate) fn edit_transcript_turn_from_menu",
    );
    assert_order(
        accept_edit_body,
        "begin_transcript_edit_mode_from_request",
        "sync_transcript_residency_ui_pins",
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
    assert!(begin_edit_body.contains("sync_transcript_residency_ui_pins"));
    assert!(cancel_edit_body.contains("sync_transcript_residency_ui_pins"));
    assert!(reconcile_edit_body.contains("sync_transcript_residency_ui_pins"));

    let reconcile_body = rust_function_body(
        presentation_reconcile_source,
        "pub(super) fn reconcile_transcript_presentation_mutation",
    );
    assert!(presentation_reconcile_source.contains("replace_transcript_presentation_turn("));
    assert!(reconcile_body.contains("invalidate_item_measurement(index)"));
    assert!(reconcile_body.contains("self.transcript_list_state.splice(start..start, count)"));
    assert!(reconcile_body.contains("self.transcript_list_state.splice(start..end, 0)"));
    assert!(reconcile_body.contains("reconcile_transcript_branch_menu_target"));
    assert!(reconcile_body.contains("reconcile_transcript_edit_mode"));
    assert_order(
        reconcile_body,
        "self.transcript_list_state.splice(start..end, 0)",
        "reconcile_transcript_branch_menu_target",
    );
}

#[test]
fn transcript_presentation_row_mutations_are_reconciled_at_shell_boundary() {
    let shell_source = include_str!("../src/shell.rs");
    let reconcile_source = include_str!("../src/shell/transcript_presentation_reconcile.rs");

    assert!(shell_source.contains("mod transcript_presentation_reconcile;"));
    assert!(!shell_source.contains("mod transcript_live_rows;"));
    assert!(
        !shell_source.contains(".transcript_presentation\n                        .replace_turn(")
    );
    assert!(!shell_source.contains(".transcript_presentation\n                    .replace_turn("));
    assert!(!shell_source.contains(".transcript_presentation.replace_turn("));
    for helper in [
        "prepend_transcript_presentation_rows",
        "append_transcript_presentation_turn",
        "replace_transcript_presentation_turn",
        "reconcile_transcript_presentation_mutation",
    ] {
        assert!(
            reconcile_source.contains(helper),
            "missing reconciliation helper {helper}"
        );
    }
    assert!(reconcile_source.contains(".prepend_from_turns(turns)"));
    assert!(reconcile_source.contains(".append_turn(source_turn_index, turn)"));
    assert!(reconcile_source.contains(".replace_turn(source_turn_index, turn)"));
    assert!(reconcile_source.contains("TranscriptPresentationMutation::Inserted"));
    assert!(reconcile_source.contains("TranscriptPresentationMutation::Removed"));
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
