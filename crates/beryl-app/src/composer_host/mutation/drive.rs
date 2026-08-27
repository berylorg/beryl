use syndic_storage::{
    DraftMutationStagingPageInputV1, DraftMutationStagingReconcileV1, DraftMutationStagingStatusV1,
    DraftPieceDurableBuildWindowLimitsV1, DraftPieceOperationStatusV1,
    DraftPieceOperationVerificationV1, DraftPiecePrepareErrorV1,
    PreparedDraftMutationStagingBatchV1,
};

use super::execution::active_session;
use super::settlement::command_selection;
use super::translation::TranslatedWidgetPage;
use super::*;

impl SyndicComposerHost {
    pub(super) fn execute_active_mutation(
        &mut self,
        store: &HomeStore,
        request: MutationCommitRequest,
        cancellation: &CommandCancellation,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        self.validate_mutation_store(pending.binding, store)?;
        if request.key() != pending.begin.proposal().key() {
            return Err(ComposerHostError::MutationMalformed);
        }
        if cancellation.is_cancelled()
            && !matches!(pending.phase, ComposerHostMutationPhase::Building { .. })
        {
            self.release_unadmitted_in_flight_page(store, pending)?;
            return self.cancel_staging_mutation(store, pending);
        }
        if matches!(pending.phase, ComposerHostMutationPhase::Finished) {
            self.transfer_finished_mutation(store, pending)?;
        }
        self.drive_build_mutation(store, cancellation, pending)
    }

    fn transfer_finished_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<(), ComposerHostError> {
        let transfer = self
            .storage
            .prepare_draft_mutation_staging_transfer(&pending.head, &pending.session)?;
        let prepared_edit = transfer.prepared_edit().clone();
        let endpoint = transfer.build().progress_receipt();
        let mut command = HomeCommand::new(store.home_revision()?);
        command.add(
            self.storage.transfer_draft_mutation_staging_to_builder(
                self.storage.revision(store)?,
                transfer,
            ),
        )?;
        let outcome = self.execute_mutation_command(store, command);
        match command_selection(store, outcome)? {
            StagingCommandResult::Source => return Err(ComposerHostError::MutationWorkPending),
            StagingCommandResult::Target => {}
            StagingCommandResult::Terminal => {
                return Err(ComposerHostError::MutationMalformed);
            }
        }
        if !matches!(
            self.storage
                .draft_mutation_staging_status(store, pending.identity)?,
            DraftMutationStagingStatusV1::Building { .. }
        ) {
            return Err(ComposerHostError::MutationMalformed);
        }
        pending.head = self
            .storage
            .draft_mutation_staging_head(store, pending.identity)?
            .ok_or(ComposerHostError::MutationMalformed)?;
        pending.session = active_session(&self.storage, store, pending.binding)?;
        pending.phase = ComposerHostMutationPhase::Building {
            prepared: prepared_edit,
            endpoint,
        };
        Ok(())
    }

    fn drive_build_mutation(
        &mut self,
        store: &HomeStore,
        cancellation: &CommandCancellation,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        let (prepared, mut endpoint) = match &pending.phase {
            ComposerHostMutationPhase::Building { prepared, endpoint } => {
                (prepared.clone(), *endpoint)
            }
            _ => return Err(ComposerHostError::MutationMalformed),
        };
        let transition_limit = self.mutation_transition_limit();
        for _ in 0..transition_limit {
            if cancellation.is_cancelled() {
                return self.cancel_build_mutation(store, pending, &prepared);
            }
            match self.advance_build_mutation(store, pending, &prepared)? {
                Some(BuildCommandResult::Pending(next)) => {
                    endpoint = next;
                    pending.phase = ComposerHostMutationPhase::Building {
                        prepared: prepared.clone(),
                        endpoint,
                    };
                    continue;
                }
                Some(BuildCommandResult::Terminal(outcome)) => return Ok(outcome),
                None => {}
            }
            if let Some(next) = self.stage_next_build_window(store, pending, endpoint)? {
                endpoint = next;
                pending.phase = ComposerHostMutationPhase::Building {
                    prepared: prepared.clone(),
                    endpoint,
                };
                continue;
            }
            match self.settle_build_mutation(store, pending, &prepared)? {
                BuildCommandResult::Pending(next) => {
                    endpoint = next;
                    pending.phase = ComposerHostMutationPhase::Building {
                        prepared: prepared.clone(),
                        endpoint,
                    };
                }
                BuildCommandResult::Terminal(outcome) => return Ok(outcome),
            }
        }
        Err(ComposerHostError::MutationWorkPending)
    }

    fn stage_next_build_window(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        endpoint: DraftPieceBuildProgressReceiptReferenceV1,
    ) -> Result<Option<DraftPieceBuildProgressReceiptReferenceV1>, ComposerHostError> {
        let Some(window) = self.storage.prepare_next_durable_draft_piece_window(
            store,
            pending.identity,
            endpoint,
            DraftPieceDurableBuildWindowLimitsV1::maximum(),
        )?
        else {
            return Ok(None);
        };
        let target_endpoint = window.target_endpoint();
        let mut command = HomeCommand::new(store.home_revision()?);
        command.add(
            self.storage
                .stage_next_durable_draft_piece_window(self.storage.revision(store)?, window),
        )?;
        let outcome = self.execute_mutation_command(store, command);
        match command_selection(store, outcome)? {
            StagingCommandResult::Target => Ok(Some(target_endpoint)),
            StagingCommandResult::Source => Err(ComposerHostError::MutationWorkPending),
            StagingCommandResult::Terminal => Err(ComposerHostError::MutationMalformed),
        }
    }

    fn advance_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        prepared: &PreparedDraftPieceEditV1,
    ) -> Result<Option<BuildCommandResult>, ComposerHostError> {
        match self.storage.prepare_draft_piece_build_advance(
            store,
            prepared.header().draft_id(),
            prepared.header().session_id(),
            prepared.header().operation_id(),
        ) {
            Ok(Some(advance)) => {
                let contribution = self
                    .storage
                    .advance_draft_piece_edit(self.storage.revision(store)?, advance);
                self.run_build_command(store, pending, prepared, contribution)
                    .map(Some)
            }
            Ok(None) => Ok(None),
            Err(DraftPiecePrepareErrorV1::Rejected(reason)) => self
                .reject_build_mutation(store, pending, prepared, reason)
                .map(Some),
            Err(error) => Err(error.into()),
        }
    }

    fn settle_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        prepared: &PreparedDraftPieceEditV1,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        let contribution = self
            .storage
            .settle_draft_piece_edit(self.storage.revision(store)?, prepared.clone());
        self.run_build_command(store, pending, prepared, contribution)
    }

    fn reject_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        prepared: &PreparedDraftPieceEditV1,
        reason: syndic_storage::DraftPieceRejectedReasonV1,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        let contribution = self.storage.reject_draft_piece_edit(
            self.storage.revision(store)?,
            prepared.clone(),
            reason,
        );
        self.run_build_command(store, pending, prepared, contribution)
    }

    pub(super) fn prepare_translated_page(
        &mut self,
        pending: &ComposerHostMutationCoordinator,
        request: MutationPageRequest,
        frontier: WidgetLaneFrontier,
        acceptance: MutationPageAcceptance,
        translated: TranslatedWidgetPage,
    ) -> Result<ComposerHostInFlightPage, ComposerHostError> {
        let lane = translated.lane;
        let mut cursor = match lane {
            DraftMutationStagingLaneV1::Source => pending.head.source().next_cursor(),
            DraftMutationStagingLaneV1::Proposal => pending.head.proposal().next_cursor(),
        };
        let mut inputs = Vec::with_capacity(translated.items.len());
        for item in translated.items {
            let successor_cursor = cursor
                .checked_add(1)
                .ok_or(ComposerHostError::MutationMalformed)?;
            inputs.push(DraftMutationStagingPageInputV1::new(
                lane,
                cursor,
                successor_cursor,
                1,
                65_536,
                Box::new([item]),
            ));
            cursor = successor_cursor;
        }
        let prepared = self.storage.prepare_draft_mutation_staging_page_batch(
            &pending.head,
            &pending.session,
            inputs.into_boxed_slice(),
        )?;
        let widget_lane = request.page().key().lane();
        Ok(ComposerHostInFlightPage {
            prepared,
            kind: ComposerHostInFlightPageKind::Widget {
                request,
                lane: widget_lane,
                frontier,
                acceptance,
            },
            #[cfg(feature = "test-faults")]
            custody_serial: self.next_mutation_custody_serial(),
            fragment_count: translated.fragment_count,
            fragment_chain: translated.fragment_chain,
            proposal_envelope_applied: translated.proposal_envelope_applied,
            last_proposal_range: translated.last_proposal_range,
            remaining_proposal_range: translated.remaining_proposal_range,
        })
    }

    pub(super) fn stage_internal_translated_page(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        finish: MutationFinishInput,
        translated: TranslatedWidgetPage,
    ) -> Result<(), ComposerHostError> {
        if pending.in_flight_page.is_some() {
            return Err(ComposerHostError::MutationPending);
        }
        let lane = translated.lane;
        let mut cursor = match lane {
            DraftMutationStagingLaneV1::Source => pending.head.source().next_cursor(),
            DraftMutationStagingLaneV1::Proposal => pending.head.proposal().next_cursor(),
        };
        let mut inputs = Vec::with_capacity(translated.items.len());
        for item in translated.items {
            let successor_cursor = cursor
                .checked_add(1)
                .ok_or(ComposerHostError::MutationMalformed)?;
            inputs.push(DraftMutationStagingPageInputV1::new(
                lane,
                cursor,
                successor_cursor,
                1,
                65_536,
                Box::new([item]),
            ));
            cursor = successor_cursor;
        }
        let prepared = self.storage.prepare_draft_mutation_staging_page_batch(
            &pending.head,
            &pending.session,
            inputs.into_boxed_slice(),
        )?;
        pending.in_flight_page = Some(ComposerHostInFlightPage {
            prepared,
            kind: ComposerHostInFlightPageKind::Internal { finish },
            #[cfg(feature = "test-faults")]
            custody_serial: self.next_mutation_custody_serial(),
            fragment_count: translated.fragment_count,
            fragment_chain: translated.fragment_chain,
            proposal_envelope_applied: translated.proposal_envelope_applied,
            last_proposal_range: translated.last_proposal_range,
            remaining_proposal_range: translated.remaining_proposal_range,
        });
        match self.drive_in_flight_page(store, pending)? {
            Some(_) => Err(ComposerHostError::MutationMalformed),
            None => Ok(()),
        }
    }

    pub(super) fn drive_in_flight_page(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<Option<MutationPageAcceptance>, ComposerHostError> {
        let result = {
            let page = pending
                .in_flight_page
                .as_ref()
                .ok_or(ComposerHostError::MutationMalformed)?;
            self.run_staging_page_batch(store, &page.prepared)
        }?;
        match result {
            StagingCommandResult::Target => {
                let page = pending
                    .in_flight_page
                    .take()
                    .ok_or(ComposerHostError::MutationMalformed)?;
                apply_in_flight_page_target(pending, page)
            }
            StagingCommandResult::Source => Err(ComposerHostError::MutationWorkPending),
            StagingCommandResult::Terminal => Err(ComposerHostError::MutationMalformed),
        }
    }

    pub(super) fn release_unadmitted_in_flight_page(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<(), ComposerHostError> {
        let Some(page) = pending.in_flight_page.as_ref() else {
            return Ok(());
        };
        let reconciliation = self
            .storage
            .reconcile_draft_mutation_staging_page_batch(store, &page.prepared)?;
        match reconciliation {
            DraftMutationStagingReconcileV1::SourceSelected => {
                let _ = pending.in_flight_page.take();
                Ok(())
            }
            DraftMutationStagingReconcileV1::TargetSelected => {
                let page = pending
                    .in_flight_page
                    .take()
                    .ok_or(ComposerHostError::MutationMalformed)?;
                apply_in_flight_page_target(pending, page)?;
                Ok(())
            }
            DraftMutationStagingReconcileV1::Terminal(_) => {
                Err(ComposerHostError::MutationMalformed)
            }
        }
    }

    #[cfg(feature = "test-faults")]
    fn next_mutation_custody_serial(&mut self) -> u64 {
        let serial = self.next_mutation_custody_serial;
        self.next_mutation_custody_serial = serial
            .checked_add(1)
            .expect("test mutation custody serial is bounded");
        serial
    }

    fn run_staging_page_batch(
        &mut self,
        store: &HomeStore,
        prepared: &PreparedDraftMutationStagingBatchV1,
    ) -> Result<StagingCommandResult, ComposerHostError> {
        for _ in 0..self.mutation_transition_limit() {
            let mut command = HomeCommand::new(store.home_revision()?);
            command.add(self.storage.draft_mutation_staging_page_batch(
                self.storage.revision(store)?,
                prepared.clone(),
            ))?;
            let _ = self.execute_mutation_command(store, command);
            match self
                .storage
                .reconcile_draft_mutation_staging_page_batch(store, prepared)?
            {
                DraftMutationStagingReconcileV1::SourceSelected => continue,
                DraftMutationStagingReconcileV1::TargetSelected => {
                    return Ok(StagingCommandResult::Target);
                }
                DraftMutationStagingReconcileV1::Terminal(_) => {
                    return Ok(StagingCommandResult::Terminal);
                }
            }
        }
        Ok(StagingCommandResult::Source)
    }

    pub(super) fn run_staging_command(
        &mut self,
        store: &HomeStore,
        prepared: &syndic_storage::PreparedDraftMutationStagingCommandV1,
        cancellation: Option<&CommandCancellation>,
    ) -> Result<StagingCommandResult, ComposerHostError> {
        for _ in 0..self.mutation_transition_limit() {
            let mut command = HomeCommand::new(store.home_revision()?);
            if let Some(cancellation) = cancellation {
                command = command.with_cancellation(cancellation.clone());
            }
            command.add(
                self.storage.draft_mutation_staging_command(
                    self.storage.revision(store)?,
                    prepared.clone(),
                ),
            )?;
            let _ = self.execute_mutation_command(store, command);
            match self
                .storage
                .reconcile_draft_mutation_staging_command(store, prepared)?
            {
                DraftMutationStagingReconcileV1::SourceSelected => continue,
                DraftMutationStagingReconcileV1::TargetSelected => {
                    return Ok(StagingCommandResult::Target);
                }
                DraftMutationStagingReconcileV1::Terminal(_) => {
                    return Ok(StagingCommandResult::Terminal);
                }
            }
        }
        Ok(StagingCommandResult::Source)
    }

    pub(super) fn run_build_command(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        prepared: &PreparedDraftPieceEditV1,
        contribution: beryl_home_store::MutationContribution,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        let mut command = HomeCommand::new(store.home_revision()?);
        command.add(contribution)?;
        let outcome = self.execute_mutation_command(store, command);
        if let CommandOutcome::Indeterminate { reconciliation, .. } = outcome {
            let handle = reconciliation.install_and_handle();
            let _ = store.reconcile(&handle)?;
        }
        let terminal_ordinal = prepared
            .header()
            .fragment_count()
            .checked_add(1)
            .ok_or(ComposerHostError::MutationMalformed)?;
        let status = self.storage.draft_piece_operation_status_page(
            store,
            prepared,
            terminal_ordinal,
            &[],
        )?;
        match status {
            DraftPieceOperationVerificationV1::More { .. } => {
                Err(ComposerHostError::MutationMalformed)
            }
            DraftPieceOperationVerificationV1::Status(
                DraftPieceOperationStatusV1::Open(build)
                | DraftPieceOperationStatusV1::Complete(build),
            ) => Ok(BuildCommandResult::Pending(build.progress_receipt())),
            DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Settled(
                settlement,
            )) => Ok(BuildCommandResult::Terminal(
                self.finish_build_settlement(pending, settlement)?,
            )),
            DraftPieceOperationVerificationV1::Status(
                DraftPieceOperationStatusV1::Absent | DraftPieceOperationStatusV1::Collision(_),
            ) => Err(ComposerHostError::MutationMalformed),
        }
    }

    fn execute_mutation_command(
        &mut self,
        store: &HomeStore,
        command: HomeCommand,
    ) -> CommandOutcome {
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.mutation_before_execute_fault.take() {
            fault(store, self.storage);
        }
        store.execute(command)
    }
}

fn apply_in_flight_page_target(
    pending: &mut ComposerHostMutationCoordinator,
    page: ComposerHostInFlightPage,
) -> Result<Option<MutationPageAcceptance>, ComposerHostError> {
    pending.head = page.prepared.target_head().clone();
    pending.session = page
        .prepared
        .target_session()
        .cloned()
        .ok_or(ComposerHostError::MutationMalformed)?;
    let acceptance = match page.kind {
        ComposerHostInFlightPageKind::Widget {
            lane,
            frontier,
            acceptance,
            ..
        } => {
            *pending.lane_mut(lane) = frontier;
            Some(acceptance)
        }
        ComposerHostInFlightPageKind::Internal { .. } => None,
    };
    pending.fragment_count = page.fragment_count;
    pending.fragment_chain = page.fragment_chain;
    pending.proposal_envelope_applied = page.proposal_envelope_applied;
    pending.last_proposal_range = page.last_proposal_range;
    pending.remaining_proposal_range = page.remaining_proposal_range;
    Ok(acceptance)
}
