use beryl_model::{AssetId, SyndicDraftMarkerId};

use super::*;

impl SyndicStorage {
    pub fn resolve_draft_mutation_staging_marker(
        &self,
        store: &beryl_home_store::HomeStore,
        begin: DraftMutationBeginV1,
        marker_id: SyndicDraftMarkerId,
        asset_id: AssetId,
        order_key: u64,
    ) -> Result<DraftPieceMarkerV1, DraftMutationStagingErrorV1> {
        let revision = self.revision(store).map_err(SyndicReadError::from)?;
        let identity = begin.identity();
        let head = self
            .draft_mutation_staging_head(store, identity)?
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        if head.lifecycle() != DraftMutationStagingLifecycleV1::Receiving
            || !admitted_writer_is_current_generation(self, &head)
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let admission = head
            .begin()
            .writer_admission()
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        if begin.with_writer_admission(admission) != head.begin()
            || begin
                .writer_admission()
                .is_some_and(|value| value != admission)
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let binding = admission.binding();
        if !admission.is_exact()
            || binding.owner().draft_id() != identity.draft_id()
            || binding.owner().session_id() != identity.session_id()
            || binding.owner().operation_id().as_bytes() != identity.operation_id().as_bytes()
            || binding.session_generation().get() != begin.session_generation()
            || binding.predecessor_candidate_generation()
                != begin.predecessor_candidate_generation()
            || binding.predecessor_root() != begin.predecessor_root()
            || binding.predecessor_history() != begin.predecessor_history()
            || binding.sealed_target_root() != admission.target_root()
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        if self.draft_mutation_staging_status(store, identity)?
            != (DraftMutationStagingStatusV1::Receiving {
                head: head.receipt(),
            })
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let admitted = self
            .point::<DraftMarkerAdmissionHeadsFamily>(store, binding.owner(), point_limit())?
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        if admitted.lifecycle() != DraftMarkerAdmissionLifecycleV1::Staging
            || admitted.home_generation() != binding.home_generation()
            || admitted.owner() != binding.owner()
            || admitted.occurrence_commitment() != binding.occurrence_commitment()
            || admitted.target_root() != admission.target_root()
            || admitted.remaining_builder_count() != admission.remaining_count()
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        let marker = admission::index::resolve_assigned_target(
            self, store, admission, marker_id, asset_id, order_key,
        )
        .map_err(|error| match error {
            admission::index::DraftMarkerAdmissionIndexPreparationErrorV1::StoreRead(error) => {
                DraftMutationStagingErrorV1::Read(error)
            }
            _ => DraftMutationStagingErrorV1::Invalid,
        })?;
        if self.revision(store).map_err(SyndicReadError::from)? != revision {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        Ok(marker)
    }
}
