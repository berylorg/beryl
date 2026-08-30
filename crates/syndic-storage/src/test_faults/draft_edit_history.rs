use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation, RecordCodec,
};

use crate::mutation::required;
use crate::{
    DraftByThreadRecord, DraftRecord, SyndicMutationError, SyndicStorage,
    codec::{DraftByThreadCodec, DraftsCodec, ThreadsFamily},
    domain::SyndicDomain,
    draft_piece::{
        DraftEditHistoryFrontierKeyV1, DraftEditHistoryFrontierV1, DraftEditHistoryFrontiersCodec,
        DraftEditHistoryFrontiersFamily, DraftEditHistoryTransitionKeyV1,
        DraftEditHistoryTransitionV1, DraftEditHistoryTransitionsCodec,
        DraftEditHistoryTransitionsFamily, DraftEditorCandidateSessionRecordKeyV1,
        DraftEditorCandidateSessionRecordV1, DraftEditorCandidateSessionV1,
        DraftEditorCandidateSessionsCodec, DraftEditorCandidateSessionsFamily,
        DraftPieceOperationIdV1, DraftPieceRootReferenceV1, DraftPieceRootsFamily,
        DraftRootHistoryPairV1, point_limit,
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

pub fn draft_edit_history_accounting_corruption(
    frontier: &crate::DraftEditHistoryFrontierV1,
) -> crate::DraftEditHistoryFrontierV1 {
    crate::draft_edit_history_accounting_corruption_for_test(frontier)
}

pub fn draft_edit_history_availability_corruption(
    frontier: &crate::DraftEditHistoryFrontierV1,
    undo_head: crate::DraftEditHistoryTransitionReferenceV1,
) -> crate::DraftEditHistoryFrontierV1 {
    crate::draft_edit_history_availability_corruption_for_test(frontier, undo_head)
}

pub fn draft_edit_history_no_head_gap(
    frontier: &crate::DraftEditHistoryFrontierV1,
) -> crate::DraftEditHistoryFrontierV1 {
    crate::draft_edit_history_no_head_gap_for_test(frontier)
}

pub fn draft_edit_history_first_transition_gap(
    frontier: &crate::DraftEditHistoryFrontierV1,
    transition: &crate::DraftEditHistoryTransitionV1,
) -> (
    crate::DraftEditHistoryFrontierV1,
    crate::DraftEditHistoryTransitionV1,
) {
    crate::draft_edit_history_first_transition_gap_for_test(frontier, transition)
}

pub fn draft_edit_history_wrong_head_root(
    frontier: &crate::DraftEditHistoryFrontierV1,
    head: &crate::DraftEditHistoryTransitionV1,
) -> crate::DraftEditHistoryFrontierV1 {
    crate::draft_edit_history_wrong_head_root_for_test(frontier, head)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftCandidatePublicationFault {
    DeleteSessionRecord(DraftEditorCandidateSessionRecordKeyV1),
    OccupyReceiptWithHead {
        receipt_key: DraftEditorCandidateSessionRecordKeyV1,
        draft_id: beryl_model::SyndicDraftId,
        session_id: crate::DraftEditorCandidateSessionIdV1,
    },
    RetargetDisposedHead {
        draft_id: beryl_model::SyndicDraftId,
        session_id: crate::DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    },
}

pub fn inject_draft_candidate_publication_fault(
    store: &HomeStore,
    storage: SyndicStorage,
    fault: DraftCandidatePublicationFault,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        InjectDraftCandidatePublicationFault(fault),
    )
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

pub fn replace_draft_edit_history_frontier_and_session(
    store: &HomeStore,
    storage: SyndicStorage,
    session: DraftEditorCandidateSessionV1,
    replacement: DraftEditHistoryFrontierV1,
    replacement_transition: Option<DraftEditHistoryTransitionV1>,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        ReplaceDraftEditHistoryFrontierAndSession {
            session,
            replacement,
            replacement_transition,
        },
    )
}

pub fn publish_draft_edit_history_pair(
    store: &HomeStore,
    storage: SyndicStorage,
    draft: DraftRecord,
    root: DraftPieceRootReferenceV1,
    history: crate::DraftEditHistoryFrontierReferenceV1,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        PublishDraftEditHistoryPair {
            draft,
            root,
            history,
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

pub fn draft_edit_history_transition_exists(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftEditHistoryTransitionKeyV1,
) -> bool {
    storage
        .point::<DraftEditHistoryTransitionsFamily>(store, key, point_limit())
        .expect("draft edit-history transition existence read must succeed")
        .is_some()
}

pub fn draft_edit_history_root_exists(
    store: &HomeStore,
    storage: SyndicStorage,
    root: DraftPieceRootReferenceV1,
) -> bool {
    storage
        .point::<DraftPieceRootsFamily>(store, root.key(), point_limit())
        .expect("draft edit-history root existence read must succeed")
        .is_some_and(|stored| stored.reference() == root)
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
        &storage.handle,
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

#[derive(Clone)]
struct ReplaceDraftEditHistoryFrontierAndSession {
    session: DraftEditorCandidateSessionV1,
    replacement: DraftEditHistoryFrontierV1,
    replacement_transition: Option<DraftEditHistoryTransitionV1>,
}

#[derive(Clone)]
struct PublishDraftEditHistoryPair {
    draft: DraftRecord,
    root: DraftPieceRootReferenceV1,
    history: crate::DraftEditHistoryFrontierReferenceV1,
}

#[derive(Clone)]
struct InjectDraftCandidatePublicationFault(DraftCandidatePublicationFault);

enum PreparedDraftCandidatePublicationFault {
    Delete(DraftEditorCandidateSessionRecordKeyV1),
    Occupy {
        receipt_key: DraftEditorCandidateSessionRecordKeyV1,
        value: DraftEditorCandidateSessionRecordV1,
    },
    Retarget {
        key: DraftEditorCandidateSessionRecordKeyV1,
        replacement: DraftEditorCandidateSessionRecordV1,
    },
}

impl DomainMutation<SyndicDomain> for InjectDraftCandidatePublicationFault {
    type Error = SyndicMutationError;
    type Prepared = PreparedDraftCandidatePublicationFault;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        match self.0 {
            DraftCandidatePublicationFault::DeleteSessionRecord(key) => {
                Ok(PreparedDraftCandidatePublicationFault::Delete(key))
            }
            DraftCandidatePublicationFault::OccupyReceiptWithHead {
                receipt_key,
                draft_id,
                session_id,
            } => Ok(PreparedDraftCandidatePublicationFault::Occupy {
                receipt_key,
                value: required::<DraftEditorCandidateSessionsFamily>(
                    reader,
                    &DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id),
                )?,
            }),
            DraftCandidatePublicationFault::RetargetDisposedHead {
                draft_id,
                session_id,
                operation_id,
            } => {
                let key = DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id);
                let DraftEditorCandidateSessionRecordV1::Head(head) =
                    required::<DraftEditorCandidateSessionsFamily>(reader, &key)?
                else {
                    return Err(SyndicMutationError::IdentityCollision);
                };
                let replacement = DraftEditorCandidateSessionV1::from_parts_with_disposal(
                    head.thread_id(),
                    head.draft_id(),
                    head.session_id(),
                    head.open_operation_id(),
                    head.session_generation(),
                    head.durable_base_selector_revision(),
                    head.durable_base_root(),
                    head.durable_base_history(),
                    head.published_candidate_generation(),
                    head.published_selector_revision(),
                    head.published_root(),
                    head.published_history(),
                    head.newest_candidate_generation(),
                    head.newest_root(),
                    head.newest_history(),
                    head.dirty_generation(),
                    head.logical_extent(),
                    head.lifecycle(),
                    Some(operation_id),
                    head.active_operation().cloned(),
                );
                Ok(PreparedDraftCandidatePublicationFault::Retarget {
                    key,
                    replacement: DraftEditorCandidateSessionRecordV1::Head(replacement),
                })
            }
        }
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match prepared {
            PreparedDraftCandidatePublicationFault::Delete(key) => {
                mutations.delete::<DraftEditorCandidateSessionsCodec>(&key)?;
            }
            PreparedDraftCandidatePublicationFault::Occupy { receipt_key, value } => {
                mutations.put::<DraftEditorCandidateSessionsCodec>(&receipt_key, &value)?;
            }
            PreparedDraftCandidatePublicationFault::Retarget { key, replacement } => {
                mutations.put::<DraftEditorCandidateSessionsCodec>(&key, &replacement)?;
            }
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for DeleteDraftEditHistoryFrontier {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<DraftEditHistoryFrontiersCodec>(&prepared.0)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for DeleteDraftEditHistoryRecord {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
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
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match prepared.0 {
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
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<DraftEditHistoryFrontiersCodec>(&prepared.stored_key, &prepared.replacement)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for ReplaceDraftEditHistoryFrontierAndSession {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryTransitionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let source = &prepared.session;
        let replacement_session = DraftEditorCandidateSessionV1::from_parts(
            source.thread_id(),
            source.draft_id(),
            source.session_id(),
            source.open_operation_id(),
            source.session_generation(),
            source.durable_base_selector_revision(),
            source.durable_base_root(),
            source.durable_base_history(),
            source.published_candidate_generation(),
            source.published_selector_revision(),
            source.published_root(),
            source.published_history(),
            source.newest_candidate_generation(),
            prepared.replacement.reference().root(),
            prepared.replacement.reference(),
            source.dirty_generation(),
            source.logical_extent(),
            source.lifecycle(),
            source.active_operation().cloned(),
        );
        mutations.put::<DraftEditHistoryFrontiersCodec>(
            &prepared.replacement.reference().key(),
            &prepared.replacement,
        )?;
        if let Some(transition) = &prepared.replacement_transition {
            mutations.put::<DraftEditHistoryTransitionsCodec>(&transition.key(), transition)?;
        }
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &DraftEditorCandidateSessionRecordKeyV1::head(source.draft_id(), source.session_id()),
            &DraftEditorCandidateSessionRecordV1::Head(replacement_session),
        )?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for PublishDraftEditHistoryPair {
    type Error = SyndicMutationError;
    type Prepared = (DraftRecord, DraftByThreadRecord);

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let revision = self
            .draft
            .revision()
            .checked_next()
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
        let thread = required::<ThreadsFamily>(reader, &self.draft.thread_id())?;
        let replacement = DraftRecord::new(
            self.draft.id(),
            self.draft.thread_id(),
            revision,
            self.draft.submission_intent(),
            DraftRootHistoryPairV1::new(self.root, self.history),
            self.draft.created_at(),
            self.draft.updated_at(),
        );
        let by_thread = DraftByThreadRecord::new(
            replacement.thread_id(),
            replacement.id(),
            revision,
            thread.revision(),
        );
        Ok((replacement, by_thread))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftsCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        Ok(())
    }

    fn contribute(
        (replacement, by_thread): Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<DraftsCodec>(&replacement.id(), &replacement)?;
        mutations.put::<DraftByThreadCodec>(&replacement.thread_id(), &by_thread)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for ReplaceDraftEditHistoryTransition {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditHistoryTransitionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations
            .put::<DraftEditHistoryTransitionsCodec>(&prepared.stored_key, &prepared.replacement)?;
        Ok(())
    }
}
