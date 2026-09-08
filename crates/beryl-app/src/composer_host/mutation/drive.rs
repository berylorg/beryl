use syndic_storage::{
    DraftMutationStagingPageInputV1, DraftMutationStagingReconcileV1,
    DraftPieceDurableBuildWindowLimitsV1, DraftPiecePrepareErrorV1,
    PreparedDraftMutationStagingBatchV1,
};

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
        if pending.build_flight.is_none() && pending.cleanup.is_none() {
            self.validate_mutation_store(pending.binding, store)?;
        }
        if request.key() != pending.begin.proposal().key() {
            return Err(ComposerHostError::MutationMalformed);
        }
        if pending.cleanup.is_some() {
            return self.drive_mutation_cleanup(store, pending);
        }
        let outcome = self.execute_active_mutation_step(store, request, cancellation, pending)?;
        if pending.evidence.is_some()
            && pending.cleanup.is_none()
            && !matches!(outcome, ComposerHostMutationOutcome::Committed { .. })
        {
            pending.cleanup = Some(super::cleanup::ComposerHostMutationCleanup::new(
                pending.identity,
                outcome.clone(),
            ));
        }
        if pending.cleanup.is_some() {
            return self.drive_mutation_cleanup(store, pending);
        }
        Ok(outcome)
    }

    fn execute_active_mutation_step(
        &mut self,
        store: &HomeStore,
        request: MutationCommitRequest,
        cancellation: &CommandCancellation,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        if pending.build_flight.is_none() {
            self.validate_mutation_store(pending.binding, store)?;
        }
        if request.key() != pending.begin.proposal().key() {
            return Err(ComposerHostError::MutationMalformed);
        }
        if cancellation.is_cancelled()
            && pending.build_flight.is_none()
            && !matches!(pending.phase, ComposerHostMutationPhase::Building { .. })
        {
            self.release_unadmitted_in_flight_page(store, pending)?;
            return self.cancel_staging_mutation(store, pending);
        }
        self.drive_build_mutation(store, cancellation, pending)
    }

    fn transfer_finished_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<(), ComposerHostError> {
        let command = self.storage.prepare_staged_draft_piece_transfer(
            store,
            pending.identity,
            pending.head.receipt(),
        )?;
        match self.run_build_command(store, pending, command)? {
            BuildCommandResult::Pending(endpoint) => {
                pending.phase = ComposerHostMutationPhase::Building { endpoint };
                Ok(())
            }
            BuildCommandResult::Terminal(_) => Err(ComposerHostError::MutationMalformed),
        }
    }

    fn drive_build_mutation(
        &mut self,
        store: &HomeStore,
        cancellation: &CommandCancellation,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<ComposerHostMutationOutcome, ComposerHostError> {
        for _ in 0..self.mutation_transition_limit() {
            if pending.build_flight.is_some() {
                match self.resume_build_command(store, pending)? {
                    BuildCommandResult::Pending(endpoint) => {
                        pending.phase = ComposerHostMutationPhase::Building { endpoint };
                        continue;
                    }
                    BuildCommandResult::Terminal(outcome) => return Ok(outcome),
                }
            }
            self.validate_mutation_store(pending.binding, store)?;
            if matches!(pending.phase, ComposerHostMutationPhase::Finished) {
                if cancellation.is_cancelled() || pending.build_noncommit.is_some() {
                    let outcome = self.cancel_staging_mutation(store, pending)?;
                    return Ok(
                        if pending.build_noncommit.is_some()
                            && matches!(outcome, ComposerHostMutationOutcome::Cancelled)
                        {
                            ComposerHostMutationOutcome::Error
                        } else {
                            outcome
                        },
                    );
                }
                self.transfer_finished_mutation(store, pending)?;
                continue;
            }
            let endpoint = pending.build_endpoint()?;
            if cancellation.is_cancelled() || pending.build_noncommit.is_some() {
                return self.cancel_build_mutation(store, pending);
            }
            let result = if let Some(result) = self.advance_build_mutation(store, pending)? {
                result
            } else if let Some(result) = self.stage_next_build_window(store, pending, endpoint)? {
                result
            } else {
                self.settle_build_mutation(store, pending)?
            };
            match result {
                BuildCommandResult::Pending(endpoint) => {
                    pending.phase = ComposerHostMutationPhase::Building { endpoint };
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
    ) -> Result<Option<BuildCommandResult>, ComposerHostError> {
        let Some(command) = self.storage.prepare_staged_draft_piece_window(
            store,
            pending.identity,
            endpoint,
            DraftPieceDurableBuildWindowLimitsV1::maximum(),
        )?
        else {
            return Ok(None);
        };
        self.run_build_command(store, pending, command).map(Some)
    }

    fn advance_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<Option<BuildCommandResult>, ComposerHostError> {
        match self.storage.prepare_staged_draft_piece_advance(
            store,
            pending.identity,
            pending.build_endpoint()?,
        ) {
            Ok(Some(command)) => self.run_build_command(store, pending, command).map(Some),
            Ok(None) => Ok(None),
            Err(syndic_storage::StagedDraftPiecePreparationErrorV1::Build(
                DraftPiecePrepareErrorV1::Rejected(reason),
            )) => self.reject_build_mutation(store, pending, reason).map(Some),
            Err(error) => Err(error.into()),
        }
    }

    fn settle_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        let command = self.storage.prepare_staged_draft_piece_terminal(
            store,
            pending.identity,
            pending.build_endpoint()?,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Settle,
        )?;
        self.run_build_command(store, pending, command)
    }

    fn reject_build_mutation(
        &mut self,
        store: &HomeStore,
        pending: &mut ComposerHostMutationCoordinator,
        reason: syndic_storage::DraftPieceRejectedReasonV1,
    ) -> Result<BuildCommandResult, ComposerHostError> {
        let command = self.storage.prepare_staged_draft_piece_terminal(
            store,
            pending.identity,
            pending.build_endpoint()?,
            syndic_storage::StagedDraftPieceTerminalElectionV1::Reject(reason),
        )?;
        self.run_build_command(store, pending, command)
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
            #[cfg(not(feature = "test-faults"))]
            let _ = self.execute_mutation_command(store, command);
            #[cfg(feature = "test-faults")]
            let outcome = self.execute_mutation_command(store, command);
            #[cfg(not(feature = "test-faults"))]
            let reconciliation = self
                .storage
                .reconcile_draft_mutation_staging_page_batch(store, prepared)?;
            #[cfg(feature = "test-faults")]
            let reconciliation = if prepared.target_head().begin().writer_admission().is_some() {
                self.storage
                    .reconcile_draft_mutation_staging_page_batch_outcome(store, prepared, outcome)?
            } else {
                self.storage
                    .reconcile_draft_mutation_staging_page_batch(store, prepared)?
            };
            match reconciliation {
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
            let outcome = self.execute_mutation_command(store, command);
            let reconciliation = if prepared.target_head().begin().writer_admission().is_some() {
                self.storage
                    .reconcile_draft_mutation_staging_command_outcome(store, prepared, outcome)?
            } else {
                self.storage
                    .reconcile_draft_mutation_staging_command(store, prepared)?
            };
            match reconciliation {
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

    fn execute_mutation_command(
        &mut self,
        store: &HomeStore,
        command: HomeCommand,
    ) -> CommandOutcome {
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.mutation_before_execute_fault.take() {
            fault(store, self.storage.clone());
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
