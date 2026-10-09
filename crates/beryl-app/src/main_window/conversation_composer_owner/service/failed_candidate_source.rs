use super::*;
use crate::main_window::MainWindowFailedComposerRetirement;
use crate::main_window::composer_slot::{dispatch::translate, native_lineage::position};
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeRecoveryCandidate};
use gpui_text_input::{RangePrepublicationEffect, RangeRestorationSeed};

pub struct MainWindowFailedResidentCandidateSource {
    service: Arc<MainWindowConversationComposerService>,
    storage: syndic_storage::SyndicStorage,
    state: beryl_state::BerylState,
    retained: MainWindowFailedComposerRetirement,
    selection: MainWindowComposerSelectionIdentity,
    window: beryl_state::SessionWindowRecord,
    selector: syndic_storage::DraftEditorCurrentSelectorV1,
    seed: RangeRestorationSeed,
    predecessor: RangeRestorationSeed,
}

impl MainWindowFailedResidentCandidateSource {
    pub(in crate::main_window) fn authenticate_adopted_return(
        candidate: &mut HomeRecoveryCandidate,
        retained: &MainWindowFailedComposerRetirement,
        service: &MainWindowConversationComposerService,
        selection: MainWindowComposerSelectionIdentity,
        storage: &syndic_storage::SyndicStorage,
        state: &beryl_state::BerylState,
        predecessor: RangeRestorationSeed,
    ) -> Result<
        (
            beryl_state::SessionWindowRecord,
            syndic_storage::DraftEditorCurrentSelectorV1,
        ),
        String,
    > {
        retained.qualify_adopted_return(selection)?;
        if predecessor.binding != retained.selection().binding().range_binding()
            || predecessor.history != Some(retained.selection().binding().range_history_frontier())
        {
            return Err("adopted return original predecessor seed changed".into());
        }
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        if access.home_id() != selection.binding().home_id()
            || access.generation() != selection.binding().home_generation()
            || service.selected_identity() != Some(selection)
        {
            return Err("adopted return current candidate identity changed".into());
        }
        let window = selection
            .validate_candidate_claim(&access, state)
            .map_err(|error| error.to_string())?;
        let selector = retained.saved_selector()?;
        if !storage
            .draft_editor_candidate_is_saved_candidate(
                &access,
                selection.binding().candidate(),
                selector,
            )
            .map_err(|error| error.to_string())?
        {
            return Err("adopted return current saved source changed".into());
        }
        Ok((window, selector))
    }

    #[inline(never)]
    pub(in crate::main_window) fn from_adopted(
        retained: Box<MainWindowFailedComposerRetirement>,
        service: Arc<MainWindowConversationComposerService>,
        selection: MainWindowComposerSelectionIdentity,
        storage: syndic_storage::SyndicStorage,
        state: beryl_state::BerylState,
        predecessor: RangeRestorationSeed,
        window: beryl_state::SessionWindowRecord,
        selector: syndic_storage::DraftEditorCurrentSelectorV1,
    ) -> Box<Self> {
        Box::new(Self {
            service,
            storage,
            state,
            retained: *retained,
            selection,
            window,
            selector,
            seed: RangeRestorationSeed {
                binding: selection.binding().range_binding(),
                history: Some(selection.binding().range_history_frontier()),
                ..predecessor
            },
            predecessor,
        })
    }

    pub fn new(
        candidate: &mut HomeRecoveryCandidate,
        retired: MainWindowFailedComposerRetirement,
        storage: syndic_storage::SyndicStorage,
        state: &beryl_state::BerylState,
        seed: RangeRestorationSeed,
        restored: Option<&beryl_state::SessionWindowRemovalEvidence>,
    ) -> Result<Self, (MainWindowFailedComposerRetirement, String)> {
        let binding = retired.selection().binding();
        if seed.binding != binding.range_binding()
            || seed.history != Some(binding.range_history_frontier())
            || seed.selection.head != seed.caret
        {
            return Err((retired, "failed resident predecessor seed changed".into()));
        }
        let positions = (|| {
            let access = candidate.recovery_access().map_err(|e| e.to_string())?;
            validate_positions(&access, &storage, binding, seed)
        })();
        if let Err(error) = positions {
            return Err((retired, error));
        }
        let (slot, window, retired) =
            retired.reconstruct(candidate, storage.clone(), state, restored)?;
        let service = Arc::new(MainWindowConversationComposerService::from_boxed_slot(
            candidate.service_reference(),
            slot,
        ));
        let selection = service.selected_identity().unwrap();
        let selector = retired.saved_selector().unwrap();
        let predecessor = seed;
        let seed = RangeRestorationSeed {
            binding: selection.binding().range_binding(),
            history: Some(selection.binding().range_history_frontier()),
            ..seed
        };
        Ok(Self {
            service,
            storage,
            state: state.clone(),
            retained: retired,
            selection,
            window,
            selector,
            seed,
            predecessor,
        })
    }

    pub fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }
    pub fn seed(&self) -> RangeRestorationSeed {
        self.seed
    }
    pub fn predecessor(&self) -> RangeRestorationSeed {
        self.predecessor
    }
    pub fn predecessor_selection(&self) -> MainWindowComposerSelectionIdentity {
        self.retained.selection()
    }
    pub fn window(&self) -> &beryl_state::SessionWindowRecord {
        &self.window
    }
    pub fn original_known_commit(&self) -> Option<bool> {
        self.retained.original_known_commit()
    }
    pub fn recovery_known_commit(&self) -> Option<bool> {
        self.retained.recovery_known_commit()
    }

    pub(crate) fn into_resources(
        self,
    ) -> (
        Arc<MainWindowConversationComposerService>,
        MainWindowFailedComposerRetirement,
    ) {
        (self.service, self.retained)
    }

    pub(crate) async fn dispose_cancelled_service(
        mut self: Box<Self>,
        candidate: &mut HomeRecoveryCandidate,
        executor: gpui::BackgroundExecutor,
    ) -> Result<Box<MainWindowFailedComposerRetirement>, (Box<Self>, String)> {
        if let Err(error) = self.settle_cancelled_service(candidate, executor).await {
            return Err((self, error));
        }
        Ok(self.take_cancelled_retirement())
    }

    pub(crate) fn take_adopted_cleanup(
        &self,
        candidate: &mut HomeRecoveryCandidate,
    ) -> Result<Option<Vec<crate::main_window::MainWindowRetiredPrepublicationCleanup>>, String>
    {
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        self.validate_source(&access)?;
        self.service
            .take_adopted_prepublication_cleanup(&self.retained, self.selection)
    }

    pub(crate) async fn settle_adopted_service(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        capsules: &[crate::main_window::MainWindowRetiredPrepublicationCleanup],
        executor: gpui::BackgroundExecutor,
    ) -> Result<(), String> {
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        self.validate_source(&access)?;
        self.retained.qualify_adopted_return(self.selection)?;
        if capsules
            .iter()
            .any(|capsule| capsule.selection() != self.selection)
        {
            return Err("adopted cleanup capsule source changed".into());
        }
        drop(access);
        if !self
            .service
            .request_native_lineage_cleanup_driver_stop_after_handoff()
        {
            return Err("adopted cleanup driver stop custody is still retained".into());
        }
        self.settle_cancelled_service(candidate, executor).await
    }

    pub(crate) async fn settle_cancelled_service(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        executor: gpui::BackgroundExecutor,
    ) -> Result<(), String> {
        loop {
            self.service.drive_native_lineage_cleanup_sources();
            let drained = self
                .service
                .native_lineage_sources
                .lock()
                .map(|sources| sources.is_empty())
                .unwrap_or(false);
            if drained
                && !self
                    .service
                    .native_lineage_driver_started
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                break;
            }
            executor.timer(std::time::Duration::from_millis(50)).await;
        }
        let settled = (|| -> Result<(), String> {
            let access = candidate
                .recovery_access()
                .map_err(|error| error.to_string())?;
            self.validate_source(&access)?;
            drop(access);
            if self
                .service
                .window_close
                .lock()
                .map_err(|_| "cancelled candidate close custody is poisoned")?
                .is_some()
            {
                return Err("cancelled candidate still owns window-close custody".into());
            }
            let service = Arc::get_mut(&mut self.service)
                .ok_or("cancelled candidate service still has aliases")?;
            let slot = service
                .slot
                .get_mut()
                .map_err(|_| "cancelled candidate slot custody is poisoned")?;
            self.retained.settle_cancelled_reconstruction(
                candidate,
                &self.storage,
                &self.state,
                slot,
            )
        })();
        settled
    }

    #[inline(never)]
    pub(crate) fn take_cancelled_retirement(
        self: Box<Self>,
    ) -> Box<MainWindowFailedComposerRetirement> {
        let Self {
            service, retained, ..
        } = *self;
        drop(service);
        Box::new(retained)
    }

    fn validate_source(&self, access: &HomeCandidateRecoveryAccess<'_>) -> Result<(), String> {
        let binding = self.selection.binding();
        if access.home_id() != binding.home_id()
            || access.generation() != binding.home_generation()
            || self.service.selected_identity() != Some(self.selection)
        {
            return Err("failed resident candidate source is stale".into());
        }
        let window = self
            .selection
            .validate_candidate_claim(access, &self.state)
            .map_err(|e| e.to_string())?;
        if window != self.window
            || !self
                .storage
                .draft_editor_candidate_is_saved_candidate(
                    access,
                    binding.candidate(),
                    self.selector,
                )
                .map_err(|e| e.to_string())?
        {
            return Err("failed resident candidate correspondence changed".into());
        }
        Ok(())
    }

    pub(crate) fn read(
        candidate: &mut HomeRecoveryCandidate,
        source: &Self,
        effect: &RangePrepublicationEffect,
    ) -> Result<MainWindowComposerCandidateRead, String> {
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        source.validate_source(&access)?;
        match effect {
            RangePrepublicationEffect::ValidateOwner(request) => {
                if request.binding != source.seed.binding || request.history != source.seed.history
                {
                    return Err("failed resident validation request is stale".into());
                }
                validate_positions(
                    &access,
                    &source.storage,
                    source.selection.binding(),
                    source.seed,
                )?;
                Ok(MainWindowComposerCandidateRead::Validation(
                    gpui_text_input::RangePrepublicationValidationResponse {
                        key: request.key,
                        binding: request.binding,
                        history: request.history,
                        current: true,
                    },
                ))
            }
            RangePrepublicationEffect::Page { request, .. } => translate::candidate_text_page(
                &source.storage,
                &access,
                source.selection.binding(),
                *request,
            )
            .map(MainWindowComposerCandidateRead::Page)
            .map_err(|e| e.to_string()),
            RangePrepublicationEffect::ObjectPage { request, .. } => {
                translate::candidate_object_page(
                    &source.storage,
                    &access,
                    source.selection.binding(),
                    *request,
                )
                .map(MainWindowComposerCandidateRead::ObjectPage)
                .map_err(|e| e.to_string())
            }
        }
    }
}

impl MainWindowComposerPrepublicationSource for MainWindowFailedResidentCandidateSource {
    fn seed(&self) -> RangeRestorationSeed {
        self.seed
    }
    fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }
    fn retain_cleanup(
        &self,
        cleanup: Arc<MainWindowNativeLineagePrepublicationSource>,
        executor: BackgroundExecutor,
    ) -> Result<(), String> {
        self.service
            .retain_native_lineage_source(cleanup, executor)
            .map_err(|e| match e {
                MainWindowNativeLineageSourceRetentionError::CapacityFull { .. } => {
                    "failed resident adoption cleanup capacity is full".into()
                }
                MainWindowNativeLineageSourceRetentionError::Failed(error) => error,
            })
    }
}

fn validate_positions(
    access: &HomeCandidateRecoveryAccess<'_>,
    storage: &syndic_storage::SyndicStorage,
    binding: crate::composer_host::ComposerHostBinding,
    seed: RangeRestorationSeed,
) -> Result<(), String> {
    let restoration = syndic_storage::DraftPieceRestorationV1::new(
        binding.root(),
        binding.history(),
        position(seed.caret).map_err(|e| e.to_string())?,
        position(seed.selection.anchor).map_err(|e| e.to_string())?,
        position(seed.scroll.position).map_err(|e| e.to_string())?,
    );
    storage
        .validate_draft_piece_restoration_candidate(access, restoration)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
