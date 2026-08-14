#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

use transcript_markdown::{
    Block, BlockRenderNode, Document, Inline, InlineRenderLine, ParsedTranscriptMarkdown,
    TranscriptMarkdownCache, TranscriptMarkdownCacheKey, TranscriptMarkdownParseCompletionResult,
};

#[test]
fn markdown_cache_reuses_completed_parse_for_same_source_generation() {
    let mut cache = TranscriptMarkdownCache::new(8, 4096);
    let key = TranscriptMarkdownCacheKey::new("turn:1:assistant");

    let first = cache.lookup(key.clone(), "**ready**");
    assert!(first.parse_request.is_some());
    assert!(first.markdown.used_parser_fallback());

    let completion = first.parse_request.unwrap().parse();
    assert_display_changed(cache.complete_parse(completion));

    let second = cache.lookup(key, "**ready**");
    assert!(second.parse_request.is_none());
    assert!(!second.markdown.used_parser_fallback());

    let stats = cache.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.ready_hits, 1);
    assert_eq!(stats.completed_parses, 1);
    assert_eq!(stats.entries, 1);
}

#[test]
fn parsed_markdown_reports_retained_block_and_inline_counts() {
    let markdown = ParsedTranscriptMarkdown::from_test_document(
        Document::new(vec![Block::paragraph(vec![
            Inline::text("plain"),
            Inline::strong(vec![Inline::text("bold")]),
        ])]),
        "plain **bold**",
    );

    let counts = markdown.structure_counts();
    assert_eq!(counts.blocks, 1);
    assert_eq!(counts.inlines, 3);
    assert_eq!(counts.media_requests, 0);
}

#[test]
fn markdown_cache_keeps_previous_parsed_snapshot_while_changed_source_parses() {
    let mut cache = TranscriptMarkdownCache::new(8, 4096);
    let key = TranscriptMarkdownCacheKey::new("turn:1:assistant");

    let first = cache.lookup(key.clone(), "plain");
    assert_display_changed(cache.complete_parse(first.parse_request.unwrap().parse()));

    let changed = cache.lookup(key.clone(), "# heading");
    assert!(changed.parse_request.is_some());
    assert!(!changed.markdown.used_parser_fallback());
    assert_eq!(paragraph_text(&changed.markdown), "plain");

    assert_display_changed(cache.complete_parse(changed.parse_request.unwrap().parse()));

    let ready = cache.lookup(key, "# heading");
    let [BlockRenderNode::Heading { level, .. }] = ready.markdown.render_plan().blocks.as_slice()
    else {
        panic!("expected parsed heading after source invalidation");
    };
    assert_eq!(*level, 1);

    let stats = cache.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.invalidations, 1);
    assert_eq!(stats.completed_parses, 2);
    assert_eq!(stats.entries, 1);
}

#[test]
fn markdown_cache_coalesces_fast_append_updates_while_parse_pending() {
    let mut cache = TranscriptMarkdownCache::new(8, 4096);
    let key = TranscriptMarkdownCacheKey::new("turn:1:assistant");

    let first = cache.lookup(key.clone(), "- first");
    assert!(first.parse_request.is_some());

    let second = cache.lookup(key.clone(), "- first\n- second");
    assert!(second.parse_request.is_none());

    let third = cache.lookup(key, "- first\n- second\n- third");
    assert!(third.parse_request.is_none());

    let stats = cache.stats();
    assert_eq!(stats.scheduled_parses, 1);
    assert_eq!(stats.pending_entries, 1);

    let result = cache.complete_parse(first.parse_request.unwrap().parse());
    assert!(result.display_changed);
    let follow_up = result
        .follow_up_request
        .expect("latest source should parse after the in-flight parse completes");
    assert_display_changed(cache.complete_parse(follow_up.parse()));
}

#[test]
fn markdown_cache_source_bytes_track_latest_coalesced_source() {
    let mut cache = TranscriptMarkdownCache::new(8, 4096);
    let key = TranscriptMarkdownCacheKey::new("turn:1:assistant");
    let latest = "- first\n- second";

    let first = cache.lookup(key.clone(), "- first");
    assert!(first.parse_request.is_some());

    let second = cache.lookup(key, latest);
    assert!(second.parse_request.is_none());
    assert_eq!(cache.stats().source_bytes, latest.len());
}

#[test]
fn markdown_cache_does_not_replace_displayed_snapshot_with_stale_completion() {
    let mut cache = TranscriptMarkdownCache::new(8, 4096);
    let key = TranscriptMarkdownCacheKey::new("turn:1:assistant");

    let first = cache.lookup(key.clone(), "# old");
    assert_display_changed(cache.complete_parse(first.parse_request.unwrap().parse()));

    let middle = cache.lookup(key.clone(), "# middle");
    let middle_completion = middle.parse_request.unwrap().parse();

    let _latest = cache.lookup(key.clone(), "# latest");
    let result = cache.complete_parse(middle_completion);
    assert!(!result.display_changed);
    assert!(result.stale);
    let follow_up = result
        .follow_up_request
        .expect("latest source should parse after stale completion frees the in-flight slot");

    let visible = cache.lookup(key.clone(), "# latest");
    assert_eq!(heading_text(&visible.markdown), "old");

    assert_display_changed(cache.complete_parse(follow_up.parse()));
    let ready = cache.lookup(key, "# latest");
    assert_eq!(heading_text(&ready.markdown), "latest");
}

#[test]
fn markdown_cache_bounds_entries_by_least_recent_use() {
    let mut cache = TranscriptMarkdownCache::new(2, 4096);

    for index in 0..3 {
        let key = TranscriptMarkdownCacheKey::new(format!("turn:{index}:assistant"));
        let lookup = cache.lookup(key, &format!("message {index}"));
        assert_display_changed(cache.complete_parse(lookup.parse_request.unwrap().parse()));
    }

    let stats = cache.stats();
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.evictions, 1);

    let evicted = cache.lookup(
        TranscriptMarkdownCacheKey::new("turn:0:assistant"),
        "message 0",
    );
    assert!(evicted.parse_request.is_some());
}

#[test]
fn markdown_cache_reports_retained_byte_estimates() {
    let mut cache = TranscriptMarkdownCache::new(8, 4096);
    let key = TranscriptMarkdownCacheKey::new("turn:1:assistant");

    let lookup = cache.lookup(key, "plain **bold**");
    assert!(lookup.parse_request.is_some());
    let pending = cache.stats();
    assert!(pending.estimated_retained_bytes >= pending.source_bytes);
    assert!(pending.in_flight_source_bytes >= "plain **bold**".len());

    assert_display_changed(cache.complete_parse(lookup.parse_request.unwrap().parse()));
    let completed = cache.stats();
    assert_eq!(completed.in_flight_source_bytes, 0);
    assert!(completed.displayed_source_bytes >= "plain **bold**".len());
    assert!(completed.parsed_source_bytes >= "plain **bold**".len());
    assert!(completed.markdown_estimated_structure_bytes > 0);
}

#[test]
fn markdown_cache_bounds_entries_by_estimated_retained_bytes() {
    let mut cache = TranscriptMarkdownCache::new_with_estimated_bytes(8, 4096, 900);

    for index in 0..4 {
        let key = TranscriptMarkdownCacheKey::new(format!("turn:{index}:assistant"));
        let lookup = cache.lookup(key, &format!("message {index}"));
        if let Some(request) = lookup.parse_request {
            let _ = cache.complete_parse(request.parse());
        }
    }

    let stats = cache.stats();
    assert!(stats.entries < 4);
    assert!(stats.estimated_retained_bytes <= 900);
    assert!(stats.evictions > 0);
}

#[test]
fn markdown_cache_rejects_parse_completion_after_scope_clear() {
    let mut cache = TranscriptMarkdownCache::new(8, 4096);
    let key = TranscriptMarkdownCacheKey::new("turn:1:assistant");

    let lookup = cache.lookup(key, "- item");
    let completion = lookup.parse_request.unwrap().parse();
    cache.clear();

    let result = cache.complete_parse(completion);
    assert!(!result.display_changed);
    assert!(result.stale);
    assert!(result.follow_up_request.is_none());
    let stats = cache.stats();
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.stale_completions, 1);
}

fn assert_display_changed(result: TranscriptMarkdownParseCompletionResult) {
    assert!(result.display_changed);
    assert!(!result.stale);
    assert!(result.follow_up_request.is_none());
}

fn paragraph_text(markdown: &ParsedTranscriptMarkdown) -> String {
    let [BlockRenderNode::Paragraph { lines, .. }] = markdown.render_plan().blocks.as_slice()
    else {
        panic!("expected one paragraph block");
    };
    lines_text(lines)
}

fn heading_text(markdown: &ParsedTranscriptMarkdown) -> String {
    let [BlockRenderNode::Heading { lines, .. }] = markdown.render_plan().blocks.as_slice() else {
        panic!("expected one heading block");
    };
    lines_text(lines)
}

fn lines_text(lines: &[InlineRenderLine]) -> String {
    lines
        .iter()
        .map(|line| {
            line.fragments
                .iter()
                .map(|fragment| fragment.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
