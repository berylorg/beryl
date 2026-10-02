use super::super::publication::{PreparedPublication, RetainedComposerCommandOutcome};
use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeCommand, HomeRecoveryCandidate};
use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1,
    DraftEditorCandidateSavedCorrespondenceV1,
};

pub struct ComposerHostFailedResident {
    binding: ComposerHostBinding,
    checkpoint: DraftEditorCandidateActivationBindingV1,
    captured_checkpoint: DraftEditorCandidateActivationBindingV1,
    activation_checkpoint: DraftEditorCandidateActivationBindingV1,
    selector: DraftEditorCurrentSelectorV1,
    thread: beryl_model::SyndicThreadId,
    canonical_home: std::path::PathBuf,
    capacity: std::num::NonZeroUsize,
    original: Option<Box<super::super::publication::retained::RetainedComposerPublication>>,
    source: Option<Box<syndic_storage::CapturedDraftEditorCandidatePublicationSourceV1>>,
    preparation: Option<PreparedPublication>,
    recovery: Option<(PreparedPublication, RetainedComposerCommandOutcome)>,
    previous_recovery: Option<RetainedComposerCommandOutcome>,
    saved: Option<DraftEditorCandidateSavedCorrespondenceV1>,
    original_correspondence:
        Option<syndic_storage::DraftEditorCandidatePublicationCorrespondenceV1>,
}

impl SyndicComposerHost {
    pub(crate) fn failed_resident_marker_custody_matches(
        &self,
        custody: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> bool {
        self.publication
            .lane
            .as_deref()
            .is_none_or(|lane| match lane {
                ComposerHostPublicationLane::Publication(pending) => {
                    pending.marker_custody_is_captured(custody)
                }
                ComposerHostPublicationLane::Disposal(_) => false,
            })
    }

    pub(crate) fn failed_resident_marker_custody_is_drained(&self) -> bool {
        self.publication
            .lane
            .as_deref()
            .is_none_or(|lane| match lane {
                ComposerHostPublicationLane::Publication(pending) => {
                    !pending.retains_unfinished_marker_authority()
                        && !matches!(
                            pending.stage,
                            PublicationStage::Sealing { .. } | PublicationStage::Releasing { .. }
                        )
                }
                _ => true,
            })
    }

    pub fn retire_failed_resident(
        self: Box<Self>,
        store: &HomeStore,
    ) -> Result<ComposerHostFailedResident, Box<Self>> {
        self.retire_failed_resident_inner(store, None)
    }

    pub fn retire_failed_resident_with_marker_custody(
        self: Box<Self>,
        store: &HomeStore,
        custody: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<ComposerHostFailedResident, Box<Self>> {
        self.retire_failed_resident_inner(store, Some(custody))
    }

    fn retire_failed_resident_inner(
        mut self: Box<Self>,
        store: &HomeStore,
        custody: Option<&crate::composer_marker_seal::DraftMarkerSealRetainedFlights>,
    ) -> Result<ComposerHostFailedResident, Box<Self>> {
        let health = store.health();
        let Some(active) = self.active.as_ref() else {
            return Err(self);
        };
        if health.state() != beryl_home_store::HomeHealthState::Failed
            || health.generation() != Some(active.binding.home_generation())
            || store.home_id() != active.binding.home_id()
            || active.session_disposed
            || !self.pending.is_empty()
            || self.pending_mutation.is_some()
            || !self.detached_mutations.is_empty()
            || self.pending_history.is_some()
            || !self.detached_history.is_empty()
            || self.settlement_custody_in_use() != 0
            || self.submission_pending()
            || self
                .publication
                .lane
                .as_deref()
                .is_some_and(|lane| match lane {
                    ComposerHostPublicationLane::Disposal(_) => true,
                    ComposerHostPublicationLane::Publication(pending) => {
                        if let Some(custody) = custody {
                            !pending.marker_custody_is_captured(custody)
                        } else {
                            pending.retains_unfinished_marker_authority()
                                || matches!(
                                    pending.stage,
                                    PublicationStage::Sealing { .. }
                                        | PublicationStage::Releasing { .. }
                                )
                        }
                    }
                })
        {
            return Err(self);
        }
        let active = self.active.take().unwrap();
        Ok(ComposerHostFailedResident {
            binding: active.binding,
            checkpoint: active.storage_candidate,
            captured_checkpoint: active.storage_candidate,
            activation_checkpoint: active.activation_candidate,
            selector: active.durable_selector,
            thread: active.thread_id,
            canonical_home: store.canonical_path().to_owned(),
            capacity: std::num::NonZeroUsize::new(self.settlement_custody_capacity).unwrap(),
            original: self.publication.retained.take(),
            source: None,
            preparation: None,
            recovery: None,
            previous_recovery: None,
            saved: None,
            original_correspondence: None,
        })
    }
}

impl ComposerHostFailedResident {
    pub fn binding(&self) -> ComposerHostBinding {
        self.binding
    }
    pub fn checkpoint(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.captured_checkpoint
    }
    pub fn selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
    pub fn qualified_publication_checkpoint(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<DraftEditorCandidateActivationBindingV1, String> {
        self.qualify_saved(candidate, storage)?;
        Ok(self
            .saved
            .as_ref()
            .map_or(self.checkpoint, |saved| saved.candidate()))
    }
    pub fn original_known_commit(&self) -> Option<bool> {
        self.original
            .as_ref()
            .and_then(|original| original.outcome.known_commit())
    }
    pub fn recovery_known_commit(&self) -> Option<bool> {
        self.recovery
            .as_ref()
            .and_then(|(_, outcome)| outcome.known_commit())
    }

    fn require_candidate(&self, candidate: &HomeRecoveryCandidate) -> Result<(), String> {
        if candidate.home_id() != self.binding.home_id()
            || candidate.service_reference().canonical_path() != self.canonical_home
            || candidate.generation() == self.binding.home_generation()
        {
            return Err("failed resident requires a fresh exact same-home candidate".into());
        }
        Ok(())
    }

    pub fn qualify_saved(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<bool, String> {
        self.require_candidate(candidate)?;
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        let correspondence = if let Some((prepared, outcome)) = &mut self.recovery {
            if !outcome.reconcile(&access)? {
                return Ok(false);
            }
            Some(
                storage
                    .qualify_published_draft_editor_candidate_candidate(
                        &access,
                        self.checkpoint,
                        &prepared.syndic,
                    )
                    .map_err(|e| e.to_string())?,
            )
        } else if let Some(original) = &mut self.original {
            if original.binding.home_id() != self.binding.home_id()
                || original.binding.home_generation() != self.binding.home_generation()
                || original.binding.candidate().session_id() != self.checkpoint.session_id()
            {
                return Err("original resident publication belongs to another source".into());
            }
            if original.outcome.reconcile(&access)? {
                if original.binding.candidate() == self.captured_checkpoint {
                    Some(
                        storage
                            .qualify_published_draft_editor_candidate_candidate(
                                &access,
                                self.checkpoint,
                                &original.prepared.syndic,
                            )
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    let classified = storage
                        .reconcile_draft_editor_candidate_publication_candidate(
                            &access,
                            &original.prepared.syndic,
                            original.outcome.committed_classification().unwrap(),
                        )
                        .map_err(|e| format!("original historical publication: {e}"))?;
                    let classified_selector = match classified {
                        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(
                            selector,
                            _,
                        ) => selector,
                        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::ExactReplay(
                            receipt,
                        ) => receipt.successor_selector(),
                        _ => {
                            return Err(
                                "original resident publication has no exact correspondence".into(),
                            );
                        }
                    };
                    let correspondence = storage
                        .qualify_retained_draft_editor_candidate_after_publication_candidate(
                            &access,
                            self.captured_checkpoint,
                            &original.prepared.syndic,
                        )
                        .map_err(|e| {
                            format!("retained candidate after original publication: {e}")
                        })?;
                    if correspondence.selector() != classified_selector {
                        return Err("original historical publication selector changed".into());
                    }
                    self.checkpoint = correspondence.candidate();
                    self.selector = correspondence.selector();
                    self.original_correspondence = Some(correspondence);
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let correspondence = match correspondence {
            Some(saved) => saved,
            None => {
                if !storage
                    .draft_editor_candidate_is_saved_candidate(
                        &access,
                        self.checkpoint,
                        self.selector,
                    )
                    .map_err(|e| format!("retained candidate saved correspondence: {e}"))?
                {
                    self.saved = None;
                    return Ok(false);
                }
                storage
                    .qualify_saved_draft_editor_candidate_candidate(
                        &access,
                        self.checkpoint,
                        self.selector,
                    )
                    .map_err(|e| e.to_string())?
            }
        };
        self.saved = Some(correspondence);
        Ok(true)
    }

    pub fn publish_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
        assets: &AssetState,
        operation: DraftPieceOperationIdV1,
        at: SyndicTimestamp,
        evidence: DraftEditorCandidatePublicationEvidenceV1,
        cancellation: CommandCancellation,
    ) -> Result<(), String> {
        if self.recovery.is_some()
            || self.source.is_some()
            || self.preparation.is_some()
            || self.qualify_saved(candidate, storage)?
        {
            return Err("failed resident publication is already saved or attempted".into());
        }
        self.prepare_and_publish_candidate(
            candidate,
            storage,
            assets,
            operation,
            at,
            evidence,
            cancellation,
        )
    }

    #[inline(never)]
    fn prepare_and_publish_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
        assets: &AssetState,
        operation: DraftPieceOperationIdV1,
        at: SyndicTimestamp,
        evidence: DraftEditorCandidatePublicationEvidenceV1,
        cancellation: CommandCancellation,
    ) -> Result<(), String> {
        self.require_candidate(candidate)?;
        if cancellation.is_cancelled() {
            return Err("resident publication cancelled before preparation".into());
        }
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        let revision = access.home_revision().map_err(|e| e.to_string())?;
        self.capture_candidate_publication_source(&access, storage, operation, at)?;
        let asset = super::super::publication::prepare_asset_plan_candidate(
            &access,
            assets,
            syndic_storage::DraftRootHistoryPairV1::new(
                self.checkpoint.root(),
                self.checkpoint.history(),
            ),
            evidence,
        )
        .map_err(|e| e.to_string())?;
        self.prepare_candidate_publication(&access, storage, evidence, asset)?;
        self.execute_candidate_publication(&access, storage, assets, revision, cancellation)
    }

    #[inline(never)]
    fn capture_candidate_publication_source(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &syndic_storage::SyndicStorage,
        operation: DraftPieceOperationIdV1,
        at: SyndicTimestamp,
    ) -> Result<(), String> {
        let source = storage
            .capture_draft_editor_candidate_publication_source_candidate(
                access,
                DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                    self.selector,
                    self.checkpoint,
                    operation,
                    at,
                ),
            )
            .map_err(|e| e.to_string())?;
        self.source = Some(Box::new(source));
        Ok(())
    }

    #[inline(never)]
    fn prepare_candidate_publication(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &syndic_storage::SyndicStorage,
        evidence: DraftEditorCandidatePublicationEvidenceV1,
        asset: super::super::publication::PublicationAssetPlan,
    ) -> Result<(), String> {
        let syndic = match storage.prepare_draft_editor_candidate_publication_candidate(
            access,
            *self.source.take().unwrap(),
            evidence,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                let (source, error) = error.into_parts();
                self.source = Some(Box::new(source));
                return Err(error.to_string());
            }
        };
        self.preparation = Some(PreparedPublication {
            syndic: Box::new(syndic),
            asset,
        });
        Ok(())
    }

    #[inline(never)]
    fn execute_candidate_publication(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
        storage: &syndic_storage::SyndicStorage,
        assets: &AssetState,
        revision: beryl_model::HomeRevision,
        cancellation: CommandCancellation,
    ) -> Result<(), String> {
        let prepared = self.preparation.as_ref().unwrap();
        let mut command = HomeCommand::new(revision).with_cancellation(cancellation);
        command
            .add(
                storage
                    .publish_draft_editor_candidate_candidate(
                        access,
                        storage
                            .revision_candidate(access)
                            .map_err(|e| e.to_string())?,
                        prepared.syndic.as_ref().clone(),
                    )
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        super::super::publication::add_asset_participant_candidate(
            &mut command,
            access,
            assets,
            prepared.asset,
        )
        .map_err(|e| e.to_string())?;
        self.recovery = Some((
            self.preparation.take().unwrap(),
            RetainedComposerCommandOutcome::new(access.execute(command)),
        ));
        self.saved = None;
        Ok(())
    }

    pub fn retry_publication(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
    ) -> Result<(), String> {
        self.require_candidate(candidate)?;
        if self.previous_recovery.is_some() {
            return Err("previous resident save outcome is still retained".into());
        }
        let (_, outcome) = self
            .recovery
            .as_mut()
            .ok_or("resident save has not been attempted")?;
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        if outcome.reconcile(&access)? {
            return Err("committed resident publication cannot be repeated".into());
        }
        let (_, outcome) = self.recovery.take().unwrap();
        self.previous_recovery = Some(outcome);
        self.saved = None;
        Ok(())
    }

    pub fn reconstruct(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: syndic_storage::SyndicStorage,
    ) -> Result<Box<SyndicComposerHost>, String> {
        let result = (|| {
            if !self.qualify_saved(candidate, &storage)? {
                return Err("resident save is not proven".into());
            }
            let (binding, _) = self.reconstruction_facts()?;
            let generation = binding.host_generation();
            let saved = self.saved.as_ref().unwrap();
            let mut host = Box::new(SyndicComposerHost::with_settlement_custody_capacity(
                storage,
                self.capacity,
            ));
            host.active = Some(Box::new(super::super::ActiveComposerHost {
                binding,
                storage_candidate: saved.candidate(),
                activation_candidate: self.activation_checkpoint,
                thread_id: self.thread,
                initial_responses: Vec::new(),
                unavailable: false,
                durable_selector: saved.selector(),
                published_candidate_generation: saved.candidate().candidate_generation(),
                published_pair: syndic_storage::DraftRootHistoryPairV1::new(
                    saved.selector().root(),
                    saved.selector().history(),
                ),
                session_disposed: false,
            }));
            host.last_generation = Some(generation);
            Ok(host)
        })();
        result
    }

    pub(crate) fn reconstruction_facts(
        &self,
    ) -> Result<(ComposerHostBinding, DraftEditorCurrentSelectorV1), String> {
        let saved = self
            .saved
            .as_ref()
            .ok_or("resident saved correspondence is unavailable")?;
        let generation = self
            .binding
            .host_generation()
            .next()
            .ok_or("resident host generation exhausted")?;
        Ok((
            ComposerHostBinding::new(
                saved.home_id(),
                saved.generation(),
                generation,
                saved.candidate(),
                self.binding.presentation_generation(),
            ),
            saved.selector(),
        ))
    }
}
