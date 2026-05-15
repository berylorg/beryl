#[path = "../src/shell/layout.rs"]
pub(crate) mod layout;

#[path = "../src/shell/syntax_highlighting.rs"]
pub(crate) mod syntax_highlighting;

mod shell {
    pub(crate) use crate::layout;
    pub(crate) use crate::syntax_highlighting;
}

#[path = "../src/shell/render/code_panel.rs"]
mod code_panel;

#[path = "../src/shell/render/code_panel_projection_cache.rs"]
mod code_panel_projection_cache;

#[path = "../src/shell/render/scrollbars.rs"]
mod scrollbars;

use code_panel::CodePanelWrapMode;
use code_panel_projection_cache::CodePanelProjectionCache;

fn complete_ready_projection(
    cache: &mut CodePanelProjectionCache,
    owner_id: &str,
    source: &str,
    wrap_mode: CodePanelWrapMode,
) -> std::sync::Arc<code_panel::CodePanelDisplayProjection> {
    let lookup = cache.lookup(owner_id, source, wrap_mode);
    assert!(lookup.projection.is_none());
    let request = lookup
        .projection_request
        .expect("first projection lookup should schedule work off render path");
    let result = cache.complete_projection(request.project());
    assert!(result.display_changed);
    assert!(!result.stale);
    assert!(result.follow_up_request.is_none());
    let ready = cache.lookup(owner_id, source, wrap_mode);
    assert!(ready.projection_request.is_none());
    ready
        .projection
        .expect("completed projection should be cached")
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
        source.as_str(),
        CodePanelWrapMode::Smart { columns: 2 },
    );

    assert!(changed.projection.is_none());
    let request = changed
        .projection_request
        .expect("wrap-mode change should schedule replacement projection");
    let result = cache.complete_projection(request.project());
    assert!(result.display_changed);
    let ready = cache.lookup(
        "panel:1",
        source.as_str(),
        CodePanelWrapMode::Smart { columns: 2 },
    );
    assert!(ready.projection_request.is_none());
    let smart = ready
        .projection
        .expect("smart-wrap projection should be ready");
    assert!(smart.display_line_count() > no_wrap.display_line_count());
    assert_eq!(cache.stats().invalidations, 1);
}

#[test]
fn projection_cache_does_not_schedule_oversized_source_on_render_lookup() {
    let source = "plain line\n".repeat(32);
    let mut cache = CodePanelProjectionCache::new(8, 32);

    let lookup = cache.lookup("panel:1", source.as_str(), CodePanelWrapMode::NoWrap);

    assert!(lookup.projection.is_none());
    assert!(lookup.projection_request.is_none());
    let stats = cache.stats();
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.pending_entries, 0);
    assert_eq!(stats.scheduled_projections, 0);
    assert_eq!(stats.uncached_oversize_lookups, 1);
    assert_eq!(stats.represented_source_bytes, 0);
}
