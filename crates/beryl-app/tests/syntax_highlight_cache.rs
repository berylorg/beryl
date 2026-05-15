#[path = "../src/shell/syntax_highlighting.rs"]
mod syntax_highlighting;

use std::{collections::HashSet, sync::Arc};

use syntax_highlighting::{SyntaxHighlightCache, SyntaxHighlightRequest, SyntaxTokenRole};

fn roles(highlight: &syntax_highlighting::SyntaxHighlight) -> Vec<SyntaxTokenRole> {
    highlight
        .tokens()
        .iter()
        .map(|token| token.role())
        .collect()
}

fn complete_request(
    cache: &mut SyntaxHighlightCache,
    request: SyntaxHighlightRequest,
) -> syntax_highlighting::SyntaxHighlightCompletionResult {
    cache.complete_highlight(request.highlight())
}

fn complete_ready_highlight(
    cache: &mut SyntaxHighlightCache,
    owner_id: &str,
    source: &str,
    label: Option<&str>,
) -> Arc<syntax_highlighting::SyntaxHighlight> {
    let lookup = cache.lookup(owner_id, source, label);
    let request = lookup
        .highlight_request
        .expect("first supported lookup should schedule highlighting");
    let result = complete_request(cache, request);
    assert!(result.display_changed);
    assert!(!result.stale);
    assert!(result.follow_up_request.is_none());
    let ready = cache.lookup(owner_id, source, label);
    assert!(ready.highlight_request.is_none());
    ready.highlight
}

#[test]
fn highlight_cache_returns_plain_pending_result_then_reuses_completed_entry_for_aliases() {
    let mut cache = SyntaxHighlightCache::new(8, 4096);

    let first = cache.lookup("panel:1", "# heading", Some("markdown"));
    assert!(first.highlight.is_plain());
    let request = first
        .highlight_request
        .expect("pending Markdown highlight should be scheduled off render path");

    let result = complete_request(&mut cache, request);
    assert!(result.display_changed);

    let second = cache.lookup("panel:1", "# heading", Some("md linenos"));

    assert!(second.highlight_request.is_none());
    assert_eq!(
        roles(&second.highlight),
        vec![SyntaxTokenRole::MarkupHeadingMarker]
    );

    let stats = cache.stats();
    assert_eq!(stats.lookups, 2);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.scheduled_highlights, 1);
    assert_eq!(stats.completed_highlights, 1);
    assert_eq!(stats.entries, 1);
}

#[test]
fn highlight_cache_separates_owner_identity_and_replaces_changed_source() {
    let mut cache = SyntaxHighlightCache::new(8, 4096);

    let first = complete_ready_highlight(&mut cache, "panel:1", "# heading", Some("markdown"));
    let second_owner =
        complete_ready_highlight(&mut cache, "panel:2", "# heading", Some("markdown"));
    let changed_pending = cache.lookup("panel:1", "`code`", Some("markdown"));

    assert!(changed_pending.highlight.is_plain());
    let changed_request = changed_pending
        .highlight_request
        .expect("changed source should schedule a replacement highlight");
    assert!(complete_request(&mut cache, changed_request).display_changed);

    let changed = cache.lookup("panel:1", "`code`", Some("markdown"));
    assert!(!Arc::ptr_eq(&first, &second_owner));
    assert!(!Arc::ptr_eq(&first, &changed.highlight));
    assert!(roles(&changed.highlight).contains(&SyntaxTokenRole::MarkupCodeSpan));

    let stats = cache.stats();
    assert_eq!(stats.misses, 2);
    assert_eq!(stats.invalidations, 1);
    assert_eq!(stats.entries, 2);
}

#[test]
fn highlight_cache_drops_pending_markdown_when_label_becomes_plain() {
    let mut cache = SyntaxHighlightCache::new(8, 4096);

    let markdown = cache.lookup("panel:1", "# heading", Some("markdown"));
    let stale_request = markdown
        .highlight_request
        .expect("Markdown lookup should schedule highlighting");

    let unsupported = cache.lookup("panel:1", "# heading", Some("rust"));
    let unlabeled = cache.lookup("panel:1", "# heading", None);

    assert!(unsupported.highlight.is_plain());
    assert!(unsupported.highlight_request.is_none());
    assert!(unlabeled.highlight.is_plain());
    assert!(unlabeled.highlight_request.is_none());
    assert_eq!(cache.stats().entries, 0);

    let stale = complete_request(&mut cache, stale_request);
    assert!(stale.stale);
    assert!(!stale.display_changed);
    assert!(stale.follow_up_request.is_none());

    let stats = cache.stats();
    assert_eq!(stats.uncached_plain_lookups, 2);
    assert_eq!(stats.stale_completions, 1);
}

#[test]
fn highlight_cache_bounds_entries_by_least_recent_use() {
    let mut cache = SyntaxHighlightCache::new(2, 4096);

    for index in 0..3 {
        complete_ready_highlight(
            &mut cache,
            &format!("panel:{index}"),
            &format!("# heading {index}"),
            Some("markdown"),
        );
    }

    let stats = cache.stats();
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.evictions, 1);

    let evicted = cache.lookup("panel:0", "# heading 0", Some("markdown"));
    assert!(evicted.highlight_request.is_some());
    assert_eq!(cache.stats().misses, 4);
}

#[test]
fn highlight_cache_bounds_represented_source_bytes() {
    let mut cache = SyntaxHighlightCache::new_with_estimated_bytes(8, 12, 4096);

    for (panel, source) in [
        ("panel:1", "# one"),
        ("panel:2", "# two"),
        ("panel:3", "# three"),
    ] {
        complete_ready_highlight(&mut cache, panel, source, Some("markdown"));
    }

    let stats = cache.stats();
    assert!(stats.represented_source_bytes <= 12);
    assert!(stats.entries < 3);
    assert!(stats.evictions > 0);
}

#[test]
fn highlight_cache_skips_oversized_completed_entries_without_retaining_tokens() {
    let mut cache = SyntaxHighlightCache::new_with_estimated_bytes(8, 4096, 80);

    let lookup = cache.lookup("panel:1", "# heading **bold**", Some("markdown"));
    assert!(lookup.highlight.is_plain());
    let result = complete_request(
        &mut cache,
        lookup
            .highlight_request
            .expect("small source should still schedule background highlighting"),
    );

    assert!(!result.display_changed);
    assert!(!result.stale);
    let stats = cache.stats();
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.uncached_oversize_lookups, 1);
}

#[test]
fn highlight_cache_does_not_schedule_oversized_source_on_render_lookup() {
    let mut cache = SyntaxHighlightCache::new(8, 32);
    let source = "# heading\n".repeat(8);

    let lookup = cache.lookup("panel:1", source.as_str(), Some("markdown"));

    assert!(lookup.highlight.is_plain());
    assert!(lookup.highlight_request.is_none());
    let stats = cache.stats();
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.pending_entries, 0);
    assert_eq!(stats.scheduled_highlights, 0);
    assert_eq!(stats.uncached_oversize_lookups, 1);
    assert_eq!(stats.represented_source_bytes, 0);
}

#[test]
fn highlight_cache_releases_disposed_panel_owners() {
    let mut cache = SyntaxHighlightCache::new(8, 4096);

    complete_ready_highlight(&mut cache, "row-a:panel:1", "# a", Some("markdown"));
    complete_ready_highlight(&mut cache, "row-b:panel:1", "# b", Some("markdown"));

    cache.release_owners_matching(|owner| owner.starts_with("row-a:"));

    let stats = cache.stats();
    assert_eq!(stats.entries, 1);

    let retained = HashSet::from(["row-b:panel:1".to_string()]);
    cache.retain_owners(&retained);
    assert_eq!(cache.stats().entries, 1);

    cache.retain_owners(&HashSet::new());
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn changed_source_while_pending_schedules_follow_up_and_keeps_plain_display() {
    let mut cache = SyntaxHighlightCache::new(8, 4096);

    complete_ready_highlight(&mut cache, "panel:1", "`old`", Some("markdown"));
    let middle = cache.lookup("panel:1", "# middle", Some("markdown"));
    assert!(middle.highlight.is_plain());
    let middle_completion = middle
        .highlight_request
        .expect("middle source should schedule highlighting")
        .highlight();

    let latest = cache.lookup("panel:1", "# latest", Some("markdown"));
    assert!(latest.highlight.is_plain());
    assert!(latest.highlight_request.is_none());

    let stale = cache.complete_highlight(middle_completion);
    assert!(stale.stale);
    assert!(!stale.display_changed);
    let follow_up = stale
        .follow_up_request
        .expect("latest source should be scheduled after stale completion");

    let pending_latest = cache.lookup("panel:1", "# latest", Some("markdown"));
    assert!(pending_latest.highlight.is_plain());
    assert!(pending_latest.highlight_request.is_none());

    assert!(complete_request(&mut cache, follow_up).display_changed);
    let ready = cache.lookup("panel:1", "# latest", Some("markdown"));
    assert_eq!(
        roles(&ready.highlight),
        vec![SyntaxTokenRole::MarkupHeadingMarker]
    );
}

#[test]
fn highlight_cache_rejects_completion_after_scope_clear() {
    let mut cache = SyntaxHighlightCache::new(8, 4096);

    let lookup = cache.lookup("panel:1", "- item", Some("markdown"));
    let completion = lookup
        .highlight_request
        .expect("Markdown lookup should schedule highlighting")
        .highlight();
    cache.clear();

    let result = cache.complete_highlight(completion);
    assert!(!result.display_changed);
    assert!(result.stale);
    assert!(result.follow_up_request.is_none());
    let stats = cache.stats();
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.stale_completions, 1);
}
