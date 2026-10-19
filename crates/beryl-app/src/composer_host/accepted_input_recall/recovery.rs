use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeCommand};

impl SyndicComposerHost {
    pub(in crate::composer_host) fn recall_recovery_accepts_binding(
        &self,
        binding: ComposerHostBinding,
    ) -> bool {
        self.binding() == Some(binding)
            || self
                .accepted_input_recall
                .as_ref()
                .is_some_and(|pending| pending.ticket.binding == binding)
    }
}

#[derive(Clone, Copy)]
pub struct ComposerHostRecallRestoration {
    prior: ComposerHostBinding,
    candidate: DraftEditorCandidateActivationBindingV1,
    positions: Option<(DraftCompositePositionV1, DraftCompositePositionV1)>,
}

impl ComposerHostRecallRestoration {
    pub const fn prior_binding(self) -> ComposerHostBinding {
        self.prior
    }

    pub const fn candidate(self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }

    pub const fn positions(self) -> Option<(DraftCompositePositionV1, DraftCompositePositionV1)> {
        self.positions
    }
}

pub(in crate::composer_host) struct RetiredAcceptedInputRecall {
    pending: Box<PendingAcceptedInputRecall>,
    cleanup: Option<(PreparedDraftComposerRecallTerminationV1, bool)>,
    previous_cleanup: Option<PreparedDraftComposerRecallTerminationV1>,
    settled: Option<ComposerHostRecallRestoration>,
}

impl RetiredAcceptedInputRecall {
    pub(in crate::composer_host) fn capture(mut pending: Box<PendingAcceptedInputRecall>) -> Self {
        pending.assets.take();
        pending.checkpoint.take();
        if let Stage::Ready(completion) = &mut pending.stage {
            completion.checkpoint.take();
        }
        Self {
            pending,
            cleanup: None,
            previous_cleanup: None,
            settled: None,
        }
    }

    pub(in crate::composer_host) fn prior_binding(&self) -> ComposerHostBinding {
        self.pending.ticket.binding
    }

    pub(in crate::composer_host) fn restoration(&self) -> Option<ComposerHostRecallRestoration> {
        self.settled
    }

    pub(in crate::composer_host) fn observe(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &SyndicStorage,
    ) -> Result<(), String> {
        self.require_candidate(access)?;
        match &mut self.pending.stage {
            Stage::Quantum { outcome, .. } => {
                outcome.reconcile(access)?;
            }
            Stage::Commit {
                prepared,
                attempted: true,
            } => {
                storage
                    .observe_composer_root_replacement_candidate(access, prepared)
                    .map_err(|error| error.to_string())?;
            }
            Stage::Terminate {
                prepared,
                attempted: true,
            } => {
                storage
                    .observe_composer_recall_termination_candidate(access, prepared)
                    .map_err(|error| error.to_string())?;
            }
            _ => {}
        }
        if let Some((prepared, true)) = &self.cleanup {
            storage
                .observe_composer_recall_termination_candidate(access, prepared)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(in crate::composer_host) fn settle(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &SyndicStorage,
    ) -> Result<ComposerHostRecallRestoration, String> {
        self.observe(access, storage)?;
        if let Some(restored) = self.settled {
            return Ok(restored);
        }
        let prior = ComposerHostRecallRestoration {
            prior: self.pending.ticket.binding,
            candidate: self.pending.ticket.binding.candidate(),
            positions: None,
        };
        let restored = match &mut self.pending.stage {
            Stage::Begin | Stage::NotCommitted => Some(prior),
            Stage::Streaming => None,
            Stage::Quantum { beginning, outcome } => {
                (!outcome.reconcile(access)? && *beginning).then_some(prior)
            }
            Stage::Commit {
                attempted: false, ..
            } if self.pending.build.is_none() => Some(prior),
            Stage::Commit {
                attempted: false, ..
            }
            | Stage::Terminate {
                attempted: false, ..
            } => None,
            Stage::Commit {
                prepared,
                attempted: true,
            } => match storage
                .resume_composer_root_replacement_candidate(access, prepared)
                .map_err(|error| error.to_string())?
            {
                DraftComposerRecallCommandOutcomeV1::Adopted(candidate) => {
                    Some(ComposerHostRecallRestoration {
                        candidate,
                        positions: Some((prepared.caret(), prepared.selection())),
                        ..prior
                    })
                }
                DraftComposerRecallCommandOutcomeV1::NotCommitted
                    if self.pending.build.is_none() =>
                {
                    Some(prior)
                }
                DraftComposerRecallCommandOutcomeV1::NotCommitted => None,
                _ => return Err("original recall adoption outcome is not exact".into()),
            },
            Stage::Terminate {
                prepared,
                attempted: true,
            } => match storage
                .resume_composer_recall_termination_candidate(access, prepared)
                .map_err(|error| error.to_string())?
            {
                DraftComposerRecallCommandOutcomeV1::Terminated => {
                    Some(terminated(prior, prepared)?)
                }
                DraftComposerRecallCommandOutcomeV1::NotCommitted => None,
                _ => return Err("original recall termination outcome is not exact".into()),
            },
            Stage::Ready(completion) => Some(ComposerHostRecallRestoration {
                candidate: completion.binding.candidate(),
                positions: completion
                    .replaced
                    .then(|| {
                        Ok::<_, ComposerHostError>((
                            canonical_position(completion.positions.caret())?,
                            canonical_position(completion.positions.selection_anchor())?,
                        ))
                    })
                    .transpose()
                    .map_err(|error| error.to_string())?,
                ..prior
            }),
        };
        let restored = match restored {
            Some(restored) => restored,
            None => self.terminate_open_builder(access, storage, prior)?,
        };
        self.settled = Some(restored);
        Ok(restored)
    }

    fn terminate_open_builder(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &SyndicStorage,
        prior: ComposerHostRecallRestoration,
    ) -> Result<ComposerHostRecallRestoration, String> {
        if self.cleanup.is_none() {
            let build = self
                .pending
                .build
                .as_ref()
                .ok_or("original recall build identity is unavailable")?;
            self.cleanup = Some((
                storage
                    .prepare_composer_recall_termination_candidate(access, build.key())
                    .map_err(|error| error.to_string())?,
                false,
            ));
        }
        let (prepared, attempted) = self.cleanup.as_mut().unwrap();
        let outcome = if *attempted {
            storage.resume_composer_recall_termination_candidate(access, prepared)
        } else {
            let mut command = HomeCommand::new(access.home_revision().map_err(|e| e.to_string())?);
            command
                .add(
                    storage
                        .terminate_composer_recall_candidate(
                            access,
                            storage
                                .revision_candidate(access)
                                .map_err(|e| e.to_string())?,
                            prepared.clone(),
                        )
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            *attempted = true;
            storage.reconcile_composer_recall_termination_candidate(
                access,
                prepared,
                access.execute(command),
            )
        }
        .map_err(|error| error.to_string())?;
        match outcome {
            DraftComposerRecallCommandOutcomeV1::Terminated => terminated(prior, prepared),
            DraftComposerRecallCommandOutcomeV1::NotCommitted => {
                self.previous_cleanup = self.cleanup.take().map(|(prepared, _)| prepared);
                Err("recall cleanup did not commit; retry requires fresh preparation".into())
            }
            _ => Err("recall cleanup has no exact terminal outcome".into()),
        }
    }

    fn require_candidate(&self, access: &HomeCandidateRecoveryAccess<'_>) -> Result<(), String> {
        let original = self.pending.ticket.binding;
        if access.home_id() != original.home_id()
            || access.generation() == original.home_generation()
        {
            return Err("recall recovery requires a fresh original-home candidate".into());
        }
        Ok(())
    }
}

fn terminated(
    prior: ComposerHostRecallRestoration,
    prepared: &PreparedDraftComposerRecallTerminationV1,
) -> Result<ComposerHostRecallRestoration, String> {
    Ok(ComposerHostRecallRestoration {
        candidate: prepared
            .candidate()
            .ok_or("recall termination has no exact candidate")?,
        ..prior
    })
}
