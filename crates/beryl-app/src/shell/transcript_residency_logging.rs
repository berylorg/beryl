use std::ops::Range;

use tracing::info;

use super::transcript_history::TranscriptHistoryPageId;

pub(super) fn log_transcript_turns_loaded(
    thread_id: &str,
    loaded_turns: usize,
    request_kind: &'static str,
    source_range: Range<usize>,
) {
    if loaded_turns == 0 {
        return;
    }

    info!(
        thread_id,
        loaded_turns,
        request_kind,
        source_range_start = source_range.start,
        source_range_end = source_range.end,
        "Loaded transcript turns"
    );
}

pub(super) fn log_transcript_turns_unloaded(
    page_id: TranscriptHistoryPageId,
    source_range: Range<usize>,
) {
    let unloaded_turns = source_range.len();
    if unloaded_turns == 0 {
        return;
    }

    info!(
        unloaded_turns,
        page_id = ?page_id,
        source_range_start = source_range.start,
        source_range_end = source_range.end,
        "Unloaded transcript turns"
    );
}
