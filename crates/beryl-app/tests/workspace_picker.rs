use beryl_model::{
    conversation::WorkspaceConversationState,
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, RuntimeMode, WorkspaceId},
};
use gpui::{Bounds, point, px, size};
use std::time::Duration;
use workspace_picker::{
    CREATE_NEW_ITEM_INDEX, WORKSPACE_DELETE_HOLD_DURATION,
    WORKSPACE_PICKER_THREAD_EDIT_WAIT_NOTICE, WorkspaceDeleteHoldSource, WorkspacePickerState,
    WorkspacePickerTransitionBlockers, WorkspacePickerTransitionPath,
    workspace_index_for_filtered_item_index, workspace_index_for_item_index, workspace_item_index,
    workspace_picker_item_count, workspace_picker_transition_disabled_reason,
    workspace_picker_transition_path_disabled_reason, workspace_row_accepts_activation,
};
use workspace_rename_policy::{
    WORKSPACE_RENAME_WAIT_TOOLTIP, WorkspaceRenameBlockers, workspace_rename_disabled_reason,
};

#[path = "../src/shell/layout.rs"]
mod layout;

#[path = "../src/shell/workspace_picker.rs"]
mod workspace_picker;

#[path = "../src/shell/workspace_rename_policy.rs"]
mod workspace_rename_policy;

#[test]
fn picker_workspace_rows_do_not_keep_keyboard_highlight_state() {
    let mut picker = WorkspacePickerState::default();
    picker.open();

    assert!(picker.is_open());

    let picker_state_source = include_str!("../src/shell/workspace_picker.rs");
    let shell_source = include_str!("../src/shell.rs");
    let key_down_body = rust_function_body(shell_source, "fn handle_workspace_picker_key_down");

    assert!(!picker_state_source.contains(
        "pub(crate) struct WorkspacePickerState {\n    open: bool,\n    highlighted_index"
    ));
    assert!(!picker_state_source.contains("pub(crate) fn move_highlight"));
    assert!(!picker_state_source.contains("pub(crate) fn set_highlighted_index"));
    assert!(!picker_state_source.contains("pub(crate) fn open_for"));
    assert!(!key_down_body.contains("workspace_picker.move_highlight"));
    assert!(!key_down_body.contains("activate_workspace_picker_item(index, window, cx)"));
}

#[test]
fn picker_dismisses_only_outside_anchor_and_popup() {
    let mut picker = WorkspacePickerState::default();
    picker.open();
    picker.set_anchor_bounds(Some(Bounds::new(
        point(px(24.0), px(12.0)),
        size(px(160.0), px(36.0)),
    )));
    picker.set_popup_bounds(Some(Bounds::new(
        point(px(24.0), px(56.0)),
        size(px(420.0), px(220.0)),
    )));

    assert!(!picker.should_dismiss_for_mouse_down(point(px(40.0), px(24.0))));
    assert!(!picker.should_dismiss_for_mouse_down(point(px(40.0), px(80.0))));
    assert!(picker.should_dismiss_for_mouse_down(point(px(500.0), px(24.0))));

    let workspace_id = BerylWorkspaceId::new("menu_workspace").unwrap();
    picker.open_row_action_menu(workspace_id, point(px(432.0), px(96.0)));
    picker.set_row_action_menu_bounds(Some(Bounds::new(
        point(px(420.0), px(90.0)),
        size(px(180.0), px(120.0)),
    )));

    assert!(!picker.should_dismiss_for_mouse_down(point(px(440.0), px(100.0))));
    assert!(!picker.should_dismiss_row_action_menu_for_mouse_down(point(px(440.0), px(100.0))));
    assert!(picker.should_dismiss_row_action_menu_for_mouse_down(point(px(40.0), px(80.0))));
}

#[test]
fn picker_popup_dimensions_follow_layout_contract() {
    assert_eq!(workspace_picker::popup_width(px(800.0)), px(752.0));
    assert_eq!(workspace_picker::popup_width(px(360.0)), px(336.0));

    let preferred = workspace_picker::popup_layout(1, 2, 0, px(1000.0), px(1000.0));
    assert_eq!(
        preferred.width,
        px(layout::WORKSPACE_PICKER_PREFERRED_WIDTH)
    );
    assert_eq!(preferred.height, px(286.0));
    assert_px_close(preferred.workspaces_column_width, px(420.0));
    assert_px_close(preferred.members_column_width, px(419.0));
    assert_eq!(preferred.divider_width, px(1.0));

    let clamped = workspace_picker::popup_layout(12, 4, 0, px(1000.0), px(600.0));
    assert_eq!(
        clamped.height,
        px(600.0 * layout::WORKSPACE_PICKER_MAX_HEIGHT_RATIO)
    );
}

#[test]
fn picker_popup_layout_includes_open_runtime_dropdown_height() {
    let closed = workspace_picker::popup_layout(0, 1, 0, px(1000.0), px(1000.0));
    let open = workspace_picker::popup_layout(0, 1, 6, px(1000.0), px(1000.0));

    assert!(open.height > closed.height);
    assert_px_close(
        open.runtime_selector_dropdown_height,
        px(layout::WORKSPACE_PICKER_RUNTIME_DROPDOWN_ROW_HEIGHT
            * layout::WORKSPACE_PICKER_RUNTIME_DROPDOWN_MAX_VISIBLE_ROWS as f32),
    );
    assert!(
        open.runtime_selector_dropdown_height
            <= open.height
                - px(layout::WORKSPACE_PICKER_HEADER_HEIGHT)
                - px(layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_COLUMN_TOP)
    );
}

#[test]
fn picker_popup_layout_caps_runtime_dropdown_to_available_height() {
    let capped = workspace_picker::popup_layout(0, 1, 10, px(1000.0), px(280.0));
    let available_dropdown_height = px(280.0 * layout::WORKSPACE_PICKER_MAX_HEIGHT_RATIO
        - layout::WORKSPACE_PICKER_HEADER_HEIGHT
        - layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_COLUMN_TOP);

    assert_eq!(
        capped.height,
        px(280.0 * layout::WORKSPACE_PICKER_MAX_HEIGHT_RATIO)
    );
    assert_px_close(
        capped.runtime_selector_dropdown_height,
        available_dropdown_height,
    );
}

#[test]
fn picker_member_list_item_count_keeps_attach_row_and_implicit_home() {
    assert_eq!(
        workspace_picker::workspace_picker_member_list_item_count(0, false),
        1
    );
    assert_eq!(
        workspace_picker::workspace_picker_member_list_item_count(0, true),
        2
    );
    assert_eq!(
        workspace_picker::workspace_picker_member_list_item_count(3, true),
        4
    );
}

#[test]
fn picker_selection_maps_create_list_row_before_workspace_rows() {
    assert_eq!(workspace_picker_item_count(0), 1);
    assert_eq!(workspace_item_index(0), 1);
    assert_eq!(workspace_index_for_item_index(CREATE_NEW_ITEM_INDEX), None);
    assert_eq!(
        workspace_index_for_item_index(workspace_item_index(2)),
        Some(2)
    );
}

#[test]
fn picker_filter_matches_workspace_names_without_reordering() {
    let alpha = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("alpha_workspace").unwrap(),
        "Alpha Workspace",
        1,
    );
    let beta = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("beta_workspace").unwrap(),
        "Beta Workspace",
        2,
    );
    let alpine = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("alpine_docs").unwrap(),
        "Alpine Docs",
        3,
    );
    let workspaces = vec![alpha, beta, alpine];
    let member_paths = workspace_picker::WorkspacePickerMemberPaths::new();

    assert_eq!(
        workspace_picker::filtered_workspace_indices(&workspaces, &member_paths, "alp"),
        vec![0, 2]
    );
}

#[test]
fn picker_filter_matches_explicit_member_paths() {
    let alpha = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("alpha_workspace").unwrap(),
        "Alpha Workspace",
        1,
    );
    let beta = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("beta_workspace").unwrap(),
        "Beta Workspace",
        2,
    );
    let mut member_paths = workspace_picker::WorkspacePickerMemberPaths::new();
    member_paths.insert(beta.id().clone(), vec![r"C:\repos\engine".to_string()]);
    let workspaces = vec![alpha, beta];

    assert_eq!(
        workspace_picker::filtered_workspace_indices(&workspaces, &member_paths, "repos\\engine"),
        vec![1]
    );
}

#[test]
fn picker_filter_empty_query_restores_full_workspace_list() {
    let alpha = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("alpha_workspace").unwrap(),
        "Alpha Workspace",
        1,
    );
    let beta = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("beta_workspace").unwrap(),
        "Beta Workspace",
        2,
    );
    let workspaces = vec![alpha, beta];
    let member_paths = workspace_picker::WorkspacePickerMemberPaths::new();

    assert_eq!(
        workspace_picker::filtered_workspace_indices(&workspaces, &member_paths, "   "),
        vec![0, 1]
    );
}

#[test]
fn picker_filter_with_no_matches_keeps_create_row_addressable() {
    let workspace = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("alpha_workspace").unwrap(),
        "Alpha Workspace",
        1,
    );
    let member_paths = workspace_picker::WorkspacePickerMemberPaths::new();

    let visible_workspace_indices =
        workspace_picker::filtered_workspace_indices(&[workspace], &member_paths, "no match");

    assert!(visible_workspace_indices.is_empty());
    assert_eq!(
        workspace_index_for_filtered_item_index(CREATE_NEW_ITEM_INDEX, &visible_workspace_indices),
        None
    );
    assert_eq!(
        workspace_picker_item_count(visible_workspace_indices.len()),
        1
    );
}

#[test]
fn picker_filtered_selection_maps_visible_rows_to_workspace_indices() {
    let visible_workspace_indices = vec![0, 2];

    assert_eq!(
        workspace_index_for_filtered_item_index(CREATE_NEW_ITEM_INDEX, &visible_workspace_indices),
        None
    );
    assert_eq!(
        workspace_index_for_filtered_item_index(
            workspace_item_index(0),
            &visible_workspace_indices
        ),
        Some(0)
    );
    assert_eq!(
        workspace_index_for_filtered_item_index(
            workspace_item_index(1),
            &visible_workspace_indices
        ),
        Some(2)
    );
    assert_eq!(
        workspace_index_for_filtered_item_index(
            workspace_item_index(2),
            &visible_workspace_indices
        ),
        None
    );
}

#[test]
fn picker_current_workspace_row_uses_active_marker_without_current_label() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let row_body = rust_function_body(render_source, "fn render_workspace_row");
    let summary_body = rust_function_body(render_source, "fn render_workspace_row_summary");
    let marker_body = rust_function_body(render_source, "fn render_workspace_active_marker");

    assert!(row_body.contains("render_workspace_active_marker"));
    assert!(!row_body.contains("highlighted"));
    assert!(!row_body.contains("primary_button_theme().normal.background"));
    assert!(!row_body.contains("primary_button_theme().normal.foreground"));
    assert!(!row_body.contains("primary_button_theme().hover.background"));
    assert!(marker_body.contains("primary_button_theme().active.background"));
    assert!(!marker_body.contains("highlighted"));
    assert!(!summary_body.contains("\"Current\""));
    assert!(!summary_body.contains("render_workspace_row_badge"));
}

#[test]
fn picker_create_row_renders_as_divided_list_row_without_nested_button_chrome() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let create_row_body = rust_function_body(render_source, "fn render_create_workspace_row");
    let attach_row_body = rust_function_body(render_source, "fn render_attach_member_row");
    let plus_body = rust_function_body(render_source, "fn render_create_add_plus_marker");

    assert!(create_row_body.contains(".border_b_1()"));
    assert!(create_row_body.contains("WORKSPACE_PICKER_CREATE_ROW_HEIGHT"));
    assert!(attach_row_body.contains("WORKSPACE_PICKER_MEMBERS_ATTACH_ROW_HEIGHT"));
    assert!(create_row_body.contains("\"Create new workspace\""));
    assert!(create_row_body.contains("render_create_add_plus_marker(shell, true)"));
    assert!(attach_row_body.contains("render_create_add_plus_marker(shell, enabled)"));
    assert!(attach_row_body.contains("\"Attach member\""));
    assert!(!create_row_body.contains("highlighted"));
    assert!(!create_row_body.contains("primary_button_theme"));
    assert!(!attach_row_body.contains("member_row_shell"));
    assert!(!attach_row_body.contains("\"Add an explicit filesystem root.\""));
    assert!(!attach_row_body.contains("\"Select a runtime before attaching.\""));
    assert!(plus_body.contains("rgb(0x72e4b8)"));
    assert!(plus_body.contains("\"+\""));
    assert!(plus_body.contains("WORKSPACE_PICKER_CREATE_ADD_PLUS_SLOT_WIDTH"));
    assert!(plus_body.contains("WORKSPACE_PICKER_CREATE_ADD_PLUS_GLYPH_Y_OFFSET"));
    assert!(plus_body.contains(".items_center()"));
    assert!(plus_body.contains(".justify_center()"));
    assert!(plus_body.contains(".relative()"));
    assert!(plus_body.contains(".top(px("));
    assert!(!create_row_body.contains("BUTTON_OUTER_HEIGHT"));
    assert!(!create_row_body.contains("BUTTON_HORIZONTAL_PADDING"));
    assert!(!create_row_body.contains("\"Untitled\""));
    assert!(!create_row_body.contains("render_workspace_row_action_trigger"));
}

#[test]
fn picker_create_and_attach_rows_share_height() {
    assert_eq!(
        layout::WORKSPACE_PICKER_MEMBERS_ATTACH_ROW_HEIGHT,
        layout::WORKSPACE_PICKER_CREATE_ROW_HEIGHT
    );
}

#[test]
fn picker_columns_render_without_workspace_total_label_or_members_filter() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let workspaces_column_body = rust_function_body(render_source, "fn render_workspaces_column");
    let members_column_body = rust_function_body(render_source, "fn render_members_column");

    assert!(workspaces_column_body.contains("\"Workspaces\""));
    assert!(!workspaces_column_body.contains("\"total\""));
    assert!(!workspaces_column_body.contains("\"N total\""));
    assert!(workspaces_column_body.contains("framed_text_input"));
    assert!(members_column_body.contains("\"Members\""));
    assert!(members_column_body.contains("render_runtime_selector_control"));
    assert!(!members_column_body.contains("framed_text_input"));
}

#[test]
fn picker_runtime_selector_rows_label_wsl_items_with_prefix() {
    let host = workspace_picker::RuntimeSelectorRow::HostWindows;
    let ubuntu = workspace_picker::RuntimeSelectorRow::WslDistro {
        distro_name: "Ubuntu".to_string(),
    };

    assert_eq!(
        workspace_picker::runtime_selector_row_label(&host),
        "host-Windows"
    );
    assert_eq!(
        workspace_picker::runtime_selector_row_label(&ubuntu),
        "WSL: Ubuntu"
    );
    assert_eq!(
        workspace_picker::runtime_selector_row_for_index(&["Debian".to_string()], 1),
        Some(workspace_picker::RuntimeSelectorRow::WslDistro {
            distro_name: "Debian".to_string(),
        })
    );
}

#[test]
fn picker_runtime_selector_dropdown_row_count_includes_status_rows() {
    let mut distro_list = workspace_picker::RuntimeSelectorDistroList::default();

    assert_eq!(
        workspace_picker::runtime_selector_dropdown_row_count(&distro_list),
        2
    );

    assert!(distro_list.begin_loading());
    assert_eq!(
        workspace_picker::runtime_selector_dropdown_row_count(&distro_list),
        2
    );

    distro_list.finish_loading(Ok(vec![]));
    assert_eq!(
        workspace_picker::runtime_selector_dropdown_row_count(&distro_list),
        2
    );

    distro_list.finish_loading(Ok(vec!["Ubuntu".to_string(), "Debian".to_string()]));
    assert_eq!(
        workspace_picker::runtime_selector_dropdown_row_count(&distro_list),
        3
    );

    distro_list.finish_loading(Err("wsl unavailable".to_string()));
    assert_eq!(
        workspace_picker::runtime_selector_dropdown_row_count(&distro_list),
        2
    );
}

#[test]
fn picker_runtime_selector_dropdown_is_attached_to_trigger() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let trigger_body = rust_function_body(render_source, "fn render_runtime_selector_trigger");
    let dropdown_body = rust_function_body(render_source, "fn render_runtime_selector_dropdown");

    assert!(trigger_body.contains(".when(dropdown_open, |this| {"));
    assert!(
        trigger_body.contains(".h(px(layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_TRIGGER_HEIGHT))")
    );
    assert!(trigger_body.contains("RUNTIME_SELECTOR_ARROW"));
    assert!(trigger_body.contains("WORKSPACE_PICKER_RUNTIME_SELECTOR_ARROW_SLOT_WIDTH"));
    assert!(trigger_body.contains("WORKSPACE_PICKER_RUNTIME_SELECTOR_ARROW_FONT_SIZE"));
    assert!(trigger_body.contains(".rounded_bl(px(0.0))"));
    assert!(trigger_body.contains(".rounded_br(px(0.0))"));
    assert!(trigger_body.contains("layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DETAIL_LINE_HEIGHT"));
    assert!(dropdown_body.contains(".id(\"workspace-runtime-selector-dropdown\")"));
    assert!(
        dropdown_body.contains("layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_RELATIVE_TOP")
    );
    assert!(dropdown_body.contains(".left_0()"));
    assert!(dropdown_body.contains(".right_0()"));
    assert!(dropdown_body.contains(".h(dropdown_height)"));
    assert!(dropdown_body.contains(".rounded_tl(px(0.0))"));
    assert!(dropdown_body.contains(".rounded_tr(px(0.0))"));
    assert!(dropdown_body.contains("workspace_picker::RuntimeSelectorRow::HostWindows"));
}

#[test]
fn picker_rename_editor_is_transient_popup_state() {
    let mut picker = WorkspacePickerState::default();
    let workspace_id = BerylWorkspaceId::new("rename_workspace").unwrap();
    picker.open();
    picker.open_rename_editor_for(workspace_id.clone());

    assert!(picker.rename_editor_open());
    assert!(picker.rename_editor_open_for(&workspace_id));
    assert_eq!(picker.rename_editor_target(), Some(&workspace_id));

    picker.close();
    assert!(!picker.rename_editor_open());

    picker.open();
    assert!(!picker.rename_editor_open());
}

#[test]
fn picker_rename_editor_suppresses_target_row_activation() {
    assert!(!workspace_row_accepts_activation(true));
    assert!(workspace_row_accepts_activation(false));
}

#[test]
fn picker_row_action_menu_state_closes_rename_and_hold_state() {
    let mut picker = WorkspacePickerState::default();
    let first_workspace = BerylWorkspaceId::new("first_workspace").unwrap();
    let second_workspace = BerylWorkspaceId::new("second_workspace").unwrap();
    picker.open();
    picker.open_rename_editor_for(first_workspace.clone());

    picker.open_row_action_menu(second_workspace.clone(), point(px(320.0), px(90.0)));
    assert!(!picker.rename_editor_open());
    assert_eq!(
        picker
            .row_action_menu_active()
            .map(|menu| menu.workspace_id()),
        Some(&second_workspace)
    );

    let started_at = std::time::Instant::now();
    assert!(picker.begin_delete_hold(
        second_workspace.clone(),
        WorkspaceDeleteHoldSource::Pointer,
        started_at
    ));
    assert!(picker.delete_hold_active());

    assert!(picker.close_row_action_menu());
    assert!(picker.row_action_menu_active().is_none());
    assert!(!picker.delete_hold_active());
}

#[test]
fn picker_workspace_delete_hold_completes_only_after_duration_and_cancels_by_source() {
    let mut picker = WorkspacePickerState::default();
    let workspace_id = BerylWorkspaceId::new("delete_workspace").unwrap();
    let started_at = std::time::Instant::now();
    picker.open();
    picker.open_row_action_menu(workspace_id.clone(), point(px(320.0), px(90.0)));

    assert!(picker.begin_delete_hold(
        workspace_id.clone(),
        WorkspaceDeleteHoldSource::Pointer,
        started_at
    ));
    assert!(
        picker
            .delete_hold_progress_for_target(&workspace_id, started_at)
            .unwrap()
            < 1.0
    );
    assert_eq!(
        picker.complete_delete_hold_if_ready(started_at + Duration::from_millis(500)),
        None
    );
    assert!(picker.cancel_delete_hold_source(WorkspaceDeleteHoldSource::Pointer));
    assert!(!picker.delete_hold_active());

    assert!(picker.begin_delete_hold(
        workspace_id.clone(),
        WorkspaceDeleteHoldSource::Keyboard,
        started_at
    ));
    assert_eq!(
        picker.complete_delete_hold_if_ready(started_at + WORKSPACE_DELETE_HOLD_DURATION),
        Some(workspace_id)
    );
    assert!(!picker.delete_hold_active());
}

#[test]
fn picker_workspace_delete_hold_cancels_for_stale_target() {
    let mut picker = WorkspacePickerState::default();
    let workspace_id = BerylWorkspaceId::new("stale_workspace").unwrap();
    picker.open();
    picker.open_row_action_menu(workspace_id.clone(), point(px(320.0), px(90.0)));
    assert!(picker.begin_delete_hold(
        workspace_id,
        WorkspaceDeleteHoldSource::Pointer,
        std::time::Instant::now()
    ));

    assert!(picker.cancel_delete_hold_for_stale_target(false));
    assert!(!picker.delete_hold_active());
}

#[test]
fn picker_workspace_rows_use_row_action_menu_instead_of_inline_buttons() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let row_menu_source = include_str!("../src/shell/render/workspace_picker_row_menu.rs");
    let shell_source = include_str!("../src/shell.rs");
    let summary_body = rust_function_body(render_source, "fn render_workspace_row_summary");
    let complete_hold_body = rust_function_body(
        shell_source,
        "fn complete_workspace_delete_hold_from_action_menu",
    );

    assert!(summary_body.contains("render_workspace_row_action_trigger"));
    assert!(!summary_body.contains("\"workspace-picker-delete\""));
    assert!(!summary_body.contains("prompt_delete_workspace"));
    assert!(row_menu_source.contains("\"Rename\""));
    assert!(row_menu_source.contains("\"Delete\""));
    assert!(row_menu_source.contains("workspace_delete_hold_row"));
    assert!(complete_hold_body.contains("begin_delete_workspace"));
    assert!(!shell_source.contains("fn prompt_delete_workspace"));
}

#[test]
fn picker_workspace_rename_targets_row_action_menu_workspace() {
    let shell_source = include_str!("../src/shell.rs");
    let begin_body = rust_function_body(shell_source, "fn begin_workspace_rename");
    let submit_body = rust_function_body(shell_source, "fn begin_submit_workspace_rename");

    assert!(begin_body.contains("row_action_menu_active"));
    assert!(begin_body.contains("open_rename_editor_for"));
    assert!(submit_body.contains("rename_editor_target"));
}

#[test]
fn picker_workspace_transitions_wait_for_in_flight_thread_edit() {
    assert_eq!(
        workspace_picker_transition_disabled_reason(WorkspacePickerTransitionBlockers::default()),
        None
    );

    for blockers in [
        WorkspacePickerTransitionBlockers {
            edit_rollback_work: true,
            ..WorkspacePickerTransitionBlockers::default()
        },
        WorkspacePickerTransitionBlockers {
            edit_replacement_work: true,
            ..WorkspacePickerTransitionBlockers::default()
        },
    ] {
        assert_eq!(
            workspace_picker_transition_disabled_reason(blockers),
            Some(WORKSPACE_PICKER_THREAD_EDIT_WAIT_NOTICE)
        );
    }
}

#[test]
fn picker_switch_create_and_delete_paths_wait_for_in_flight_thread_edit() {
    for path in [
        WorkspacePickerTransitionPath::SwitchWorkspace,
        WorkspacePickerTransitionPath::CreateWorkspace,
        WorkspacePickerTransitionPath::DeleteWorkspace,
    ] {
        assert_eq!(
            workspace_picker_transition_path_disabled_reason(
                path,
                WorkspacePickerTransitionBlockers::default()
            ),
            None
        );

        for blockers in [
            WorkspacePickerTransitionBlockers {
                edit_rollback_work: true,
                ..WorkspacePickerTransitionBlockers::default()
            },
            WorkspacePickerTransitionBlockers {
                edit_replacement_work: true,
                ..WorkspacePickerTransitionBlockers::default()
            },
        ] {
            assert_eq!(
                workspace_picker_transition_path_disabled_reason(path, blockers),
                Some(WORKSPACE_PICKER_THREAD_EDIT_WAIT_NOTICE)
            );
        }
    }
}

#[test]
fn shell_workspace_switch_create_and_delete_call_sites_apply_thread_edit_blocker() {
    let shell_source = include_str!("../src/shell.rs");

    assert_shell_transition_call_site_blocks_before_worker_start(
        shell_source,
        "fn activate_workspace_picker_item",
        "WorkspacePickerTransitionPath::SwitchWorkspace",
        "spawn_switch_workspace_worker",
    );
    assert_shell_transition_call_site_blocks_before_worker_start(
        shell_source,
        "fn begin_workspace_picker_create_new",
        "WorkspacePickerTransitionPath::CreateWorkspace",
        "spawn_create_workspace_worker",
    );
    assert_shell_transition_call_site_blocks_before_worker_start(
        shell_source,
        "fn begin_delete_workspace",
        "WorkspacePickerTransitionPath::DeleteWorkspace",
        "spawn_delete_workspace_worker",
    );
}

#[test]
fn shell_picker_create_and_delete_replacement_open_selected_runtime_via_primary_target() {
    let shell_source = include_str!("../src/shell.rs");
    let finish_create_body =
        rust_function_body(shell_source, "fn finish_workspace_picker_opened_workspace");
    let finish_delete_body =
        rust_function_body(shell_source, "fn finish_workspace_picker_deleted_workspace");

    assert!(finish_create_body.contains("loaded.selected_runtime().is_some()"));
    assert!(finish_create_body.contains("ShellState::WorkspaceLoaded(loaded)"));
    assert!(finish_create_body.contains("begin_open_target(RetryTarget::WorkspacePrimary"));
    assert!(!finish_create_body.contains("RetryTarget::HostPath"));
    assert!(!finish_create_body.contains("WorkspaceOpenIntent::UseAsPrimaryMember"));

    assert!(finish_delete_body.contains("loaded.selected_runtime().is_some()"));
    assert!(finish_delete_body.contains("ShellState::WorkspaceLoaded(loaded)"));
    assert!(finish_delete_body.contains("begin_open_target(RetryTarget::WorkspacePrimary"));
    assert!(!finish_delete_body.contains("RetryTarget::HostPath"));
    assert!(!finish_delete_body.contains("WorkspaceOpenIntent::UseAsPrimaryMember"));
}

#[test]
fn picker_member_path_projection_lists_explicit_member_paths_only() {
    let workspace = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("summary_workspace").unwrap(),
        "Summary Workspace",
        1,
    );
    let mut state = WorkspaceConversationState::default();

    state.select_runtime(RuntimeMode::HostWindows).unwrap();
    let projection =
        workspace_picker::workspace_picker_member_paths_from_states(&[workspace.clone()], |_| {
            Some(state.clone())
        });

    assert_eq!(
        projection.get(workspace.id()).unwrap(),
        &Vec::<String>::new()
    );

    state
        .attach_execution_target(&WorkspaceId::host_windows(r"C:\work\primary"))
        .unwrap();
    state
        .attach_execution_target(&WorkspaceId::host_windows(r"C:\work\secondary"))
        .unwrap();
    let projection =
        workspace_picker::workspace_picker_member_paths_from_states(&[workspace.clone()], |_| {
            Some(state.clone())
        });

    assert_eq!(
        projection.get(workspace.id()).unwrap(),
        &[
            r"C:\work\primary".to_string(),
            r"C:\work\secondary".to_string()
        ]
    );
}

#[test]
fn picker_member_path_projection_tolerates_unavailable_workspace_state() {
    let workspace = BerylWorkspaceManifest::named(
        BerylWorkspaceId::new("missing_state_workspace").unwrap(),
        "Missing State Workspace",
        1,
    );

    let projection =
        workspace_picker::workspace_picker_member_paths_from_states(&[workspace.clone()], |_| None);

    assert_eq!(
        projection.get(workspace.id()).unwrap(),
        &Vec::<String>::new()
    );
}

#[test]
fn workspace_rename_policy_reports_wait_reason_for_workspace_work() {
    assert_eq!(
        workspace_rename_disabled_reason(WorkspaceRenameBlockers::default()),
        None
    );

    for blockers in [
        WorkspaceRenameBlockers {
            workspace_lifecycle: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            graph_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            transcript_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            inventory_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            image_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            status_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            title_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            member_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            picker_work: true,
            ..WorkspaceRenameBlockers::default()
        },
        WorkspaceRenameBlockers {
            persistence_work: true,
            ..WorkspaceRenameBlockers::default()
        },
    ] {
        assert_eq!(
            workspace_rename_disabled_reason(blockers),
            Some(WORKSPACE_RENAME_WAIT_TOOLTIP)
        );
    }
}

fn assert_shell_transition_call_site_blocks_before_worker_start(
    shell_source: &str,
    function_signature: &str,
    path_token: &str,
    worker_start_token: &str,
) {
    let body = rust_function_body(shell_source, function_signature);
    let blocker_index = body
        .find("block_workspace_picker_transition_if_needed")
        .unwrap_or_else(|| panic!("{function_signature} must call the transition blocker"));
    let path_index = body
        .find(path_token)
        .unwrap_or_else(|| panic!("{function_signature} must pass {path_token}"));
    let worker_index = body
        .find(worker_start_token)
        .unwrap_or_else(|| panic!("{function_signature} must start {worker_start_token}"));

    assert!(
        blocker_index < worker_index,
        "{function_signature} must block thread-edit transitions before starting {worker_start_token}"
    );
    assert!(
        path_index < worker_index,
        "{function_signature} must bind {path_token} before starting {worker_start_token}"
    );
}

fn assert_px_close(actual: gpui::Pixels, expected: gpui::Pixels) {
    assert!(
        (f32::from(actual) - f32::from(expected)).abs() < 0.01,
        "expected {actual:?} to be within 0.01px of {expected:?}"
    );
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let signature_index = source
        .find(function_signature)
        .unwrap_or_else(|| panic!("missing shell function {function_signature}"));
    let after_signature = &source[signature_index..];
    let open_offset = after_signature
        .find('{')
        .unwrap_or_else(|| panic!("missing body for shell function {function_signature}"));
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

    panic!("unterminated body for shell function {function_signature}");
}
