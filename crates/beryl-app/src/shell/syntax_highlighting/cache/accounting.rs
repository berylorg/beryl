use std::time::Duration;

use super::super::model::SyntaxHighlight;
use super::{SyntaxHighlightCacheEntry, request::SyntaxHighlightCacheKey};

const ESTIMATED_TOKEN_BYTES: usize = 40;

pub(super) fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

pub(super) fn syntax_highlight_entry_estimate(
    key: &SyntaxHighlightCacheKey,
    entry: &SyntaxHighlightCacheEntry,
) -> usize {
    key.owner_id()
        .len()
        .saturating_add(key.language.label().len())
        .saturating_add(entry.latest_source.len())
        .saturating_add(
            entry
                .in_flight
                .as_ref()
                .map_or(0, |in_flight| in_flight.source.len()),
        )
        .saturating_add(syntax_highlight_token_estimate(entry.displayed.as_ref()))
}

pub(super) fn syntax_highlight_completed_entry_estimate(
    key: &SyntaxHighlightCacheKey,
    entry: &SyntaxHighlightCacheEntry,
    highlight: &SyntaxHighlight,
) -> usize {
    key.owner_id()
        .len()
        .saturating_add(key.language.label().len())
        .saturating_add(entry.latest_source.len())
        .saturating_add(syntax_highlight_token_estimate(highlight))
}

fn syntax_highlight_token_estimate(highlight: &SyntaxHighlight) -> usize {
    highlight
        .tokens()
        .len()
        .saturating_mul(ESTIMATED_TOKEN_BYTES)
}
