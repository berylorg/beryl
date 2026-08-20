use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation, RecordCodec,
};

use crate::{
    SyndicMutationError, SyndicStorage,
    domain::SyndicDomain,
    draft_piece::{
        DraftEditHistoryFrontierKeyV1, DraftEditHistoryFrontierV1, DraftEditHistoryFrontiersCodec,
        DraftEditHistoryFrontiersFamily, DraftEditHistoryTransitionKeyV1,
        DraftEditHistoryTransitionV1, DraftEditHistoryTransitionsCodec,
    },
};

pub fn draft_edit_history_overflow_errors(
    root: crate::DraftPieceRootReferenceV1,
    session_id: crate::DraftEditorCandidateSessionIdV1,
    operation_id: crate::DraftPieceOperationIdV1,
    position: crate::DraftCompositePositionV1,
) -> [crate::DraftEditHistoryAppendErrorV1; 4] {
    crate::draft_edit_history_overflow_errors_for_test(root, session_id, operation_id, position)
}

pub fn draft_edit_history_stored_charge_components(
    frontier: &crate::DraftEditHistoryFrontierV1,
    transition: &crate::DraftEditHistoryTransitionV1,
) -> Result<[u64; 6], crate::DraftEditHistoryAppendErrorV1> {
    crate::draft_edit_history_stored_charge_components_for_test(frontier, transition)
}

pub fn occupy_canonical_empty_draft_edit_history(
    store: &HomeStore,
    storage: SyndicStorage,
    draft_id: beryl_model::SyndicDraftId,
    policy: crate::DraftEditHistoryPolicyV1,
) -> MutationContribution {
    let root = crate::canonical_empty_draft_piece_root_v1(
        draft_id,
        beryl_model::DraftRevision::new(1).expect("fixture revision is nonzero"),
        crate::canonical_empty_draft_root_operation_id_v1(draft_id),
    );
    let frontier = crate::canonical_empty_draft_edit_history_v1(root.reference(), policy);
    replace_draft_edit_history_frontier(store, storage, frontier.reference().key(), frontier)
}

#[allow(clippy::too_many_arguments)]
pub fn alternative_ordinary_draft_edit_history(
    source: &crate::DraftEditHistoryFrontierV1,
    successor_generation: u64,
    successor_root: crate::DraftPieceRootReferenceV1,
    before_caret: crate::DraftCompositePositionV1,
    before_selection: crate::DraftCompositePositionV1,
    after_caret: crate::DraftCompositePositionV1,
    after_selection: crate::DraftCompositePositionV1,
    operation_id: crate::DraftPieceOperationIdV1,
) -> (
    crate::DraftEditHistoryTransitionV1,
    crate::DraftEditHistoryFrontierV1,
) {
    crate::alternative_ordinary_draft_edit_history_for_test(
        source,
        successor_generation,
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        operation_id,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditHistoryRecordDeletion {
    Frontier(DraftEditHistoryFrontierKeyV1),
    Transition(DraftEditHistoryTransitionKeyV1),
}

pub fn delete_draft_edit_history_frontier(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftEditHistoryFrontierKeyV1,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DeleteDraftEditHistoryFrontier(key),
    )
}

pub fn delete_draft_edit_history_record(
    store: &HomeStore,
    storage: SyndicStorage,
    deletion: DraftEditHistoryRecordDeletion,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        DeleteDraftEditHistoryRecord(deletion),
    )
}

pub fn replace_draft_edit_history_frontier(
    store: &HomeStore,
    storage: SyndicStorage,
    stored_key: DraftEditHistoryFrontierKeyV1,
    replacement: DraftEditHistoryFrontierV1,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        ReplaceDraftEditHistoryFrontier {
            stored_key,
            replacement,
        },
    )
}

pub fn replace_draft_edit_history_transition(
    store: &HomeStore,
    storage: SyndicStorage,
    stored_key: DraftEditHistoryTransitionKeyV1,
    replacement: DraftEditHistoryTransitionV1,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        ReplaceDraftEditHistoryTransition {
            stored_key,
            replacement,
        },
    )
}

pub fn inject_draft_edit_history_frontier_digest_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftEditHistoryFrontierKeyV1,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let encoded_key =
        <DraftEditHistoryFrontiersCodec as RecordCodec<SyndicDomain>>::encode_key(&key)
            .expect("fixture history key must encode");
    let frontier = storage
        .point::<DraftEditHistoryFrontiersFamily>(
            store,
            key,
            crate::SyndicPointReadLimit::new(65_536).expect("fixture point bound is nonzero"),
        )
        .expect("fixture history reads")
        .expect("fixture history exists");
    let mut payload =
        <DraftEditHistoryFrontiersCodec as RecordCodec<SyndicDomain>>::encode_value(&frontier)
            .expect("fixture history must encode");
    let last = payload
        .last_mut()
        .expect("fixture history encoding is nonempty");
    *last ^= 0xA5;
    store.inject_persisted_corrupt_record::<SyndicDomain, DraftEditHistoryFrontiersCodec>(
        storage.handle,
        &encoded_key,
        &payload,
    )
}

#[derive(Clone, Copy)]
struct DeleteDraftEditHistoryFrontier(DraftEditHistoryFrontierKeyV1);

#[derive(Clone, Copy)]
struct DeleteDraftEditHistoryRecord(DraftEditHistoryRecordDeletion);

#[derive(Clone)]
struct ReplaceDraftEditHistoryFrontier {
    stored_key: DraftEditHistoryFrontierKeyV1,
    replacement: DraftEditHistoryFrontierV1,
}

#[derive(Clone)]
struct ReplaceDraftEditHistoryTransition {
    stored_key: DraftEditHistoryTransitionKeyV1,
    replacement: DraftEditHistoryTransitionV1,
}

impl DomainMutation<SyndicDomain> for DeleteDraftEditHistoryFrontier {
    type Error = SyndicMutationError;

    fn validate(&self, _reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<DraftEditHistoryFrontiersCodec>(&self.0)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for DeleteDraftEditHistoryRecord {
    type Error = SyndicMutationError;

    fn validate(&self, _reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.0 {
            DraftEditHistoryRecordDeletion::Frontier(_) => {
                reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
            }
            DraftEditHistoryRecordDeletion::Transition(_) => {
                reservation.reserve_records::<DraftEditHistoryTransitionsCodec>(1)?;
            }
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.0 {
            DraftEditHistoryRecordDeletion::Frontier(key) => {
                mutations.delete::<DraftEditHistoryFrontiersCodec>(&key)?;
            }
            DraftEditHistoryRecordDeletion::Transition(key) => {
                mutations.delete::<DraftEditHistoryTransitionsCodec>(&key)?;
            }
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for ReplaceDraftEditHistoryFrontier {
    type Error = SyndicMutationError;

    fn validate(&self, _reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<DraftEditHistoryFrontiersCodec>(&self.stored_key, &self.replacement)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for ReplaceDraftEditHistoryTransition {
    type Error = SyndicMutationError;

    fn validate(&self, _reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditHistoryTransitionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<DraftEditHistoryTransitionsCodec>(&self.stored_key, &self.replacement)?;
        Ok(())
    }
}
