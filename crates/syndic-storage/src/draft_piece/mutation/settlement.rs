use super::*;

pub(super) struct PreparedSettlementContribution {
    root: Option<DraftPieceRootRecordV1>,
    transition: Option<DraftEditHistoryTransitionV1>,
    history: Option<DraftEditHistoryFrontierV1>,
    target_session: DraftEditorCandidateSessionV1,
    settlement: DraftPieceSettlementV1,
    terminal: DraftPieceBuildRecordV1,
    receipt: DraftPieceBuildProgressReceiptV1,
    writer: Option<PreparedWriterClosure>,
}

enum PreparedWriterClosure {
    Settled(PreparedDraftMarkerWriterSettlementV1),
    Terminal(PreparedDraftMarkerWriterTerminalV1),
}

pub(super) fn prepare(
    prepared: &PreparedDraftPieceEditV1,
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<Option<PreparedSettlementContribution>, SyndicMutationError> {
    if let Some(settlement) =
        point::<DraftPieceSettlementsFamily>(reader, &settlement_key(prepared))?
    {
        return if settlement_is_settle_target(&settlement)
            && settlement_matches(reader, &settlement, prepared)?
        {
            Ok(None)
        } else {
            Err(SyndicMutationError::IdentityCollision)
        };
    }
    let build = required_build(reader, &settlement_key(prepared))?;
    if !build_matches(&build, prepared)
        || build.lifecycle() != DraftPieceBuildLifecycleV1::Complete
        || build.successor().is_none()
        || build.build_digest().is_none()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let expected_session = expected_active_session(prepared, &build)?;
    authenticate_source_transition(
        reader,
        &build,
        &expected_session,
        next_progress_key(&build)?,
    )?;
    let successor = build
        .successor()
        .ok_or(SyndicMutationError::IdentityCollision)?;
    if point::<DraftPieceRootsFamily>(reader, &successor.key())?.is_some() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    read_and_authenticate(prepared, reader, build).map(Some)
}

pub(super) fn contribute(
    prepared: PreparedSettlementContribution,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    if let Some(root) = prepared.root {
        mutations.put::<DraftPieceRootsCodec>(&root.reference().key(), &root)?;
    }
    if let Some(transition) = prepared.transition {
        mutations.put::<DraftEditHistoryTransitionsCodec>(&transition.key(), &transition)?;
    }
    if let Some(history) = prepared.history {
        mutations.put::<DraftEditHistoryFrontiersCodec>(&history.reference().key(), &history)?;
    }
    put_session_head(mutations, &prepared.target_session)?;
    mutations
        .put::<DraftPieceSettlementsCodec>(&prepared.settlement.key(), &prepared.settlement)?;
    put_build_transition(mutations, &prepared.terminal, &prepared.receipt)?;
    if let Some(writer) = prepared.writer {
        match writer {
            PreparedWriterClosure::Settled(writer) => {
                contribute_draft_marker_writer_settlement_v1(writer, mutations)?
            }
            PreparedWriterClosure::Terminal(writer) => {
                contribute_draft_marker_writer_terminal_v1(writer, mutations)?
            }
        }
    }
    Ok(())
}

fn read_and_authenticate(
    prepared: &PreparedDraftPieceEditV1,
    reader: &DomainReader<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
) -> Result<PreparedSettlementContribution, SyndicMutationError> {
    let current = session_head(reader, build.draft_id(), build.session_id())?;
    if current.active_operation() != Some(&custody_for(&build)) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let fragment_endpoint =
        required::<DraftPieceBuildProgressFamily>(reader, &build.progress_receipt().key())?
            .fragment_endpoint();
    let successor = build
        .successor()
        .ok_or(SyndicMutationError::IdentityCollision)?;

    let writer_admission = build.writer_admission();
    let mut contribution = if current.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && current.newest_candidate_generation() == build.predecessor_candidate_generation()
        && current.newest_root() == build.predecessor_root()
        && current.newest_history() == build.predecessor_history()
    {
        contribute_committed(
            prepared,
            reader,
            build,
            current,
            successor,
            fragment_endpoint,
        )?
    } else {
        contribute_conflict(prepared, reader, build, current, fragment_endpoint)?
    };
    contribution.writer = writer_admission
        .map(|admission| {
            if matches!(
                contribution.settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Committed { .. }
            ) {
                prepare_draft_marker_writer_settlement_v1(reader, admission, true)
                    .map(PreparedWriterClosure::Settled)
            } else {
                prepare_draft_marker_writer_terminal_v1(
                    reader,
                    admission,
                    contribution.settlement.terminal_receipt().digest(),
                )
                .map(PreparedWriterClosure::Terminal)
            }
        })
        .transpose()?;
    Ok(contribution)
}

fn contribute_committed(
    prepared: &PreparedDraftPieceEditV1,
    reader: &DomainReader<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    successor: DraftPieceRootReferenceV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<PreparedSettlementContribution, SyndicMutationError> {
    let observed_history = authenticated_history_frontier(reader, current.newest_history())?;
    let append_history = if matches!(
        observed_history.reference().key(),
        DraftEditHistoryFrontierKeyV1::Publication { .. }
    ) {
        observed_history
            .fork_session(current.session_id())
            .ok_or(SyndicMutationError::IdentityCollision)?
    } else {
        observed_history.clone()
    };
    match append_ordinary_draft_edit_history_with_retention_v1(
        reader,
        &append_history,
        current
            .newest_candidate_generation()
            .checked_add(1)
            .ok_or(SyndicMutationError::IdentityCollision)?,
        successor,
        build.predecessor_caret(),
        build.predecessor_selection(),
        build.caret(),
        build.selection(),
        build.operation_id(),
    ) {
        Ok((transition, adopted_history)) => write_committed(
            prepared,
            build,
            current,
            successor,
            observed_history,
            transition,
            adopted_history,
            fragment_endpoint,
        ),
        Err(DraftEditHistoryRetentionErrorV1::CapacityUnavailable) => write_noncommit(
            prepared,
            build,
            current,
            observed_history,
            DraftPieceSettlementOutcomeV1::Error(
                DraftPieceErrorReasonV1::HistoryCapacityUnavailable,
            ),
            DraftPieceBuildLifecycleV1::Error,
            fragment_endpoint,
        ),
        Err(DraftEditHistoryRetentionErrorV1::Invalid) => {
            Err(SyndicMutationError::IdentityCollision)
        }
    }
}

fn contribute_conflict(
    prepared: &PreparedDraftPieceEditV1,
    reader: &DomainReader<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<PreparedSettlementContribution, SyndicMutationError> {
    let observed_history = authenticated_history_frontier(reader, current.newest_history())?;
    if matches!(
        observed_history.reference().key(),
        DraftEditHistoryFrontierKeyV1::Publication { .. }
    ) {
        observed_history
            .fork_session(current.session_id())
            .ok_or(SyndicMutationError::IdentityCollision)?;
    }
    let outcome = DraftPieceSettlementOutcomeV1::Conflict {
        current_candidate_generation: current.newest_candidate_generation(),
        current_root: current.newest_root(),
        current_history: current.newest_history(),
    };
    write_noncommit(
        prepared,
        build,
        current,
        observed_history,
        outcome,
        DraftPieceBuildLifecycleV1::Conflict,
        fragment_endpoint,
    )
}

fn write_committed(
    prepared: &PreparedDraftPieceEditV1,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    successor: DraftPieceRootReferenceV1,
    observed_history: DraftEditHistoryFrontierV1,
    transition: DraftEditHistoryTransitionV1,
    adopted_history: DraftEditHistoryFrontierV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<PreparedSettlementContribution, SyndicMutationError> {
    let root = DraftPieceRootRecordV1::new(successor);
    let next = current
        .adopted(successor, adopted_history.reference())
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let outcome = DraftPieceSettlementOutcomeV1::Committed {
        candidate_generation: next.newest_candidate_generation(),
        successor,
        history: adopted_history.reference(),
        caret: build.caret(),
        selection: build.selection(),
    };
    let closure = Box::new(DraftPieceSettlementClosureV1::Committed(
        DraftPieceCommittedAdoptionV1::new(
            current,
            next.clone(),
            root.clone(),
            observed_history,
            transition.clone(),
            adopted_history.clone(),
        ),
    ));
    let mut contribution = write_settlement(
        prepared,
        build,
        next,
        outcome,
        closure,
        DraftPieceBuildLifecycleV1::Committed,
        fragment_endpoint,
    )?;
    contribution.root = Some(root);
    contribution.transition = Some(transition);
    contribution.history = Some(adopted_history);
    Ok(contribution)
}

fn write_noncommit(
    prepared: &PreparedDraftPieceEditV1,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    observed_history: DraftEditHistoryFrontierV1,
    outcome: DraftPieceSettlementOutcomeV1,
    lifecycle: DraftPieceBuildLifecycleV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<PreparedSettlementContribution, SyndicMutationError> {
    let cleared = current
        .clear_active_operation(&custody_for(&build))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let closure = Box::new(DraftPieceSettlementClosureV1::Noncommit(
        DraftPieceNoncommitClosureV1::new(cleared.clone(), observed_history, build.successor()),
    ));
    write_settlement(
        prepared,
        build,
        cleared,
        outcome,
        closure,
        lifecycle,
        fragment_endpoint,
    )
}

fn write_settlement(
    prepared: &PreparedDraftPieceEditV1,
    build: DraftPieceBuildRecordV1,
    target_session: DraftEditorCandidateSessionV1,
    outcome: DraftPieceSettlementOutcomeV1,
    closure: Box<DraftPieceSettlementClosureV1>,
    lifecycle: DraftPieceBuildLifecycleV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<PreparedSettlementContribution, SyndicMutationError> {
    let key = settlement_key(prepared);
    let (terminal, receipt) = terminal_build(&build, lifecycle, fragment_endpoint)?;
    let settlement = DraftPieceSettlementV1::new_boxed(
        key,
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
        Some(build),
        receipt.reference(),
        outcome,
        closure,
    );
    Ok(PreparedSettlementContribution {
        root: None,
        transition: None,
        history: None,
        target_session,
        settlement,
        terminal,
        receipt,
        writer: None,
    })
}
