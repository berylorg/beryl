use super::*;

#[derive(Clone)]
pub struct PreparedDraftEditorCandidateSessionAbandonFreshV1 {
    request: DraftEditorCandidateSessionDisposeRequestV1,
    canonical_request: Vec<u8>,
    before_head: DraftEditorCandidateSessionV1,
    open_receipt: DraftEditorCandidateSessionOpenReceiptV1,
    initially_absent: bool,
}

impl PreparedDraftEditorCandidateSessionAbandonFreshV1 {
    pub const fn request(&self) -> DraftEditorCandidateSessionDisposeRequestV1 {
        self.request
    }

    pub fn canonical_request(&self) -> &[u8] {
        &self.canonical_request
    }
}

#[derive(Clone)]
struct AbandonFreshMutation {
    prepared: PreparedDraftEditorCandidateSessionAbandonFreshV1,
}

pub(super) fn request_matches_head_and_open(
    request: DraftEditorCandidateSessionDisposeRequestV1,
    head: &DraftEditorCandidateSessionV1,
    open: &DraftEditorCandidateSessionOpenReceiptV1,
) -> bool {
    open.is_open()
        && open.head() == head
        && disposal_request_names_head(request, head)
        && head.abandoned_fresh(request.operation_id()).is_some()
}

pub(super) fn history_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditorCandidateSessionV1,
    newest: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicMutationError> {
    let durable =
        required::<DraftEditHistoryFrontiersFamily>(reader, &head.durable_base_history().key())?;
    if newest.reference() != head.newest_history()
        || durable.reference() != head.durable_base_history()
        || durable.fork_session(head.session_id()).as_ref() != Some(newest)
    {
        return Ok(false);
    }
    authenticate_draft_edit_history_frontier_v1(reader, &durable)?;
    authenticate_draft_edit_history_frontier_v1(reader, newest)?;
    Ok(true)
}

fn committed_from_resolution(
    resolution: ReconciliationResolution,
) -> Result<bool, DraftEditorCandidatePublicationCommandErrorV1> {
    match resolution {
        ReconciliationResolution::ExactNew { .. } => Ok(true),
        ReconciliationResolution::ExactOld => Ok(false),
        ReconciliationResolution::ExactSuccessor { .. } => {
            Err(DraftEditorCandidatePublicationCommandErrorV1::UnauthorizedReconciliationSuccessor)
        }
        ReconciliationResolution::Collision => {
            Err(DraftEditorCandidatePublicationCommandErrorV1::ReconciliationCollision)
        }
    }
}

#[cfg(feature = "test-faults")]
pub fn test_abandon_fresh_reconciliation_resolution(
    resolution: ReconciliationResolution,
) -> Result<bool, DraftEditorCandidatePublicationCommandErrorV1> {
    committed_from_resolution(resolution)
}

impl DomainMutation<SyndicDomain> for AbandonFreshMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let request = self.prepared.request;
        if let Some(record) =
            point::<DraftEditorCandidateSessionsFamily>(reader, &disposal_key(request))?
        {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(SyndicMutationError::IdentityCollision);
            };
            return validate_disposal_receipt(
                reader,
                receipt
                    .disposal()
                    .ok_or(SyndicMutationError::IdentityCollision)?,
            );
        }
        let DraftEditorCandidateSessionRecordV1::Head(head) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &session_key(request.draft_id(), request.session_id()),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        let DraftEditorCandidateSessionRecordV1::OpenReceipt(open) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
                    head.draft_id(),
                    head.session_id(),
                    head.open_operation_id(),
                ),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if head != self.prepared.before_head
            || open != self.prepared.open_receipt
            || !request_matches_head_and_open(request, &head, &open)
        {
            return Ok(());
        }
        let newest =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if !history_is_exact(reader, &head, &newest)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(2)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let request = self.prepared.request;
        if point::<DraftEditorCandidateSessionsFamily>(reader, &disposal_key(request))?.is_some() {
            return Ok(());
        }
        let DraftEditorCandidateSessionRecordV1::Head(head) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &session_key(request.draft_id(), request.session_id()),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        let DraftEditorCandidateSessionRecordV1::OpenReceipt(open) =
            required::<DraftEditorCandidateSessionsFamily>(
                reader,
                &DraftEditorCandidateSessionRecordKeyV1::open_receipt(
                    head.draft_id(),
                    head.session_id(),
                    head.open_operation_id(),
                ),
            )?
        else {
            return Err(SyndicMutationError::IdentityCollision);
        };
        if head != self.prepared.before_head
            || open != self.prepared.open_receipt
            || !request_matches_head_and_open(request, &head, &open)
        {
            return Ok(());
        }
        let newest =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if !history_is_exact(reader, &head, &newest)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let after = head
            .abandoned_fresh(request.operation_id())
            .ok_or(SyndicMutationError::IdentityCollision)?;
        let receipt = DraftEditorCandidateSessionDisposeReceiptV1::new(
            self.prepared.canonical_request.clone(),
            head,
            after.clone(),
            newest,
        );
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &session_key(after.draft_id(), after.session_id()),
            &DraftEditorCandidateSessionRecordV1::Head(after),
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &disposal_key(request),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::from_disposal(receipt),
            ),
        )?;
        Ok(())
    }
}

impl SyndicStorage {
    pub fn prepare_abandon_fresh_draft_editor_candidate_session(
        &self,
        store: &HomeStore,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionAbandonFreshV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        if !request.expected_pair().is_coherent() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        let occupied =
            self.point::<DraftEditorCandidateSessionsFamily>(store, disposal_key(request), limit)?;
        let head = match self.point::<DraftEditorCandidateSessionsFamily>(
            store,
            session_key(request.draft_id(), request.session_id()),
            limit,
        )? {
            Some(DraftEditorCandidateSessionRecordV1::Head(head)) => head,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        let open_receipt = match self.point::<DraftEditorCandidateSessionsFamily>(
            store,
            DraftEditorCandidateSessionRecordKeyV1::open_receipt(
                head.draft_id(),
                head.session_id(),
                head.open_operation_id(),
            ),
            limit,
        )? {
            Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt))
                if receipt.is_open() =>
            {
                receipt
            }
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if occupied
            .as_ref()
            .is_some_and(|record| !matches!(record, DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) if receipt.disposal().is_some()))
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        Ok(PreparedDraftEditorCandidateSessionAbandonFreshV1 {
            request,
            canonical_request: canonical_candidate_disposal_request_bytes(request),
            before_head: head,
            open_receipt,
            initially_absent: occupied.is_none(),
        })
    }

    pub fn abandon_fresh_draft_editor_candidate_session(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftEditorCandidateSessionAbandonFreshV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, AbandonFreshMutation { prepared })
    }

    pub fn reconcile_abandon_fresh_draft_editor_candidate_session(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidateSessionAbandonFreshOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let committed = match outcome {
            CommandOutcome::NotCommitted { .. } => false,
            CommandOutcome::Committed { .. } => true,
            CommandOutcome::Indeterminate { reconciliation, .. } => committed_from_resolution(
                store
                    .reconcile(&reconciliation.install_and_handle())
                    .map_err(DraftEditorCandidatePublicationCommandErrorV1::Reconciliation)?,
            )?,
        };
        let request = prepared.request;
        let limit = point_limit();
        if let Some(record) =
            self.point::<DraftEditorCandidateSessionsFamily>(store, disposal_key(request), limit)?
        {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            };
            let receipt = receipt
                .disposal()
                .cloned()
                .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
            if !validate_disposal_receipt_in_store(self, store, &receipt)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            let is_fresh_abandonment = matches!(
                disposal_receipt_parts(&receipt),
                Some((_, DisposalTransitionKind::FreshAbandonment))
            );
            if receipt.request_bytes() != prepared.canonical_request || !is_fresh_abandonment {
                return Ok(
                    DraftEditorCandidateSessionAbandonFreshOutcomeV1::OccupiedIdentityCollision(
                        DraftEditorCandidateSessionDisposeCollisionProofV1::new(request, receipt),
                    ),
                );
            }
            return if committed && prepared.initially_absent {
                Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(
                    receipt.after_head().clone(),
                ))
            } else {
                Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(receipt))
            };
        }
        let head = match self.draft_editor_candidate_session(
            store,
            request.draft_id(),
            request.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(head)
            | DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        if head.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Disposed {
            return Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::AlreadyDisposed(head));
        }
        if !request_matches_head_and_open(request, &head, &prepared.open_receipt)
            || head != prepared.before_head
        {
            return Ok(DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(
                head,
            ));
        }
        Err(if committed {
            DraftEditorCandidatePublicationCommandErrorV1::Invariant
        } else {
            DraftEditorCandidatePublicationCommandErrorV1::NotCommitted
        })
    }
}
