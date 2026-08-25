use super::model::{
    ComposerHostSubmissionDiagnostics, ComposerHostSubmissionStage, PendingSubmissionStage,
};
use crate::composer_host::SyndicComposerHost;
#[cfg(feature = "test-faults")]
use syndic_storage::FirstAcceptance;

impl SyndicComposerHost {
    pub fn submission_diagnostics(&self) -> ComposerHostSubmissionDiagnostics {
        let Some(pending) = self.submission.pending.as_ref() else {
            return ComposerHostSubmissionDiagnostics {
                pending: false,
                stage: None,
                retained_roots: 0,
                retained_materializations: 0,
                command_attempted: false,
            };
        };
        let (stage, roots, materializations, attempted) = match &pending.stage {
            PendingSubmissionStage::Flushing(_) => {
                (ComposerHostSubmissionStage::Flushing, 0, 0, false)
            }
            PendingSubmissionStage::Capturing => {
                (ComposerHostSubmissionStage::Capturing, 0, 0, false)
            }
            PendingSubmissionStage::Materializing { reconciliation, .. } => (
                ComposerHostSubmissionStage::Materializing,
                1,
                0,
                reconciliation.is_some(),
            ),
            PendingSubmissionStage::Accepting { reconciliation, .. } => (
                ComposerHostSubmissionStage::Accepting,
                1,
                1,
                reconciliation.is_some(),
            ),
            PendingSubmissionStage::Transitioning => {
                unreachable!("submission transition is synchronous")
            }
        };
        ComposerHostSubmissionDiagnostics {
            pending: true,
            stage: Some(stage),
            retained_roots: roots,
            retained_materializations: materializations,
            command_attempted: attempted,
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_submission_acceptance(&self) -> Option<FirstAcceptance> {
        match self
            .submission
            .pending
            .as_ref()
            .map(|pending| &pending.stage)
        {
            Some(PendingSubmissionStage::Accepting { acceptance, .. }) => Some(acceptance.clone()),
            _ => None,
        }
    }
}
