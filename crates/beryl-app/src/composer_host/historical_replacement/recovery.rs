use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeCommand, ReconciliationResolution};

pub(in crate::composer_host) struct HistoricalPreflush {
    pub(in crate::composer_host) prior: ComposerHostBinding,
    pub(in crate::composer_host) ticket: crate::composer_host::ComposerHostFlushTicket,
    pub(in crate::composer_host) successor: Option<ComposerHostBinding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostHistoricalRestoration {
    prior: ComposerHostBinding,
    candidate: DraftEditorCandidateActivationBindingV1,
    selector: DraftEditorCurrentSelectorV1,
    intent: DraftSubmissionIntent,
    imported_position: Option<DraftCompositePositionV1>,
}

impl ComposerHostHistoricalRestoration {
    pub const fn prior_binding(self) -> ComposerHostBinding {
        self.prior
    }
    pub const fn candidate(self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }
    pub const fn selector(self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
    pub const fn intent(self) -> DraftSubmissionIntent {
        self.intent
    }
    pub const fn imported_position(self) -> Option<DraftCompositePositionV1> {
        self.imported_position
    }
}

pub(in crate::composer_host) struct RetiredHistoricalReplacement {
    pending: Box<PendingHistoricalReplacement>,
    prior_candidate: DraftEditorCandidateActivationBindingV1,
    prior_selector: DraftEditorCurrentSelectorV1,
    captured_selector: DraftEditorCurrentSelectorV1,
    prior_intent: DraftSubmissionIntent,
    cleanup: Option<(PreparedDraftReplacementImportTerminationV1, bool)>,
    previous_cleanup: Option<PreparedDraftReplacementImportTerminationV1>,
    settled: Option<ComposerHostHistoricalRestoration>,
}

impl RetiredHistoricalReplacement {
    pub(in crate::composer_host) fn capture(
        mut pending: Box<PendingHistoricalReplacement>,
        selector: DraftEditorCurrentSelectorV1,
        intent: DraftSubmissionIntent,
    ) -> Self {
        pending.assets.take();
        let prior_candidate = pending.ticket.binding.candidate();
        let prior_selector = pending
            .import
            .as_ref()
            .map_or(selector, |import| import.basis().selector());
        Self {
            pending,
            prior_candidate,
            prior_selector,
            captured_selector: selector,
            prior_intent: intent,
            cleanup: None,
            previous_cleanup: None,
            settled: None,
        }
    }

    pub(in crate::composer_host) fn restoration(
        &self,
    ) -> Option<ComposerHostHistoricalRestoration> {
        self.settled
    }

    pub(in crate::composer_host) fn settle(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &SyndicStorage,
    ) -> Result<ComposerHostHistoricalRestoration, String> {
        self.observe(access, storage)?;
        if access.home_id() != self.pending.ticket.binding.home_id()
            || access.generation() == self.pending.ticket.binding.home_generation()
        {
            return Err("historical recovery requires the fresh original home candidate".into());
        }
        if let Some(settled) = self.settled {
            return Ok(settled);
        }
        let prior = ComposerHostHistoricalRestoration {
            prior: self.pending.prior_publication_binding,
            candidate: self.prior_candidate,
            selector: self.prior_selector,
            intent: self.prior_intent,
            imported_position: None,
        };
        let result = match &mut self.pending.stage {
            HistoricalStage::Begin
            | HistoricalStage::NotCommitted
            | HistoricalStage::Cancel {
                executed: false, ..
            } => Some(prior),
            HistoricalStage::Streaming
            | HistoricalStage::Start {
                executed: false, ..
            }
            | HistoricalStage::Terminate {
                executed: false, ..
            } => None,
            HistoricalStage::Quantum { begin, outcome } => {
                let committed = match outcome {
                    QuantumOutcome::Committed => true,
                    QuantumOutcome::Noncommit => false,
                    QuantumOutcome::Pending(handle) => {
                        let committed = match access
                            .retry_reconciliation(handle)
                            .map_err(|e| e.to_string())?
                        {
                            ReconciliationResolution::ExactNew { .. } => true,
                            ReconciliationResolution::ExactOld => false,
                            ReconciliationResolution::Collision
                            | ReconciliationResolution::ExactSuccessor { .. } => {
                                return Err(
                                    "historical import original quantum is not exact".into()
                                );
                            }
                        };
                        *outcome = if committed {
                            QuantumOutcome::Committed
                        } else {
                            QuantumOutcome::Noncommit
                        };
                        committed
                    }
                };
                (!committed && *begin).then_some(prior)
            }
            HistoricalStage::Start {
                prepared,
                executed: true,
            } => {
                match storage
                    .resume_replacement_draft_start_candidate(access, prepared)
                    .map_err(|e| e.to_string())?
                {
                    DraftReplacementCommandOutcomeV1::Started(selector) => {
                        let DraftPieceSettlementClosureV1::SealedImport(selection) =
                            prepared.settlement().closure()
                        else {
                            return Err("historical recovery start has no sealed selection".into());
                        };
                        if selector != selection.successor_selector() {
                            return Err("historical recovery selector changed".into());
                        }
                        Some(ComposerHostHistoricalRestoration {
                            candidate: DraftEditorCandidateActivationBindingV1::from_head(
                                selection.after_session(),
                            ),
                            selector,
                            intent: DraftSubmissionIntent::Replacement(selection.basis().intent()),
                            imported_position: Some(self.pending.position),
                            ..prior
                        })
                    }
                    DraftReplacementCommandOutcomeV1::NotCommitted => None,
                    _ => return Err("historical recovery start outcome changed".into()),
                }
            }
            HistoricalStage::Cancel {
                prepared,
                executed: true,
            } => {
                match storage
                    .resume_replacement_intent_cancellation_candidate(access, prepared)
                    .map_err(|e| e.to_string())?
                {
                    DraftReplacementCommandOutcomeV1::IntentCancelled(selector) => {
                        Some(ComposerHostHistoricalRestoration {
                            candidate: DraftEditorCandidateActivationBindingV1::from_head(
                                prepared.receipt().after_head(),
                            ),
                            selector,
                            intent: DraftSubmissionIntent::Ordinary,
                            ..prior
                        })
                    }
                    DraftReplacementCommandOutcomeV1::NotCommitted => Some(prior),
                    _ => return Err("historical recovery cancellation outcome changed".into()),
                }
            }
            HistoricalStage::Terminate {
                prepared,
                executed: true,
            } => {
                match storage
                    .resume_replacement_import_termination_candidate(access, prepared)
                    .map_err(|e| e.to_string())?
                {
                    DraftReplacementCommandOutcomeV1::ImportTerminated => {
                        Some(terminated(prior, prepared)?)
                    }
                    DraftReplacementCommandOutcomeV1::NotCommitted => None,
                    _ => return Err("historical recovery termination outcome changed".into()),
                }
            }
            HistoricalStage::Ready(completion) => Some(ComposerHostHistoricalRestoration {
                candidate: completion.binding().candidate(),
                selector: self.captured_selector,
                intent: completion.intent(),
                imported_position: completion
                    .requires_rebind()
                    .then_some(self.pending.position),
                ..prior
            }),
        };
        let result = match result {
            Some(result) => result,
            None => self.terminate_open_import(access, storage, prior)?,
        };
        self.settled = Some(result);
        Ok(result)
    }

    pub(in crate::composer_host) fn observe(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &SyndicStorage,
    ) -> Result<(), String> {
        if access.home_id() != self.pending.ticket.binding.home_id()
            || access.generation() == self.pending.ticket.binding.home_generation()
        {
            return Err("historical recovery requires the fresh original home candidate".into());
        }
        match &mut self.pending.stage {
            HistoricalStage::Quantum {
                outcome: outcome @ QuantumOutcome::Pending(_),
                ..
            } => {
                let QuantumOutcome::Pending(handle) = outcome else {
                    unreachable!()
                };
                let committed = match access
                    .retry_reconciliation(handle)
                    .map_err(|e| e.to_string())?
                {
                    ReconciliationResolution::ExactNew { .. } => true,
                    ReconciliationResolution::ExactOld => false,
                    ReconciliationResolution::Collision
                    | ReconciliationResolution::ExactSuccessor { .. } => {
                        return Err("historical import original quantum is not exact".into());
                    }
                };
                *outcome = if committed {
                    QuantumOutcome::Committed
                } else {
                    QuantumOutcome::Noncommit
                };
            }
            HistoricalStage::Start {
                prepared,
                executed: true,
            } => {
                storage
                    .observe_replacement_draft_start_candidate(access, prepared)
                    .map_err(|e| e.to_string())?;
            }
            HistoricalStage::Cancel {
                prepared,
                executed: true,
            } => {
                storage
                    .observe_replacement_intent_cancellation_candidate(access, prepared)
                    .map_err(|e| e.to_string())?;
            }
            HistoricalStage::Terminate {
                prepared,
                executed: true,
            } => {
                storage
                    .observe_replacement_import_termination_candidate(access, prepared)
                    .map_err(|e| e.to_string())?;
            }
            _ => {}
        }
        if let Some((prepared, true)) = &self.cleanup {
            storage
                .observe_replacement_import_termination_candidate(access, prepared)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn terminate_open_import(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &SyndicStorage,
        prior: ComposerHostHistoricalRestoration,
    ) -> Result<ComposerHostHistoricalRestoration, String> {
        if self.cleanup.is_none() {
            let import = self
                .pending
                .import
                .as_ref()
                .ok_or("historical import identity is unavailable")?;
            self.cleanup = Some((
                storage
                    .prepare_replacement_import_termination_candidate(access, import)
                    .map_err(|e| e.to_string())?,
                false,
            ));
        }
        let (prepared, executed) = self.cleanup.as_mut().unwrap();
        let outcome = if *executed {
            storage.resume_replacement_import_termination_candidate(access, prepared)
        } else {
            let mut command = HomeCommand::new(access.home_revision().map_err(|e| e.to_string())?);
            command
                .add(
                    storage
                        .terminate_replacement_draft_import_candidate(
                            access,
                            storage
                                .revision_candidate(access)
                                .map_err(|e| e.to_string())?,
                            prepared.clone(),
                        )
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            *executed = true;
            storage.reconcile_replacement_import_termination_candidate(
                access,
                prepared,
                access.execute(command),
            )
        }
        .map_err(|e| e.to_string())?;
        match outcome {
            DraftReplacementCommandOutcomeV1::ImportTerminated => terminated(prior, prepared),
            DraftReplacementCommandOutcomeV1::NotCommitted => {
                self.previous_cleanup = self.cleanup.take().map(|(prepared, _)| prepared);
                Err(
                    "historical candidate cleanup did not commit; retry requires fresh preparation"
                        .into(),
                )
            }
            _ => Err("historical candidate cleanup has no committed terminal".into()),
        }
    }
}

fn terminated(
    prior: ComposerHostHistoricalRestoration,
    prepared: &PreparedDraftReplacementImportTerminationV1,
) -> Result<ComposerHostHistoricalRestoration, String> {
    let DraftPieceSettlementClosureV1::Noncommit(closure) = prepared.settlement().closure() else {
        return Err("historical recovery termination has no original checkpoint".into());
    };
    Ok(ComposerHostHistoricalRestoration {
        candidate: DraftEditorCandidateActivationBindingV1::from_head(closure.observed_session()),
        ..prior
    })
}

impl SyndicComposerHost {
    pub fn begin_historical_replacement_flush(
        &mut self,
        binding: ComposerHostBinding,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, ComposerHostError> {
        if self.binding() != Some(binding) || self.historical_replacement.is_some() {
            return Err(ComposerHostError::OldBinding);
        }
        let admission =
            self.begin_flush(crate::composer_host::ComposerHostFlushPurpose::Submission)?;
        if let crate::composer_host::ComposerHostFlushAdmission::Started { ticket, .. }
        | crate::composer_host::ComposerHostFlushAdmission::Joined { ticket, .. } = admission
        {
            self.historical_preflush = Some(HistoricalPreflush {
                prior: binding,
                ticket,
                successor: None,
            });
        }
        Ok(admission)
    }

    pub(in crate::composer_host) fn seal_historical_preflush(
        &mut self,
        ticket: crate::composer_host::ComposerHostFlushTicket,
    ) {
        let binding = self.binding();
        if let Some(preflush) = self
            .historical_preflush
            .as_mut()
            .filter(|preflush| preflush.ticket == ticket)
        {
            preflush.successor = binding;
        }
    }

    pub fn clear_historical_preflush(&mut self, binding: ComposerHostBinding) {
        if self.binding() == Some(binding) && self.historical_replacement.is_none() {
            self.historical_preflush = None;
        }
    }

    pub fn historical_recovery_prior_binding(&self) -> Option<ComposerHostBinding> {
        self.historical_replacement
            .as_ref()
            .map(|pending| pending.prior_publication_binding)
            .or_else(|| {
                self.historical_preflush
                    .as_ref()
                    .filter(|preflush| {
                        self.binding().is_some_and(|binding| {
                            binding.home_id() == preflush.prior.home_id()
                                && binding.home_generation() == preflush.prior.home_generation()
                                && binding.host_generation() == preflush.prior.host_generation()
                                && binding.candidate().draft_id()
                                    == preflush.prior.candidate().draft_id()
                                && binding.candidate().session_id()
                                    == preflush.prior.candidate().session_id()
                        })
                    })
                    .map(|preflush| preflush.prior)
            })
    }
    pub fn historical_recovery_accepts_binding(&self, binding: ComposerHostBinding) -> bool {
        self.recall_recovery_accepts_binding(binding)
            || self.binding() == Some(binding)
            || self.historical_recovery_prior_binding() == Some(binding)
            || self
                .historical_replacement
                .as_ref()
                .is_some_and(|pending| pending.ticket.binding == binding)
    }
    pub fn observe_historical_replacement_recovery(
        &mut self,
        store: &HomeStore,
        storage: &SyndicStorage,
        ticket: ComposerHostHistoricalTicket,
    ) -> Result<ComposerHostHistoricalRecovery, ComposerHostError> {
        let pending = self
            .historical_replacement
            .as_mut()
            .filter(|pending| pending.ticket == ticket)
            .ok_or(ComposerHostError::OldBinding)?;
        if store.home_id() != ticket.binding.home_id() {
            return Err(ComposerHostError::OldBinding);
        }
        let outcome = match &mut pending.stage {
            HistoricalStage::Begin
            | HistoricalStage::Cancel {
                executed: false, ..
            } => ComposerHostHistoricalRecovery::Unadmitted,
            HistoricalStage::Streaming
            | HistoricalStage::Start {
                executed: false, ..
            }
            | HistoricalStage::Terminate {
                executed: false, ..
            } => ComposerHostHistoricalRecovery::ImportOpen,
            HistoricalStage::Quantum { begin, outcome } => {
                if super::drive::settle_quantum(store, outcome)? || !*begin {
                    ComposerHostHistoricalRecovery::ImportOpen
                } else {
                    ComposerHostHistoricalRecovery::OriginalOutcome(
                        DraftReplacementCommandOutcomeV1::NotCommitted,
                    )
                }
            }
            HistoricalStage::Start {
                prepared,
                executed: true,
            } => import_outcome(storage.resume_replacement_draft_start(store, prepared)?),
            HistoricalStage::Cancel {
                prepared,
                executed: true,
            } => ComposerHostHistoricalRecovery::OriginalOutcome(
                storage.resume_replacement_intent_cancellation(store, prepared)?,
            ),
            HistoricalStage::Terminate {
                prepared,
                executed: true,
            } => import_outcome(storage.resume_replacement_import_termination(store, prepared)?),
            HistoricalStage::Ready(completion) => {
                ComposerHostHistoricalRecovery::PublicationReady(*completion)
            }
            HistoricalStage::NotCommitted => ComposerHostHistoricalRecovery::OriginalOutcome(
                DraftReplacementCommandOutcomeV1::NotCommitted,
            ),
        };
        Ok(outcome)
    }
}

fn import_outcome(outcome: DraftReplacementCommandOutcomeV1) -> ComposerHostHistoricalRecovery {
    if outcome == DraftReplacementCommandOutcomeV1::NotCommitted {
        ComposerHostHistoricalRecovery::ImportOpen
    } else {
        ComposerHostHistoricalRecovery::OriginalOutcome(outcome)
    }
}
