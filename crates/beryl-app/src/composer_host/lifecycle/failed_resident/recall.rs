use super::*;
use crate::composer_host::accepted_input_recall::ComposerHostRecallRestoration;

impl ComposerHostFailedResident {
    pub fn recall_prior_binding(&self) -> Option<ComposerHostBinding> {
        self.recall
            .as_ref()
            .map(RetiredAcceptedInputRecall::prior_binding)
    }

    pub fn recall_restoration(&self) -> Option<ComposerHostRecallRestoration> {
        self.recall
            .as_ref()
            .and_then(RetiredAcceptedInputRecall::restoration)
    }

    pub(super) fn settle_recall(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<(), String> {
        if self.recall_applied || self.recall.is_none() {
            return Ok(());
        }
        if let Some(original) = &mut self.original {
            if original.binding.home_id() != self.binding.home_id()
                || original.binding.home_generation() != self.binding.home_generation()
                || original.binding.candidate().session_id() != self.checkpoint.session_id()
            {
                return Err("original recall publication belongs to another source".into());
            }
            if original.outcome.reconcile(access)? {
                match storage
                    .reconcile_draft_editor_candidate_publication_candidate(
                        access,
                        &original.prepared.syndic,
                        original.outcome.committed_classification().unwrap(),
                    )
                    .map_err(|error| error.to_string())?
                {
                    syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(..)
                    | syndic_storage::DraftEditorCandidatePublicationOutcomeV1::ExactReplay(_) => {}
                    _ => return Err("original recall publication is not exact".into()),
                }
            }
        }
        let restored = self.recall.as_mut().unwrap().settle(access, storage)?;
        self.checkpoint = restored.candidate();
        if restored.positions().is_some() {
            self.activation_checkpoint = restored.candidate();
        }
        self.recall_applied = true;
        Ok(())
    }
}
