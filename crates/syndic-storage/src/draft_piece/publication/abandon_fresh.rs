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

pub(crate) fn fresh_opening_history_is_exact_with_access(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    head: &DraftEditorCandidateSessionV1,
) -> Result<bool, SyndicReadError> {
    let durable = storage.point_with_access::<DraftEditHistoryFrontiersFamily>(
        store,
        head.durable_base_history().key(),
        point_limit(),
    )?;
    let newest = storage.point_with_access::<DraftEditHistoryFrontiersFamily>(
        store,
        head.newest_history().key(),
        point_limit(),
    )?;
    let (Some(durable), Some(newest)) = (durable, newest) else {
        return Ok(false);
    };
    Ok(durable.reference() == head.durable_base_history()
        && newest.reference() == head.newest_history()
        && durable.fork_session(head.session_id()).as_ref() == Some(&newest)
        && draft_edit_history_frontier_is_authenticated_with_access(storage, store, &durable)?
        && draft_edit_history_frontier_is_authenticated_with_access(storage, store, &newest)?)
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
    type Prepared = Option<PreparedAbandonFreshMutation>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let request = self.prepared.request;
        if let Some(receipt) = existing_disposal_receipt(reader, request)? {
            validate_disposal_receipt(
                reader,
                receipt
                    .disposal()
                    .ok_or(SyndicMutationError::IdentityCollision)?,
            )?;
            return Ok(None);
        }
        let Some(head) = matching_fresh_head(reader, &self.prepared)? else {
            return Ok(None);
        };
        let newest =
            required::<DraftEditHistoryFrontiersFamily>(reader, &head.newest_history().key())?;
        if !history_is_exact(reader, &head, &newest)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
        finish_fresh_preparation(self.prepared, head, newest).map(Some)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(2)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let Some(prepared) = prepared else {
            return Ok(());
        };
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &session_key(prepared.after.draft_id(), prepared.after.session_id()),
            &DraftEditorCandidateSessionRecordV1::Head(prepared.after),
        )?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &disposal_key(prepared.request),
            &DraftEditorCandidateSessionRecordV1::OpenReceipt(
                DraftEditorCandidateSessionOpenReceiptV1::from_disposal(prepared.receipt),
            ),
        )?;
        Ok(())
    }
}

fn existing_disposal_receipt(
    reader: &DomainReader<'_, SyndicDomain>,
    request: DraftEditorCandidateSessionDisposeRequestV1,
) -> Result<Option<DraftEditorCandidateSessionOpenReceiptV1>, SyndicMutationError> {
    match point::<DraftEditorCandidateSessionsFamily>(reader, &disposal_key(request))? {
        None => Ok(None),
        Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)) => Ok(Some(receipt)),
        Some(_) => Err(SyndicMutationError::IdentityCollision),
    }
}

fn matching_fresh_head(
    reader: &DomainReader<'_, SyndicDomain>,
    prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
) -> Result<Option<DraftEditorCandidateSessionV1>, SyndicMutationError> {
    let request = prepared.request;
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
    if head != prepared.before_head
        || open != prepared.open_receipt
        || !request_matches_head_and_open(request, &head, &open)
    {
        return Ok(None);
    }
    Ok(Some(head))
}

fn finish_fresh_preparation(
    prepared: PreparedDraftEditorCandidateSessionAbandonFreshV1,
    head: DraftEditorCandidateSessionV1,
    newest: DraftEditHistoryFrontierV1,
) -> Result<PreparedAbandonFreshMutation, SyndicMutationError> {
    let request = prepared.request;
    let after = head
        .abandoned_fresh(request.operation_id())
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let receipt = DraftEditorCandidateSessionDisposeReceiptV1::new(
        prepared.canonical_request,
        head,
        after.clone(),
        newest,
    );
    Ok(PreparedAbandonFreshMutation {
        request,
        after,
        receipt,
    })
}

struct PreparedAbandonFreshMutation {
    request: DraftEditorCandidateSessionDisposeRequestV1,
    after: DraftEditorCandidateSessionV1,
    receipt: DraftEditorCandidateSessionDisposeReceiptV1,
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
        self.prepare_abandon_fresh_draft_editor_candidate_session_with_access(
            ReadAccess::Ordinary(store),
            request,
        )
    }

    pub fn prepare_abandon_fresh_draft_editor_candidate_session_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionAbandonFreshV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.prepare_abandon_fresh_draft_editor_candidate_session_with_access(
            ReadAccess::Candidate(store),
            request,
        )
    }

    fn prepare_abandon_fresh_draft_editor_candidate_session_with_access(
        &self,
        store: ReadAccess<'_>,
        request: DraftEditorCandidateSessionDisposeRequestV1,
    ) -> Result<
        PreparedDraftEditorCandidateSessionAbandonFreshV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        if !request.expected_pair().is_coherent() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let limit = point_limit();
        let occupied = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            disposal_key(request),
            limit,
        )?;
        let head = match self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            session_key(request.draft_id(), request.session_id()),
            limit,
        )? {
            Some(DraftEditorCandidateSessionRecordV1::Head(head)) => head,
            _ => return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant),
        };
        let open_receipt = match self.point_with_access::<DraftEditorCandidateSessionsFamily>(
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
        self.reconcile_abandon_fresh_draft_editor_candidate_session_with_access(
            ReadAccess::Ordinary(store),
            prepared,
            outcome,
        )
    }

    pub fn reconcile_abandon_fresh_draft_editor_candidate_session_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidateSessionAbandonFreshOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        self.reconcile_abandon_fresh_draft_editor_candidate_session_with_access(
            ReadAccess::Candidate(store),
            prepared,
            outcome,
        )
    }

    fn reconcile_abandon_fresh_draft_editor_candidate_session_with_access(
        &self,
        store: ReadAccess<'_>,
        prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
        outcome: CommandOutcome,
    ) -> Result<
        DraftEditorCandidateSessionAbandonFreshOutcomeV1,
        DraftEditorCandidatePublicationCommandErrorV1,
    > {
        let committed = reconcile_command_outcome(store, outcome)?;
        let revision = self
            .revision_with_access(store)
            .map_err(SyndicReadError::Read)?;
        if let Some(receipt) = read_abandonment_receipt(self, store, prepared.request)? {
            if !validate_disposal_receipt_with_access(self, store, &receipt)? {
                return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
            }
            ensure_abandonment_revision(self, store, revision)?;
            return Ok(classify_abandonment_receipt(prepared, committed, receipt));
        }
        let result = classify_absent_abandonment(self, store, prepared, committed);
        ensure_abandonment_revision(self, store, revision)?;
        result
    }
}

fn reconcile_command_outcome(
    store: ReadAccess<'_>,
    outcome: CommandOutcome,
) -> Result<bool, DraftEditorCandidatePublicationCommandErrorV1> {
    match outcome {
        CommandOutcome::NotCommitted { .. } => Ok(false),
        CommandOutcome::Committed { .. } => Ok(true),
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            let handle = reconciliation.install_and_handle();
            committed_from_resolution(
                match store {
                    ReadAccess::Ordinary(store) => store.reconcile(&handle),
                    ReadAccess::Candidate(store) => store.reconcile(&handle),
                }
                .map_err(DraftEditorCandidatePublicationCommandErrorV1::Reconciliation)?,
            )
        }
    }
}

fn read_abandonment_receipt(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    request: DraftEditorCandidateSessionDisposeRequestV1,
) -> Result<
    Option<DraftEditorCandidateSessionDisposeReceiptV1>,
    DraftEditorCandidatePublicationCommandErrorV1,
> {
    let Some(record) = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        disposal_key(request),
        point_limit(),
    )?
    else {
        return Ok(None);
    };
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
        return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
    };
    let receipt = receipt
        .disposal()
        .cloned()
        .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
    Ok(Some(receipt))
}

fn classify_abandonment_receipt(
    prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
    committed: bool,
    receipt: DraftEditorCandidateSessionDisposeReceiptV1,
) -> DraftEditorCandidateSessionAbandonFreshOutcomeV1 {
    let is_fresh_abandonment = matches!(
        disposal_receipt_parts(&receipt),
        Some((_, DisposalTransitionKind::FreshAbandonment))
    );
    if receipt.request_bytes() != prepared.canonical_request || !is_fresh_abandonment {
        return DraftEditorCandidateSessionAbandonFreshOutcomeV1::OccupiedIdentityCollision(
            DraftEditorCandidateSessionDisposeCollisionProofV1::new(prepared.request, receipt),
        );
    }
    if committed && prepared.initially_absent {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(receipt.after_head().clone())
    } else {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(receipt)
    }
}

fn classify_absent_abandonment(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    prepared: &PreparedDraftEditorCandidateSessionAbandonFreshV1,
    committed: bool,
) -> Result<
    DraftEditorCandidateSessionAbandonFreshOutcomeV1,
    DraftEditorCandidatePublicationCommandErrorV1,
> {
    let request = prepared.request;
    let head = match abandonment_session_with_access(
        storage,
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

fn ensure_abandonment_revision(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    revision: DomainRevision,
) -> Result<(), DraftEditorCandidatePublicationCommandErrorV1> {
    if storage
        .revision_with_access(store)
        .map_err(SyndicReadError::Read)?
        != revision
    {
        return Err(SyndicReadError::ConcurrentChange {
            operation: "fresh editor abandonment outcome",
        }
        .into());
    }
    Ok(())
}

fn abandonment_session_with_access(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> Result<DraftEditorCandidateSessionReadOutcomeV1, SyndicReadError> {
    if let ReadAccess::Ordinary(store) = store {
        return storage.draft_editor_candidate_session(store, draft_id, session_id);
    }
    let key = session_key(draft_id, session_id);
    let first = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
        store,
        key,
        point_limit(),
    )?;
    let exact = match &first {
        Some(DraftEditorCandidateSessionRecordV1::Head(head)) => match head.lifecycle() {
            DraftEditorCandidateSessionLifecycleV1::Active => {
                if checkpoint::has_opening_identity(head) && checkpoint::has_saved_identity(head) {
                    let receipt = storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
                        store,
                        DraftEditorCandidateSessionRecordKeyV1::open_receipt(
                            head.draft_id(),
                            head.session_id(),
                            head.open_operation_id(),
                        ),
                        point_limit(),
                    )?;
                    matches!(receipt, Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt))
                        if receipt.is_open() && receipt.head() == head)
                        && head.active_operation().is_none()
                        && fresh_opening_history_is_exact_with_access(storage, store, head)?
                } else {
                    session::idle_candidate_closure_is_exact_with_access(storage, store, head)?
                }
            }
            DraftEditorCandidateSessionLifecycleV1::Disposed => {
                let operation = head
                    .disposal_operation_id()
                    .ok_or(SyndicReadError::Invariant(
                        "disposed editor has no disposal identity",
                    ))?;
                match storage.point_with_access::<DraftEditorCandidateSessionsFamily>(
                    store,
                    DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
                        draft_id, session_id, operation,
                    ),
                    point_limit(),
                )? {
                    Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)) => {
                        match receipt.disposal() {
                            Some(receipt) => {
                                validate_disposal_receipt_with_access(storage, store, receipt)?
                            }
                            None => false,
                        }
                    }
                    _ => false,
                }
            }
        },
        None => true,
        _ => false,
    };
    if storage.point_with_access::<DraftEditorCandidateSessionsFamily>(store, key, point_limit())?
        != first
    {
        return Ok(DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange);
    }
    if !exact {
        return Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure);
    }
    Ok(match first {
        None => DraftEditorCandidateSessionReadOutcomeV1::Absent,
        Some(DraftEditorCandidateSessionRecordV1::Head(head)) => match head.lifecycle() {
            DraftEditorCandidateSessionLifecycleV1::Active => {
                DraftEditorCandidateSessionReadOutcomeV1::Active(head)
            }
            DraftEditorCandidateSessionLifecycleV1::Disposed => {
                DraftEditorCandidateSessionReadOutcomeV1::Disposed(head)
            }
        },
        _ => DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure,
    })
}
