#[path = "../src/shell/layout.rs"]
pub(crate) mod layout;

#[path = "../src/shell/syntax_highlighting.rs"]
pub(crate) mod syntax_highlighting;

mod shell {
    pub(crate) use crate::layout;
    pub(crate) use crate::syntax_highlighting;

    pub(crate) struct ShellRenderStyleSnapshot;

    impl ShellRenderStyleSnapshot {
        pub(crate) fn scrollbar_thumb_color(&self) -> u32 {
            0x000000
        }
    }

    pub(crate) struct ShellView;

    impl ShellView {
        pub(crate) fn scrollbar_thumb_color(&self) -> u32 {
            0x000000
        }
    }
}

#[path = "../src/shell/render/code_panel.rs"]
mod code_panel;

#[path = "../src/shell/render/code_panel_projection_cache.rs"]
mod code_panel_projection_cache;

#[path = "../src/shell/render/scrollbars.rs"]
mod scrollbars;

use code_panel::CodePanelWrapMode;
use code_panel_projection_cache::{CodePanelProjectionCache, CodePanelSourceRevision};

fn source_revision(source: &str, label: Option<&str>) -> CodePanelSourceRevision {
    CodePanelSourceRevision::new(source, source, label, "```", "```")
}

fn source_revision_with_fences(
    source: &str,
    label: Option<&str>,
    opening_fence: &str,
    closing_fence: &str,
) -> CodePanelSourceRevision {
    CodePanelSourceRevision::new(source, source, label, opening_fence, closing_fence)
}

fn complete_ready_projection(
    cache: &mut CodePanelProjectionCache,
    owner_id: &str,
    source: &str,
    wrap_mode: CodePanelWrapMode,
) -> std::sync::Arc<code_panel::CodePanelDisplayProjection> {
    complete_ready_projection_for_revision(
        cache,
        owner_id,
        source_revision(source, Some("text")),
        wrap_mode,
    )
}

fn complete_ready_projection_for_revision(
    cache: &mut CodePanelProjectionCache,
    owner_id: &str,
    revision: CodePanelSourceRevision,
    wrap_mode: CodePanelWrapMode,
) -> std::sync::Arc<code_panel::CodePanelDisplayProjection> {
    let lookup = cache.lookup(owner_id, revision.clone(), wrap_mode);
    assert!(lookup.ready.is_none());
    let request = lookup
        .projection_request
        .expect("first projection lookup should schedule work off render path");
    let result = cache.complete_projection(request.project());
    assert!(result.display_changed);
    assert!(!result.stale);
    assert!(result.follow_up_request.is_none());
    let ready = cache.lookup(owner_id, revision, wrap_mode);
    assert!(ready.projection_request.is_none());
    ready
        .ready
        .expect("completed projection should be cached")
        .projection
}

#[test]
fn projection_cache_returns_pending_then_reuses_completed_projection() {
    let source = "plain line\n".repeat(10_000);
    let mut cache =
        CodePanelProjectionCache::new_with_estimated_bytes(8, source.len() + 1, 4_000_000);

    let projection = complete_ready_projection(
        &mut cache,
        "panel:1",
        source.as_str(),
        CodePanelWrapMode::NoWrap,
    );

    assert_eq!(projection.display_line_count(), 10_001);
    let stats = cache.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.scheduled_projections, 1);
    assert_eq!(stats.completed_projections, 1);
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.display_lines, 10_001);
}

#[test]
fn projection_cache_separates_changed_wrap_mode_without_reusing_stale_rows() {
    let source = "abcdef\n".repeat(5_000);
    let mut cache =
        CodePanelProjectionCache::new_with_estimated_bytes(8, source.len() + 1, 4_000_000);
    let no_wrap = complete_ready_projection(
        &mut cache,
        "panel:1",
        source.as_str(),
        CodePanelWrapMode::NoWrap,
    );

    let changed = cache.lookup(
        "panel:1",
        source_revision(source.as_str(), Some("text")),
        CodePanelWrapMode::Smart { columns: 2 },
    );

    assert!(changed.ready.is_none());
    let request = changed
        .projection_request
        .expect("wrap-mode change should schedule replacement projection");
    let result = cache.complete_projection(request.project());
    assert!(result.display_changed);
    let ready = cache.lookup(
        "panel:1",
        source_revision(source.as_str(), Some("text")),
        CodePanelWrapMode::Smart { columns: 2 },
    );
    assert!(ready.projection_request.is_none());
    let smart = ready.ready.expect("smart-wrap projection should be ready");
    let smart = smart.projection;
    assert!(smart.display_line_count() > no_wrap.display_line_count());
    assert_eq!(cache.stats().invalidations, 1);
}

#[test]
fn projection_cache_keeps_previous_projection_while_large_update_is_pending() {
    let source = "plain line\n".repeat(5_000);
    let updated_source = format!("{source}new tail\n");
    let mut cache =
        CodePanelProjectionCache::new_with_estimated_bytes(8, updated_source.len() + 1, 4_000_000);
    let previous = complete_ready_projection(
        &mut cache,
        "panel:1",
        source.as_str(),
        CodePanelWrapMode::NoWrap,
    );

    let pending = cache.lookup(
        "panel:1",
        source_revision(updated_source.as_str(), Some("text")),
        CodePanelWrapMode::NoWrap,
    );

    let displayed = pending
        .ready
        .expect("pending large update should keep the previous display stable");
    assert!(std::sync::Arc::ptr_eq(&displayed.projection, &previous));
    assert_eq!(displayed.source_revision.display_source(), source.as_str());
    let request = pending
        .projection_request
        .expect("large update should still schedule a replacement projection");
    let result = cache.complete_projection(request.project());
    assert!(result.display_changed);
    assert!(!result.stale);
    let ready = cache.lookup(
        "panel:1",
        source_revision(updated_source.as_str(), Some("text")),
        CodePanelWrapMode::NoWrap,
    );
    assert!(ready.projection_request.is_none());
    assert_eq!(
        ready.ready.unwrap().projection.display_line_count(),
        previous.display_line_count() + 1
    );
}

#[test]
fn projection_cache_pairs_stale_theme_display_with_stale_action_source_revision() {
    let source = "name = \"Old\"\n".repeat(2_000);
    let updated_source = "name = \"New\"\n".repeat(2_000);
    let mut cache =
        CodePanelProjectionCache::new_with_estimated_bytes(8, updated_source.len() + 1, 4_000_000);
    let previous = complete_ready_projection_for_revision(
        &mut cache,
        "panel:theme",
        source_revision_with_fences(
            source.as_str(),
            Some("beryl-theme"),
            "```beryl-theme",
            "```",
        ),
        CodePanelWrapMode::NoWrap,
    );

    let pending = cache.lookup(
        "panel:theme",
        source_revision_with_fences(
            updated_source.as_str(),
            Some("beryl-theme"),
            "```beryl-theme",
            "```",
        ),
        CodePanelWrapMode::NoWrap,
    );

    let displayed = pending
        .ready
        .expect("pending theme update should keep the previous display stable");
    assert!(std::sync::Arc::ptr_eq(&displayed.projection, &previous));
    assert_eq!(displayed.source_revision.display_source(), source.as_str());
    assert_eq!(displayed.source_revision.copy_source(), source.as_str());
    assert_eq!(
        displayed.source_revision.syntax_label(),
        Some("beryl-theme")
    );
    assert_eq!(
        displayed.source_revision.copy_opening_fence(),
        "```beryl-theme"
    );
    let request = pending
        .projection_request
        .expect("large theme update should still schedule replacement projection");

    let result = cache.complete_projection(request.project());
    assert!(result.display_changed);
    let ready = cache.lookup(
        "panel:theme",
        source_revision_with_fences(
            updated_source.as_str(),
            Some("beryl-theme"),
            "```beryl-theme",
            "```",
        ),
        CodePanelWrapMode::NoWrap,
    );
    let ready = ready
        .ready
        .expect("latest theme projection should be ready");
    assert_eq!(
        ready.source_revision.display_source(),
        updated_source.as_str()
    );
    assert_eq!(ready.source_revision.copy_source(), updated_source.as_str());
    assert_eq!(ready.source_revision.syntax_label(), Some("beryl-theme"));
    assert_eq!(ready.source_revision.copy_opening_fence(), "```beryl-theme");
}

#[test]
fn projection_cache_keeps_previous_projection_through_stale_streaming_updates() {
    let source = "plain line\n".repeat(5_000);
    let updated_source = format!("{source}first tail\n");
    let latest_source = format!("{updated_source}second tail\n");
    let mut cache =
        CodePanelProjectionCache::new_with_estimated_bytes(8, latest_source.len() + 1, 4_000_000);
    let previous = complete_ready_projection(
        &mut cache,
        "panel:1",
        source.as_str(),
        CodePanelWrapMode::NoWrap,
    );

    let pending = cache.lookup(
        "panel:1",
        source_revision(updated_source.as_str(), Some("text")),
        CodePanelWrapMode::NoWrap,
    );
    let stale_request = pending.projection_request.unwrap();
    let pending_ready = pending.ready.unwrap();
    assert!(std::sync::Arc::ptr_eq(&pending_ready.projection, &previous));
    assert_eq!(
        pending_ready.source_revision.display_source(),
        source.as_str()
    );

    let newer_pending = cache.lookup(
        "panel:1",
        source_revision(latest_source.as_str(), Some("text")),
        CodePanelWrapMode::NoWrap,
    );
    assert!(newer_pending.projection_request.is_none());
    let newer_pending_ready = newer_pending.ready.unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &newer_pending_ready.projection,
        &previous
    ));
    assert_eq!(
        newer_pending_ready.source_revision.display_source(),
        source.as_str()
    );

    let stale_result = cache.complete_projection(stale_request.project());
    assert!(stale_result.stale);
    assert!(!stale_result.display_changed);
    let follow_up = stale_result
        .follow_up_request
        .expect("stale completion should schedule the latest streamed source");
    let still_pending = cache.lookup(
        "panel:1",
        source_revision(latest_source.as_str(), Some("text")),
        CodePanelWrapMode::NoWrap,
    );
    assert!(still_pending.projection_request.is_none());
    let still_pending_ready = still_pending.ready.unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &still_pending_ready.projection,
        &previous
    ));
    assert_eq!(
        still_pending_ready.source_revision.display_source(),
        source.as_str()
    );

    let latest_result = cache.complete_projection(follow_up.project());
    assert!(latest_result.display_changed);
    assert!(!latest_result.stale);
    let ready = cache.lookup(
        "panel:1",
        source_revision(latest_source.as_str(), Some("text")),
        CodePanelWrapMode::NoWrap,
    );
    assert!(ready.projection_request.is_none());
    assert_eq!(
        ready
            .ready
            .as_ref()
            .unwrap()
            .projection
            .display_line_count(),
        previous.display_line_count() + 2
    );
    assert_eq!(
        ready.ready.unwrap().source_revision.display_source(),
        latest_source.as_str()
    );
}

#[test]
fn projection_cache_does_not_schedule_oversized_source_on_render_lookup() {
    let source = "plain line\n".repeat(32);
    let mut cache = CodePanelProjectionCache::new(8, 32);

    let lookup = cache.lookup(
        "panel:1",
        source_revision(source.as_str(), Some("text")),
        CodePanelWrapMode::NoWrap,
    );

    assert!(lookup.ready.is_none());
    assert!(lookup.projection_request.is_none());
    let stats = cache.stats();
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.pending_entries, 0);
    assert_eq!(stats.scheduled_projections, 0);
    assert_eq!(stats.uncached_oversize_lookups, 1);
    assert_eq!(stats.represented_source_bytes, 0);
}
