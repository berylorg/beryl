use serde::{Deserialize, Serialize};

use super::ConversationTurnId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTokenUsageBreakdown {
    cached_input_tokens: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationThreadTokenUsageSnapshot {
    turn_id: ConversationTurnId,
    last: ConversationTokenUsageBreakdown,
    total: ConversationTokenUsageBreakdown,
    model_context_window: Option<i64>,
    observed_at_millis: u64,
}

impl ConversationTokenUsageBreakdown {
    pub fn new(
        cached_input_tokens: i64,
        input_tokens: i64,
        output_tokens: i64,
        reasoning_output_tokens: i64,
        total_tokens: i64,
    ) -> Self {
        Self {
            cached_input_tokens,
            input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
        }
    }

    pub fn cached_input_tokens(&self) -> i64 {
        self.cached_input_tokens
    }

    pub fn input_tokens(&self) -> i64 {
        self.input_tokens
    }

    pub fn output_tokens(&self) -> i64 {
        self.output_tokens
    }

    pub fn reasoning_output_tokens(&self) -> i64 {
        self.reasoning_output_tokens
    }

    pub fn total_tokens(&self) -> i64 {
        self.total_tokens
    }
}

impl ConversationThreadTokenUsageSnapshot {
    pub fn new(
        turn_id: ConversationTurnId,
        last: ConversationTokenUsageBreakdown,
        total: ConversationTokenUsageBreakdown,
        model_context_window: Option<i64>,
        observed_at_millis: u64,
    ) -> Self {
        Self {
            turn_id,
            last,
            total,
            model_context_window,
            observed_at_millis,
        }
    }

    pub fn turn_id(&self) -> &ConversationTurnId {
        &self.turn_id
    }

    pub fn last(&self) -> &ConversationTokenUsageBreakdown {
        &self.last
    }

    pub fn total(&self) -> &ConversationTokenUsageBreakdown {
        &self.total
    }

    pub fn model_context_window(&self) -> Option<i64> {
        self.model_context_window
    }

    pub fn observed_at_millis(&self) -> u64 {
        self.observed_at_millis
    }
}
