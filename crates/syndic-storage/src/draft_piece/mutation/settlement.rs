use super::*;

pub(super) fn contribute(
    prepared: &PreparedDraftPieceEditV1,
    reader: &DomainReader<'_, SyndicDomain>,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    if point::<DraftPieceSettlementsFamily>(reader, &settlement_key(prepared))?.is_some() {
        return Ok(());
    }
    read_and_authenticate(prepared, reader, mutations)
}

fn read_and_authenticate(
    prepared: &PreparedDraftPieceEditV1,
    reader: &DomainReader<'_, SyndicDomain>,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    let build = required_build(reader, &settlement_key(prepared))?;
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

    if current.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
        && current.newest_candidate_generation() == build.predecessor_candidate_generation()
        && current.newest_root() == build.predecessor_root()
        && current.newest_history() == build.predecessor_history()
    {
        contribute_committed(
            prepared,
            reader,
            mutations,
            build,
            current,
            successor,
            fragment_endpoint,
        )
    } else {
        contribute_conflict(
            prepared,
            reader,
            mutations,
            build,
            current,
            fragment_endpoint,
        )
    }
}

fn contribute_committed(
    prepared: &PreparedDraftPieceEditV1,
    reader: &DomainReader<'_, SyndicDomain>,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    successor: DraftPieceRootReferenceV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(), SyndicMutationError> {
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
            mutations,
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
            mutations,
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
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(), SyndicMutationError> {
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
        mutations,
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
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    successor: DraftPieceRootReferenceV1,
    observed_history: DraftEditHistoryFrontierV1,
    transition: DraftEditHistoryTransitionV1,
    adopted_history: DraftEditHistoryFrontierV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(), SyndicMutationError> {
    let root = DraftPieceRootRecordV1::new(successor);
    let next = current
        .adopted(successor, adopted_history.reference())
        .ok_or(SyndicMutationError::IdentityCollision)?;
    mutations.put::<DraftPieceRootsCodec>(&successor.key(), &root)?;
    mutations.put::<DraftEditHistoryTransitionsCodec>(&transition.key(), &transition)?;
    mutations.put::<DraftEditHistoryFrontiersCodec>(
        &adopted_history.reference().key(),
        &adopted_history,
    )?;
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
            root,
            observed_history,
            transition,
            adopted_history,
        ),
    ));
    write_settlement(
        prepared,
        mutations,
        build,
        next,
        outcome,
        closure,
        DraftPieceBuildLifecycleV1::Committed,
        fragment_endpoint,
    )
}

fn write_noncommit(
    prepared: &PreparedDraftPieceEditV1,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
    current: DraftEditorCandidateSessionV1,
    observed_history: DraftEditHistoryFrontierV1,
    outcome: DraftPieceSettlementOutcomeV1,
    lifecycle: DraftPieceBuildLifecycleV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(), SyndicMutationError> {
    let cleared = current
        .clear_active_operation(&custody_for(&build))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let closure = Box::new(DraftPieceSettlementClosureV1::Noncommit(
        DraftPieceNoncommitClosureV1::new(cleared.clone(), observed_history, build.successor()),
    ));
    write_settlement(
        prepared,
        mutations,
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
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    build: DraftPieceBuildRecordV1,
    target_session: DraftEditorCandidateSessionV1,
    outcome: DraftPieceSettlementOutcomeV1,
    closure: Box<DraftPieceSettlementClosureV1>,
    lifecycle: DraftPieceBuildLifecycleV1,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
) -> Result<(), SyndicMutationError> {
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
    put_session_head(mutations, &target_session)?;
    mutations.put::<DraftPieceSettlementsCodec>(&key, &settlement)?;
    put_build_transition(mutations, &terminal, &receipt)?;
    Ok(())
}
