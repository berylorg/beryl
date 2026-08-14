use std::time::{SystemTime, UNIX_EPOCH};

use beryl_backend::{ThreadTokenUsage, TokenUsageBreakdown};
use beryl_model::conversation::{
    ConversationThreadTokenUsageSnapshot, ConversationTokenUsageBreakdown, ConversationTurnId,
};

pub(crate) fn thread_token_usage_snapshot(
    turn_id: &str,
    token_usage: &ThreadTokenUsage,
    observed_at_millis: u64,
) -> ConversationThreadTokenUsageSnapshot {
    ConversationThreadTokenUsageSnapshot::new(
        ConversationTurnId::new(turn_id.to_string()),
        token_usage_breakdown(&token_usage.last),
        token_usage_breakdown(&token_usage.total),
        token_usage.model_context_window,
        observed_at_millis,
    )
}

pub(crate) fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn token_usage_breakdown(value: &TokenUsageBreakdown) -> ConversationTokenUsageBreakdown {
    ConversationTokenUsageBreakdown::new(
        value.cached_input_tokens,
        value.input_tokens,
        value.output_tokens,
        value.reasoning_output_tokens,
        value.total_tokens,
    )
}
