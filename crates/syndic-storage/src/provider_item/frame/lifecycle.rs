use beryl_model::CasItemId;

use super::{
    ProviderItemFrameV1, ProviderItemKind, ProviderItemObservationV1, ProviderItemValidationError,
    ProviderLifecycleTimestampMsV1,
};
use crate::provider_item::{
    ProviderFrameHistorySupportV1, ProviderFrameObservationSummaryV1,
    ProviderFrameStructuralValidationV1,
};

/// Constant-resident lifecycle validator for one item stream.
#[derive(Clone, Debug, Default)]
pub struct ProviderItemStreamValidatorV1 {
    item_id: Option<CasItemId>,
    kind: Option<ProviderItemKind>,
    next_ordinal: u64,
    started_at: Option<ProviderLifecycleTimestampMsV1>,
    completed: bool,
    history_support: ProviderFrameHistorySupportV1,
}

impl ProviderItemStreamValidatorV1 {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            item_id: None,
            kind: None,
            next_ordinal: 1,
            started_at: None,
            completed: false,
            history_support: ProviderFrameHistorySupportV1::Supported,
        }
    }

    pub fn observe(
        &mut self,
        frame: &ProviderItemFrameV1,
    ) -> Result<(), ProviderItemValidationError> {
        let observation = match frame.observation() {
            ProviderItemObservationV1::Started { observed_at, .. } => {
                ProviderFrameObservationSummaryV1::Started(*observed_at)
            }
            ProviderItemObservationV1::Delta(_) => ProviderFrameObservationSummaryV1::Delta,
            ProviderItemObservationV1::Completed { observed_at, item } => {
                super::super::validate::validate_completed_status(item)?;
                ProviderFrameObservationSummaryV1::Completed(*observed_at)
            }
        };
        self.observe_facts(
            frame.ordinal().get(),
            frame.item_id(),
            frame.kind(),
            observation,
            frame.history_support(),
        )
    }

    /// Observes facts produced by constant-resident structural frame validation.
    pub fn observe_structural(
        &mut self,
        frame: &ProviderFrameStructuralValidationV1,
    ) -> Result<(), ProviderItemValidationError> {
        self.observe_facts(
            frame.reference().ordinal().get(),
            frame.reference().item_id(),
            frame.reference().item_kind(),
            frame.observation(),
            frame.history_support(),
        )
    }

    fn observe_facts(
        &mut self,
        ordinal: u64,
        item_id: &CasItemId,
        kind: ProviderItemKind,
        observation: ProviderFrameObservationSummaryV1,
        history_support: ProviderFrameHistorySupportV1,
    ) -> Result<(), ProviderItemValidationError> {
        if ordinal != self.next_ordinal {
            return Err(ProviderItemValidationError::FrameOrdinalConflict {
                expected: self.next_ordinal,
                actual: ordinal,
            });
        }
        if self.completed {
            return Err(ProviderItemValidationError::EventAfterCompletion);
        }
        if let Some(expected) = &self.item_id {
            if expected != item_id {
                return Err(ProviderItemValidationError::ItemIdentityMismatch);
            }
        } else {
            self.item_id = Some(item_id.clone());
        }
        if let Some(expected) = self.kind {
            if expected != kind {
                return Err(ProviderItemValidationError::ItemKindMismatch {
                    expected,
                    actual: kind,
                });
            }
        } else {
            self.kind = Some(kind);
        }
        match observation {
            ProviderFrameObservationSummaryV1::Started(observed_at) => {
                if kind.permits_completion_only() {
                    return Err(ProviderItemValidationError::CompletionOnlyItemStarted);
                }
                if self.started_at.is_some() {
                    return Err(ProviderItemValidationError::DuplicateItemStart);
                }
                self.started_at = Some(observed_at);
            }
            ProviderFrameObservationSummaryV1::Delta => {
                if self.started_at.is_none() {
                    return Err(ProviderItemValidationError::MissingItemStart);
                }
            }
            ProviderFrameObservationSummaryV1::Completed(observed_at) => {
                if let Some(started_at) = self.started_at {
                    if observed_at < started_at {
                        return Err(ProviderItemValidationError::CompletionBeforeStart {
                            started: started_at.get(),
                            completed: observed_at.get(),
                        });
                    }
                } else if !kind.permits_completion_only() {
                    return Err(ProviderItemValidationError::MissingItemStart);
                }
                self.completed = true;
            }
        }
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(ProviderItemValidationError::FrameOrdinalExhausted)?;
        self.history_support = self.history_support.merge(history_support);
        Ok(())
    }

    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.completed
    }

    #[must_use]
    pub const fn kind(&self) -> Option<ProviderItemKind> {
        self.kind
    }

    /// Returns the monotonic history-support result across every accepted frame.
    #[must_use]
    pub const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        self.history_support
    }

    /// Reports structural lifecycle completion only when no accepted frame blocked history.
    #[must_use]
    pub const fn is_history_complete(&self) -> bool {
        self.completed && self.history_support.is_supported()
    }
}
