use beryl_home_store::{HomeStore, ReconciliationResolution};

use super::model::PendingSubmissionStage;
use crate::composer_host::{ComposerHostError, SyndicComposerHost};

impl SyndicComposerHost {
    pub(in crate::composer_host) fn dispose_pending_submission(
        &mut self,
        store: &HomeStore,
    ) -> Result<(), ComposerHostError> {
        let (reconciliation, acceptance) = match self.submission.pending.as_ref().map(|p| &p.stage)
        {
            Some(PendingSubmissionStage::Accepting {
                acceptance,
                reconciliation,
            }) => (reconciliation.clone(), Some(acceptance)),
            Some(PendingSubmissionStage::Materializing { reconciliation, .. }) => {
                (reconciliation.clone(), None)
            }
            _ => (None, None),
        };
        if let Some(handle) = reconciliation {
            match store.reconcile(&handle)? {
                ReconciliationResolution::ExactNew { .. }
                | ReconciliationResolution::ExactSuccessor { .. } => {
                    if let Some(acceptance) = acceptance {
                        self.remember_accepted_input(acceptance);
                    }
                }
                ReconciliationResolution::ExactOld | ReconciliationResolution::Collision => {}
            }
        }
        self.submission.pending = None;
        Ok(())
    }
}
