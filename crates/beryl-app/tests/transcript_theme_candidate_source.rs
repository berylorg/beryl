#[test]
fn beryl_theme_candidates_are_normal_code_blocks_with_extra_header_actions() {
    let controls_source = include_str!("../src/shell/render/transcript/code_panel_controls.rs");
    let block_source = include_str!("../src/shell/render/transcript/block_markdown.rs");
    let code_panel_source = include_str!("../src/shell/render/code_panel.rs");

    assert!(controls_source.contains("is_theme_candidate_language(syntax_label)"));
    assert!(controls_source.contains("\"theme-preview\""));
    assert!(controls_source.contains("\"theme-preview-stop\""));
    assert!(controls_source.contains("\"theme-install\""));
    assert!(controls_source.contains("self.soft_wrap_action(panel_id)"));
    assert!(controls_source.contains("source_revision.copy_source()"));
    assert!(block_source.contains("code_panel_controls.header("));
    assert!(block_source.contains("code_panel_source_revision(code)"));
    assert!(block_source.contains("code.header_copy_source()"));
    assert!(block_source.contains("labeled_code_block("));
    assert!(code_panel_source.contains("pub(crate) type CodePanelHeaderActionCallback"));
    assert!(code_panel_source.contains("Arc<dyn Fn(&mut Window, &mut App)>"));
}

#[test]
fn code_panel_actions_and_syntax_use_displayed_source_revision() {
    let controls_source = include_str!("../src/shell/render/transcript/code_panel_controls.rs");
    let block_source = include_str!("../src/shell/render/transcript/block_markdown.rs");

    assert!(controls_source.contains("CodePanelSourceRevision"));
    assert!(controls_source.contains("source_revision.copy_source()"));
    assert!(
        controls_source.contains("source_revision.and_then(CodePanelSourceRevision::syntax_label)")
    );
    assert!(
        block_source.contains("let display_source_revision = display_projection.source_revision;")
    );
    assert!(block_source.contains("let display_revision = display_source_revision.as_ref();"));
    assert!(
        block_source.contains("code_panel_controls.header(panel_id.as_str(), display_revision)")
    );
    assert!(block_source.contains("revision.display_source()"));
    assert!(block_source.contains("revision.syntax_label()"));
    assert!(block_source.contains("display_projection_input"));
    assert!(!block_source.contains(
        "code_panel_controls.header(\n        panel_id.as_str(),\n        code.language.as_deref(),"
    ));
}

#[test]
fn transcript_snapshots_carry_panel_state_without_synthetic_theme_offer_rows() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");
    let shell_source = include_str!("../src/shell.rs");

    assert!(transcript_source.contains("theme_candidates: ThemeCandidatePanelSnapshot"));
    assert!(transcript_source.contains("theme_candidates: Arc<ThemeCandidatePanelSnapshot>"));
    assert!(shell_source.contains("self.theme_candidate_state.snapshot()"));
    assert!(!transcript_source.contains("ThemeCandidateRow"));
    assert!(!transcript_source.contains("ThemeOfferRow"));
    assert!(!shell_source.contains("ThemeCandidateRow"));
    assert!(!shell_source.contains("ThemeOfferRow"));
}

#[test]
fn theme_candidate_panel_actions_defer_shell_updates_out_of_transcript_panel_update() {
    let transcript_source = include_str!("../src/shell/render/transcript.rs");

    assert!(transcript_source.contains(
        "cx.defer(move |cx| {\n            shell.update(cx, |shell, cx| {\n                shell.preview_transcript_theme_candidate"
    ));
    assert!(transcript_source.contains(
        "cx.defer(move |cx| {\n            shell.update(cx, |shell, cx| {\n                shell.stop_transcript_theme_candidate_preview"
    ));
    assert!(transcript_source.contains(
        "window.defer(cx, move |window, cx| {\n            shell.update(cx, |shell, cx| {\n                shell.prompt_install_transcript_theme_candidate"
    ));
    assert!(!transcript_source.contains(
        "self.shell.update(cx, |shell, cx| {\n            shell.preview_transcript_theme_candidate"
    ));
    assert!(!transcript_source.contains(
        "self.shell.update(cx, |shell, cx| {\n            shell.stop_transcript_theme_candidate_preview"
    ));
}

#[test]
fn install_theme_candidate_prompts_before_starting_repository_worker() {
    let shell_source = include_str!("../src/shell.rs");
    let prompt_index = shell_source
        .find("window.prompt(")
        .expect("theme candidate install should prompt the operator");
    let begin_index = shell_source
        .find("begin_theme_candidate_install")
        .expect("theme candidate install should use the repository worker");

    assert!(prompt_index < begin_index);
    assert!(shell_source.contains("theme_repository_store()"));
    assert!(shell_source.contains("spawn_theme_candidate_install_worker"));
    assert!(shell_source.contains("record_theme_repository_snapshot"));
}
