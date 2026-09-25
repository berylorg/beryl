use super::*;
use beryl_home_store::{CommandError, CommitReceipt, CommittedLocalFinalization};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreSetRetainedReason {
    BeginRestoreLocalFinalization,
    ThreadlessInitializationLocalFinalization,
    RestoredClaimLocalFinalization { window_id: WindowId },
}

pub struct RestoreSetRetainedCommand<'a> {
    pub reason: RestoreSetRetainedReason,
    pub receipt: &'a CommitReceipt,
    pub failure: Option<&'a CommandError>,
    pub local_finalization: &'a CommittedLocalFinalization,
}

impl MainWindowRestoreSet {
    pub fn retained_command(&self) -> Option<RestoreSetRetainedCommand<'_>> {
        if let Some(command) = self.discovery.retained_command() {
            return Some(command);
        }
        let current = self.current.as_ref()?;
        Some(RestoreSetRetainedCommand {
            reason: RestoreSetRetainedReason::RestoredClaimLocalFinalization {
                window_id: current.window_id(),
            },
            receipt: current.claim_activation_receipt()?,
            failure: current.claim_activation_failure(),
            local_finalization: current.claim_activation_local_finalization()?,
        })
    }

    pub(super) fn pending(self) -> MainWindowRestoreSetOutcome {
        if let Some(reason) = self.retained_command().map(|command| command.reason) {
            MainWindowRestoreSetOutcome::Retained {
                custody: self,
                reason,
            }
        } else {
            MainWindowRestoreSetOutcome::Pending(self)
        }
    }
}
