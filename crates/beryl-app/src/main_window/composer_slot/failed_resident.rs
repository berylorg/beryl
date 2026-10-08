use super::*;
use crate::composer_host::ComposerHostFailedResident;
use crate::main_window::MainWindowComposerDraftState;
use beryl_home_store::HomeRecoveryCandidate;

pub struct MainWindowFailedComposerRetirement {
    selection: MainWindowComposerSelectionIdentity,
    host: Option<Box<ComposerHostFailedResident>>,
    last_activation_generation: u64,
    reconstructed: Option<MainWindowComposerSelectionIdentity>,
}

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn take_failed_resident(
        &mut self,
        store: &HomeStore,
    ) -> Option<MainWindowFailedComposerRetirement> {
        self.take_failed_resident_inner(store, None)
    }

    pub(in crate::main_window) fn take_failed_resident_with_marker_custody(
        &mut self,
        store: &HomeStore,
        custody: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Option<MainWindowFailedComposerRetirement> {
        self.take_failed_resident_inner(store, Some(custody))
    }

    fn take_failed_resident_inner(
        &mut self,
        store: &HomeStore,
        custody: Option<&crate::composer_marker_seal::DraftMarkerSealRetainedFlights>,
    ) -> Option<MainWindowFailedComposerRetirement> {
        if self.disposed
            || self.pending.is_some()
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self.native_lineage_suspension.is_some()
            || self.selected.as_ref().is_none_or(|selected| {
                !selected.dispatcher.is_drained()
                    || selected.host.binding() != Some(selected.identity.binding())
                    || selected.dispatcher.binding != selected.identity.binding()
            })
        {
            return None;
        }
        let SelectedComposer {
            identity,
            dispatcher,
            draft_state,
            host,
        } = self.selected.take().unwrap();
        let result = match custody {
            Some(custody) => {
                Box::new(host).retire_failed_resident_with_marker_custody(store, custody)
            }
            None => Box::new(host).retire_failed_resident(store),
        };
        match result {
            Ok(host) => {
                self.disposed = true;
                Some(MainWindowFailedComposerRetirement {
                    selection: identity,
                    host: Some(Box::new(host)),
                    last_activation_generation: self.last_activation_generation,
                    reconstructed: None,
                })
            }
            Err(host) => {
                self.selected = Some(SelectedComposer {
                    identity,
                    dispatcher,
                    draft_state,
                    host: *host,
                });
                None
            }
        }
    }
}

impl MainWindowFailedComposerRetirement {
    pub(crate) fn from_failed_thread_creation(
        selection: MainWindowComposerSelectionIdentity,
        host: ComposerHostFailedResident,
        last_activation_generation: u64,
    ) -> Self {
        Self {
            selection,
            host: Some(Box::new(host)),
            last_activation_generation,
            reconstructed: None,
        }
    }
    pub(crate) fn prior_selector(&self) -> syndic_storage::DraftEditorCurrentSelectorV1 {
        self.host.as_ref().unwrap().selector()
    }

    pub fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }
    pub fn original_known_commit(&self) -> Option<bool> {
        self.host.as_ref().unwrap().original_known_commit()
    }
    pub fn recovery_known_commit(&self) -> Option<bool> {
        self.host.as_ref().unwrap().recovery_known_commit()
    }

    pub(in crate::main_window) fn saved_selector(
        &self,
    ) -> Result<syndic_storage::DraftEditorCurrentSelectorV1, String> {
        self.host
            .as_ref()
            .unwrap()
            .reconstruction_facts()
            .map(|(_, selector)| selector)
    }

    pub fn qualify_member(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        state: &beryl_state::BerylState,
        restored: Option<&beryl_state::SessionWindowRemovalEvidence>,
    ) -> Result<beryl_state::SessionWindowRecord, String> {
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        if access.home_id() != self.selection.binding().home_id()
            || access.generation() == self.selection.binding().home_generation()
        {
            return Err("failed resident membership requires fresh same-home access".into());
        }
        if let Some(evidence) = restored {
            if evidence.window().window_id() != self.selection.window_id()
                || evidence.window().selected_thread() != Some(self.selection.claim())
                || state
                    .session()
                    .classify_window_removal_candidate(&access, evidence)
                    .map_err(|e| e.to_string())?
                    != beryl_state::SessionWindowRemovalState::Recovered
            {
                return Err("failed resident restoration correspondence is unavailable".into());
            }
            let bootstrap = state
                .session()
                .minimal_bootstrap_candidate(&access)
                .map_err(|e| e.to_string())?
                .ok_or("failed resident membership is unavailable")?;
            let window = bootstrap
                .windows()
                .iter()
                .find(|window| window.window_id() == self.selection.window_id())
                .ok_or("failed resident restored window is missing")?;
            let claim = window
                .selected_thread()
                .ok_or("failed resident restored claim is missing")?;
            let paired = state
                .session()
                .window_claim_catalog_source_candidate(&access, self.selection.window_id())
                .map_err(|e| e.to_string())?;
            if !paired.claim().is_some_and(|paired| {
                paired.thread_id() == claim.thread_id()
                    && paired.generation() == claim.generation()
                    && paired.revision() == claim.revision()
                    && paired.state() == beryl_state::ThreadClaimState::Active
            }) {
                return Err("failed resident restored paired claim changed".into());
            }
            Ok(window.clone())
        } else {
            self.selection
                .validate_candidate_claim(&access, state)
                .map_err(|e| e.to_string())
        }
    }

    pub fn qualify_saved(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &SyndicStorage,
        state: &beryl_state::BerylState,
        restored: Option<&beryl_state::SessionWindowRemovalEvidence>,
    ) -> Result<bool, String> {
        self.qualify_member(candidate, state, restored)?;
        self.host
            .as_mut()
            .unwrap()
            .qualify_saved(candidate, storage)
    }

    pub fn qualified_publication_checkpoint(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &SyndicStorage,
        state: &beryl_state::BerylState,
        restored: Option<&beryl_state::SessionWindowRemovalEvidence>,
    ) -> Result<syndic_storage::DraftEditorCandidateActivationBindingV1, String> {
        self.qualify_member(candidate, state, restored)?;
        self.host
            .as_mut()
            .unwrap()
            .qualified_publication_checkpoint(candidate, storage)
    }

    pub fn publish_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &SyndicStorage,
        state: &beryl_state::BerylState,
        restored: Option<&beryl_state::SessionWindowRemovalEvidence>,
        operation: DraftPieceOperationIdV1,
        at: syndic_storage::SyndicTimestamp,
        evidence: syndic_storage::DraftEditorCandidatePublicationEvidenceV1,
        cancellation: CommandCancellation,
    ) -> Result<(), String> {
        self.qualify_member(candidate, state, restored)?;
        self.host.as_mut().unwrap().publish_candidate(
            candidate,
            storage,
            &state.assets(),
            operation,
            at,
            evidence,
            cancellation,
        )
    }

    pub fn retry_publication(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
    ) -> Result<(), String> {
        self.host.as_mut().unwrap().retry_publication(candidate)
    }

    pub fn reconstruct(
        mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: SyndicStorage,
        state: &beryl_state::BerylState,
        restored: Option<&beryl_state::SessionWindowRemovalEvidence>,
    ) -> Result<
        (
            Box<MainWindowComposerSlot>,
            beryl_state::SessionWindowRecord,
            Self,
        ),
        (Self, String),
    > {
        if self.reconstructed.is_some() {
            return Err((
                self,
                "failed resident reconstruction is already owned".into(),
            ));
        }
        let window = match self.qualify_member(candidate, state, restored) {
            Ok(window) => window,
            Err(error) => return Err((self, error)),
        };
        let facts = (|| {
            let retained = self.host.as_mut().unwrap();
            if !retained.qualify_saved(candidate, &storage)? {
                return Err("resident save is not proven".to_owned());
            }
            let (binding, selector) = retained.reconstruction_facts()?;
            let draft_state = MainWindowComposerDraftState::new(
                binding,
                binding.candidate().candidate_generation(),
                syndic_storage::DraftRootHistoryPairV1::new(selector.root(), selector.history()),
            )
            .map_err(|e| format!("resident draft state is invalid: {e:?}"))?;
            Ok((binding, draft_state))
        })();
        let (binding, draft_state) = match facts {
            Ok(facts) => facts,
            Err(error) => return Err((self, error)),
        };
        let host = match self
            .host
            .as_mut()
            .unwrap()
            .reconstruct(candidate, storage.clone())
        {
            Ok(host) => host,
            Err(error) => return Err((self, error)),
        };
        let mut slot = Box::new(MainWindowComposerSlot::from_selected(
            self.selection.window_id(),
            storage,
            MainWindowComposerMarkerMetadataAuthority::new(state.assets()),
            SelectedComposer {
                identity: MainWindowComposerSelectionIdentity {
                    window_id: self.selection.window_id(),
                    claim: window.selected_thread().unwrap(),
                    binding,
                },
                dispatcher: MainWindowComposerDispatcher::new(binding),
                draft_state,
                host: *host,
            },
        ));
        slot.last_activation_generation = self.last_activation_generation;
        self.reconstructed = slot.selected_identity();
        Ok((slot, window, self))
    }

    pub(crate) fn settle_cancelled_reconstruction(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &SyndicStorage,
        state: &beryl_state::BerylState,
        slot: &mut MainWindowComposerSlot,
    ) -> Result<(), String> {
        let expected = self
            .reconstructed
            .ok_or("original resident reconstruction is not owned")?;
        if !self
            .host
            .as_mut()
            .unwrap()
            .qualify_saved(candidate, storage)?
        {
            return Err("cancelled resident reconstruction original save is not proven".into());
        }
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        if access.home_id() != expected.binding().home_id()
            || access.generation() != expected.binding().home_generation()
            || expected.window_id() != self.selection.window_id()
            || slot.window_id != expected.window_id()
            || slot.selected_identity() != Some(expected)
            || slot.disposed
            || slot.pending.is_some()
            || slot.disposal_stage.is_some()
            || slot.submission_successor.is_some()
            || slot.thread_predecessor_save.is_some()
            || slot.failed_thread_successor.is_some()
            || slot.completed_thread_successor.is_some()
            || slot.capture_thread_cleanup
            || slot.native_lineage_suspension.is_some()
            || slot.window_close.is_some()
            || slot.last_activation_generation != self.last_activation_generation
        {
            return Err("cancelled resident reconstruction correspondence changed".into());
        }
        expected
            .validate_candidate_claim(&access, state)
            .map_err(|error| error.to_string())?;
        let (binding, selector) = self.host.as_ref().unwrap().reconstruction_facts()?;
        if binding != expected.binding()
            || !storage
                .draft_editor_candidate_is_saved_candidate(&access, binding.candidate(), selector)
                .map_err(|error| error.to_string())?
        {
            return Err("cancelled resident reconstruction saved source changed".into());
        }
        let selected = slot.selected.as_mut().unwrap();
        let published = selected.draft_state.published();
        if selected.dispatcher.binding != binding
            || !selected.dispatcher.is_drained()
            || selected.draft_state.adopted() != binding
            || published.candidate_generation() != binding.candidate().candidate_generation()
            || published.root() != selector.root()
            || published.history() != selector.history()
        {
            return Err("cancelled resident reconstruction dispatcher is retained".into());
        }
        selected
            .host
            .retire_cancelled_failed_resident_reconstruction(binding, selector)?;
        slot.selected.take();
        slot.disposed = true;
        self.reconstructed = None;
        Ok(())
    }
}
