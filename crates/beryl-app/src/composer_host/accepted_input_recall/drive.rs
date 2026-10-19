use super::*;
use beryl_home_store::{CommandCancellation, CommandOutcome, HomeCommand};
use beryl_state::AssetState;

impl SyndicComposerHost {
    #[cfg(feature = "test-faults")]
    pub fn test_refuse_next_recall_preparation(&mut self, refusal: ComposerHostRecallRefusal) {
        self.recall_preparation_refusal = Some(refusal);
    }
    #[cfg(feature = "test-faults")]
    pub fn test_accepted_input_recall_stage(&self) -> Option<&'static str> {
        self.accepted_input_recall
            .as_ref()
            .map(|pending| match pending.stage {
                Stage::Begin => "beginning",
                Stage::Streaming => "preparing",
                Stage::Quantum { .. } => "reconciling",
                Stage::Commit { .. } => "adopting",
                Stage::Terminate { .. } => "terminating",
                Stage::Ready(_) => "ready",
                Stage::NotCommitted => "not-committed",
            })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_recall_after_execute_fault(
        &mut self,
        hook: impl FnOnce(&HomeStore, &CommandOutcome) + Send + 'static,
    ) {
        assert!(self.recall_after_execute_fault.is_none());
        self.recall_after_execute_fault = Some(Box::new(hook));
    }

    fn execute_recall_command(
        &mut self,
        store: &HomeStore,
        command: HomeCommand,
    ) -> CommandOutcome {
        let outcome = store.execute(command);
        #[cfg(feature = "test-faults")]
        if let Some(hook) = self.recall_after_execute_fault.take() {
            hook(store, &outcome);
        }
        outcome
    }
    pub fn advance_accepted_input_recall(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostRecallTicket,
        assets: &AssetState,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostRecallAdvance, ComposerHostError> {
        if self
            .accepted_input_recall
            .as_ref()
            .is_none_or(|pending| pending.ticket != ticket)
            || store.home_id() != ticket.binding.home_id()
        {
            return Err(ComposerHostError::OldBinding);
        }
        let mut pending = self.accepted_input_recall.take().unwrap();
        pending.cancelled |= cancellation.is_cancelled();
        let assets = pending.assets.get_or_insert_with(|| assets.clone()).clone();
        let result = self.drive_accepted_input_recall(store, &mut pending, &assets, cancellation);
        if !matches!(result, Ok(ComposerHostRecallAdvance::NotCommitted)) {
            self.accepted_input_recall = Some(pending);
        }
        result
    }

    pub(in crate::composer_host) fn drain_accepted_input_recall_for_disposal(
        &mut self,
        store: &HomeStore,
    ) -> Result<bool, ComposerHostError> {
        let Some(pending) = self.accepted_input_recall.as_mut() else {
            return Ok(true);
        };
        pending.cancelled = true;
        if let Stage::Ready(completion) = &pending.stage {
            let binding = completion.binding();
            let ticket = pending.ticket;
            self.acknowledge_accepted_input_recall(ticket, binding)?;
            return Ok(true);
        }
        let Some(assets) = pending.assets.clone() else {
            if matches!(
                pending.stage,
                Stage::Begin
                    | Stage::Commit {
                        attempted: false,
                        ..
                    }
            ) {
                self.accepted_input_recall = None;
                return Ok(true);
            }
            return Err(ComposerHostError::MutationAdmittedWorkUnavailable);
        };
        let ticket = pending.ticket;
        let cancellation = CommandCancellation::new();
        cancellation.cancel();
        match self.advance_accepted_input_recall(store, ticket, &assets, &cancellation)? {
            ComposerHostRecallAdvance::Progress => Ok(false),
            ComposerHostRecallAdvance::NotCommitted => Ok(true),
            ComposerHostRecallAdvance::Ready(completion) => {
                self.acknowledge_accepted_input_recall(ticket, completion.binding())?;
                Ok(true)
            }
        }
    }

    fn drive_accepted_input_recall(
        &mut self,
        store: &HomeStore,
        pending: &mut PendingAcceptedInputRecall,
        assets: &AssetState,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostRecallAdvance, ComposerHostError> {
        match &mut pending.stage {
            Stage::Ready(completion) => {
                return Ok(ComposerHostRecallAdvance::Ready(completion.clone()));
            }
            Stage::NotCommitted => return Ok(ComposerHostRecallAdvance::NotCommitted),
            Stage::Quantum { beginning, outcome } => {
                super::super::request::validate_store(pending.ticket.binding, store)?;
                if settle_quantum(store, outcome)? {
                    pending.stage = Stage::Streaming;
                } else if *beginning {
                    pending.stage = Stage::NotCommitted;
                } else {
                    if !pending.cancelled {
                        pending.refusal = Some(ComposerHostRecallRefusal::CommandRefused);
                    }
                    self.prepare_recall_termination(store, pending)?;
                }
            }
            Stage::Begin => {
                if pending.cancelled {
                    pending.stage = Stage::NotCommitted;
                } else {
                    super::super::request::validate_store(pending.ticket.binding, store)?;
                    let mut command = HomeCommand::new(store.home_revision()?);
                    command.add(self.storage.begin_composer_recall(
                        self.storage.revision(store)?,
                        pending.build.as_ref().unwrap().clone(),
                    ))?;
                    pending.stage = Stage::Quantum {
                        beginning: true,
                        outcome: retain_quantum(self.execute_recall_command(
                            store,
                            command.with_cancellation(cancellation.clone()),
                        )),
                    };
                }
            }
            Stage::Streaming => {
                if pending.cancelled {
                    self.prepare_recall_termination(store, pending)?;
                } else {
                    if let Err(error) =
                        self.stream_accepted_input_recall(store, pending, assets, cancellation)
                    {
                        if let Some(refusal) = definite_preparation_refusal(&error)
                            && super::super::request::validate_store(pending.ticket.binding, store)
                                .is_ok()
                        {
                            pending.refusal = Some(refusal);
                            pending.cancelled = true;
                            self.prepare_recall_termination(store, pending)?;
                        } else {
                            return Err(error);
                        }
                    }
                }
            }
            Stage::Commit {
                prepared,
                attempted,
            } => {
                if !*attempted && pending.cancelled {
                    if pending.build.is_some() {
                        self.prepare_recall_termination(store, pending)?;
                    } else {
                        pending.stage = Stage::NotCommitted;
                    }
                } else {
                    let outcome = if *attempted {
                        self.storage
                            .resume_composer_root_replacement(store, prepared)?
                    } else {
                        super::super::request::validate_store(pending.ticket.binding, store)?;
                        let mut command = HomeCommand::new(store.home_revision()?);
                        command.add(self.storage.commit_composer_root_replacement(
                            self.storage.revision(store)?,
                            prepared.clone(),
                        ))?;
                        *attempted = true;
                        let outcome = self.execute_recall_command(
                            store,
                            command.with_cancellation(cancellation.clone()),
                        );
                        pending.cancelled |= cancellation.is_cancelled();
                        self.storage
                            .reconcile_composer_root_replacement(store, prepared, outcome)?
                    };
                    match outcome {
                        DraftComposerRecallCommandOutcomeV1::Adopted(candidate) => {
                            let prepared = prepared.clone();
                            let completion =
                                self.adopt_recalled_root(store, pending, &prepared, candidate)?;
                            pending.stage = Stage::Ready(completion);
                        }
                        DraftComposerRecallCommandOutcomeV1::NotCommitted => {
                            if !pending.cancelled {
                                pending.refusal = Some(ComposerHostRecallRefusal::CommandRefused);
                            }
                            if pending.build.is_some() {
                                self.prepare_recall_termination(store, pending)?;
                            } else {
                                pending.stage = Stage::NotCommitted;
                            }
                        }
                        _ => return Err(ComposerHostError::MutationUnavailable),
                    }
                }
            }
            Stage::Terminate {
                prepared,
                attempted,
            } => {
                let outcome = if *attempted {
                    self.storage
                        .resume_composer_recall_termination(store, prepared)?
                } else {
                    let mut command = HomeCommand::new(store.home_revision()?);
                    command.add(self.storage.terminate_composer_recall(
                        self.storage.revision(store)?,
                        prepared.clone(),
                    ))?;
                    *attempted = true;
                    let outcome = self.execute_recall_command(store, command);
                    self.storage
                        .reconcile_composer_recall_termination(store, prepared, outcome)?
                };
                match outcome {
                    DraftComposerRecallCommandOutcomeV1::Terminated => {
                        let candidate = prepared
                            .candidate()
                            .ok_or(ComposerHostError::MutationAdmittedWorkUnavailable)?;
                        let previous = pending.ticket.binding;
                        let active = self.active.as_mut().ok_or(ComposerHostError::OldBinding)?;
                        if active.binding != previous
                            || candidate.root() != previous.root()
                            || candidate.history() != previous.history()
                        {
                            return Err(ComposerHostError::OldBinding);
                        }
                        let binding = ComposerHostBinding::new(
                            previous.home_id(),
                            previous.home_generation(),
                            previous.host_generation(),
                            candidate,
                            previous.presentation_generation(),
                        );
                        active.binding = binding;
                        active.storage_candidate = candidate;
                        pending.stage = Stage::Ready(ComposerHostRecallCompletion {
                            previous,
                            binding,
                            positions: pending.before_positions,
                            checkpoint: None,
                            replaced: false,
                            refusal: pending.refusal,
                        });
                    }
                    _ => return Err(ComposerHostError::MutationAdmittedWorkUnavailable),
                }
            }
        }
        match &pending.stage {
            Stage::Ready(completion) => Ok(ComposerHostRecallAdvance::Ready(completion.clone())),
            Stage::NotCommitted => Ok(ComposerHostRecallAdvance::NotCommitted),
            _ => Ok(ComposerHostRecallAdvance::Progress),
        }
    }

    fn stream_accepted_input_recall(
        &mut self,
        store: &HomeStore,
        pending: &mut PendingAcceptedInputRecall,
        assets: &AssetState,
        cancellation: &CommandCancellation,
    ) -> Result<(), ComposerHostError> {
        super::super::request::validate_store(pending.ticket.binding, store)?;
        #[cfg(feature = "test-faults")]
        if let Some(refusal) = self.recall_preparation_refusal.take() {
            return Err(match refusal {
                ComposerHostRecallRefusal::CapacityUnavailable => {
                    DraftComposerRecallErrorV1::CapacityUnavailable
                }
                ComposerHostRecallRefusal::SourceChanged => DraftComposerRecallErrorV1::Conflict,
                _ => DraftComposerRecallErrorV1::Proof,
            }
            .into());
        }
        let build = pending.build.as_ref().unwrap();
        let step = match self.storage.prepare_composer_recall_step(store, build)? {
            DraftComposerRecallAdvancePreparationV1::Ready(step) => step,
            DraftComposerRecallAdvancePreparationV1::Image(image) => {
                self.prepare_recalled_image(store, assets, build, image)?
            }
            DraftComposerRecallAdvancePreparationV1::Complete(_) => {
                let prepared = self.storage.prepare_composer_recall_commit(store, build)?;
                pending.stage = Stage::Commit {
                    prepared,
                    attempted: false,
                };
                return Ok(());
            }
        };
        let mut command = HomeCommand::new(store.home_revision()?);
        command.add(
            self.storage
                .advance_composer_recall(self.storage.revision(store)?, step),
        )?;
        pending.stage = Stage::Quantum {
            beginning: false,
            outcome: retain_quantum(
                self.execute_recall_command(store, command.with_cancellation(cancellation.clone())),
            ),
        };
        Ok(())
    }

    fn prepare_recalled_image(
        &self,
        store: &HomeStore,
        assets: &AssetState,
        build: &PreparedDraftComposerRecallV1,
        image: DraftComposerRecallImageSourceV1,
    ) -> Result<PreparedDraftComposerRecallAdvanceV1, ComposerHostError> {
        let proof = build
            .source()
            .asset_reference_set()
            .ok_or(DraftComposerRecallErrorV1::Proof)?;
        let entry = assets
            .marker_reference(store, proof, image.marker_id())?
            .ok_or(DraftComposerRecallErrorV1::Proof)?;
        let asset = entry.asset_id();
        let page = image
            .descriptive_page(asset)
            .ok_or(DraftComposerRecallErrorV1::Proof)?;
        let witness = assets.composer_recall_marker_witness(store, page)?;
        let mut attempt = self
            .storage
            .prepare_composer_recall_image_proof(store, image, asset, witness)?;
        let receipt = store
            .compose_proof(
                attempt
                    .take_command()
                    .ok_or(DraftComposerRecallErrorV1::Proof)?,
            )
            .map_err(|_| DraftComposerRecallErrorV1::Proof)?;
        Ok(attempt.consume(&self.storage, store, receipt)?)
    }

    fn prepare_recall_termination(
        &self,
        store: &HomeStore,
        pending: &mut PendingAcceptedInputRecall,
    ) -> Result<(), ComposerHostError> {
        let prepared = self.storage.prepare_composer_recall_termination(
            store,
            pending
                .build
                .as_ref()
                .ok_or(ComposerHostError::MutationUnavailable)?
                .key(),
        )?;
        pending.stage = Stage::Terminate {
            prepared,
            attempted: false,
        };
        Ok(())
    }
}

fn retain_quantum(outcome: CommandOutcome) -> Quantum {
    Quantum::new(outcome)
}

fn definite_preparation_refusal(error: &ComposerHostError) -> Option<ComposerHostRecallRefusal> {
    match error {
        ComposerHostError::AcceptedInputRecall(DraftComposerRecallErrorV1::CapacityUnavailable) => {
            Some(ComposerHostRecallRefusal::CapacityUnavailable)
        }
        ComposerHostError::AcceptedInputRecall(DraftComposerRecallErrorV1::Proof) => {
            Some(ComposerHostRecallRefusal::ProofRejected)
        }
        ComposerHostError::AcceptedInputRecall(DraftComposerRecallErrorV1::Build(
            DraftPiecePrepareErrorV1::Rejected(_)
            | DraftPiecePrepareErrorV1::InvalidRoot
            | DraftPiecePrepareErrorV1::Absent,
        )) => Some(ComposerHostRecallRefusal::ProofRejected),
        ComposerHostError::AcceptedInputRecall(DraftComposerRecallErrorV1::Build(
            DraftPiecePrepareErrorV1::ConcurrentChange,
        )) => Some(ComposerHostRecallRefusal::SourceChanged),
        ComposerHostError::AcceptedInputRecall(
            DraftComposerRecallErrorV1::Conflict | DraftComposerRecallErrorV1::Ineligible,
        ) => Some(ComposerHostRecallRefusal::SourceChanged),
        ComposerHostError::AcceptedInputRecallAsset(
            beryl_state::AssetComposerRecallError::Rejected
            | beryl_state::AssetComposerRecallError::Sidecar(_),
        ) => Some(ComposerHostRecallRefusal::SourceUnavailable),
        _ => None,
    }
}

fn settle_quantum(store: &HomeStore, outcome: &mut Quantum) -> Result<bool, ComposerHostError> {
    if let Quantum::Indeterminate { handle, result, .. } = outcome {
        if result.as_ref().is_none_or(|result| result.is_err()) {
            *result = Some(store.retry_reconciliation(handle));
        }
    }
    if matches!(
        outcome,
        Quantum::Committed {
            later_failure: Some(_),
            ..
        } | Quantum::Committed {
            local_finalization: Some(_),
            ..
        }
    ) {
        return Err(ComposerHostError::MutationAdmittedWorkUnavailable);
    }
    outcome
        .known_commit()
        .ok_or(ComposerHostError::MutationAdmittedWorkUnavailable)
}
