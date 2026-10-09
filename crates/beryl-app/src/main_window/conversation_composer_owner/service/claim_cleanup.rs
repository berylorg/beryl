use super::super::MainWindowRetiredPrepublicationCleanup;
use super::claim_publication::MainWindowComposerClaimWidgetWork;
use super::*;
use beryl_state::WindowClaimSelection;

pub(crate) struct MainWindowClaimRetirementSource {
    pub(crate) kind: MainWindowClaimRetirementKind,
    pub(crate) prior: MainWindowComposerSelectionIdentity,
    pub(crate) selected: MainWindowComposerSelectionIdentity,
    pub(crate) saved: Option<MainWindowRetiredClaimPredecessorSave>,
    pub(crate) committed_target: Option<WindowClaimSelection>,
    pub(crate) planned_target: WindowClaimSelection,
    pub(crate) receipt: Option<crate::main_window::MainWindowComposerActivationReceipt>,
    pub(crate) completed_predecessor: Option<MainWindowCompletedThreadPredecessorDisposal>,
    pub(crate) completed_successor: Option<MainWindowCompletedThreadSuccessorCleanup>,
    pub(crate) completed_progress: Option<MainWindowCompletedThreadSuccessorProgress>,
    pub(crate) mounted_successor: Option<MainWindowComposerSelectionIdentity>,
    pub(in crate::main_window) widget_work: Option<MainWindowComposerClaimWidgetWork>,
    pub(crate) release: Option<MainWindowComposerWidgetRelease>,
    pub(crate) successor_release: Option<MainWindowComposerWidgetRelease>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MainWindowClaimRetirementKind {
    ThreadCreation,
    OrdinarySelection,
}

impl MainWindowConversationComposerService {
    fn qualify_final_prepublication_cleanup(
        &self,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        flush: crate::composer_host::ComposerHostFlushTicket,
        selected: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        let health = self.store.health();
        if health.state() != beryl_home_store::HomeHealthState::Healthy
            || health.generation() != Some(selected.binding().home_generation())
            || self.store.home_id() != selected.binding().home_id()
            || !close.matches_editor(selected)
            || !self.window_close_is_current(close)
            || !self
                .slot
                .lock()
                .is_ok_and(|slot| slot.final_cleanup_close_is_current(close, flush, selected))
        {
            return Err("final cleanup Healthy close or selected source changed".into());
        }
        Ok(())
    }

    pub(in crate::main_window) fn take_final_prepublication_cleanup(
        &self,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        flush: crate::composer_host::ComposerHostFlushTicket,
        selected: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<Vec<MainWindowRetiredPrepublicationCleanup>>, String> {
        self.qualify_final_prepublication_cleanup(close, flush, selected)?;
        let mut sources = self
            .native_lineage_sources
            .lock()
            .map_err(|_| "final prepublication source custody is poisoned")?;
        for source in sources.iter() {
            if Arc::strong_count(source) != 1 || Arc::weak_count(source) != 0 {
                return Ok(None);
            }
            source.qualify_cleanup_handoff(selected)?;
        }
        let mut capsules = Vec::with_capacity(sources.len());
        for source in sources.iter_mut() {
            capsules.push(
                Arc::get_mut(source)
                    .expect("qualified sole final source")
                    .take_qualified_cleanup(),
            );
        }
        sources.clear();
        self.native_lineage_capacity_epoch
            .fetch_add(1, Ordering::AcqRel);
        drop(sources);
        Ok(Some(capsules))
    }

    pub(in crate::main_window) fn stop_final_prepublication_cleanup(
        &self,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        flush: crate::composer_host::ComposerHostFlushTicket,
        selected: MainWindowComposerSelectionIdentity,
        capsules: &[MainWindowRetiredPrepublicationCleanup],
    ) -> Result<bool, String> {
        self.qualify_final_prepublication_cleanup(close, flush, selected)?;
        if capsules
            .iter()
            .any(|capsule| !same_cleanup_owner(capsule.selection(), selected))
        {
            return Err("final cleanup capsule owner changed".into());
        }
        Ok(self.request_native_lineage_cleanup_driver_stop_after_handoff())
    }

    pub(in crate::main_window) fn take_failed_resident_prepublication_cleanup(
        &self,
        selected: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<Vec<MainWindowRetiredPrepublicationCleanup>>, String> {
        self.qualify_failed_resident_source(selected)?;
        let mut sources = self
            .native_lineage_sources
            .lock()
            .map_err(|_| "failed resident prepublication source custody is poisoned")?;
        for source in sources.iter() {
            if Arc::strong_count(source) != 1 || Arc::weak_count(source) != 0 {
                return Ok(None);
            }
            source.qualify_cleanup_handoff(selected)?;
        }
        let mut capsules = Vec::with_capacity(sources.len());
        for source in sources.iter_mut() {
            capsules.push(
                Arc::get_mut(source)
                    .expect("qualified sole failed resident source")
                    .take_qualified_cleanup(),
            );
        }
        sources.clear();
        self.native_lineage_capacity_epoch
            .fetch_add(1, Ordering::AcqRel);
        drop(sources);
        Ok(Some(capsules))
    }

    pub(in crate::main_window) fn stop_failed_resident_prepublication_cleanup(
        &self,
        selected: MainWindowComposerSelectionIdentity,
        capsules: &[MainWindowRetiredPrepublicationCleanup],
    ) -> Result<bool, String> {
        self.qualify_failed_resident_source(selected)?;
        if capsules
            .iter()
            .any(|capsule| !same_cleanup_owner(capsule.selection(), selected))
        {
            return Err("failed resident cleanup capsule owner changed".into());
        }
        Ok(self.request_native_lineage_cleanup_driver_stop_after_handoff())
    }

    pub(in crate::main_window::conversation_composer_owner) fn take_adopted_prepublication_cleanup(
        self: &Arc<Self>,
        original: &crate::main_window::MainWindowFailedComposerRetirement,
        selected: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<Vec<MainWindowRetiredPrepublicationCleanup>>, String> {
        original.qualify_adopted_return(selected)?;
        if self.selected_identity() != Some(selected) {
            return Err("adopted cleanup current source identity changed".into());
        }
        let mut sources = self
            .native_lineage_sources
            .lock()
            .map_err(|_| "adopted prepublication source custody is poisoned")?;
        for source in sources.iter() {
            if Arc::strong_count(source) != 1 || Arc::weak_count(source) != 0 {
                return Ok(None);
            }
            source.qualify_cleanup_handoff(selected)?;
        }
        let mut capsules = Vec::with_capacity(sources.len());
        for source in sources.iter_mut() {
            capsules.push(
                Arc::get_mut(source)
                    .expect("qualified sole adopted source")
                    .take_qualified_cleanup(),
            );
        }
        sources.clear();
        self.native_lineage_capacity_epoch
            .fetch_add(1, Ordering::AcqRel);
        drop(sources);
        let _ = self.request_native_lineage_cleanup_driver_stop_after_handoff();
        Ok(Some(capsules))
    }

    pub(crate) fn take_failed_claim_prepublication_cleanup(
        self: &Arc<Self>,
        original: &MainWindowClaimRetirementSource,
    ) -> Result<Vec<MainWindowRetiredPrepublicationCleanup>, String> {
        self.authenticate_failed_claim_cleanup_selection(
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
            } else if original.planned_target == source.selection().claim()
                && original.mounted_successor == Some(source.selection())
            {
                source.selection()
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
        drop(sources);
        let _ = self.request_native_lineage_cleanup_driver_stop_after_handoff();
        Ok(capsules)
    }

    #[cfg(test)]
    pub(crate) fn test_failed_claim_retirement_diagnostics(self: &Arc<Self>) -> String {
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
            "service_strong={},service_weak={},store_strong={},store_weak={},native_driver={},native_sources={:?},driver_progress=({})",
            Arc::strong_count(self),
            Arc::weak_count(self),
            Arc::strong_count(&self.store),
            Arc::weak_count(&self.store),
            self.native_lineage_driver_started.load(Ordering::Acquire),
            sources,
            self.test_native_lineage_driver_progress()
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
    pub(crate) fn authenticate_failed_claim_cleanup_selection(
        &self,
        expected: MainWindowComposerSelectionIdentity,
        receipt: Option<crate::main_window::MainWindowComposerActivationReceipt>,
        prior: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.qualify_failed_resident_home(expected)?;
        self.slot
            .lock()
            .map_err(|_| "failed thread creation source lock failed")?
            .authenticate_failed_claim_cleanup_selection(expected, receipt, prior)
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

    pub(crate) fn retire_failed_claim_cleanup(
        mut self: Arc<Self>,
        mut source: Box<MainWindowClaimRetirementSource>,
        markers: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<
        Box<MainWindowFailedClaimRetirement>,
        (Arc<Self>, Box<MainWindowClaimRetirementSource>, String),
    > {
        if let Err(error) = self.authenticate_failed_claim_cleanup_selection(
            source.selected,
            source.receipt,
            source.prior,
        ) {
            return Err((self, source, error));
        }
        let _ = self.request_native_lineage_cleanup_driver_stop_after_handoff();
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
            .and_then(|slot| slot.take_failed_claim_cleanup(&service.store, &mut source, markers));
        result.map_err(|error| (self, source, error))
    }
}

fn same_cleanup_owner(
    actual: MainWindowComposerSelectionIdentity,
    expected: MainWindowComposerSelectionIdentity,
) -> bool {
    let actual_binding = actual.binding();
    let expected_binding = expected.binding();
    actual.window_id() == expected.window_id()
        && actual.claim() == expected.claim()
        && actual_binding.home_id() == expected_binding.home_id()
        && actual_binding.home_generation() == expected_binding.home_generation()
        && actual_binding.host_generation() == expected_binding.host_generation()
        && actual_binding.candidate().draft_id() == expected_binding.candidate().draft_id()
        && actual_binding.candidate().session_id() == expected_binding.candidate().session_id()
        && actual_binding.candidate().session_generation()
            == expected_binding.candidate().session_generation()
        && actual_binding.presentation_generation() == expected_binding.presentation_generation()
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/failed_thread_creation_retirement.rs"
    ));
}
