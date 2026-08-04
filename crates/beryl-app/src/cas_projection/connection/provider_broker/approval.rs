use std::sync::Mutex;

use crate::cas_projection::connection::router::ApprovalInterruptionObligation;

pub(super) struct ApprovalInterruptionSlot {
    state: Mutex<SlotState>,
}

enum SlotState {
    Empty,
    Reserved { close_after_install: bool },
    Pending(PendingApprovalInterruption),
    Closed,
}

pub(in crate::cas_projection::connection) struct PendingApprovalInterruption {
    obligation: ApprovalInterruptionObligation,
}

impl ApprovalInterruptionSlot {
    pub(super) const fn new() -> Self {
        Self {
            state: Mutex::new(SlotState::Empty),
        }
    }

    pub(super) fn reserve(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !matches!(*state, SlotState::Empty) {
            return false;
        }
        *state = SlotState::Reserved {
            close_after_install: false,
        };
        true
    }

    pub(super) fn install(&self, obligation: ApprovalInterruptionObligation) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match std::mem::replace(&mut *state, SlotState::Empty) {
            SlotState::Reserved {
                close_after_install: false,
            } => {
                *state = SlotState::Pending(PendingApprovalInterruption { obligation });
                true
            }
            SlotState::Reserved {
                close_after_install: true,
            } => {
                *state = SlotState::Closed;
                false
            }
            SlotState::Closed => {
                *state = SlotState::Closed;
                false
            }
            SlotState::Empty | SlotState::Pending(_) => {
                unreachable!("the sole ingester installs only its own interruption reservation")
            }
        }
    }

    pub(super) fn cancel_reservation(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if matches!(*state, SlotState::Reserved { .. }) {
            let closing = matches!(
                *state,
                SlotState::Reserved {
                    close_after_install: true,
                    ..
                }
            );
            *state = if closing {
                SlotState::Closed
            } else {
                SlotState::Empty
            };
        }
    }

    pub(super) fn take(&self) -> Option<PendingApprovalInterruption> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match std::mem::replace(&mut *state, SlotState::Empty) {
            SlotState::Pending(pending) => Some(pending),
            other => {
                *state = other;
                None
            }
        }
    }

    pub(super) fn clear_pending(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if matches!(&*state, SlotState::Pending(_)) {
            *state = SlotState::Empty;
        }
    }

    pub(super) fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match std::mem::replace(&mut *state, SlotState::Closed) {
            SlotState::Reserved { .. } => {
                *state = SlotState::Reserved {
                    close_after_install: true,
                };
            }
            SlotState::Empty | SlotState::Pending(_) | SlotState::Closed => {}
        }
    }
}

impl PendingApprovalInterruption {
    pub(in crate::cas_projection::connection) const fn obligation(
        &self,
    ) -> &ApprovalInterruptionObligation {
        &self.obligation
    }

    pub(in crate::cas_projection::connection) const fn obligation_mut(
        &mut self,
    ) -> &mut ApprovalInterruptionObligation {
        &mut self.obligation
    }
}
