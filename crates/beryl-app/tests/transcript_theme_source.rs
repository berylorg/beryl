const SHELL_SOURCE: &str = include_str!("../src/shell.rs");
const SHELL_SELECTED_THREAD_ACTIVATION_SOURCE: &str =
    include_str!("../src/shell/selected_thread_activation.rs");
const SHELL_RENDER_THEME_SOURCE: &str = include_str!("../src/shell/render_theme.rs");
const SHELL_RENDER_THEME_FRAME_SOURCE: &str = include_str!("../src/shell/render_theme/frame.rs");
const SHELL_RENDER_THEME_ROLE_STYLE_SOURCE: &str =
    include_str!("../src/shell/render_theme/role_style.rs");
const SHELL_RENDER_SOURCE: &str = include_str!("../src/shell/render.rs");
const SHELL_RENDER_COMMON_SOURCE: &str = include_str!("../src/shell/render/common.rs");
const SHELL_RENDER_SCROLLBARS_SOURCE: &str = include_str!("../src/shell/render/scrollbars.rs");
const SHELL_TRANSCRIPT_PANEL_SNAPSHOT_SOURCE: &str =
    include_str!("../src/shell/transcript_panel_snapshot.rs");
const TRANSCRIPT_SOURCE: &str = include_str!("../src/shell/render/transcript.rs");
const TRANSCRIPT_PRESENTATION_SOURCE: &str =
    include_str!("../src/shell/transcript_presentation.rs");
const TRANSCRIPT_HISTORY_RESIDENCY_SOURCE: &str =
    include_str!("../src/shell/transcript_history/residency.rs");
const TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE: &str =
    include_str!("../src/shell/render/transcript/code_panel_controls.rs");
const TRANSCRIPT_NESTED_SCROLL_SOURCE: &str =
    include_str!("../src/shell/render/transcript/nested_scroll.rs");
const TRANSCRIPT_STREAM_PROJECTION_SOURCE: &str =
    include_str!("../src/shell/render/transcript/stream_projection.rs");
const TRANSCRIPT_THEME_SOURCE: &str = include_str!("../src/shell/render/transcript/theme.rs");
const TRANSCRIPT_INLINE_MARKDOWN_SOURCE: &str =
    include_str!("../src/shell/render/transcript/inline_markdown.rs");
const TRANSCRIPT_BLOCK_MARKDOWN_SOURCE: &str =
    include_str!("../src/shell/render/transcript/block_markdown.rs");
const TRANSCRIPT_TEXT_BLOCKS_SOURCE: &str =
    include_str!("../src/shell/render/transcript/text_blocks.rs");
const TRANSCRIPT_MEDIA_BLOCKS_SOURCE: &str =
    include_str!("../src/shell/render/transcript/media_blocks.rs");
const TRANSCRIPT_MEDIA_PRELOAD_SOURCE: &str =
    include_str!("../src/shell/render/transcript/media_preload.rs");
const TRANSCRIPT_MEDIA_PRELOAD_COORDINATOR_STATE_SOURCE: &str =
    include_str!("../src/shell/render/transcript/media_preload/coordinator_state.rs");
const TRANSCRIPT_ROW_MODEL_SOURCE: &str =
    include_str!("../src/shell/transcript_presentation/row_model.rs");
const TRANSCRIPT_TURN_BLOCKS_SOURCE: &str =
    include_str!("../src/shell/render/transcript/turn_blocks.rs");
const TRANSCRIPT_MARKDOWN_RENDER_CACHE_SOURCE: &str =
    include_str!("../src/shell/render/transcript/markdown_cache.rs");
const TRANSCRIPT_MARKDOWN_CODE_PANELS_SOURCE: &str =
    include_str!("../src/shell/transcript_markdown/code_panels.rs");
const TRANSCRIPT_TURN_MEDIA_UNITS_SOURCE: &str =
    include_str!("../src/shell/render/transcript/turn_media_units.rs");
const DIAGNOSTIC_DYNAMIC_TOOLS_SOURCE: &str = include_str!("../src/diagnostic_dynamic_tools.rs");
const CODE_PANEL_SOURCE: &str = include_str!("../src/shell/render/code_panel.rs");
const CODE_PANEL_BODY_SOURCE: &str = include_str!("../src/shell/render/code_panel/body.rs");
const CODE_PANEL_STYLED_TEXT_SOURCE: &str =
    include_str!("../src/shell/render/code_panel/styled_text.rs");

#[test]
fn phase6_transcript_render_sources_use_transcript_theme_boundary() {
    assert!(TRANSCRIPT_SOURCE.contains("pub theme: Arc<TranscriptTheme>"));
    assert!(!TRANSCRIPT_SOURCE.contains("pub appearance: AppearanceSettings"));
    assert_eq!(
        SHELL_TRANSCRIPT_PANEL_SNAPSHOT_SOURCE
            .matches("theme: transcript_theme.clone()")
            .count(),
        3
    );
    assert_eq!(
        SHELL_RENDER_THEME_SOURCE
            .matches("TranscriptTheme::from_active_theme(")
            .count(),
        1
    );

    for (path, source) in PHASE6_RENDER_SOURCES {
        assert!(
            !source.contains("AppearanceSettings"),
            "{path} should not consume legacy appearance settings"
        );
        assert!(
            !source.contains("appearance."),
            "{path} should not read legacy appearance fields"
        );
        assert!(
            !source.contains("rgb(0x"),
            "{path} should not embed visible hex colors"
        );
        assert!(
            !source.contains("rgba("),
            "{path} should not embed visible rgba colors"
        );
        assert!(
            !source.contains("hsla("),
            "{path} should not embed visible hsla colors"
        );
    }
}

#[test]
fn pending_thread_activation_does_not_replace_transcript_region() {
    let render_body = rust_function_body(
        TRANSCRIPT_SOURCE,
        "fn render(&mut self, window: &mut Window, cx: &mut Context<Self>)",
    );

    assert!(TRANSCRIPT_SOURCE.contains("pub pending_thread_activation_label: Option<String>"));
    assert!(SHELL_SELECTED_THREAD_ACTIVATION_SOURCE.contains("fn begin_thread_activation"));
    assert!(SHELL_SOURCE.contains("surface.begin_thread_activation("));
    assert!(!TRANSCRIPT_SOURCE.contains("pending_thread_activation_state"));
    assert!(!TRANSCRIPT_SOURCE.contains("has_pending_thread_activation"));
    assert!(!TRANSCRIPT_TEXT_BLOCKS_SOURCE.contains("Opening {label}"));
    assert!(!render_body.contains(".when_some(pending_thread_activation_label"));
    assert!(render_body.contains(".when(!has_turns"));
    assert!(render_body.contains(".when(has_turns"));
}

#[test]
fn phase12_render_theme_cache_owns_hot_theme_resolution() {
    let render_style_snapshot_body = rust_function_body(SHELL_SOURCE, "fn render_style_snapshot");
    let style_snapshot_body =
        rust_function_body(SHELL_RENDER_THEME_SOURCE, "fn style_snapshot(&self)");
    let transcript_panel_snapshot_body = rust_function_body(
        SHELL_TRANSCRIPT_PANEL_SNAPSHOT_SOURCE,
        "fn transcript_panel_snapshot",
    );

    assert!(SHELL_SOURCE.contains("render_theme_cache: RefCell<ShellRenderThemeCache>"));
    assert!(SHELL_RENDER_THEME_SOURCE.contains("struct ShellRenderThemeCache"));
    assert!(SHELL_RENDER_THEME_SOURCE.contains("style_snapshot: ShellRenderStyleSnapshot"));
    assert!(SHELL_RENDER_THEME_SOURCE.contains("pub(super) struct ShellRenderStyleSnapshot"));
    assert!(SHELL_SOURCE.contains("fn publish_active_theme_projection"));
    assert!(SHELL_SOURCE.contains("initial_render_theme_projection"));
    assert!(SHELL_SOURCE.contains("ShellRenderThemeCache::new(projection)"));
    assert!(
        render_style_snapshot_body.contains("self.render_theme_cache.borrow().style_snapshot()")
    );
    assert!(!render_style_snapshot_body.contains("active_theme.lock"));
    assert!(!render_style_snapshot_body.contains("ShellRenderThemeCache::new"));
    assert!(!render_style_snapshot_body.contains("ThemeResolver"));
    assert!(!render_style_snapshot_body.contains("resolve_style("));
    assert!(!render_style_snapshot_body.contains("from_active_theme"));
    assert!(SHELL_RENDER_THEME_SOURCE.contains("ShellRenderStyleSnapshot::new"));
    assert!(SHELL_RENDER_THEME_SOURCE.contains("TranscriptTheme::from_active_theme"));
    assert!(style_snapshot_body.contains("self.style_snapshot.clone()"));
    assert!(!render_style_snapshot_body.contains("active_theme_projection()"));

    assert_eq!(
        transcript_panel_snapshot_body
            .matches("theme: transcript_theme.clone()")
            .count(),
        3
    );
    assert!(
        transcript_panel_snapshot_body
            .contains("let style_snapshot = self.render_style_snapshot()")
    );
    assert!(transcript_panel_snapshot_body.contains("style_snapshot.transcript_theme()"));
    assert!(!transcript_panel_snapshot_body.contains("active_theme.lock"));
    assert!(!transcript_panel_snapshot_body.contains("resolve_style("));
    assert!(!transcript_panel_snapshot_body.contains("from_active_theme"));
    assert!(TRANSCRIPT_SOURCE.contains("let theme = snapshot.theme.clone();"));
    assert!(!TRANSCRIPT_SOURCE.contains("Arc::new(snapshot.theme.clone())"));
}

#[test]
fn separator_render_snapshot_uses_single_color_property() {
    assert!(SHELL_RENDER_THEME_SOURCE.contains("separator_color: style_single_color("));
    assert!(!SHELL_RENDER_THEME_SOURCE.contains("separator_color: style_border("));
    assert!(
        SHELL_RENDER_THEME_SOURCE.contains("scrollbar_thumb_color: style_single_color_packed_rgb(")
    );
    assert!(SHELL_RENDER_THEME_ROLE_STYLE_SOURCE.contains("BerylThemeProperty::Color"));
    assert!(SHELL_RENDER_THEME_ROLE_STYLE_SOURCE.contains("pub(super) fn style_single_color"));
    assert!(TRANSCRIPT_BLOCK_MARKDOWN_SOURCE.contains(".bg(theme.thematic_break.color())"));
    assert!(TRANSCRIPT_SOURCE.contains("theme.selection.text_background()"));
    assert!(!TRANSCRIPT_SOURCE.contains("theme.selection.background()"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("pub(crate) fn color(&self) -> Rgba"));
}

#[test]
fn phase14_shell_render_consumes_frame_style_snapshot() {
    assert!(SHELL_RENDER_SOURCE.contains("let frame = self.render_frame();"));
    assert!(SHELL_RENDER_SOURCE.contains("let shell = &frame;"));
    assert!(
        SHELL_RENDER_THEME_FRAME_SOURCE
            .contains("pub(in crate::shell) struct ShellRenderFrame<'a>")
    );
    assert!(SHELL_SOURCE.contains("ShellRenderFrame::new(self, self.render_style_snapshot())"));
    assert!(SHELL_RENDER_SCROLLBARS_SOURCE.contains("style: &ShellRenderStyleSnapshot"));
    assert!(SHELL_RENDER_SCROLLBARS_SOURCE.contains("style.scrollbar_thumb_color()"));
    assert!(SHELL_RENDER_COMMON_SOURCE.contains("panel_shell_with_style"));

    for (path, source) in PHASE14_SHELL_RENDER_SOURCES {
        assert!(
            source.contains("ShellRenderFrame"),
            "{path} should receive the frame style boundary"
        );
        assert!(
            !source.contains("with_render_theme_cache"),
            "{path} should not synchronize with the theme cache during render"
        );
        assert!(
            !source.contains("active_theme_projection()"),
            "{path} should not clone active theme projection during render"
        );
    }
}

#[test]
fn phase14_transcript_theme_precomputes_inline_code_styles() {
    let transcript_role_style_body = rust_function_body(
        TRANSCRIPT_THEME_SOURCE,
        "pub(crate) struct TranscriptRoleStyle",
    );

    assert!(!TRANSCRIPT_THEME_SOURCE.contains("active: ActiveThemeProjection"));
    assert!(!transcript_role_style_body.contains("ResolvedStyle"));
    assert!(!transcript_role_style_body.contains("resolved"));
    assert!(!TRANSCRIPT_THEME_SOURCE.contains("inline_code_for"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("struct TranscriptInlineCodeStyles"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("inline_code: TranscriptInlineCodeStyles"));
    assert!(
        TRANSCRIPT_THEME_SOURCE
            .contains("assistant_final: inline_code_style(theme, &paragraph.resolved)")
    );
    assert!(
        TRANSCRIPT_THEME_SOURCE.contains("heading: inline_code_style(theme, &heading.resolved)")
    );
    assert!(
        TRANSCRIPT_INLINE_MARKDOWN_SOURCE.contains(
            "theme.inline_code_style(inline_code_host(block_role, style, fragment_style))"
        )
    );
    assert!(!TRANSCRIPT_INLINE_MARKDOWN_SOURCE.contains("resolve_style("));
}

#[test]
fn phase13_transcript_render_avoids_redundant_hot_path_work() {
    let profile_new_body =
        rust_function_body(TRANSCRIPT_SOURCE, "fn new(\n        started_at: Instant,");
    let markdown_units_body = rust_function_body(
        TRANSCRIPT_TURN_MEDIA_UNITS_SOURCE,
        "fn markdown_render_units",
    );
    let no_media_check = markdown_units_body
        .find("if markdown.media_requests().is_empty()")
        .expect("markdown render units should check no-media input first");
    let segmentation = markdown_units_body
        .find("markdown_media_run_segments(markdown)")
        .expect("markdown render units should still split media-bearing Markdown");

    assert!(
        TRANSCRIPT_SOURCE.contains(
            "tracing::enabled!(Level::DEBUG).then(|| self.markdown_cache.borrow().stats())"
        )
    );
    assert!(TRANSCRIPT_SOURCE.contains("&& profiler.should_log_slow()"));
    assert!(!TRANSCRIPT_SOURCE.contains(
        "panel_state_inspected_row_count,\n            self.markdown_cache.borrow().stats(),"
    ));
    assert!(
        TRANSCRIPT_SOURCE.contains("markdown_cache_start: Option<TranscriptMarkdownCacheStats>")
    );
    assert!(profile_new_body.contains("markdown_cache_start,"));
    assert!(no_media_check < segmentation);
    assert!(markdown_units_body.contains("source: Cow::Borrowed(markdown.source())"));
    assert!(TRANSCRIPT_STREAM_PROJECTION_SOURCE.contains("Cow::Borrowed(authoritative_text)"));
    assert!(TRANSCRIPT_STREAM_PROJECTION_SOURCE.contains("self.entries.remove(&key);"));
    assert!(!TRANSCRIPT_SOURCE.contains("collect_turn_card_markdown_code_panel_ids("));
    assert!(TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE.contains("rendered_panel_ids"));
    assert!(TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE.contains("insert(panel_id.clone())"));
    assert!(TRANSCRIPT_SOURCE.contains("retain_rendered_code_panel_state"));
    assert!(TRANSCRIPT_SOURCE.contains("scoped_soft_wrapped_panel_keys_for_rows"));
}

#[test]
fn phase2_transcript_media_preload_uses_budgeted_coordinator() {
    let report_body = rust_function_body(
        TRANSCRIPT_SOURCE,
        "fn report_transcript_media_preload_facts",
    );
    let drain_body = rust_function_body(
        TRANSCRIPT_SOURCE,
        "fn drain_transcript_media_preload_coordinator",
    );
    let coordinator_drain_body = rust_function_body(
        TRANSCRIPT_MEDIA_PRELOAD_SOURCE,
        "pub(super) fn drain_pending",
    );
    let stale_check = coordinator_drain_body
        .find("if !row_is_current")
        .expect("drain should reject stale rows before media work");
    let media_work = coordinator_drain_body
        .find("preload_turn_media_runs")
        .expect("drain should invoke coordinator media work");

    assert!(TRANSCRIPT_SOURCE.contains("media_preload: TranscriptMediaPreloadCoordinator"));
    assert!(TRANSCRIPT_SOURCE.contains("TranscriptMediaPreloadRequest {"));
    assert!(TRANSCRIPT_SOURCE.contains("view.report_transcript_media_preload_facts(request);"));
    assert!(TRANSCRIPT_SOURCE.contains("window.defer(cx, move |window, cx|"));
    assert!(TRANSCRIPT_SOURCE.contains("view.drain_transcript_media_preload_coordinator"));
    assert!(!TRANSCRIPT_SOURCE.contains("fn preload_transcript_media_range"));
    assert!(!TRANSCRIPT_SOURCE.contains("media_preload::preload_turn_media_runs"));
    assert!(!TRANSCRIPT_SOURCE.contains("markdown_media_run_segments"));
    assert!(!report_body.contains("markdown_for"));
    assert!(!report_body.contains("media_for"));
    assert!(report_body.contains("begin_preload_frame"));
    assert!(report_body.contains("request_preload"));
    assert!(drain_body.contains("pending_preload_range"));
    assert!(drain_body.contains("row.identity.as_str().to_string()"));
    assert!(
        coordinator_drain_body.contains("stats.rows_stale = stats.rows_stale.saturating_add(1);")
    );
    assert!(stale_check < media_work);
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("mod coordinator_state;"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("TranscriptMediaPreloadCoordinator"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("TranscriptMediaPreloadBudget"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("request_preload"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("drain_pending"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("can_coalesce"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("superseded_requests"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("pending_preload_range"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("media_run_segments"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("budget.admit_markdown_source"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("remaining_load_requests"));
    assert!(TRANSCRIPT_MEDIA_PRELOAD_SOURCE.contains("remaining_upload_bytes"));
    assert!(
        TRANSCRIPT_MEDIA_PRELOAD_COORDINATOR_STATE_SOURCE
            .contains("TranscriptMediaPreloadDrainBudget")
    );
    assert!(
        TRANSCRIPT_MEDIA_PRELOAD_COORDINATOR_STATE_SOURCE.contains("preload_requests_can_coalesce")
    );
    assert!(TRANSCRIPT_MEDIA_PRELOAD_COORDINATOR_STATE_SOURCE.contains("ranges_overlap_or_touch"));
    assert!(
        TRANSCRIPT_MEDIA_PRELOAD_COORDINATOR_STATE_SOURCE
            .contains("impl TranscriptMediaRunSegmentCacheKey")
    );
    assert!(
        TRANSCRIPT_MEDIA_PRELOAD_COORDINATOR_STATE_SOURCE
            .contains("TranscriptMarkdownSourceRevision::new")
    );
    assert!(TRANSCRIPT_MEDIA_PRELOAD_COORDINATOR_STATE_SOURCE.contains("note_media_run"));
    assert!(TRANSCRIPT_MEDIA_BLOCKS_SOURCE.contains("remaining_load_requests"));
    assert!(TRANSCRIPT_MEDIA_BLOCKS_SOURCE.contains("remaining_upload_bytes"));
    assert!(TRANSCRIPT_MEDIA_BLOCKS_SOURCE.contains("preload_media_for"));
}

#[test]
fn phase3_transcript_rows_use_presentation_models_and_measurement_keys() {
    let render_card_body = rust_function_body(
        TRANSCRIPT_TURN_BLOCKS_SOURCE,
        "pub(super) fn render_turn_card",
    );
    let measurement_reconcile_body = rust_function_body(
        TRANSCRIPT_SOURCE,
        "fn reconcile_transcript_row_measurement_keys",
    );
    let markdown_completion_body = rust_function_body(
        TRANSCRIPT_MARKDOWN_RENDER_CACHE_SOURCE,
        "fn schedule_markdown_parse",
    );
    let preload_body = rust_function_body(
        TRANSCRIPT_MEDIA_PRELOAD_SOURCE,
        "fn preload_turn_media_runs",
    );

    assert!(TRANSCRIPT_PRESENTATION_SOURCE.contains("mod row_model;"));
    assert!(TRANSCRIPT_PRESENTATION_SOURCE.contains("model: Arc<TranscriptRowPresentationModel>"));
    assert!(TRANSCRIPT_PRESENTATION_SOURCE.contains("markdown_key_row_identities"));
    assert!(TRANSCRIPT_PRESENTATION_SOURCE.contains("row_index_for_markdown_key"));
    assert!(TRANSCRIPT_ROW_MODEL_SOURCE.contains("TranscriptRowPresentationModel"));
    assert!(TRANSCRIPT_ROW_MODEL_SOURCE.contains("TranscriptRowMeasurementKey"));
    assert!(TRANSCRIPT_ROW_MODEL_SOURCE.contains("TranscriptRowNarrativeUnit"));
    assert!(TRANSCRIPT_TURN_BLOCKS_SOURCE.contains("row_model.narrative_units()"));
    assert!(!render_card_body.contains("turn.narrative_entries()"));
    assert!(preload_body.contains("row_model.narrative_units()"));
    assert!(!preload_body.contains("turn.narrative_entries()"));
    assert!(TRANSCRIPT_SOURCE.contains("row_measurement_keys: HashMap"));
    assert!(measurement_reconcile_body.contains("measurement_key_for_row"));
    assert!(measurement_reconcile_body.contains("theme_revision"));
    assert!(measurement_reconcile_body.contains("transcript_width"));
    assert!(TRANSCRIPT_SOURCE.contains("row_code_panel_state_digest"));
    assert!(TRANSCRIPT_SOURCE.contains("promoted_media_key"));
    assert!(
        markdown_completion_body.contains("invalidate_transcript_row_measurement_for_markdown_key")
    );
}

#[test]
fn phase4_transcript_code_panel_state_is_typed_and_row_scoped() {
    assert!(TRANSCRIPT_MARKDOWN_CODE_PANELS_SOURCE.contains("TranscriptCodePanelIdentity"));
    assert!(TRANSCRIPT_MARKDOWN_CODE_PANELS_SOURCE.contains("TranscriptCodePanelLocalIdentity"));
    assert!(TRANSCRIPT_MARKDOWN_CODE_PANELS_SOURCE.contains("pub(crate) fn parse(panel_id: &str)"));
    assert!(TRANSCRIPT_SOURCE.contains("HashSet<TranscriptCodePanelIdentity>"));
    assert!(TRANSCRIPT_SOURCE.contains("HashMap<TranscriptCodePanelIdentity, Pixels>"));
    assert!(TRANSCRIPT_SOURCE.contains("HashMap<TranscriptCodePanelIdentity, ScrollHandle>"));
    assert!(
        TRANSCRIPT_SOURCE
            .contains("HashMap<TranscriptCodePanelIdentity, ScrollbarVisibilityState>")
    );
    assert!(TRANSCRIPT_SOURCE.contains("code_panel_owner_ids_by_row"));
    assert!(TRANSCRIPT_SOURCE.contains("record_rendered_code_panel_owners"));
    assert!(TRANSCRIPT_SOURCE.contains("code_panel_owner_ids_for_rows"));
    assert!(TRANSCRIPT_SOURCE.contains("row_identities.contains(panel_identity.row_identity())"));
    assert!(TRANSCRIPT_SOURCE.contains("owner_ids.contains(owner_id)"));
    assert!(!TRANSCRIPT_SOURCE.contains("panel_id_belongs_to_any_row"));
    assert!(!TRANSCRIPT_SOURCE.contains("markdown_code_panel_id_belongs_to_row"));
    assert!(
        TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE
            .contains("panel_id_for(&self, code_path: &str) -> TranscriptCodePanelIdentity")
    );
    assert!(
        TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE
            .contains("rendered_panel_ids: Rc<RefCell<HashSet<TranscriptCodePanelIdentity>>>")
    );
    assert!(TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE.contains("panel_id.as_str()"));
    assert!(
        TRANSCRIPT_NESTED_SCROLL_SOURCE
            .contains("selected_panel_identity: Option<TranscriptCodePanelIdentity>")
    );
    assert!(TRANSCRIPT_NESTED_SCROLL_SOURCE.contains("retain_visible_panel_identities"));
}

#[test]
fn phase16_transcript_frame_metrics_are_bounded_and_content_free() {
    let transcript_panel_snapshot_body = rust_function_body(
        SHELL_TRANSCRIPT_PANEL_SNAPSHOT_SOURCE,
        "fn transcript_panel_snapshot",
    );
    let render_metrics_body = rust_function_body(
        TRANSCRIPT_PRESENTATION_SOURCE,
        "pub(crate) fn render_metrics(&self)",
    );
    let residency_counts_body = rust_function_body(
        TRANSCRIPT_HISTORY_RESIDENCY_SOURCE,
        "pub(crate) fn retained_counts(&self)",
    );
    let residency_frame_body =
        rust_function_body(SHELL_SOURCE, "fn transcript_residency_frame_diagnostic");

    assert!(TRANSCRIPT_SOURCE.contains("frame_metrics: Rc<RefCell<TranscriptFrameMetricsLog>>"));
    assert!(TRANSCRIPT_SOURCE.contains("let frame_started_at = Instant::now();"));
    assert!(TRANSCRIPT_SOURCE.contains("let snapshot_started_at = Instant::now();"));
    assert!(TRANSCRIPT_SOURCE.contains("TranscriptFrameProfile::new("));
    assert!(TRANSCRIPT_SOURCE.contains("view.frame_metrics"));
    assert!(TRANSCRIPT_SOURCE.contains(".record(profiler.finish_metric())"));
    assert!(TRANSCRIPT_SOURCE.contains("selected_thread_id: self.selected_thread_id.clone()"));
    assert!(TRANSCRIPT_SOURCE.contains("presentation_range: Some(range_diagnostic"));
    assert!(TRANSCRIPT_SOURCE.contains("visible_range: Some(range_diagnostic"));
    assert!(TRANSCRIPT_SOURCE.contains("dominant_cost_category"));
    assert!(TRANSCRIPT_SOURCE.contains("observe_media_preload"));
    assert!(TRANSCRIPT_SOURCE.contains("snapshot_micros"));
    assert!(TRANSCRIPT_SOURCE.contains("render_state_pruning_micros"));
    assert!(TRANSCRIPT_SOURCE.contains("style_snapshot_micros"));
    assert!(TRANSCRIPT_SOURCE.contains("composer_measurement_micros"));
    assert!(TRANSCRIPT_SOURCE.contains("render_state_pruning"));
    assert!(TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE.contains("observe_code_panel_render"));
    assert!(TRANSCRIPT_CODE_PANEL_CONTROLS_SOURCE.contains("observe_inline_text_construction"));
    assert!(TRANSCRIPT_BLOCK_MARKDOWN_SOURCE.contains("Instant::now()"));
    assert!(TRANSCRIPT_BLOCK_MARKDOWN_SOURCE.contains("observe_inline_text_construction"));
    assert!(TRANSCRIPT_MEDIA_BLOCKS_SOURCE.contains("observe_media_run_render"));
    assert!(DIAGNOSTIC_DYNAMIC_TOOLS_SOURCE.contains("TranscriptFrameMetricsLog"));
    assert!(DIAGNOSTIC_DYNAMIC_TOOLS_SOURCE.contains("READ_TRANSCRIPT_FRAME_METRICS_TOOL"));
    assert!(DIAGNOSTIC_DYNAMIC_TOOLS_SOURCE.contains("snapshot_micros"));
    assert!(DIAGNOSTIC_DYNAMIC_TOOLS_SOURCE.contains("render_state_pruning_micros"));
    assert!(!DIAGNOSTIC_DYNAMIC_TOOLS_SOURCE.contains("transcript_text"));
    assert!(
        SHELL_SOURCE.contains("dispatch_beryl_transcript_frame_metrics_dynamic_tool_call"),
        "frame-metrics diagnostics should avoid full retained-state snapshots"
    );
    assert!(
        TRANSCRIPT_PRESENTATION_SOURCE.contains("render_metrics: TranscriptRenderMetrics"),
        "presentation state should own cached aggregate frame metrics"
    );
    assert!(
        TRANSCRIPT_PRESENTATION_SOURCE.contains("metrics: TranscriptPresentationRowMetrics"),
        "presentation rows should cache their own metric contribution"
    );
    assert!(render_metrics_body.contains("let mut metrics = self.render_metrics;"));
    assert!(render_metrics_body.contains("metrics.total_turns = self.rows.len();"));
    assert!(
        !render_metrics_body.contains(".iter()"),
        "render_metrics must not scan retained transcript rows"
    );
    assert!(
        !render_metrics_body.contains("text_char_count"),
        "render_metrics must not inspect retained turn text"
    );
    assert_eq!(
        transcript_panel_snapshot_body
            .matches(".render_metrics()")
            .count(),
        3,
        "render snapshots may read O(1) cached transcript metrics for each shell state"
    );
    assert!(residency_counts_body.contains("self.stats."));
    assert!(
        !residency_counts_body.contains(".values()"),
        "residency retained counts must copy maintained stats rather than scanning entries"
    );
    assert!(
        !residency_frame_body.contains(".retained_counts()"),
        "frame residency diagnostics must not scan retained history/page state"
    );
    assert!(
        !transcript_panel_snapshot_body.contains("retained_state_snapshot"),
        "transcript snapshot construction must not run retained-state diagnostics"
    );
}

#[test]
fn transcript_theme_adapter_has_no_literal_visual_fallbacks() {
    assert!(!TRANSCRIPT_THEME_SOURCE.contains("rgb(0x"));
    assert!(!TRANSCRIPT_THEME_SOURCE.contains("DEFAULT_CODE_FONT_FAMILY"));
    assert!(!TRANSCRIPT_THEME_SOURCE.contains("DEFAULT_CODE_FONT_SIZE"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("Beryl theme role {} must resolve"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("missing resolved property"));
}

#[test]
fn phase6_style_sensitive_transcript_caches_clear_on_theme_revision() {
    let body = rust_function_body(TRANSCRIPT_SOURCE, "fn sync_theme_revision");

    assert!(TRANSCRIPT_SOURCE.contains("self.sync_theme_revision(snapshot.theme.revision())"));
    assert!(TRANSCRIPT_SOURCE.contains("self.handled_theme_revision = None"));
    assert!(body.contains("self.syntax_highlight_cache.borrow_mut().clear()"));
    assert!(body.contains("self.code_panel_projection_cache.borrow_mut().clear()"));
    assert!(body.contains("self.visible_text_frame.clear()"));
    assert!(body.contains("self.next_visible_text_frame.clear()"));
    assert!(body.contains("self.visible_text_geometry.clear()"));
    assert!(body.contains("self.next_visible_text_geometry.clear()"));
    assert!(body.contains("self.visible_text_geometry_viewport_bounds = None"));
    assert!(body.contains("self.visible_text_hit_geometry.clear()"));
    assert!(body.contains("self.next_visible_text_hit_geometry.clear()"));
}

#[test]
fn phase6_code_panel_uses_themed_syntax_roles_and_typography() {
    assert!(TRANSCRIPT_THEME_SOURCE.contains("CodePanelSyntaxTheme::from_role_foregrounds"));
    assert!(CODE_PANEL_STYLED_TEXT_SOURCE.contains("role_foregrounds"));
    assert!(CODE_PANEL_STYLED_TEXT_SOURCE.contains("syntax_theme.foreground_for_role(span.role)"));
    assert!(CODE_PANEL_BODY_SOURCE.contains(".text_size(px(syntax_theme.font_size()))"));
    assert!(CODE_PANEL_BODY_SOURCE.contains(".line_height(px(syntax_theme.line_height()))"));
    assert!(
        CODE_PANEL_BODY_SOURCE.contains(".font_family(syntax_theme.font_family().to_string())")
    );
    assert!(CODE_PANEL_SOURCE.contains("smart_wrap_columns_for_style"));
    assert!(TRANSCRIPT_SOURCE.contains("theme.code_panel_body_text.font_family()"));

    for mapping in SYNTAX_ROLE_MAPPINGS {
        assert!(
            TRANSCRIPT_THEME_SOURCE.contains(mapping),
            "missing syntax role mapping {mapping}"
        );
    }
}

#[test]
fn phase52_transcript_code_panel_render_uses_split_surface_and_text_roles() {
    assert!(TRANSCRIPT_THEME_SOURCE.contains("BerylThemeRole::TranscriptUserInputText"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("BerylThemeRole::TranscriptQuotePopupText"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("BerylThemeRole::CodePanelHeaderText"));
    assert!(TRANSCRIPT_THEME_SOURCE.contains("BerylThemeRole::CodePanelBodyText"));
    assert!(TRANSCRIPT_SOURCE.contains("theme.code_panel_body_text.font_family()"));
    assert!(TRANSCRIPT_SOURCE.contains("theme.quote_popup_text.foreground()"));
    assert!(TRANSCRIPT_TEXT_BLOCKS_SOURCE.contains("theme.code_panel_body.background()"));
    assert!(TRANSCRIPT_TEXT_BLOCKS_SOURCE.contains("theme.code_panel_body_text.foreground()"));
    assert!(TRANSCRIPT_TEXT_BLOCKS_SOURCE.contains("theme.code_panel_header_text.foreground()"));
    assert!(!TRANSCRIPT_TEXT_BLOCKS_SOURCE.contains("theme.code_panel_body.font_family()"));
    assert!(!TRANSCRIPT_TEXT_BLOCKS_SOURCE.contains("theme.code_panel_header.font_weight()"));
}

const PHASE6_RENDER_SOURCES: &[(&str, &str)] = &[
    (
        "src/shell/render/transcript.rs",
        include_str!("../src/shell/render/transcript.rs"),
    ),
    (
        "src/shell/render/transcript/block_markdown.rs",
        include_str!("../src/shell/render/transcript/block_markdown.rs"),
    ),
    (
        "src/shell/render/transcript/inline_markdown.rs",
        include_str!("../src/shell/render/transcript/inline_markdown.rs"),
    ),
    (
        "src/shell/render/transcript/item_blocks.rs",
        include_str!("../src/shell/render/transcript/item_blocks.rs"),
    ),
    (
        "src/shell/render/transcript/media_blocks.rs",
        include_str!("../src/shell/render/transcript/media_blocks.rs"),
    ),
    (
        "src/shell/render/transcript/text_blocks.rs",
        include_str!("../src/shell/render/transcript/text_blocks.rs"),
    ),
    (
        "src/shell/render/transcript/turn_blocks.rs",
        include_str!("../src/shell/render/transcript/turn_blocks.rs"),
    ),
    (
        "src/shell/render/transcript/turn_item_media_units.rs",
        include_str!("../src/shell/render/transcript/turn_item_media_units.rs"),
    ),
    (
        "src/shell/render/transcript/turn_user_media_units.rs",
        include_str!("../src/shell/render/transcript/turn_user_media_units.rs"),
    ),
    (
        "src/shell/render/code_panel.rs",
        include_str!("../src/shell/render/code_panel.rs"),
    ),
    (
        "src/shell/render/code_panel/body.rs",
        include_str!("../src/shell/render/code_panel/body.rs"),
    ),
    (
        "src/shell/render/code_panel/styled_text.rs",
        include_str!("../src/shell/render/code_panel/styled_text.rs"),
    ),
];

const PHASE14_SHELL_RENDER_SOURCES: &[(&str, &str)] = &[
    (
        "src/shell/render/common.rs",
        include_str!("../src/shell/render/common.rs"),
    ),
    (
        "src/shell/render/conversation.rs",
        include_str!("../src/shell/render/conversation.rs"),
    ),
    (
        "src/shell/render/startup.rs",
        include_str!("../src/shell/render/startup.rs"),
    ),
    (
        "src/shell/render/workspace_picker.rs",
        include_str!("../src/shell/render/workspace_picker.rs"),
    ),
    (
        "src/shell/render/thread_selector.rs",
        include_str!("../src/shell/render/thread_selector.rs"),
    ),
    (
        "src/shell/render/status_operation.rs",
        include_str!("../src/shell/render/status_operation.rs"),
    ),
    (
        "src/shell/render/graph_overlay.rs",
        include_str!("../src/shell/render/graph_overlay.rs"),
    ),
];

const SYNTAX_ROLE_MAPPINGS: &[&str] = &[
    "SyntaxTokenRole::MarkupHeadingMarker => BerylThemeRole::SyntaxMarkupHeadingMarker",
    "SyntaxTokenRole::MarkupQuoteMarker => BerylThemeRole::SyntaxMarkupQuoteMarker",
    "SyntaxTokenRole::MarkupListMarker => BerylThemeRole::SyntaxMarkupListMarker",
    "SyntaxTokenRole::MarkupThematicBreak => BerylThemeRole::SyntaxMarkupThematicBreak",
    "SyntaxTokenRole::MarkupFenceDelimiter => BerylThemeRole::SyntaxMarkupFenceDelimiter",
    "SyntaxTokenRole::MarkupFenceInfo => BerylThemeRole::SyntaxMarkupFenceInfo",
    "SyntaxTokenRole::MarkupCodeBlock => BerylThemeRole::SyntaxMarkupCodeBlock",
    "SyntaxTokenRole::MarkupCodeSpanDelimiter => BerylThemeRole::SyntaxMarkupCodeSpanDelimiter",
    "SyntaxTokenRole::MarkupCodeSpan => BerylThemeRole::SyntaxMarkupCodeSpan",
    "SyntaxTokenRole::MarkupEmphasisDelimiter => BerylThemeRole::SyntaxMarkupEmphasisDelimiter",
    "SyntaxTokenRole::MarkupStrongDelimiter => BerylThemeRole::SyntaxMarkupStrongDelimiter",
    "SyntaxTokenRole::MarkupLinkText => BerylThemeRole::SyntaxMarkupLinkText",
    "SyntaxTokenRole::MarkupLinkDestination => BerylThemeRole::SyntaxMarkupLinkDestination",
    "SyntaxTokenRole::MarkupImageMarker => BerylThemeRole::SyntaxMarkupImageMarker",
    "SyntaxTokenRole::MarkupPunctuation => BerylThemeRole::SyntaxMarkupPunctuation",
    "SyntaxTokenRole::MarkupHtml => BerylThemeRole::SyntaxMarkupHtml",
    "SyntaxTokenRole::Escape => BerylThemeRole::SyntaxEscape",
    "SyntaxTokenRole::SyntaxStructuralPunctuation => BerylThemeRole::SyntaxStructuralPunctuation",
    "SyntaxTokenRole::SyntaxKey => BerylThemeRole::SyntaxKey",
    "SyntaxTokenRole::SyntaxString => BerylThemeRole::SyntaxString",
    "SyntaxTokenRole::SyntaxNumber => BerylThemeRole::SyntaxNumber",
    "SyntaxTokenRole::SyntaxBoolean => BerylThemeRole::SyntaxBoolean",
    "SyntaxTokenRole::SyntaxNull => BerylThemeRole::SyntaxNull",
    "SyntaxTokenRole::SyntaxDateTime => BerylThemeRole::SyntaxDateTime",
    "SyntaxTokenRole::SyntaxComment => BerylThemeRole::SyntaxComment",
    "SyntaxTokenRole::SyntaxSectionHeader => BerylThemeRole::SyntaxSectionHeader",
    "SyntaxTokenRole::SyntaxAssignment => BerylThemeRole::SyntaxAssignment",
    "SyntaxTokenRole::SyntaxEscape => BerylThemeRole::SyntaxTokenEscape",
    "SyntaxTokenRole::SyntaxError => BerylThemeRole::SyntaxError",
];

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let signature_index = source
        .find(function_signature)
        .unwrap_or_else(|| panic!("missing function signature {function_signature}"));
    let after_signature = &source[signature_index..];
    let body_start = signature_index
        + after_signature
            .find('{')
            .unwrap_or_else(|| panic!("missing function body for {function_signature}"));
    let mut depth = 0usize;

    for (offset, character) in source[body_start..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[body_start..body_start + offset + character.len_utf8()];
                }
            }
            _ => {}
        }
    }

    panic!("unterminated function body for {function_signature}");
}
