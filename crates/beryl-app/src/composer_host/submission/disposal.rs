use beryl_home_store::{HomeStore, ReconciliationResolution};

use super::model::PendingSubmissionStage;
use crate::composer_host::{ComposerHostError, SyndicComposerHost};

impl SyndicComposerHost {
    pub(in crate::composer_host) fn dispose_pending_submission(
        &mut self,
        store: &HomeStore,
    ) -> Result<(), ComposerHostError> {
        let reconciliation =
            self.submission
                .pending
                .as_ref()
                .and_then(|pending| match &pending.stage {
                    PendingSubmissionStage::Materializing { reconciliation, .. }
                    | PendingSubmissionStage::Accepting { reconciliation, .. } => {
                        reconciliation.clone()
                    }
                    PendingSubmissionStage::Flushing(_)
                    | PendingSubmissionStage::Capturing
                    | PendingSubmissionStage::Transitioning => None,
                });
        if let Some(handle) = reconciliation {
            match store.reconcile(&handle)? {
                ReconciliationResolution::ExactOld
                | ReconciliationResolution::ExactNew { .. }
                | ReconciliationResolution::ExactSuccessor { .. }
                | ReconciliationResolution::Collision => {}
            }
        }
        self.submission.pending = None;
        Ok(())
    }
}
