//! Validated requirements for free-space admission of a new durable turn.

use std::num::NonZeroU64;

use thiserror::Error;

use crate::DurableStartFootprint;

/// Immutable Beryl product policy for the shared bounded durable-start envelope.
pub const DURABLE_START_ADMISSION_BUDGET_BYTES: u64 = 268_435_456;

/// Immutable nonzero capture headroom required in addition to the durable-start budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinimumTurnCaptureReserve(NonZeroU64);

impl MinimumTurnCaptureReserve {
    /// Validates the separately configured capture headroom.
    pub fn try_new(reserve_bytes: u64) -> Result<Self, TurnStartAdmissionRequirementError> {
        NonZeroU64::new(reserve_bytes)
            .map(Self)
            .ok_or(TurnStartAdmissionRequirementError::ZeroMinimumTurnCaptureReserve)
    }

    /// Returns the capture headroom supplied by app configuration.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Opaque fixed-budget-plus-capture requirement accepted by a home free-space query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnStartAdmissionRequirement {
    durable_start_budget_bytes: u64,
    direct_journal_append_bytes: u64,
    queued_journal_append_bytes: u64,
    minimum_turn_capture_reserve: MinimumTurnCaptureReserve,
    total_bytes: u64,
}

impl TurnStartAdmissionRequirement {
    /// Validates direct and queued owner-derived envelopes against the fixed product policy.
    ///
    /// The supplied footprints must be composed from their typed owners before this boundary. No
    /// constructor accepts a precomputed aggregate byte count.
    pub fn try_new(
        direct: DurableStartFootprint,
        queued: DurableStartFootprint,
        minimum_turn_capture_reserve: MinimumTurnCaptureReserve,
    ) -> Result<Self, TurnStartAdmissionRequirementError> {
        let direct_journal_append_bytes = direct.journal_append_bytes();
        if direct_journal_append_bytes > DURABLE_START_ADMISSION_BUDGET_BYTES {
            return Err(
                TurnStartAdmissionRequirementError::DirectDurableStartBudgetDrift {
                    journal_append_bytes: direct_journal_append_bytes,
                    budget_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES,
                },
            );
        }
        let queued_journal_append_bytes = queued.journal_append_bytes();
        if queued_journal_append_bytes > DURABLE_START_ADMISSION_BUDGET_BYTES {
            return Err(
                TurnStartAdmissionRequirementError::QueuedDurableStartBudgetDrift {
                    journal_append_bytes: queued_journal_append_bytes,
                    budget_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES,
                },
            );
        }
        let total_bytes = DURABLE_START_ADMISSION_BUDGET_BYTES
            .checked_add(minimum_turn_capture_reserve.get())
            .ok_or(TurnStartAdmissionRequirementError::ArithmeticOverflow {
                budget_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES,
                capture_reserve_bytes: minimum_turn_capture_reserve.get(),
            })?;
        Ok(Self {
            durable_start_budget_bytes: DURABLE_START_ADMISSION_BUDGET_BYTES,
            direct_journal_append_bytes,
            queued_journal_append_bytes,
            minimum_turn_capture_reserve,
            total_bytes,
        })
    }

    /// Returns the immutable durable-start component for diagnostics.
    #[must_use]
    pub const fn durable_start_budget_bytes(self) -> u64 {
        self.durable_start_budget_bytes
    }

    /// Returns the validated direct envelope for diagnostics.
    #[must_use]
    pub const fn direct_journal_append_bytes(self) -> u64 {
        self.direct_journal_append_bytes
    }

    /// Returns the validated queued envelope for diagnostics.
    #[must_use]
    pub const fn queued_journal_append_bytes(self) -> u64 {
        self.queued_journal_append_bytes
    }

    /// Returns the configured capture component for diagnostics.
    #[must_use]
    pub const fn minimum_turn_capture_reserve(self) -> MinimumTurnCaptureReserve {
        self.minimum_turn_capture_reserve
    }

    /// Returns the fixed validated query threshold for diagnostics.
    ///
    /// [`crate::HomeStore::query_free_space`] accepts this opaque requirement rather than this raw
    /// diagnostic value.
    #[must_use]
    pub const fn total_bytes(self) -> u64 {
        self.total_bytes
    }
}

/// Failure while validating a turn-start admission requirement.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TurnStartAdmissionRequirementError {
    /// The separately configured capture headroom was zero.
    #[error("minimum turn-capture reserve must be nonzero")]
    ZeroMinimumTurnCaptureReserve,
    /// The direct idle-submission envelope exceeded the immutable product budget.
    #[error(
        "direct durable-start journal envelope {journal_append_bytes} exceeds fixed budget {budget_bytes}"
    )]
    DirectDurableStartBudgetDrift {
        /// Derived Fjall journal append envelope.
        journal_append_bytes: u64,
        /// Immutable product budget.
        budget_bytes: u64,
    },
    /// The queued accepted-input-promotion envelope exceeded the immutable product budget.
    #[error(
        "queued durable-start journal envelope {journal_append_bytes} exceeds fixed budget {budget_bytes}"
    )]
    QueuedDurableStartBudgetDrift {
        /// Derived Fjall journal append envelope.
        journal_append_bytes: u64,
        /// Immutable product budget.
        budget_bytes: u64,
    },
    /// The fixed budget and configured capture headroom could not be added.
    #[error(
        "turn-start admission requirement overflowed adding budget {budget_bytes} and capture reserve {capture_reserve_bytes}"
    )]
    ArithmeticOverflow {
        /// Immutable product budget.
        budget_bytes: u64,
        /// Separately configured capture headroom.
        capture_reserve_bytes: u64,
    },
}
