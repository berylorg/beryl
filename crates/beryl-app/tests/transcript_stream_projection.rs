#[path = "../src/shell/render/transcript/stream_projection.rs"]
mod stream_projection;

use std::{
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
fn stream_projection_context_returns_owned_visible_text() {
    let projection = Rc::new(RefCell::new(projection()));
    let context = TranscriptStreamProjectionContext::new(projection);
    let key = TranscriptStreamProjectionKey::new("turn:1:item:assistant");

    assert_eq!(
        context.visible_text(key, "complete paragraph", true, Instant::now()),
        "complete paragraph"
    );
}

fn projection() -> TranscriptStreamProjection {
    TranscriptStreamProjection::new(TranscriptStreamProjectionConfig {
        coalesce_interval: Duration::from_millis(80),
        max_uncommitted_chars: 4096,
    })
}
