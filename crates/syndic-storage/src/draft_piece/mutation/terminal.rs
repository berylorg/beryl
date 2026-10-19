use super::*;

impl DomainMutation<SyndicDomain> for TerminalMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<Box<PreparedTerminalMutation>>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if let Some(settlement) =
            point::<DraftPieceSettlementsFamily>(reader, &settlement_key(&self.prepared))?
        {
            return if settlement_is_terminal_target(&settlement, self.kind)
                && settlement_matches(reader, &settlement, &self.prepared)?
            {
                Ok(None)
            } else {
                Err(SyndicMutationError::IdentityCollision)
            };
        }
        match prepared_build(reader, &self.prepared)? {
            Some(build) => prepare_existing(reader, &self.prepared, self.kind, build),
            None => prepare_first(reader, &self.prepared, self.kind),
        }
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        reservation.reserve_records::<DraftPieceSettlementsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            put_session_head(mutations, &prepared.session)?;
            mutations.put::<DraftPieceSettlementsCodec>(
                &prepared.settlement.key(),
                &prepared.settlement,
            )?;
            put_build_transition(mutations, &prepared.build, &prepared.receipt)?;
            if let Some(writer) = prepared.writer {
                contribute_draft_marker_writer_terminal_v1(*writer, mutations)?;
            }
        }
        Ok(())
    }
}

#[inline(never)]
fn prepare_first(
    reader: &DomainReader<'_, SyndicDomain>,
    prepared: &PreparedDraftPieceEditV1,
    kind: TerminalKind,
) -> Result<Option<Box<PreparedTerminalMutation>>, SyndicMutationError> {
    if prepared
        .prebuild_rejection()
        .is_some_and(|reason| !matches!(kind, TerminalKind::Rejected(actual) if actual == reason))
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let session = session_head(
        reader,
        prepared.header.draft_id(),
        prepared.header.session_id(),
    )?;
    let target_root = DraftPieceRootKeyV1::editor_candidate(
        prepared.header.draft_id(),
        prepared.header.session_id(),
        prepared.header.operation_id(),
    );
    if session != *prepared.source_session()
        || point::<DraftPieceRootsFamily>(reader, &target_root)?.is_some()
        || point::<DraftPieceBuildProgressFamily>(
            reader,
            &DraftPieceBuildProgressReceiptKeyV1::new(
                prepared.header.draft_id(),
                prepared.header.session_id(),
                prepared.header.operation_id(),
                1,
            ),
        )?
        .is_some()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let header = prepared.header;
    let (outcome, lifecycle) = match kind {
        TerminalKind::Cancelled => (
            DraftPieceSettlementOutcomeV1::Cancelled,
            DraftPieceBuildLifecycleV1::Cancelled,
        ),
        TerminalKind::Rejected(reason) => (
            DraftPieceSettlementOutcomeV1::Rejected(reason),
            DraftPieceBuildLifecycleV1::Rejected,
        ),
        TerminalKind::Error(reason) => (
            DraftPieceSettlementOutcomeV1::Error(reason),
            DraftPieceBuildLifecycleV1::Error,
        ),
    };
    let (build, receipt) = first_transition(prepared, lifecycle)?;
    let claimed = prepared
        .source_session()
        .with_active_operation(custody_for(&build))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let cleared = claimed
        .clear_active_operation(&custody_for(&build))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let settlement = DraftPieceSettlementV1::new_boxed(
        settlement_key(&prepared),
        prepared.proposal_digest(),
        header.predecessor_candidate_generation(),
        header.predecessor_root(),
        header.predecessor_history(),
        header.fragment_count(),
        header.fragment_chain(),
        header.predecessor_caret(),
        header.predecessor_selection(),
        header.caret(),
        header.selection(),
        None,
        prepared.canonical_header().to_vec(),
        None,
        Some(receipt.reference()),
        outcome,
        Box::new(DraftPieceSettlementClosureV1::Noncommit(
            DraftPieceNoncommitClosureV1::new(
                cleared.clone(),
                authenticated_history_frontier(reader, cleared.newest_history())?,
                None,
            ),
        )),
    );
    Ok(Some(Box::new(PreparedTerminalMutation {
        build,
        receipt,
        session: Box::new(cleared),
        settlement: Box::new(settlement),
        writer: None,
    })))
}

#[inline(never)]
fn prepare_existing(
    reader: &DomainReader<'_, SyndicDomain>,
    prepared: &PreparedDraftPieceEditV1,
    kind: TerminalKind,
    build: Box<DraftPieceBuildRecordV1>,
) -> Result<Option<Box<PreparedTerminalMutation>>, SyndicMutationError> {
    if prepared.prebuild_rejection().is_some() {
        return Err(SyndicMutationError::IdentityCollision);
    }

    if !build_matches(&build, &prepared)
        || !matches!(
            build.lifecycle(),
            DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
        )
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let current = session_head(reader, build.draft_id(), build.session_id())?;
    if current.active_operation() != Some(&custody_for(&build)) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let expected_session = expected_active_session(&prepared, &build)?;
    authenticate_source_transition(
        reader,
        &build,
        &expected_session,
        next_progress_key(&build)?,
    )?;
    let fragment_endpoint = source_fragment_endpoint(reader, &build)?;
    if let Some(successor) = build.successor()
        && point::<DraftPieceRootsFamily>(reader, &successor.key())?.is_some()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let (outcome, lifecycle) = match kind {
        TerminalKind::Cancelled => (
            DraftPieceSettlementOutcomeV1::Cancelled,
            DraftPieceBuildLifecycleV1::Cancelled,
        ),
        TerminalKind::Rejected(reason) => (
            DraftPieceSettlementOutcomeV1::Rejected(reason),
            DraftPieceBuildLifecycleV1::Rejected,
        ),
        TerminalKind::Error(reason) => (
            DraftPieceSettlementOutcomeV1::Error(reason),
            DraftPieceBuildLifecycleV1::Error,
        ),
    };
    let cleared = current
        .clear_active_operation(&custody_for(&build))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let (terminal, receipt) = existing_transition(&build, lifecycle, fragment_endpoint)?;
    let writer = writer_terminal(reader, &build, receipt.reference().digest())?;
    let settlement = DraftPieceSettlementV1::new_boxed(
        settlement_key(&prepared),
        build.proposal_digest(),
        build.predecessor_candidate_generation(),
        build.predecessor_root(),
        build.predecessor_history(),
        build.fragment_count(),
        build.fragment_chain(),
        build.predecessor_caret(),
        build.predecessor_selection(),
        build.caret(),
        build.selection(),
        build.build_digest(),
        build.canonical_header().to_vec(),
        Some(build.as_ref().clone()),
        Some(receipt.reference()),
        outcome,
        Box::new(DraftPieceSettlementClosureV1::Noncommit(
            DraftPieceNoncommitClosureV1::new(
                cleared.clone(),
                authenticated_history_frontier(reader, cleared.newest_history())?,
                build.successor(),
            ),
        )),
    );
    Ok(Some(Box::new(PreparedTerminalMutation {
        build: terminal,
        receipt,
        session: Box::new(cleared),
        settlement: Box::new(settlement),
        writer,
    })))
}

#[inline(never)]
fn prepared_build(
    reader: &DomainReader<'_, SyndicDomain>,
    prepared: &PreparedDraftPieceEditV1,
) -> Result<Option<Box<DraftPieceBuildRecordV1>>, SyndicMutationError> {
    point_build(reader, &settlement_key(prepared)).map(|value| value.map(Box::new))
}

#[inline(never)]
fn first_transition(
    prepared: &PreparedDraftPieceEditV1,
    lifecycle: DraftPieceBuildLifecycleV1,
) -> Result<
    (
        Box<DraftPieceBuildRecordV1>,
        Box<DraftPieceBuildProgressReceiptV1>,
    ),
    SyndicMutationError,
> {
    let (build, receipt) = terminal_first_build(prepared, lifecycle)?;
    Ok((Box::new(build), Box::new(receipt)))
}

#[inline(never)]
fn source_fragment_endpoint(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
) -> Result<Option<DraftPieceCanonicalFragmentEndpointV1>, SyndicMutationError> {
    let receipt =
        required::<DraftPieceBuildProgressFamily>(reader, &build.progress_receipt().key())?;
    Ok(receipt.fragment_endpoint())
}

#[inline(never)]
fn existing_transition(
    build: &DraftPieceBuildRecordV1,
    lifecycle: DraftPieceBuildLifecycleV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<
    (
        Box<DraftPieceBuildRecordV1>,
        Box<DraftPieceBuildProgressReceiptV1>,
    ),
    SyndicMutationError,
> {
    let (terminal, receipt) = terminal_build(build, lifecycle, fragment_endpoint)?;
    Ok((Box::new(terminal), Box::new(receipt)))
}

#[inline(never)]
fn writer_terminal(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
    terminal_digest: DraftPieceDigestV1,
) -> Result<Option<Box<PreparedDraftMarkerWriterTerminalV1>>, SyndicMutationError> {
    build
        .writer_admission()
        .map(|admission| {
            prepare_draft_marker_writer_terminal_v1(reader, admission, terminal_digest)
                .map(Box::new)
        })
        .transpose()
}
