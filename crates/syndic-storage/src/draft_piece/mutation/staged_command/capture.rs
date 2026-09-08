use super::*;
mod preparation;

pub(super) struct CommandMutation {
    pub(super) command: CommandKind,
    pub(super) source: Box<CapturedState>,
    pub(super) generation: HomeGeneration,
    pub(super) writer_progress_allowed: bool,
    pub(super) capture: CaptureSlot,
}

pub(super) enum PreparedCommandMutation {
    Transfer(Box<<TransferMutation as DomainMutation<SyndicDomain>>::Prepared>),
    Window(Box<<StageDurableWindowMutation as DomainMutation<SyndicDomain>>::Prepared>),
    Advance(Box<<AdvanceMutation as DomainMutation<SyndicDomain>>::Prepared>),
    Settle(Box<<SettleMutation as DomainMutation<SyndicDomain>>::Prepared>),
    Terminal(Box<<TerminalMutation as DomainMutation<SyndicDomain>>::Prepared>),
    Replay(Box<DraftPieceSettlementV1>),
}

impl DomainMutation<SyndicDomain> for CommandMutation {
    type Error = SyndicMutationError;
    type Prepared = PreparedCommandMutation;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.writer_progress_allowed {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let mut source = self.source.clone();
        if !matches!(self.command, CommandKind::Transfer(_))
            && !matches!(&self.command, CommandKind::Advance(advance) if advance.bounded.is_some())
        {
            let staging =
                required::<DraftMutationStagingHeadsFamily>(reader, &source.staging.identity())?;
            if staging != source.staging {
                return Err(SyndicMutationError::IdentityCollision);
            }
            crate::draft_piece::staging::authenticate_staging_head_reader(reader, &staging)?;
            let expected = source
                .build
                .as_ref()
                .ok_or(SyndicMutationError::IdentityCollision)?;
            let current = required_build(reader, &build_key(expected))?;
            if current == *expected {
                source.session = session_head(reader, current.draft_id(), current.session_id())?;
            } else if matches!(self.command, CommandKind::Terminal(..)) {
                let settled =
                    required::<DraftPieceSettlementsFamily>(reader, &build_key(expected))?;
                if settled.terminal_source() != Some(expected) {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
        }
        let (prepared, target) = match self.command {
            CommandKind::Transfer(value) => preparation::transfer(reader, &source, value)?,
            CommandKind::Window(value) => preparation::window(reader, &source, value)?,
            CommandKind::Advance(value) => preparation::advance(reader, &source, value)?,
            CommandKind::Terminal(value, StagedDraftPieceTerminalElectionV1::Settle) => {
                preparation::settle(reader, &source, value, self.generation)?
            }
            CommandKind::Terminal(value, election) => {
                preparation::terminal(reader, &source, value, election)?
            }
        };
        if let Some(settlement) = &target.settlement {
            if settlement.terminal_source() != source.build.as_ref() {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        let replayed = matches!(
            &prepared,
            PreparedCommandMutation::Settle(value) if value.is_none()
        ) || matches!(
            &prepared,
            PreparedCommandMutation::Terminal(value) if value.is_none()
        );
        let prepared = if replayed {
            source = target.clone();
            PreparedCommandMutation::Replay(Box::new(
                target
                    .settlement
                    .clone()
                    .ok_or(SyndicMutationError::IdentityCollision)?,
            ))
        } else {
            prepared
        };
        let mut capture = self
            .capture
            .lock()
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
        if capture.is_some() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        *capture = Some(Box::new(CapturedCommand {
            source,
            target,
            replayed,
        }));
        Ok(prepared)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match &self.command {
            CommandKind::Transfer(value) => TransferMutation {
                prepared: value.as_ref().clone(),
                writer_progress_allowed: self.writer_progress_allowed,
            }
            .reserve_reconciliation(reservation),
            CommandKind::Window(value) => StageDurableWindowMutation {
                prepared: value.as_ref().clone(),
                writer_progress_allowed: self.writer_progress_allowed,
            }
            .reserve_reconciliation(reservation),
            CommandKind::Advance(value) => AdvanceMutation {
                prepared: value.as_ref().clone(),
                writer_progress_allowed: self.writer_progress_allowed,
            }
            .reserve_reconciliation(reservation),
            CommandKind::Terminal(value, StagedDraftPieceTerminalElectionV1::Settle) => {
                SettleMutation {
                    prepared: value.as_ref().clone(),
                    home_generation: self.generation,
                    reconstructed_cleanup_admissions: Box::default(),
                }
                .reserve_reconciliation(reservation)
            }
            CommandKind::Terminal(value, election) => TerminalMutation {
                prepared: value.as_ref().clone(),
                kind: terminal_kind(*election)?,
            }
            .reserve_reconciliation(reservation),
        }
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match prepared {
            PreparedCommandMutation::Transfer(value) => {
                TransferMutation::contribute(*value, mutations)
            }
            PreparedCommandMutation::Window(value) => {
                StageDurableWindowMutation::contribute(*value, mutations)
            }
            PreparedCommandMutation::Advance(value) => {
                AdvanceMutation::contribute(*value, mutations)
            }
            PreparedCommandMutation::Settle(value) => SettleMutation::contribute(*value, mutations),
            PreparedCommandMutation::Terminal(value) => {
                TerminalMutation::contribute(*value, mutations)
            }
            PreparedCommandMutation::Replay(settlement) => mutations
                .put::<DraftPieceSettlementsCodec>(&settlement.key(), &settlement)
                .map_err(Into::into),
        }
    }
}

fn terminal_kind(
    election: StagedDraftPieceTerminalElectionV1,
) -> Result<TerminalKind, SyndicMutationError> {
    match election {
        StagedDraftPieceTerminalElectionV1::Cancel => Ok(TerminalKind::Cancelled),
        StagedDraftPieceTerminalElectionV1::Reject(reason) => Ok(TerminalKind::Rejected(reason)),
        StagedDraftPieceTerminalElectionV1::Error(reason) => Ok(TerminalKind::Error(reason)),
        StagedDraftPieceTerminalElectionV1::Settle => Err(SyndicMutationError::IdentityCollision),
    }
}

fn replay_target(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CapturedState,
) -> Result<CapturedState, SyndicMutationError> {
    let identity = source.staging.identity();
    let key = DraftPieceSettlementKeyV1::new(
        identity.draft_id(),
        identity.session_id(),
        identity.operation_id().as_piece_operation(),
    );
    let settlement = point::<DraftPieceSettlementsFamily>(reader, &key)?;
    let terminal_admission = match settlement.as_ref() {
        Some(settlement)
            if !matches!(
                settlement.outcome(),
                DraftPieceSettlementOutcomeV1::Committed { .. }
            ) =>
        {
            match settlement
                .terminal_source()
                .and_then(DraftPieceBuildRecordV1::writer_admission)
            {
                Some(admission) => {
                    let owner = admission.binding().owner();
                    let command =
                        staging_terminal_command(owner, settlement.terminal_receipt().digest());
                    Some((
                        required::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?,
                        required::<DraftMarkerAdmissionReceiptsFamily>(
                            reader,
                            &DraftMarkerAdmissionReceiptKeyV1::new(owner, command),
                        )?,
                    ))
                }
                None => None,
            }
        }
        _ => None,
    };
    Ok(CapturedState {
        staging: required::<DraftMutationStagingHeadsFamily>(reader, &identity)?,
        build: Some(required_build(reader, &key)?),
        session: session_head(reader, identity.draft_id(), identity.session_id())?,
        settlement,
        terminal_admission,
    })
}
