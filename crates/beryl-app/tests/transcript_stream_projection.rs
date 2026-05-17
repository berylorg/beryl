#[path = "../src/shell/render/transcript/stream_projection.rs"]
mod stream_projection;

use std::{
    borrow::Cow,
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use stream_projection::{
    TranscriptStreamProjection, TranscriptStreamProjectionConfig,
    TranscriptStreamProjectionContext, TranscriptStreamProjectionKey,
};

#[test]
fn stream_projection_commits_list_items_atomically() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();

    assert_eq!(
        projection.visible_text(key.clone(), "- first", false, started_at),
        ""
    );
    assert_eq!(
        projection.visible_text(
            key.clone(),
            "- first\n- sec",
            false,
            started_at + Duration::from_millis(10),
        ),
        "- first\n"
    );
    assert_eq!(
        projection.visible_text(
            key,
            "- first\n- second\n\nTrailing paragraph starts",
            false,
            started_at + Duration::from_millis(20),
        ),
        "- first\n- second\n\n"
    );
}

#[test]
fn stream_projection_flushes_remaining_text_on_item_completion() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();

    assert_eq!(
        projection.visible_text(key.clone(), "partial paragraph", false, started_at),
        ""
    );
    assert_eq!(
        projection.visible_text(
            key,
            "partial paragraph",
            true,
            started_at + Duration::from_millis(10),
        ),
        "partial paragraph"
    );
}

#[test]
fn stream_projection_advances_long_paragraph_after_bounded_interval() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();

    assert_eq!(
        projection.visible_text(
            key.clone(),
            "A paragraph without a Markdown block boundary",
            false,
            started_at,
        ),
        ""
    );
    assert_eq!(
        projection.visible_text(
            key,
            "A paragraph without a Markdown block boundary keeps growing",
            false,
            started_at + Duration::from_millis(81),
        ),
        "A paragraph without a Markdown block boundary keeps growing"
    );
}

#[test]
fn stream_projection_commits_closed_fenced_code_blocks() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();

    assert_eq!(
        projection.visible_text(key.clone(), "```rust\nfn main() {}\n", false, started_at,),
        ""
    );
    assert_eq!(
        projection.visible_text(
            key,
            "```rust\nfn main() {}\n```\nnext paragraph starts",
            false,
            started_at + Duration::from_millis(10),
        ),
        "```rust\nfn main() {}\n```\n"
    );
}

#[test]
fn stream_projection_keeps_incomplete_fence_hidden_until_bounded_release() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();
    let partial_fence = "```markdown\n# partial";

    assert_eq!(
        projection.visible_text(key.clone(), partial_fence, false, started_at),
        ""
    );
    assert_eq!(
        projection.visible_text(
            key,
            partial_fence,
            false,
            started_at + Duration::from_millis(81),
        ),
        partial_fence
    );
}

#[test]
fn stream_projection_drops_visible_text_when_partial_snapshot_rewrites_source() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();

    assert_eq!(
        projection.visible_text(key.clone(), "complete paragraph", true, started_at),
        "complete paragraph"
    );
    assert_eq!(
        projection.visible_text(
            key.clone(),
            "replacement paragraph without a stable boundary",
            false,
            started_at + Duration::from_millis(10),
        ),
        ""
    );
    assert_eq!(
        projection.visible_text(
            key,
            "replacement paragraph without a stable boundary",
            false,
            started_at + Duration::from_millis(91),
        ),
        "replacement paragraph without a stable boundary"
    );
}

#[test]
fn stream_projection_clear_drops_visible_prefixes() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();

    assert_eq!(
        projection.visible_text(key.clone(), "first paragraph", true, started_at),
        "first paragraph"
    );

    projection.clear();

    assert_eq!(
        projection.visible_text(
            key,
            "second paragraph",
            false,
            started_at + Duration::from_millis(10),
        ),
        ""
    );
}

#[test]
fn stream_projection_retained_counts_include_keys_text_and_uncommitted_entries() {
    let mut projection = projection();
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");
    let started_at = Instant::now();

    projection.visible_text(key, "first paragraph", true, started_at);
    projection.visible_text(
        TranscriptStreamProjectionKey::new("turn:1:item:reasoning"),
        "open paragraph without stable boundary",
        false,
        started_at,
    );

    let counts = projection.retained_counts();
    assert_eq!(counts.entries, 1);
    assert!(counts.key_bytes >= "turn:1:item:reasoning".len());
    assert_eq!(counts.text_bytes, 0);
    assert_eq!(counts.uncommitted_entries, 1);
}

#[test]
fn stream_projection_context_borrows_completed_visible_text() {
    let projection = Rc::new(RefCell::new(projection()));
    let context = TranscriptStreamProjectionContext::new(projection);
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");

    let visible = context.visible_text(key, "complete paragraph", true, Instant::now());

    assert_eq!(visible, "complete paragraph");
    assert!(matches!(visible, Cow::Borrowed("complete paragraph")));
}

#[test]
fn stream_projection_does_not_retain_completed_entries() {
    let mut projection = projection();
    let started_at = Instant::now();

    for index in 0..=16 {
        projection.visible_text(
            TranscriptStreamProjectionKey::new(format!("turn:{index}:item:assistant")),
            "complete paragraph",
            true,
            started_at,
        );
    }

    let counts = projection.retained_counts();
    assert_eq!(counts.entries, 0);
    assert_eq!(counts.text_bytes, 0);
    assert_eq!(counts.uncommitted_entries, 0);
}

#[test]
fn stream_projection_keeps_uncommitted_entries_while_pruning_completed_entries() {
    let mut projection = projection();
    let started_at = Instant::now();
    projection.visible_text(
        TranscriptStreamProjectionKey::new("turn:active:item:assistant"),
        "partial paragraph without a stable boundary",
        false,
        started_at,
    );

    for index in 0..=16 {
        projection.visible_text(
            TranscriptStreamProjectionKey::new(format!("turn:{index}:item:assistant")),
            "complete paragraph",
            true,
            started_at,
        );
    }

    let counts = projection.retained_counts();
    assert_eq!(counts.entries, 1);
    assert_eq!(counts.text_bytes, 0);
    assert_eq!(counts.uncommitted_entries, 1);
}

fn projection() -> TranscriptStreamProjection {
    TranscriptStreamProjection::new(TranscriptStreamProjectionConfig {
        coalesce_interval: Duration::from_millis(80),
        max_uncommitted_chars: 4096,
    })
}
