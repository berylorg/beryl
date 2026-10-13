use super::*;
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, ReconciliationResolution,
};
use beryl_state::AssetState;

impl SyndicComposerHost {
    #[cfg(feature = "test-faults")]
    pub fn test_arm_historical_after_execute_fault(
        &mut self,
        hook: impl FnOnce(&HomeStore, &CommandOutcome) + Send + 'static,
    ) {
        assert!(self.historical_after_execute_fault.is_none());
        self.historical_after_execute_fault = Some(Box::new(hook));
    }

    fn execute_historical_command(
        &mut self,
        store: &HomeStore,
        command: HomeCommand,
    ) -> CommandOutcome {
        let outcome = store.execute(command);
        #[cfg(feature = "test-faults")]
        if let Some(hook) = self.historical_after_execute_fault.take() {
            hook(store, &outcome);
        }
        outcome
    }
    pub fn advance_historical_replacement(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostHistoricalTicket,
        assets: &AssetState,
        at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostHistoricalAdvance, ComposerHostError> {
        if self
            .historical_replacement
            .as_ref()
            .is_none_or(|pending| pending.ticket != ticket)
        {
            return Err(ComposerHostError::OldBinding);
        }
        if store.home_id() != ticket.binding.home_id() {
            return Err(ComposerHostError::OldBinding);
        }
        let mut pending = self.historical_replacement.take().unwrap();
        pending.cancelled |= cancellation.is_cancelled();
        let assets = pending.assets.get_or_insert_with(|| assets.clone()).clone();
        let at = *pending.at.get_or_insert(at);
        let result = self.drive_historical_replacement(store, &mut pending, &assets, at);
        if !matches!(result, Ok(ComposerHostHistoricalAdvance::NotCommitted)) {
            self.historical_replacement = Some(pending);
        }
        result
    }

    pub(in crate::composer_host) fn drain_historical_replacement_for_disposal(
        &mut self,
        store: &HomeStore,
    ) -> Result<bool, ComposerHostError> {
        let Some(pending) = self.historical_replacement.as_mut() else {
            return Ok(true);
        };
        pending.cancelled = true;
        if let HistoricalStage::Ready(completion) = pending.stage {
            let ticket = pending.ticket;
            self.acknowledge_historical_replacement(ticket, completion.binding())?;
            return Ok(true);
        }
        let Some(assets) = pending.assets.clone() else {
            if matches!(
                pending.stage,
                HistoricalStage::Begin
                    | HistoricalStage::Cancel {
                        executed: false,
                        ..
                    }
            ) {
                self.historical_replacement = None;
                return Ok(true);
            }
            return Err(DraftReplacementErrorV1::Unavailable.into());
        };
        let ticket = pending.ticket;
        let at = pending.at.ok_or(DraftReplacementErrorV1::Unavailable)?;
        let cancellation = CommandCancellation::new();
        cancellation.cancel();
        match self.advance_historical_replacement(store, ticket, &assets, at, &cancellation)? {
            ComposerHostHistoricalAdvance::Progress => Ok(false),
            ComposerHostHistoricalAdvance::Ready(completion) => {
                self.acknowledge_historical_replacement(ticket, completion.binding())?;
                Ok(true)
            }
            ComposerHostHistoricalAdvance::NotCommitted => Ok(true),
        }
    }

    fn drive_historical_replacement(
        &mut self,
        store: &HomeStore,
        pending: &mut PendingHistoricalReplacement,
        assets: &AssetState,
        at: SyndicTimestamp,
    ) -> Result<ComposerHostHistoricalAdvance, ComposerHostError> {
        match &mut pending.stage {
            HistoricalStage::Ready(completion) => {
                return Ok(ComposerHostHistoricalAdvance::Ready(*completion));
            }
            HistoricalStage::NotCommitted => {
                return Ok(ComposerHostHistoricalAdvance::NotCommitted);
            }
            HistoricalStage::Quantum { begin, outcome } => {
                let committed = settle_quantum(store, outcome)?;
                if committed {
                    pending.stage = HistoricalStage::Streaming;
                } else if *begin {
                    pending.stage = HistoricalStage::NotCommitted;
                } else {
                    self.prepare_historical_termination(store, pending)?;
                }
            }
            HistoricalStage::Begin => {
                if pending.cancelled {
                    pending.stage = HistoricalStage::NotCommitted;
                } else {
                    super::super::request::validate_store(pending.ticket.binding, store)?;
                    let mut command = HomeCommand::new(store.home_revision()?);
                    command.add(self.storage.begin_replacement_draft_import(
                        self.storage.revision(store)?,
                        pending.import.as_ref().unwrap().clone(),
                    ))?;
                    pending.stage = HistoricalStage::Quantum {
                        begin: true,
                        outcome: retain_quantum(self.execute_historical_command(store, command)),
                    };
                }
            }
            HistoricalStage::Streaming => {
                self.stream_historical_replacement(store, pending, assets, at)?;
            }
            HistoricalStage::Start { prepared, executed } => {
                if !*executed && pending.cancelled {
                    self.prepare_historical_termination(store, pending)?;
                } else {
                    let outcome = if *executed {
                        self.storage
                            .resume_replacement_draft_start(store, prepared)?
                    } else {
                        super::super::request::validate_store(pending.ticket.binding, store)?;
                        let mut command = HomeCommand::new(store.home_revision()?);
                        command.add(self.storage.start_replacement_draft(
                            self.storage.revision(store)?,
                            prepared.clone(),
                        ))?;
                        command.add(assets.historical_input_owner_selection(
                            store,
                            prepared.basis().selector().draft_id(),
                            prepared.basis().item_id(),
                            prepared.basis().asset_reference_set(),
                        )?)?;
                        *executed = true;
                        let outcome = self.execute_historical_command(store, command);
                        self.storage
                            .reconcile_replacement_draft_start(store, prepared, outcome)?
                    };
                    match outcome {
                        DraftReplacementCommandOutcomeV1::Started(selector) => {
                            let completion = self.adopt_historical_start(
                                store,
                                pending.ticket.binding,
                                prepared,
                                selector,
                                pending.position,
                            )?;
                            pending.stage = HistoricalStage::Ready(completion);
                        }
                        DraftReplacementCommandOutcomeV1::NotCommitted => {
                            self.prepare_historical_termination(store, pending)?
                        }
                        _ => return Err(DraftReplacementErrorV1::Conflict.into()),
                    }
                }
            }
            HistoricalStage::Cancel { prepared, executed } => {
                let outcome = if *executed {
                    self.storage
                        .resume_replacement_intent_cancellation(store, prepared)?
                } else {
                    if pending.cancelled {
                        pending.stage = HistoricalStage::NotCommitted;
                        return Ok(ComposerHostHistoricalAdvance::Progress);
                    }
                    super::super::request::validate_store(pending.ticket.binding, store)?;
                    let mut command = HomeCommand::new(store.home_revision()?);
                    command.add(self.storage.cancel_replacement_intent(
                        self.storage.revision(store)?,
                        prepared.clone(),
                    ))?;
                    *executed = true;
                    let outcome = self.execute_historical_command(store, command);
                    self.storage
                        .reconcile_replacement_intent_cancellation(store, prepared, outcome)?
                };
                match outcome {
                    DraftReplacementCommandOutcomeV1::IntentCancelled(selector) => {
                        let completion = self.adopt_historical_cancellation(
                            pending.ticket.binding,
                            prepared,
                            selector,
                        )?;
                        pending.stage = HistoricalStage::Ready(completion);
                    }
                    DraftReplacementCommandOutcomeV1::NotCommitted => {
                        pending.stage = HistoricalStage::NotCommitted
                    }
                    _ => return Err(DraftReplacementErrorV1::Conflict.into()),
                }
            }
            HistoricalStage::Terminate { prepared, executed } => {
                let outcome = if *executed {
                    self.storage
                        .resume_replacement_import_termination(store, prepared)?
                } else {
                    super::super::request::validate_store(pending.ticket.binding, store)?;
                    let mut command = HomeCommand::new(store.home_revision()?);
                    command.add(self.storage.terminate_replacement_draft_import(
                        self.storage.revision(store)?,
                        prepared.clone(),
                    ))?;
                    *executed = true;
                    let outcome = self.execute_historical_command(store, command);
                    self.storage
                        .reconcile_replacement_import_termination(store, prepared, outcome)?
                };
                match outcome {
                    DraftReplacementCommandOutcomeV1::ImportTerminated => {
                        let completion =
                            self.adopt_historical_termination(pending.ticket.binding, prepared)?;
                        pending.stage = HistoricalStage::Ready(completion);
                    }
                    // Proven noncommit retains the import; termination must remain explicit.
                    DraftReplacementCommandOutcomeV1::NotCommitted => {
                        return Err(DraftReplacementErrorV1::Unavailable.into());
                    }
                    _ => return Err(DraftReplacementErrorV1::Conflict.into()),
                }
            }
        }
        match pending.stage {
            HistoricalStage::Ready(completion) => {
                Ok(ComposerHostHistoricalAdvance::Ready(completion))
            }
            _ => Ok(ComposerHostHistoricalAdvance::Progress),
        }
    }

    #[inline(never)]
    fn stream_historical_replacement(
        &mut self,
        store: &HomeStore,
        pending: &mut PendingHistoricalReplacement,
        assets: &AssetState,
        at: SyndicTimestamp,
    ) -> Result<(), ComposerHostError> {
        super::super::request::validate_store(pending.ticket.binding, store)?;
        if pending.cancelled {
            return self.prepare_historical_termination(store, pending);
        }
        let import = pending.import.as_ref().unwrap();
        let step = match self
            .storage
            .prepare_replacement_import_step(store, import)?
        {
            DraftReplacementAdvancePreparationV1::Ready(step) => step,
            DraftReplacementAdvancePreparationV1::Image(image) => {
                self.prepare_historical_image_step(store, assets, import, image)?
            }
            DraftReplacementAdvancePreparationV1::Complete(_) => {
                let prepared = self
                    .storage
                    .prepare_replacement_draft_start(store, import, at)?;
                pending.stage = HistoricalStage::Start {
                    prepared,
                    executed: false,
                };
                return Ok(());
            }
        };
        let mut command = HomeCommand::new(store.home_revision()?);
        command.add(
            self.storage
                .advance_replacement_draft_import(self.storage.revision(store)?, step),
        )?;
        pending.stage = HistoricalStage::Quantum {
            begin: false,
            outcome: retain_quantum(self.execute_historical_command(store, command)),
        };
        Ok(())
    }

    #[inline(never)]
    fn prepare_historical_image_step(
        &self,
        store: &HomeStore,
        assets: &AssetState,
        import: &PreparedDraftReplacementImportV1,
        image: syndic_storage::DraftReplacementImageSourceV1,
    ) -> Result<syndic_storage::PreparedDraftReplacementAdvanceV1, ComposerHostError> {
        let proof = import
            .basis()
            .asset_reference_set()
            .ok_or(DraftReplacementErrorV1::Proof)?;
        let entry = assets
            .marker_reference(store, proof, image.marker_id())?
            .ok_or(DraftReplacementErrorV1::Proof)?;
        let asset = entry.asset_id();
        let page = image
            .descriptive_page(asset)
            .ok_or(DraftReplacementErrorV1::Proof)?;
        let witness = assets.historical_input_marker_witness(store, page)?;
        let mut attempt = self
            .storage
            .prepare_replacement_image_proof(store, image, asset, witness)?;
        let receipt = store
            .compose_proof(
                attempt
                    .take_command()
                    .ok_or(DraftReplacementErrorV1::Proof)?,
            )
            .map_err(|_| DraftReplacementErrorV1::Proof)?;
        Ok(attempt.consume(&self.storage, store, receipt)?)
    }

    fn prepare_historical_termination(
        &self,
        store: &HomeStore,
        pending: &mut PendingHistoricalReplacement,
    ) -> Result<(), ComposerHostError> {
        let import = pending
            .import
            .as_ref()
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let prepared = self.storage.prepare_replacement_import_termination(
            store,
            DraftPieceSettlementKeyV1::new(
                import.basis().selector().draft_id(),
                import.session_id(),
                import.operation_id(),
            ),
        )?;
        pending.stage = HistoricalStage::Terminate {
            prepared,
            executed: false,
        };
        Ok(())
    }
}

fn retain_quantum(outcome: CommandOutcome) -> QuantumOutcome {
    match outcome {
        CommandOutcome::Committed { .. } => QuantumOutcome::Committed,
        CommandOutcome::NotCommitted { .. } => QuantumOutcome::Noncommit,
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            QuantumOutcome::Pending(reconciliation.install_and_handle())
        }
    }
}

pub(super) fn settle_quantum(
    store: &HomeStore,
    outcome: &mut QuantumOutcome,
) -> Result<bool, ComposerHostError> {
    if let QuantumOutcome::Pending(handle) = outcome {
        *outcome = match store.retry_reconciliation(handle)? {
            ReconciliationResolution::ExactNew { .. } => QuantumOutcome::Committed,
            ReconciliationResolution::ExactOld => QuantumOutcome::Noncommit,
            ReconciliationResolution::ExactSuccessor { .. }
            | ReconciliationResolution::Collision => {
                return Err(DraftReplacementErrorV1::Unavailable.into());
            }
        };
    }
    match outcome {
        QuantumOutcome::Committed => Ok(true),
        QuantumOutcome::Noncommit => Ok(false),
        _ => Err(DraftReplacementErrorV1::Unavailable.into()),
    }
}
