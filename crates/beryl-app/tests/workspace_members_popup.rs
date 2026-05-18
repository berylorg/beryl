use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_backend::canonicalize_host_path;
use beryl_model::{
    conversation::{PrimaryWorkspaceMember, WorkspaceConversationState},
    workspace::{RuntimeMode, WorkspaceId, WorkspaceMemberId},
};
use gpui::{Bounds, point, px, size};
use workspace_members::{
    MemberPickerValidationError, NewThreadExecutionTargetError, WorkspaceMembersState,
    apply_workspace_member_attachment, apply_workspace_member_detach,
    apply_workspace_member_primary_selection, reconcile_workspace_member_availability,
    resolve_new_thread_execution_target, resolve_runtime_home_directory,
    validate_host_member_picker_path, validate_wsl_member_picker_path,
};
use workspace_picker::{
    RuntimeSelectorDistroList, RuntimeSelectorDistroListStatus, WorkspacePickerState,
};

#[path = "../src/shell/layout.rs"]
mod layout;

#[path = "../src/shell/workspace_picker.rs"]
mod workspace_picker;

#[path = "../src/shell/workspace_members.rs"]
mod workspace_members;

#[test]
fn members_state_tracks_path_prompt_without_standalone_popup_chrome() {
    let mut state = WorkspaceMembersState::default();

    assert!(!state.path_prompt_active());
    state.set_path_prompt_active(true);
    assert!(state.path_prompt_active());
    state.set_path_prompt_active(false);
    assert!(!state.path_prompt_active());
}

#[test]
fn merged_picker_dismissal_includes_runtime_dropdown_and_member_menu() {
    let mut picker = WorkspacePickerState::default();
    picker.open();
    picker.set_anchor_bounds(Some(Bounds::new(
        point(px(24.0), px(12.0)),
        size(px(160.0), px(36.0)),
    )));
    picker.set_popup_bounds(Some(Bounds::new(
        point(px(24.0), px(56.0)),
        size(px(840.0), px(320.0)),
    )));

    assert!(!picker.should_dismiss_for_mouse_down(point(px(40.0), px(24.0))));
    assert!(!picker.should_dismiss_for_mouse_down(point(px(40.0), px(80.0))));
    assert!(picker.should_dismiss_for_mouse_down(point(px(900.0), px(24.0))));

    assert!(picker.toggle_runtime_selector_dropdown(2));
    picker.set_runtime_selector_trigger_bounds(Some(Bounds::new(
        point(px(472.0), px(120.0)),
        size(px(320.0), px(26.0)),
    )));
    picker.set_runtime_selector_dropdown_bounds(Some(Bounds::new(
        point(px(472.0), px(145.0)),
        size(px(320.0), px(88.0)),
    )));

    assert!(!picker.should_dismiss_for_mouse_down(point(px(480.0), px(130.0))));
    assert!(!picker.should_dismiss_for_mouse_down(point(px(480.0), px(170.0))));
    assert!(
        !picker
            .should_dismiss_runtime_selector_dropdown_for_mouse_down(point(px(480.0), px(170.0)))
    );
    assert!(
        picker.should_dismiss_runtime_selector_dropdown_for_mouse_down(point(px(100.0), px(260.0)))
    );

    let member_id = WorkspaceMemberId::new("primary_member").unwrap();
    picker.open_member_action_menu(member_id, point(px(760.0), px(188.0)));
    assert!(!picker.runtime_selector_dropdown_is_open());
    picker.set_member_action_menu_bounds(Some(Bounds::new(
        point(px(700.0), px(180.0)),
        size(px(180.0), px(90.0)),
    )));

    assert!(!picker.should_dismiss_for_mouse_down(point(px(720.0), px(200.0))));
    assert!(!picker.should_dismiss_member_action_menu_for_mouse_down(point(px(720.0), px(200.0))));
    assert!(picker.should_dismiss_member_action_menu_for_mouse_down(point(px(100.0), px(260.0))));
}

#[test]
fn members_column_uses_picker_rows_and_no_old_popup_module() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let render_mod_source = include_str!("../src/shell/render.rs");
    let conversation_source = include_str!("../src/shell/render/conversation.rs");
    let members_source = include_str!("../src/shell/workspace_members.rs");
    let members_column_body = rust_function_body(render_source, "fn render_members_column");
    let member_rows_body = rust_function_body(render_source, "fn render_member_rows");
    let member_row_shell_body = rust_function_body(render_source, "fn member_row_shell");
    let old_popup_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shell/render/workspace_members.rs");

    assert!(members_column_body.contains("render_runtime_selector_control"));
    assert!(members_column_body.contains("render_scrollable_member_rows"));
    assert!(member_rows_body.contains("render_attach_member_row"));
    assert!(member_rows_body.contains("render_implicit_home_member_row"));
    assert!(member_rows_body.contains("render_explicit_member_row"));
    assert!(member_rows_body.contains("has_available_explicit_members"));
    assert!(member_row_shell_body.contains(".border_b_1()"));
    assert!(member_row_shell_body.contains("render_workspace_active_marker"));
    assert!(member_row_shell_body.contains("let background = shell.role_background("));
    assert!(member_row_shell_body.contains("BerylThemeRole::WorkspacePickerMemberRow"));
    assert!(member_row_shell_body.contains("shell.popup_surface_background()"));
    assert!(!member_row_shell_body.contains("let background = if primary"));
    assert!(!member_row_shell_body.contains("disabled_secondary_button"));
    assert!(!render_mod_source.contains("mod workspace_members;"));
    assert!(!conversation_source.contains("render_workspace_members_button"));
    assert!(!conversation_source.contains("toggle_workspace_members_popup"));
    assert!(!members_source.contains("WorkspaceMembersPopupState"));
    assert!(!members_source.contains("fn popup_width"));
    assert!(!members_source.contains("fn popup_height"));
    assert!(!old_popup_path.exists());
}

#[test]
fn members_column_actions_route_primary_detach_and_attach_guard() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let shell_source = include_str!("../src/shell.rs");
    let action_menu_body =
        rust_function_body(render_source, "fn render_workspace_member_action_menu");
    let explicit_label_body = rust_function_body(render_source, "fn explicit_member_display_label");
    let implicit_row_body = rust_function_body(render_source, "fn render_implicit_home_member_row");
    let attach_prompt_body = rust_function_body(shell_source, "fn prompt_attach_workspace_member");
    let finish_prompt_body =
        rust_function_body(shell_source, "fn finish_workspace_member_path_prompt");

    assert!(action_menu_body.contains("\"Make primary\""));
    assert!(action_menu_body.contains("make_workspace_member_primary"));
    assert!(action_menu_body.contains("else if available"));
    assert!(action_menu_body.contains("\"Path not found\""));
    assert!(action_menu_body.contains("\"Detach\""));
    assert!(action_menu_body.contains("prompt_detach_workspace_member"));
    assert!(explicit_label_body.contains("format!(\"{label} - path not found\")"));
    assert!(implicit_row_body.contains("\"Home directory\""));
    assert!(!implicit_row_body.contains("\"Primary\""));
    assert!(attach_prompt_body.contains("path_prompt_active()"));
    assert!(attach_prompt_body.contains("set_path_prompt_active(true)"));
    assert!(finish_prompt_body.contains("set_path_prompt_active(false)"));
}

#[test]
fn runtime_or_member_activation_opens_primary_workspace_target_from_loaded_shell() {
    let shell_source = include_str!("../src/shell.rs");
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let helper_body =
        rust_function_body(shell_source, "fn begin_primary_workspace_open_if_selected");
    let runtime_body = rust_function_body(shell_source, "fn select_workspace_runtime");
    let attach_body = rust_function_body(shell_source, "fn attach_workspace_member");
    let primary_body = rust_function_body(shell_source, "fn make_workspace_member_primary");
    let begin_attach_body =
        rust_function_body(shell_source, "fn begin_workspace_member_attach_resolution");
    let finish_attach_body =
        rust_function_body(shell_source, "fn finish_workspace_member_attach_resolution");
    let finish_prompt_body =
        rust_function_body(shell_source, "fn finish_workspace_member_path_prompt");
    let workspace_shell_state_mut_body =
        rust_function_body(shell_source, "fn workspace_shell_state_mut");
    let runtime_row_body = rust_function_body(render_source, "fn render_runtime_selector_row");

    assert!(workspace_shell_state_mut_body.contains("ShellState::BackendUnavailable"));
    assert!(helper_body.contains("workspace_shell_state_mut()"));
    assert!(helper_body.contains("selected_runtime().is_none()"));
    assert!(helper_body.contains("workspace_picker.close()"));
    assert!(helper_body.contains("RetryTarget::WorkspacePrimary"));
    assert!(helper_body.contains("begin_open_target"));
    assert!(!helper_body.contains("WorkspaceOpenIntent::UseAsPrimaryMember"));

    assert!(runtime_body.contains("begin_primary_workspace_open_if_selected(window, cx)"));
    assert!(!runtime_body.contains("WorkspaceOpenIntent::UseAsPrimaryMember"));
    assert!(runtime_row_body.contains("select_workspace_runtime(runtime.clone(), window, cx)"));

    assert!(attach_body.contains("begin_primary_workspace_open_if_selected(window, cx)"));
    assert!(!attach_body.contains("WorkspaceOpenIntent::UseAsPrimaryMember"));
    assert!(primary_body.contains("begin_primary_workspace_open_if_selected(window, cx)"));
    assert!(!primary_body.contains("WorkspaceOpenIntent::UseAsPrimaryMember"));
    assert!(begin_attach_body.contains("window.window_handle()"));
    assert!(begin_attach_body.contains("cx.update_window"));
    assert!(finish_attach_body.contains("attach_workspace_member(execution_target, window, cx)"));
    assert!(finish_prompt_body.contains("begin_workspace_member_attach_resolution"));
    assert!(finish_prompt_body.contains("window,"));
}

#[test]
fn runtime_selector_distro_list_sorts_dedups_and_reports_failures() {
    let mut list = RuntimeSelectorDistroList::default();

    assert!(list.should_refresh());
    assert!(list.begin_loading());
    assert!(!list.begin_loading());
    list.finish_loading(Ok(vec![
        "Ubuntu".to_string(),
        "Debian".to_string(),
        "Ubuntu".to_string(),
    ]));
    assert_eq!(list.status(), &RuntimeSelectorDistroListStatus::Loaded);
    assert_eq!(
        list.distro_names(),
        &["Debian".to_string(), "Ubuntu".to_string()]
    );
    assert!(!list.should_refresh());

    list.finish_loading(Err("wsl unavailable".to_string()));
    assert_eq!(
        list.status(),
        &RuntimeSelectorDistroListStatus::Failed("wsl unavailable".to_string())
    );
    assert!(list.distro_names().is_empty());
    assert!(list.should_refresh());
}

#[test]
fn runtime_selector_rendering_stays_enabled_when_explicit_members_are_attached() {
    let render_source = include_str!("../src/shell/render/workspace_picker.rs");
    let control_body = rust_function_body(render_source, "fn render_runtime_selector_control");
    let trigger_body = rust_function_body(render_source, "fn render_runtime_selector_trigger");
    let dropdown_body = rust_function_body(render_source, "fn render_runtime_selector_dropdown");

    assert!(!control_body.contains("runtime_locked"));
    assert!(control_body.contains("render_runtime_selector_trigger"));
    assert!(control_body.contains("toggle_workspace_runtime_selector_dropdown"));
    assert!(trigger_body.contains("\"Used for new attachments and home fallback.\""));
    assert!(!trigger_body.contains("\"Locked while explicit members are attached.\""));
    assert!(dropdown_body.contains(".id(\"workspace-runtime-selector-dropdown\")"));
    assert!(
        dropdown_body.contains(".left(px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_X))")
    );
    assert!(
        dropdown_body.contains(".right(px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_X))")
    );
}

#[test]
fn runtime_selector_rows_label_wsl_distros_with_prefix() {
    let distro_names = vec!["Ubuntu".to_string()];
    let row = workspace_picker::runtime_selector_row_for_index(&distro_names, 1).unwrap();

    assert_eq!(
        workspace_picker::runtime_selector_row_label(&row),
        "WSL: Ubuntu"
    );
}

#[test]
fn member_attachment_promotes_first_member_and_preserves_primary_on_secondary_attach() {
    let primary = WorkspaceId::host_windows(r"C:\work\primary");
    let secondary = WorkspaceId::host_windows(r"C:\work\secondary");
    let mut state = WorkspaceConversationState::default();

    assert!(apply_workspace_member_attachment(&mut state, &primary).unwrap());
    let primary_member_id = state.primary_explicit_member().unwrap().id().clone();

    assert!(apply_workspace_member_attachment(&mut state, &secondary).unwrap());

    assert_eq!(state.explicit_members().len(), 2);
    assert_eq!(
        state.primary_explicit_member().unwrap().id(),
        &primary_member_id
    );
}

#[test]
fn detaching_primary_falls_back_then_unlocks_runtime_with_implicit_home() {
    let first = WorkspaceId::host_windows(r"C:\work\first");
    let second = WorkspaceId::host_windows(r"C:\work\second");
    let mut state = WorkspaceConversationState::default();

    apply_workspace_member_attachment(&mut state, &first).unwrap();
    apply_workspace_member_attachment(&mut state, &second).unwrap();
    let first_member_id = state.explicit_members()[0].id().clone();
    let second_member_id = state.explicit_members()[1].id().clone();
    apply_workspace_member_primary_selection(&mut state, &second_member_id).unwrap();

    apply_workspace_member_detach(&mut state, &second_member_id).unwrap();
    assert_eq!(
        state.primary_explicit_member().unwrap().id(),
        &first_member_id
    );

    apply_workspace_member_detach(&mut state, &first_member_id).unwrap();
    assert!(state.explicit_members().is_empty());
    assert!(matches!(
        state.primary_member(),
        Some(PrimaryWorkspaceMember::ImplicitHome(
            RuntimeMode::HostWindows
        ))
    ));
    assert!(
        state
            .select_runtime(RuntimeMode::WslLinux {
                distro_name: "Debian".to_string(),
            })
            .unwrap()
    );
}

#[test]
fn host_member_picker_rejects_wsl_unc_paths() {
    let error = validate_host_member_picker_path(PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me"))
        .unwrap_err();

    assert!(matches!(
        error,
        MemberPickerValidationError::HostRejectedWslUnc { .. }
    ));
}

#[test]
fn wsl_member_picker_accepts_selected_distro_unc_paths() {
    let path = validate_wsl_member_picker_path(
        "Ubuntu",
        PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me\project"),
    )
    .unwrap();

    assert_eq!(path, PathBuf::from("/home/me/project"));
}

#[test]
fn new_thread_execution_target_uses_primary_explicit_member() {
    let primary = WorkspaceId::host_windows(r"C:\work\primary");
    let secondary = WorkspaceId::host_windows(r"C:\work\secondary");
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&primary).unwrap();
    state.attach_execution_target(&secondary).unwrap();

    let target = resolve_new_thread_execution_target(&state, &secondary).unwrap();

    assert_eq!(target, primary);
}

#[test]
fn new_thread_execution_target_uses_primary_explicit_member_across_active_runtime() {
    let primary = WorkspaceId::host_windows(r"C:\work\primary");
    let active = WorkspaceId::wsl_linux("Ubuntu", "/home/me/project");
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&primary).unwrap();

    let target = resolve_new_thread_execution_target(&state, &active).unwrap();

    assert_eq!(target, primary);
}

#[test]
fn new_thread_execution_target_requires_selected_runtime() {
    let state = WorkspaceConversationState::default();
    let active = WorkspaceId::host_windows(r"C:\Users\me");

    let error = resolve_new_thread_execution_target(&state, &active).unwrap_err();

    assert!(matches!(
        error,
        NewThreadExecutionTargetError::MissingRuntimeSelection
    ));
}

#[test]
fn new_thread_execution_target_resolves_host_implicit_home() {
    let active = WorkspaceId::host_windows(r"C:\Users\me");
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();

    let target = resolve_new_thread_execution_target(&state, &active).unwrap();

    assert_eq!(target.runtime_mode(), &RuntimeMode::HostWindows);
    assert_eq!(
        target.canonical_path(),
        resolve_runtime_home_directory(&RuntimeMode::HostWindows)
            .unwrap()
            .as_path()
    );
}

#[test]
fn reconcile_workspace_member_availability_marks_missing_primary_unavailable_and_promotes_fallback()
{
    let (missing_path, fallback_path) = unique_missing_and_available_paths();
    fs::create_dir_all(&fallback_path).unwrap();
    let fallback_path = canonicalize_host_path(&fallback_path).unwrap();
    let missing = WorkspaceId::host_windows(missing_path);
    let fallback = WorkspaceId::host_windows(fallback_path);
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&missing).unwrap();
    state.attach_execution_target(&fallback).unwrap();
    let missing_id = state.explicit_members()[0].id().clone();
    let fallback_id = state.explicit_members()[1].id().clone();

    assert!(reconcile_workspace_member_availability(&mut state));

    assert_eq!(state.explicit_members()[0].id(), &missing_id);
    assert!(!state.explicit_members()[0].is_available());
    assert_eq!(
        state.durable_primary_explicit_member_id(),
        Some(&fallback_id)
    );
    let _ = fs::remove_dir_all(fallback.canonical_path());
}

#[test]
fn wsl_member_picker_rejects_other_distro_unc_paths() {
    let error = validate_wsl_member_picker_path(
        "Ubuntu",
        PathBuf::from(r"\\wsl.localhost\Debian\home\me\project"),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        MemberPickerValidationError::WslDistroMismatch { .. }
    ));
}

fn unique_missing_and_available_paths() -> (PathBuf, PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    (
        std::env::temp_dir().join(format!("beryl_missing_member_{nanos}")),
        std::env::temp_dir().join(format!("beryl_available_member_{nanos}")),
    )
}

#[test]
fn wsl_member_picker_rejects_non_unc_paths() {
    let error = validate_wsl_member_picker_path("Ubuntu", PathBuf::from(r"C:\Users\me\project"))
        .unwrap_err();

    assert!(matches!(
        error,
        MemberPickerValidationError::WslRequiresUnc { .. }
    ));
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
