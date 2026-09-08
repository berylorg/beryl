use super::*;

type CapturedPreparation = Result<(PreparedCommandMutation, Box<CapturedState>), SyndicMutationError>;

pub(super) fn transfer(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CapturedState,
    value: Box<PreparedDraftMutationTransferV1>,
) -> CapturedPreparation {
    let prepared = TransferMutation {
        prepared: *value,
        writer_progress_allowed: true,
    }
    .prepare(reader)?;
    let target = match &prepared {
        Some((value, _)) => CapturedState {
            staging: value.target_head.clone(),
            build: Some(value.build.clone()),
            session: value.target_session.clone(),
            settlement: None,
            terminal_admission: None,
        },
        None => replay_target(reader, source)?,
    };
    Ok((
        PreparedCommandMutation::Transfer(Box::new(prepared)),
        Box::new(target),
    ))
}

pub(super) fn window(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CapturedState,
    value: Box<PreparedDraftPieceStagingWindowV1>,
) -> CapturedPreparation {
    let prepared = StageDurableWindowMutation {
        prepared: *value,
        writer_progress_allowed: true,
    }
    .prepare(reader)?;
    let target = match &prepared {
        Some(value) => CapturedState {
            staging: source.staging.clone(),
            build: Some(value.target_build.clone()),
            session: value.target_session.clone(),
            settlement: None,
            terminal_admission: None,
        },
        None => replay_target(reader, source)?,
    };
    Ok((PreparedCommandMutation::Window(Box::new(prepared)), Box::new(target)))
}

pub(super) fn advance(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CapturedState,
    value: Box<PreparedDraftPieceAdvanceV1>,
) -> CapturedPreparation {
    let prepared = AdvanceMutation {
        prepared: *value,
        writer_progress_allowed: true,
    }
    .prepare(reader)?;
    let target = match &prepared {
        Some((value, _)) => CapturedState {
            staging: source.staging.clone(),
            build: Some(value.next.clone()),
            session: value.next_session.clone(),
            settlement: None,
            terminal_admission: None,
        },
        None => replay_target(reader, source)?,
    };
    Ok((PreparedCommandMutation::Advance(Box::new(prepared)), Box::new(target)))
}

pub(super) fn settle(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CapturedState,
    value: Box<PreparedDraftPieceEditV1>,
    generation: HomeGeneration,
) -> CapturedPreparation {

    let prepared = SettleMutation {
        prepared: *value,
        home_generation: generation,
        reconstructed_cleanup_admissions: Box::default(),
    }
    .prepare(reader)?;
    let target = match &prepared {
        Some(value) => CapturedState {
            staging: source.staging.clone(),
            build: Some(value.terminal.clone()),
            session: value.target_session.clone(),
            settlement: Some(value.settlement.clone()),
            terminal_admission: value.terminal_writer_evidence(),
        },
        None => replay_target(reader, source)?,
    };
    Ok((PreparedCommandMutation::Settle(Box::new(prepared)), Box::new(target)))
}

pub(super) fn terminal(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CapturedState,
    value: Box<PreparedDraftPieceEditV1>,
    election: StagedDraftPieceTerminalElectionV1,
) -> CapturedPreparation {

    let prepared = TerminalMutation {
        prepared: *value,
        kind: terminal_kind(election)?,
    }
    .prepare(reader)?;
    let target = match &prepared {
        Some(value) => CapturedState {
            staging: source.staging.clone(),
            build: Some(value.build.clone()),
            session: value.session.clone(),
            settlement: Some(value.settlement.clone()),
            terminal_admission: value
                .writer
                .as_ref()
                .map(|writer| writer.outcome_evidence()),
        },
        None => replay_target(reader, source)?,
    };
    Ok((
        PreparedCommandMutation::Terminal(Box::new(prepared)),
        Box::new(target),
    ))
}
