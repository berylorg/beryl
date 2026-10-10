use beryl_model::{CasThreadId, CasTurnId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountQuotaObservation {
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextTokenBreakdown {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextTokenUsage {
    pub total: ContextTokenBreakdown,
    pub last: ContextTokenBreakdown,
    pub model_context_window: Option<i64>,
}

impl ContextTokenUsage {
    pub fn remaining_percent(self) -> Option<u8> {
        let window = i128::from(self.model_context_window.filter(|value| *value > 0)?);
        let remaining = (window - i128::from(self.last.input_tokens)).clamp(0, window);
        u8::try_from(remaining * 100 / window).ok()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadContextObservation {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
    usage: Option<ContextTokenUsage>,
}

impl ThreadContextObservation {
    pub(crate) fn decoded(
        thread_id: CasThreadId,
        turn_id: CasTurnId,
        usage: Option<ContextTokenUsage>,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            usage,
        }
    }

    pub fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    pub fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }

    pub const fn usage(&self) -> Option<ContextTokenUsage> {
        self.usage
    }
}
