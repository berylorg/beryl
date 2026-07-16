use beryl_model::{InputGateRevision, SyndicThreadId};

use crate::{InputGateState, SyndicRecordError};

/// Maximum simultaneously live accepted fragments for one thread.
pub const MAX_LIVE_ACCEPTED_INPUTS: u32 = 256;

/// Maximum logical UTF-8 bytes across one thread's live accepted fragments.
pub const MAX_LIVE_ACCEPTED_UTF8_BYTES: u64 = 268_435_456;

/// Exact current input-admission and live-route accounting for one thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputGateRecord {
    thread_id: SyndicThreadId,
    revision: InputGateRevision,
    state: InputGateState,
    accepted_high_water: u64,
    live_steering_count: u32,
    live_next_turn_count: u32,
    live_logical_utf8_bytes: u64,
}

impl InputGateRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_id: SyndicThreadId,
        revision: InputGateRevision,
        state: InputGateState,
        accepted_high_water: u64,
        live_steering_count: u32,
        live_next_turn_count: u32,
        live_logical_utf8_bytes: u64,
    ) -> Result<Self, SyndicRecordError> {
        let live_count = live_steering_count
            .checked_add(live_next_turn_count)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live accepted-input count",
            })?;
        if live_count > MAX_LIVE_ACCEPTED_INPUTS {
            return Err(SyndicRecordError::LiveAcceptedInputCountTooLarge {
                maximum: MAX_LIVE_ACCEPTED_INPUTS,
                actual: live_count,
            });
        }
        if live_logical_utf8_bytes > MAX_LIVE_ACCEPTED_UTF8_BYTES {
            return Err(SyndicRecordError::LiveAcceptedInputBytesTooLarge {
                maximum: MAX_LIVE_ACCEPTED_UTF8_BYTES,
                actual: live_logical_utf8_bytes,
            });
        }
        Ok(Self {
            thread_id,
            revision,
            state,
            accepted_high_water,
            live_steering_count,
            live_next_turn_count,
            live_logical_utf8_bytes,
        })
    }

    pub fn idle(thread_id: SyndicThreadId) -> Self {
        Self::new(
            thread_id,
            InputGateRevision::new(1).expect("first input-gate revision"),
            InputGateState::Idle,
            0,
            0,
            0,
            0,
        )
        .expect("empty input gate is within V1 bounds")
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> InputGateRevision {
        self.revision
    }
    #[must_use]
    pub const fn state(&self) -> &InputGateState {
        &self.state
    }
    #[must_use]
    pub const fn accepted_high_water(&self) -> u64 {
        self.accepted_high_water
    }
    #[must_use]
    pub const fn live_steering_count(&self) -> u32 {
        self.live_steering_count
    }
    #[must_use]
    pub const fn live_next_turn_count(&self) -> u32 {
        self.live_next_turn_count
    }
    #[must_use]
    pub const fn live_logical_utf8_bytes(&self) -> u64 {
        self.live_logical_utf8_bytes
    }
    #[must_use]
    pub const fn live_count(&self) -> u32 {
        self.live_steering_count + self.live_next_turn_count
    }
}
