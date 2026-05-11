#[test]
fn workspace_shell_rendering_uses_initialized_controls_and_shared_composer_frame() {
    let render_source = include_str!("../src/shell/render/conversation.rs");
    let ready_shell_body = rust_function_body(render_source, "pub(super) fn render_ready_shell");
    let workspace_surface_body = rust_function_body(render_source, "fn render_workspace_surface");
    let toolbar_body = rust_function_body(render_source, "fn render_toolbar");
    let thread_strip_body = rust_function_body(render_source, "fn render_thread_strip");
    let split_surface_body = rust_function_body(render_source, "fn render_split_surface");
    let composer_body = rust_function_body(render_source, "fn render_composer(");
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
    assert!(composer_body.contains("render_composer_input_area"));
    assert!(loaded_composer_body.contains("render_composer_input_area"));
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
    assert!(!render_source.contains("disabled_secondary_button"));
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
