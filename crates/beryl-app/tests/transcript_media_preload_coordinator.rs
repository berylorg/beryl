#[path = "../src/shell/render/transcript/media_preload/coordinator_state.rs"]
mod coordinator_state;

use std::time::{Duration, Instant};

use coordinator_state::{
    TranscriptMediaPreloadBudget, TranscriptMediaPreloadDrainBudget,
    TranscriptMediaPreloadDrainStats, TranscriptMediaRunSegmentCacheKey, duration_micros,
    preload_requests_can_coalesce, preload_row_distance, union_range,
};

#[test]
fn drain_budget_enforces_row_item_source_upload_and_load_limits() {
    let budget = small_budget();
    let mut drain = TranscriptMediaPreloadDrainBudget::new(budget, Instant::now(), 2);

    assert!(drain.can_start_row());
    assert!(drain.admit_markdown_source(8));
    assert!(!drain.admit_markdown_source(5));
    assert!(drain.budget_exhausted);

    let mut drain = TranscriptMediaPreloadDrainBudget::new(budget, Instant::now(), 2);
    drain.note_media_run(2, 1, 1, 48);

    assert_eq!(drain.media_items_considered, 2);
    assert_eq!(drain.media_items_preloaded, 1);
    assert_eq!(drain.scheduled_loads, 1);
    assert_eq!(drain.segment_cache_hits, 0);
    assert_eq!(drain.segment_cache_misses, 0);
    assert_eq!(drain.remaining_load_requests(), 1);
    assert_eq!(drain.remaining_upload_bytes(), 16);
    assert!(drain.can_start_row());

    drain.note_media_run(1, 1, 1, 16);

    assert_eq!(drain.media_items_considered, 3);
    assert_eq!(drain.remaining_load_requests(), 0);
    assert_eq!(drain.remaining_upload_bytes(), 0);
    assert!(drain.budget_exhausted);
    assert!(!drain.can_start_row());
}

#[test]
fn drain_stats_default_is_content_free_and_zeroed() {
    let stats = TranscriptMediaPreloadDrainStats::default();

    assert_eq!(stats.generation, 0);
    assert_eq!(stats.rows_considered, 0);
    assert_eq!(stats.rows_stale, 0);
    assert!(!stats.budget_exhausted);
}

#[test]
fn drain_budget_stops_after_row_and_time_limits() {
    let budget = TranscriptMediaPreloadBudget {
        max_rows_per_drain: 1,
        max_drain_time: Duration::from_millis(1),
        ..small_budget()
    };
    let mut drain = TranscriptMediaPreloadDrainBudget::new(budget, Instant::now(), 1);

    assert!(drain.can_start_row());
    drain.rows_processed = 1;
    assert!(!drain.can_start_row());

    let started_at = Instant::now() - Duration::from_millis(5);
    let mut drain = TranscriptMediaPreloadDrainBudget::new(budget, started_at, 1);
    drain.rows_processed = 1;

    assert!(!drain.can_start_row());
}

#[test]
fn preload_requests_coalesce_only_for_matching_scope_and_touching_ranges() {
    let thread_a = Some("thread-a".to_string());
    let thread_b = Some("thread-b".to_string());
    let workspace_a = "workspace-a";
    let workspace_b = "workspace-b";

    assert!(preload_requests_can_coalesce(
        &thread_a,
        &workspace_a,
        &(4..10),
        &thread_a,
        &workspace_a,
        &(10..16),
    ));
    assert!(preload_requests_can_coalesce(
        &thread_a,
        &workspace_a,
        &(4..10),
        &thread_a,
        &workspace_a,
        &(8..12),
    ));
    assert!(!preload_requests_can_coalesce(
        &thread_a,
        &workspace_a,
        &(4..10),
        &thread_a,
        &workspace_a,
        &(11..16),
    ));
    assert!(!preload_requests_can_coalesce(
        &thread_a,
        &workspace_a,
        &(4..10),
        &thread_b,
        &workspace_a,
        &(8..12),
    ));
    assert!(!preload_requests_can_coalesce(
        &thread_a,
        &workspace_a,
        &(4..10),
        &thread_a,
        &workspace_b,
        &(8..12),
    ));
    assert_eq!(union_range(4..10, 8..12), 4..12);
}

#[test]
fn segment_cache_key_tracks_markdown_key_and_source_revision() {
    let source = "before ![cat](cat.png)";
    let same = TranscriptMediaRunSegmentCacheKey::new("turn:1:item:a", source);

    assert_eq!(
        same,
        TranscriptMediaRunSegmentCacheKey::new("turn:1:item:a", source)
    );
    assert_ne!(
        same,
        TranscriptMediaRunSegmentCacheKey::new("turn:1:item:a", "before ![hat](hat.png)")
    );
    assert_ne!(
        same,
        TranscriptMediaRunSegmentCacheKey::new("turn:1:item:b", source)
    );
}

#[test]
fn row_distance_prioritizes_visible_rows_then_nearest_edges() {
    let visible = 10..13;

    assert_eq!(preload_row_distance(10, &visible), 0);
    assert_eq!(preload_row_distance(12, &visible), 0);
    assert_eq!(preload_row_distance(9, &visible), 1);
    assert_eq!(preload_row_distance(13, &visible), 1);
    assert_eq!(preload_row_distance(8, &visible), 2);
    assert_eq!(preload_row_distance(15, &visible), 3);
}

#[test]
fn duration_micros_saturates_to_u64() {
    assert_eq!(duration_micros(Duration::from_micros(42)), 42);
    assert_eq!(duration_micros(Duration::MAX), u64::MAX);
}

fn small_budget() -> TranscriptMediaPreloadBudget {
    TranscriptMediaPreloadBudget {
        max_rows_per_drain: 4,
        max_media_items_per_drain: 3,
        max_markdown_source_bytes_per_drain: 10,
        max_source_backed_upload_bytes_per_drain: 64,
        max_in_flight_loads: 2,
        max_drain_time: Duration::from_millis(100),
    }
}
