use super::super::MainWindowRetiredPrepublicationCleanup;
use super::claim_publication::MainWindowComposerClaimWidgetWork;
use super::*;
use beryl_state::WindowClaimSelection;

pub(crate) struct MainWindowThreadCreationRetirementSource {
    pub(crate) prior: MainWindowComposerSelectionIdentity,
    pub(crate) selected: MainWindowComposerSelectionIdentity,
    pub(crate) saved: Option<MainWindowRetiredThreadPredecessorSave>,
    pub(crate) committed_target: Option<WindowClaimSelection>,
    pub(crate) receipt: Option<crate::main_window::MainWindowComposerActivationReceipt>,
    pub(crate) completed_predecessor: Option<MainWindowCompletedThreadPredecessorDisposal>,
    pub(crate) completed_successor: Option<MainWindowCompletedThreadSuccessorCleanup>,
    pub(crate) completed_progress: Option<MainWindowCompletedThreadSuccessorProgress>,
    pub(crate) mounted_successor: Option<MainWindowComposerSelectionIdentity>,
    pub(in crate::main_window) widget_work: Option<MainWindowComposerClaimWidgetWork>,
    pub(crate) release: Option<MainWindowComposerWidgetRelease>,
}

impl MainWindowConversationComposerService {
    pub(crate) fn take_failed_thread_creation_prepublication_cleanup(
        self: &Arc<Self>,
        original: &MainWindowThreadCreationRetirementSource,
    ) -> Result<Vec<MainWindowRetiredPrepublicationCleanup>, String> {
        self.authenticate_failed_thread_creation_selection(
            original.selected,
            original.receipt,
            original.prior,
        )?;
        let mut sources = self
            .native_lineage_sources
            .lock()
            .map_err(|_| "original prepublication source custody is poisoned")?;
        for source in sources.iter() {
            if Arc::strong_count(source) != 1 || Arc::weak_count(source) != 0 {
                return Err(
                    "original prepublication producer or cleanup step is still retained".into(),
                );
            }
            let expected = if source.selection().claim() == original.prior.claim() {
                original.prior
            } else if original.committed_target == Some(source.selection().claim()) {
                original.selected
            } else {
                return Err("original prepublication source belongs to another editor".into());
            };
            source.qualify_cleanup_handoff(expected)?;
        }
        let mut capsules = Vec::with_capacity(sources.len());
        for source in sources.iter_mut() {
            let source = Arc::get_mut(source).expect("qualified sole prepublication source");
            capsules.push(source.take_qualified_cleanup());
        }
        sources.clear();
        self.native_lineage_capacity_epoch
            .fetch_add(1, Ordering::AcqRel);
        Ok(capsules)
    }

    #[cfg(test)]
    pub(crate) fn test_failed_thread_creation_retirement_diagnostics(self: &Arc<Self>) -> String {
        let sources = self.native_lineage_sources.lock();
        let sources = match sources {
            Ok(sources) => sources
                .iter()
                .map(|source| {
                    #[cfg(feature = "test-faults")]
                    {
                        format!(
                            "window={:?},claim={:?},home_generation={:?},session={:?},candidate={},drained={},custody={:?}",
                            source.selection().window_id(),
                            source.selection().claim(),
                            source.selection().binding().home_generation(),
                            source.selection().binding().candidate().session_id(),
                            source.selection().binding().candidate().candidate_generation(),
                            source.drained(),
                            source.diagnostics()
                        )
                    }
                    #[cfg(not(feature = "test-faults"))]
                    {
                        format!(
                            "window={:?},claim={:?},home_generation={:?},session={:?},candidate={},drained={}",
                            source.selection().window_id(),
                            source.selection().claim(),
                            source.selection().binding().home_generation(),
                            source.selection().binding().candidate().session_id(),
                            source.selection().binding().candidate().candidate_generation(),
                            source.drained()
                        )
                    }
                })
                .collect::<Vec<_>>(),
            Err(_) => vec!["poisoned".into()],
        };
        format!(
            "service_strong={},service_weak={},store_strong={},store_weak={},native_driver={},native_sources={:?}",
            Arc::strong_count(self),
            Arc::weak_count(self),
            Arc::strong_count(&self.store),
            Arc::weak_count(&self.store),
            self.native_lineage_driver_started.load(Ordering::Acquire),
            sources
        )
    }

    pub(crate) fn begin_thread_creation_activation(
        &self,
        claim: WindowClaimSelection,
        request: crate::composer_host::ComposerHostActivationRequest,
        retirement_operation_id: syndic_storage::DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
    ) -> Result<crate::main_window::MainWindowComposerActivationAdvance, String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "thread creation activation source lock failed")?
            .begin_thread_creation_activation(
                &self.store,
                claim,
                request,
                retirement_operation_id,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }
    pub(crate) fn take_completed_thread_successor_cleanup(
        &self,
        target: WindowClaimSelection,
    ) -> Result<Option<MainWindowCompletedThreadSuccessorCleanup>, String> {
        self.slot
            .lock()
            .map_err(|_| "completed successor source lock failed")?
            .take_completed_thread_successor_cleanup(target)
    }
    pub(crate) fn settle_completed_thread_successor_cleanup(
        &self,
        cap: MainWindowCompletedThreadSuccessorCleanup,
    ) -> Result<
        MainWindowCompletedThreadSuccessorProgress,
        (MainWindowCompletedThreadSuccessorCleanup, String),
    > {
        let storage = match self.slot.lock() {
            Ok(slot) => slot.clipboard_storage(),
            Err(_) => return Err((cap, "completed successor source lock failed".into())),
        };
        cap.settle_current(&self.store, &storage)
    }
    pub(crate) fn authenticate_failed_thread_creation_selection(
        &self,
        expected: MainWindowComposerSelectionIdentity,
        receipt: Option<crate::main_window::MainWindowComposerActivationReceipt>,
        prior: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.qualify_failed_resident_home(expected)?;
        self.slot
            .lock()
            .map_err(|_| "failed thread creation source lock failed")?
            .authenticate_failed_thread_creation_selection(expected, receipt, prior)
    }

    pub(crate) fn take_completed_thread_predecessor_disposal(
        &self,
        receipt: crate::main_window::MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
    ) -> Result<MainWindowCompletedThreadPredecessorDisposal, String> {
        self.slot
            .lock()
            .map_err(|_| "completed thread predecessor source lock failed")?
            .take_completed_thread_predecessor_disposal(&self.store, receipt, expected)
    }

    pub(crate) fn retire_failed_thread_creation(
        mut self: Arc<Self>,
        mut source: Box<MainWindowThreadCreationRetirementSource>,
        markers: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<
        Box<MainWindowFailedThreadCreationRetirement>,
        (
            Arc<Self>,
            Box<MainWindowThreadCreationRetirementSource>,
            String,
        ),
    > {
        let Some(service) = Arc::get_mut(&mut self) else {
            return Err((
                self,
                source,
                "failed thread creation retains service references".into(),
            ));
        };
        if Arc::get_mut(&mut service.store).is_none()
            || service
                .native_lineage_driver_started
                .load(Ordering::Acquire)
            || !service
                .native_lineage_sources
                .get_mut()
                .is_ok_and(|sources| sources.is_empty())
        {
            return Err((
                self,
                source,
                "failed thread creation retains runtime authorities".into(),
            ));
        }
        let result = service
            .slot
            .get_mut()
            .map_err(|_| "failed thread creation source lock failed".to_owned())
            .and_then(|slot| {
                slot.take_failed_thread_creation(&service.store, &mut source, markers)
            });
        result.map_err(|error| (self, source, error))
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/failed_thread_creation_retirement.rs"
    ));
}
