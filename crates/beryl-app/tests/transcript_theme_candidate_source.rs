const SHELL_SOURCE: &str = include_str!("../src/shell.rs");
const SYNDIC_TRANSCRIPT_PANEL_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/panel.rs");
const SYNDIC_TRANSCRIPT_COMMAND_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/command.rs");

#[test]
fn transcript_theme_candidate_controls_are_not_visible_host_compatibility_shims() {
    assert!(SHELL_SOURCE.contains("theme_repository_store()"));
    assert!(SHELL_SOURCE.contains("spawn_theme_candidate_install_worker"));
    assert!(SHELL_SOURCE.contains("record_theme_repository_snapshot"));

    for source in [SHELL_SOURCE, SYNDIC_TRANSCRIPT_PANEL_SOURCE] {
        assert!(!source.contains("ThemeCandidateRow"));
        assert!(!source.contains("ThemeOfferRow"));
        assert!(!source.contains("ThemeCandidatePanelSnapshot"));
    }

    assert!(!SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("preview_transcript_theme_candidate"));
    assert!(!SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("stop_transcript_theme_candidate_preview"));
    assert!(!SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("prompt_install_transcript_theme_candidate"));
}

#[test]
fn empty_resident_host_reports_transcript_commands_unavailable() {
    assert!(SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("unavailable_command(&self"));
    assert!(SYNDIC_TRANSCRIPT_COMMAND_SOURCE.contains("Unavailable(DisabledTranscriptCommand)"));
    assert!(SYNDIC_TRANSCRIPT_COMMAND_SOURCE.contains("Self::Unavailable"));
    assert!(
        SYNDIC_TRANSCRIPT_COMMAND_SOURCE
            .contains("resident transcript data is not available for this command")
    );
}

#[test]
fn live_shell_transcript_commands_route_through_resident_boundaries() {
    let quote_body = rust_function_body(SHELL_SOURCE, "fn insert_transcript_quote_into_draft");
    let copy_body = rust_function_body(SHELL_SOURCE, "fn copy_transcript_selection_action");
    let context_menu_body = rust_function_body(
        SHELL_SOURCE,
        "fn open_transcript_context_menu_from_resident_target",
    );
    let edit_body = rust_function_body(SHELL_SOURCE, "fn edit_resident_context_target_from_panel");
    let branch_body =
        rust_function_body(SHELL_SOURCE, "fn branch_resident_context_target_from_panel");
    let preview_body = rust_function_body(
        SHELL_SOURCE,
        "fn preview_resident_transcript_media_from_panel",
    );
    let media_copy_body =
        rust_function_body(SHELL_SOURCE, "fn copy_resident_transcript_media_from_panel");
    let media_copy_write_body = rust_function_body(
        SHELL_SOURCE,
        "fn write_resident_media_copy_payload_to_clipboard",
    );
    let media_save_body =
        rust_function_body(SHELL_SOURCE, "fn save_resident_transcript_media_to_path");
    let media_save_write_body =
        rust_function_body(SHELL_SOURCE, "fn write_resident_media_save_payload_to_path");
    let scroll_body = rust_function_body(SHELL_SOURCE, "fn apply_transcript_scroll_command");

    assert!(quote_body.contains("unavailable_command(\"quote_transcript_selection\")"));
    assert!(quote_body.contains(".resident_quote_payload()"));
    assert!(quote_body.contains("payload.quoted_markdown"));
    assert!(quote_body.contains("replace_selected_text(&quoted_markdown"));
    assert!(!quote_body.contains("transcript_quote::quote_insertion_for_draft"));
    assert!(copy_body.contains("unavailable_command(\"copy_transcript_selection\")"));
    assert!(copy_body.contains(".resident_copy_payload()"));
    assert!(!copy_body.contains("transcript_quote::quote_insertion_for_draft"));
    assert!(context_menu_body.contains(".resident_context_menu_command_target()"));
    assert!(context_menu_body.contains("unavailable_command(\"open_transcript_context_menu\")"));
    assert!(!context_menu_body.contains("transcript_branch"));
    assert!(!context_menu_body.contains("transcript_edit"));
    assert!(edit_body.contains(".resident_context_menu_command_target()"));
    assert!(edit_body.contains("ResidentEditCommandTarget::from_context_menu_command_target"));
    assert!(edit_body.contains("unavailable_command(\"edit_resident_context_target\")"));
    assert!(!edit_body.contains("transcript_edit"));
    assert!(branch_body.contains(".resident_context_menu_command_target()"));
    assert!(branch_body.contains("ResidentBranchCommandTarget::from_context_menu_command_target"));
    assert!(branch_body.contains("unavailable_command(\"branch_resident_context_target\")"));
    assert!(!branch_body.contains("transcript_branch"));
    assert!(preview_body.contains(".resident_media_preview_command_target()"));
    assert!(preview_body.contains("ResidentMediaPreviewCommandTarget::Targeted(payload)"));
    assert!(preview_body.contains("unavailable_command(\"preview_resident_transcript_media\")"));
    assert!(!preview_body.contains(".resident_media_action_payload()"));
    assert!(!preview_body.contains("transcript_image"));
    assert!(media_copy_body.contains(".resident_media_copy_command_target()"));
    assert!(media_copy_body.contains("ResidentMediaCopyCommandTarget::Targeted(payload)"));
    assert!(media_copy_body.contains("unavailable_command(\"copy_resident_transcript_media\")"));
    assert!(media_copy_write_body.contains("ClipboardItem::new_image(&image)"));
    assert!(media_copy_write_body.contains("Image::from_bytes(format, payload.bytes().to_vec())"));
    assert!(!media_copy_body.contains(".resident_media_action_payload()"));
    assert!(!media_copy_write_body.contains("read_from_clipboard"));
    assert!(media_save_body.contains(".resident_media_save_command_target()"));
    assert!(media_save_body.contains("ResidentMediaSaveCommandTarget::Targeted(payload)"));
    assert!(media_save_body.contains("unavailable_command(\"save_resident_transcript_media\")"));
    assert!(media_save_write_body.contains("payload.complete()"));
    assert!(media_save_write_body.contains("fs::write(destination.path(), payload.bytes())"));
    assert!(!media_save_body.contains(".resident_media_action_payload()"));
    assert!(!media_save_write_body.contains("read_from_clipboard"));
    assert!(!media_save_write_body.contains("Image::from_bytes"));
    assert!(scroll_body.contains("ScrollTranscriptCommand::Wheel"));
    assert!(scroll_body.contains("panel.manual_scroll_delta("));
    assert!(scroll_body.contains("let manual_delta_px = -delta_y;"));
    assert!(!scroll_body.contains("apply_transcript_wheel_command"));
    assert!(!scroll_body.contains("transcript_presentation().len()"));
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
