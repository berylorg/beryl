const SHELL: &str = include_str!("../src/shell.rs");
const TRANSITION_ACTIVATION: &str =
    include_str!("../src/shell/phase_thread_transition_activation.rs");
const TRANSITION: &str = include_str!("../src/shell/phase_thread_transition.rs");
const TRANSITION_APPLICATOR: &str =
    include_str!("../src/shell/phase_thread_transition_applicator.rs");
const TRANSITION_LIFECYCLE: &str =
    include_str!("../src/shell/phase_thread_transition_lifecycle.rs");
const WORKSPACE_DELETION: &str = include_str!("../src/shell/phase_thread_workspace_deletion.rs");
const STATUS: &str = include_str!("../src/shell/status_operation.rs");
const BRANCH: &str = include_str!("../src/shell/transcript_branch_menu.rs");
const EDIT: &str = include_str!("../src/shell/transcript_edit_mode.rs");
const GRAPH_START: &str = include_str!("../src/shell/graph_thread_start.rs");
const CHECKLIST: &str = include_str!("../src/shell/checklist_thread_menu.rs");
const CONVERSATION_RENDER: &str = include_str!("../src/shell/render/conversation.rs");
const BRANCH_RENDER: &str = include_str!("../src/shell/render/transcript_branch_menu.rs");
const GRAPH_ROWS_RENDER: &str = include_str!("../src/shell/render/graph_overlay/rows.rs");
const LIFECYCLE: &str = include_str!("../src/shell/lifecycle.rs");

#[test]
fn finished_worker_releases_receivers_and_consumes_handoff_before_input_fallback() {
    let poll = rust_function_body(SHELL, "fn poll_turn_updates");
    assert_order(
        poll,
        "self.turn_receiver = None",
        "self.shell_tool_receiver = None",
    );
    assert_order(
        poll,
        "self.shell_tool_receiver = None",
        "take_phase_continue_new_thread_handoff_for_finished_worker",
    );
    assert_order(
        poll,
        "self.phase_continue_new_thread_handoff.take()",
        "self.begin_lifecycle_phase_thread_preparation",
    );
    assert_order(
        poll,
        "self.begin_lifecycle_phase_thread_preparation",
        "self.begin_pending_turn_input_queue_for_thread",
    );
}

#[test]
fn active_task_blocks_controls_while_cancelling_tasks_remain_polled() {
    let frame_poll = rust_function_body(SHELL, "fn has_frame_poll_work");
    let poll = rust_function_body(SHELL, "fn poll(");
    let cancel = rust_function_body(TRANSITION_LIFECYCLE, "fn cancel_phase_thread_preparation");
    let busy = rust_function_body(
        TRANSITION_LIFECYCLE,
        "fn lifecycle_phase_thread_transition_active",
    );
    assert!(frame_poll.contains("self.phase_thread_transition.has_poll_work()"));
    assert!(poll.contains("self.poll_phase_thread_preparation_updates"));
    assert!(cancel.contains("phase_thread_transition.cancel_active"));
    assert!(TRANSITION.contains("task.cancel(retention_deadline)"));
    assert!(TRANSITION.contains("self.next_generation()"));
    assert!(busy.contains("phase_thread_transition.blocks_controls()"));
    assert!(SHELL.matches("cancel_phase_thread_preparation();").count() >= 3);
}

#[test]
fn composer_new_thread_selector_and_graph_refs_share_transition_gate() {
    for (signature, occurrence) in [
        ("fn queue_turn_from_composer(&mut", 0),
        ("fn start_new_thread(&mut self, _:", 0),
        ("fn select_thread_selector_member(", 1),
        ("fn select_thread_selector_thread(", 1),
        ("fn activate_thread_selector_target(", 0),
        ("fn select_graph_thread_ref(", 0),
    ] {
        let body = rust_nth_function_body(SHELL, signature, occurrence);
        assert!(
            body.contains("lifecycle_phase_thread_transition")
                || body.contains("reject_lifecycle_phase_thread_transition_action"),
            "missing lifecycle transition gate in {signature}"
        );
    }
    let render = rust_function_body(CONVERSATION_RENDER, "fn render_workspace_surface");
    assert!(render.contains("backend_controls_disabled"));
    assert!(render.contains("new_thread_controls_disabled_message"));
    assert!(render.contains("thread_selector_controls_disabled_message"));
    assert!(GRAPH_ROWS_RENDER.contains("let transition_active"));
    assert!(GRAPH_ROWS_RENDER.contains(".when(row_openable"));
}

#[test]
fn graph_checklist_branch_edit_history_compaction_and_stop_paths_are_gated() {
    let cases = [
        (GRAPH_START, "fn start_thread_from_semantic_node"),
        (
            CHECKLIST,
            "pub(crate) fn start_checklist_item_thread_from_menu",
        ),
        (BRANCH, "fn accept_transcript_branch_menu_action"),
        (BRANCH, "pub(crate) fn edit_transcript_turn_from_menu"),
        (
            EDIT,
            "pub(crate) fn begin_transcript_edit_mode_from_request",
        ),
        (SHELL, "fn begin_older_thread_history_page_if_needed"),
        (
            STATUS,
            "pub(crate) fn compact_selected_thread_from_status_popup",
        ),
        (
            STATUS,
            "pub(crate) fn begin_soft_stop_selected_turn_from_control",
        ),
        (STATUS, "fn begin_hard_stop_hold_from_status_popup_source"),
        (
            STATUS,
            "pub(crate) fn begin_hard_stop_selected_turn_from_control",
        ),
    ];
    for (source, signature) in cases {
        let body = rust_function_body(source, signature);
        assert!(
            body.contains("lifecycle_phase_thread_transition")
                || body.contains("reject_lifecycle_phase_thread_transition_action")
                || body.contains("new_thread_controls_disabled_message"),
            "missing lifecycle transition gate in {signature}"
        );
    }
    assert!(BRANCH_RENDER.contains("lifecycle_transition_active"));
    let edit_row = rust_function_body(BRANCH_RENDER, "fn edit_row");
    let disabled_edit = rust_function_body(BRANCH_RENDER, "fn disabled_lifecycle_edit_row");
    assert!(edit_row.contains("disabled_lifecycle_edit_row"));
    assert!(disabled_edit.contains("PHASE_THREAD_TRANSITION_BUSY_MESSAGE"));
    assert!(!disabled_edit.contains("cx.listener"));
}

#[test]
fn prepared_child_is_registered_directly_with_binding_provenance_and_metadata() {
    let register = rust_function_body(
        TRANSITION_ACTIVATION,
        "fn register_prepared_phase_thread_child",
    );
    let activate = rust_function_body(
        TRANSITION_ACTIVATION,
        "fn activate_registered_phase_thread_child",
    );
    assert!(register.contains("RegisteredConversationThread::new"));
    assert!(register.contains("with_member_binding(request.member_binding().clone())"));
    assert!(register.contains("with_beryl_created()"));
    assert!(register.contains("record_thread_orchestration_root"));
    assert!(register.contains("activate_thread(&child_thread_id)"));
    assert!(register.contains("persist_current_workspace_state(true)"));
    assert!(activate.contains("surface.load_thread_history(&child)"));
    assert!(activate.contains("surface.set_thread_session_metadata(session_metadata)"));
    assert!(activate.contains("mark_member_thread_inventory_refresh_needed"));
    assert!(activate.contains("queue_pending_turn_fragment"));
    assert!(activate.contains("begin_pending_turn_input_queue_for_thread"));
    assert!(!activate.contains("thread_activation_receiver"));
    assert!(!activate.contains("member_thread_inventory_receiver"));
    assert!(register.contains("registration.child_thread_id().clone()"));
    assert!(register.contains("registration.created_at_millis()"));
    assert!(register.contains("registration.updated_at_millis()"));
    assert!(!register.contains("registration.preview()"));
}

#[test]
fn continuation_start_uses_ordinary_late_bound_turn_assembly_and_title_eligibility() {
    let activate = rust_function_body(
        TRANSITION_ACTIVATION,
        "fn activate_registered_phase_thread_child",
    );
    let pending = rust_function_body(SHELL, "fn begin_pending_turn_input_queue_for_thread");
    assert!(activate.contains("thread_automatic_title_generation_eligible"));
    assert!(activate.contains("pending_turn_start_options"));
    assert!(pending.contains("turn_options_with_current_developer_instructions"));
    assert!(pending.contains("spawn_turn_worker"));
    assert!(activate.contains("finish_turn_failure"));
    assert!(activate.contains("take_pending_turn_input_queue_for_thread"));
}

#[test]
fn failure_paths_preserve_selection_truth_and_refresh_only_when_required() {
    let completion = rust_function_body(TRANSITION_APPLICATOR, "fn apply_phase_thread_completion");
    let disconnected = rust_function_body(
        TRANSITION_ACTIVATION,
        "fn poll_phase_thread_preparation_updates",
    );
    for variant in [
        "DefinitiveForkFailure",
        "IndeterminateFork",
        "CancelledBeforeFork",
        "KnownChildFailure",
    ] {
        assert!(completion.contains(variant), "missing {variant} handling");
    }
    assert!(completion.contains("unidentified backend child"));
    assert!(completion.contains("may remain orphaned"));
    assert!(completion.contains("failure.child_id"));
    assert!(completion.contains("decision.refresh_original_workspace"));
    assert!(completion.contains(".register_prepared"));
    assert!(TRANSITION.contains("TryRecvError::Disconnected"));
    assert!(disconnected.contains("reduce_phase_thread_disconnect"));
    assert!(disconnected.contains("apply_phase_thread_indeterminate_completion"));
}

#[test]
fn no_selection_or_second_visible_turn_occurs_before_verified_preparation() {
    let begin = rust_function_body(
        TRANSITION_LIFECYCLE,
        "fn begin_lifecycle_phase_thread_preparation",
    );
    let poll = rust_function_body(
        TRANSITION_ACTIVATION,
        "fn poll_phase_thread_preparation_updates",
    );
    let activate = rust_function_body(
        TRANSITION_ACTIVATION,
        "fn activate_registered_phase_thread_child",
    );
    assert!(!begin.contains("activate_thread("));
    assert!(!begin.contains("upsert_selected_thread"));
    assert!(!begin.contains("spawn_turn_worker"));
    assert_order(
        poll,
        "guard_phase_thread_preparation_result",
        "apply_phase_thread_completion",
    );
    assert!(TRANSITION_APPLICATOR.contains("host.register_prepared_child"));
    assert!(activate.contains("surface.load_thread_history(&child)"));
    assert_order(
        activate,
        "surface.load_thread_history(&child)",
        "begin_pending_turn_input_queue_for_thread",
    );
}

#[test]
fn accepted_workspace_replacement_invalidates_before_ui_or_diagnostic_worker_spawn() {
    let ui = rust_function_body(SHELL, "fn activate_workspace_picker_item");
    assert_order(
        ui,
        "self.invalidate_phase_thread_for_accepted_workspace_replacement(true)",
        "spawn_switch_workspace_worker",
    );
    let diagnostic = rust_function_body(SHELL, "fn handle_switch_workspace_tool_result");
    assert_order(
        diagnostic,
        "self.invalidate_phase_thread_for_accepted_workspace_replacement(true)",
        "spawn_switch_workspace_worker",
    );
    assert!(TRANSITION_LIFECYCLE.contains("phase_thread_transition.next_generation"));
}

#[test]
fn accepted_create_and_only_active_delete_invalidate_immediately_before_deletion_drain() {
    let create = rust_function_body(SHELL, "fn begin_workspace_picker_create_new");
    assert_order(
        create,
        "self.invalidate_phase_thread_for_accepted_workspace_replacement(true)",
        "spawn_create_workspace_worker",
    );
    let delete = rust_function_body(SHELL, "fn begin_delete_workspace");
    assert_order(
        delete,
        "PhaseThreadWorkspaceDeletionDrain::accept",
        "self.poll_phase_thread_workspace_deletion()",
    );
    assert!(delete.contains("else"));
    assert!(delete.contains("spawn_delete_workspace_worker"));
}

#[test]
fn active_delete_drains_then_releases_deferred_truth_before_capturing_persistence_barrier() {
    let shell_poll = rust_function_body(SHELL, "fn poll(&mut self");
    assert_order(
        shell_poll,
        "poll_phase_thread_preparation_updates",
        "poll_phase_thread_workspace_deletion",
    );

    let shell_deletion_poll = rust_function_body(
        TRANSITION_LIFECYCLE,
        "fn poll_phase_thread_workspace_deletion",
    );
    assert!(
        shell_deletion_poll.contains("poll_phase_thread_workspace_deletion(&mut deletion, self)")
    );
    let deletion_poll = rust_function_body(
        WORKSPACE_DELETION,
        "fn poll_phase_thread_workspace_deletion",
    );
    assert_order(
        deletion_poll,
        "host.take_deferred_phase_thread_outcomes_for_workspace",
        "replay_and_release_phase_thread_workspace_deletion_outcomes",
    );
    assert_order(
        deletion_poll,
        "host.publish_released_phase_thread_outcomes",
        "host.capture_persistence_barrier_and_start_delete_worker",
    );
}

#[test]
fn active_delete_blocks_activation_reports_known_child_and_clears_on_worker_failure() {
    let busy = rust_function_body(
        TRANSITION_LIFECYCLE,
        "fn lifecycle_phase_thread_transition_active",
    );
    assert!(busy.contains("phase_thread_workspace_deletion.is_some()"));

    let publish = rust_function_body(
        TRANSITION_LIFECYCLE,
        "fn publish_released_phase_thread_outcomes",
    );
    assert!(publish.contains("known_remaining_child_ids"));
    assert!(publish.contains("mark_member_thread_inventory_refresh_needed"));
    assert!(WORKSPACE_DELETION.contains("known_remaining_child_ids"));
    assert!(WORKSPACE_DELETION.contains("remains in the backend"));

    let finish = rust_function_body(TRANSITION_LIFECYCLE, "fn finish_workspace_picker_action");
    assert!(
        finish
            .matches("self.complete_phase_thread_workspace_deletion()")
            .count()
            >= 2
    );
    assert_order(
        finish,
        "self.complete_phase_thread_workspace_deletion()",
        "failed to delete Beryl workspace from the picker",
    );
}

#[test]
fn shutdown_drains_phase_owner_flushes_persistence_then_stops_servers() {
    let begin = rust_function_body(SHELL, "fn begin_application_shutdown");
    let shell_poll = rust_function_body(SHELL, "fn poll(&mut self");
    let poll = rust_function_body(SHELL, "fn poll_application_shutdown_updates");
    let worker = rust_function_body(SHELL, "fn spawn_application_shutdown_worker");
    let drop = rust_function_body(SHELL, "fn drop(&mut self)");
    assert!(begin.contains("phase_thread_transition.cancel_all"));
    assert!(begin.contains("application_shutdown_phase_deadline"));
    assert!(!begin.contains("backend_servers.drain"));
    assert_order(
        begin,
        "complete_phase_thread_workspace_deletion",
        "phase_thread_transition.cancel_all",
    );
    assert_order(
        poll,
        "phase_thread_transition.has_poll_work()",
        "workspace_persistence_queue.flush()",
    );
    assert_order(
        poll,
        "workspace_persistence_queue.flush()",
        "spawn_application_shutdown_worker",
    );
    assert_order(
        worker,
        "workspace_persistence_flush.wait",
        "for server in active_servers",
    );
    assert!(drop.contains("phase_thread_transition.cancel_all"));
    assert_order(
        shell_poll,
        "poll_application_shutdown_updates",
        "schedule_poll_if_needed",
    );
}

#[test]
fn source_queue_failure_and_deferred_workspace_replay_use_production_paths() {
    let begin = rust_function_body(
        TRANSITION_LIFECYCLE,
        "fn begin_lifecycle_phase_thread_preparation",
    );
    assert_order(
        begin,
        "fail_accepted_source_pending_input",
        "spawn_phase_thread_preparation_worker",
    );
    assert!(TRANSITION_LIFECYCLE.contains("fail_pending_turn_input_queue_for_thread"));
    assert!(TRANSITION_ACTIVATION.contains("defer_outcome"));
    assert!(TRANSITION_ACTIVATION.contains("apply_deferred_prepared_registration"));
    assert!(
        TRANSITION_ACTIVATION
            .contains("apply_deferred_phase_thread_outcomes_for_current_workspace")
    );
    assert!(LIFECYCLE.contains("apply_deferred_phase_thread_outcomes_for_current_workspace"));
}

#[test]
fn member_mutations_use_prospective_production_guard_and_replay_before_inventory_refresh() {
    for signature in [
        "fn select_workspace_runtime",
        "fn attach_workspace_member",
        "fn detach_workspace_member",
    ] {
        let body = rust_function_body(SHELL, signature);
        assert!(body.contains("prospective_workspace_state_invalidates_phase_thread"));
        assert_order(
            body,
            "prospective_workspace_state_invalidates_phase_thread",
            "cancel_phase_thread_preparation",
        );
    }
    for signature in ["fn attach_workspace_member", "fn detach_workspace_member"] {
        let body = rust_function_body(SHELL, signature);
        assert_order(
            body,
            "apply_deferred_phase_thread_outcomes_for_current_workspace",
            "reset_member_thread_inventory_for_workspace_state",
        );
    }
    let replay = rust_function_body(
        TRANSITION_ACTIVATION,
        "fn apply_deferred_phase_thread_outcomes_for_current_workspace",
    );
    assert_order(
        replay,
        "apply_deferred_prepared_registration",
        "mark_member_thread_inventory_refresh_needed",
    );
    assert!(replay.contains("restore_deferred_outcomes"));
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    rust_nth_function_body(source, function_signature, 0)
}

fn rust_nth_function_body<'a>(
    source: &'a str,
    function_signature: &str,
    occurrence: usize,
) -> &'a str {
    let signature_index = source
        .match_indices(function_signature)
        .nth(occurrence)
        .map(|(index, _)| index)
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
                    return &source[body_start..body_start + offset + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated body for {function_signature}");
}

fn assert_order(source: &str, before: &str, after: &str) {
    let before = source
        .find(before)
        .unwrap_or_else(|| panic!("missing {before}"));
    let after = source
        .find(after)
        .unwrap_or_else(|| panic!("missing {after}"));
    assert!(before < after, "expected ordered source markers");
}
