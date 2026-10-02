use super::*;

impl MainWindowConversationComposerService {
    pub(in crate::main_window) fn failed_resident_marker_custody_matches(
        &self,
        custody: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> bool {
        self.slot.lock().is_ok_and(|slot| {
            slot.selected_host()
                .is_some_and(|host| host.failed_resident_marker_custody_matches(custody))
        })
    }

    pub(in crate::main_window) fn failed_resident_marker_custody_is_drained(&self) -> bool {
        self.slot.lock().is_ok_and(|slot| {
            slot.selected_host()
                .is_some_and(|host| host.failed_resident_marker_custody_is_drained())
        })
    }

    pub(in crate::main_window) fn qualify_published_failed_resident(
        &self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        if self.store.canonical_path() != store.canonical_path()
            || self.store.home_id() != store.home_id()
            || self.selected_identity() != Some(selection)
        {
            return Err("failed resident published home or source changed".into());
        }
        Ok(())
    }
    pub(in crate::main_window) fn qualify_failed_resident_source(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.qualify_failed_resident_home(selection)?;
        if self.selected_identity() != Some(selection) {
            return Err("failed resident selected source changed".into());
        }
        Ok(())
    }

    pub(in crate::main_window) fn qualify_failed_resident_home(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        let health = self.store.health();
        if health.state() != beryl_home_store::HomeHealthState::Failed
            || health.generation() != Some(selection.binding().home_generation())
            || self.store.home_id() != selection.binding().home_id()
        {
            return Err("failed resident home or selected source changed".into());
        }
        Ok(())
    }
    pub fn retire_failed_resident(
        self: Arc<Self>,
    ) -> Result<crate::main_window::MainWindowFailedComposerRetirement, Arc<Self>> {
        self.retire_failed_resident_inner(None)
    }

    pub(crate) fn retire_failed_resident_with_marker_custody(
        self: Arc<Self>,
        custody: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<crate::main_window::MainWindowFailedComposerRetirement, Arc<Self>> {
        self.retire_failed_resident_inner(Some(custody))
    }

    fn retire_failed_resident_inner(
        mut self: Arc<Self>,
        custody: Option<&crate::composer_marker_seal::DraftMarkerSealRetainedFlights>,
    ) -> Result<crate::main_window::MainWindowFailedComposerRetirement, Arc<Self>> {
        let Some(service) = Arc::get_mut(&mut self) else {
            return Err(self);
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
            return Err(self);
        }
        let retirement = service.slot.get_mut().ok().and_then(|slot| match custody {
            Some(custody) => slot.take_failed_resident_with_marker_custody(&service.store, custody),
            None => slot.take_failed_resident(&service.store),
        });
        retirement.ok_or(self)
    }
}
