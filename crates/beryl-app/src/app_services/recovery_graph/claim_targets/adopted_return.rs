use super::*;

impl PreparedRecoveryServiceGraph {
    pub(crate) fn take_adopted_resident_cleanup(
        &mut self,
        source: &crate::main_window::MainWindowFailedResidentCandidateSource,
    ) -> Result<Option<Vec<crate::main_window::MainWindowRetiredPrepublicationCleanup>>, String>
    {
        let (candidate, _) = self
            .services
            .as_mut()
            .and_then(|services| services.cas.as_mut())
            .and_then(|cas| cas.app_preparation_parts())
            .ok_or("adopted cleanup original candidate is unavailable")?;
        source.take_adopted_cleanup(candidate)
    }

    pub(crate) fn authenticate_adopted_resident_return(
        &mut self,
        returned: &crate::main_window::MainWindowFailedResidentAdoptionReturn,
    ) -> Result<
        (
            beryl_state::SessionWindowRecord,
            syndic_storage::DraftEditorCurrentSelectorV1,
        ),
        String,
    > {
        let source = self
            .failed_residents
            .iter()
            .find(|source| source.window == returned.window_id())
            .ok_or("adopted return original graph source is unavailable")?;
        if source.retired.is_some() || source.seed != returned.original_restoration() {
            return Err("adopted return graph already owns the original source".into());
        }
        let (candidate, _) = self
            .services
            .as_mut()
            .and_then(|services| services.cas.as_mut())
            .and_then(|cas| cas.app_preparation_parts())
            .ok_or("adopted return original candidate is unavailable")?;
        returned.authenticate_source(candidate, &self.syndic, &self.state)
    }

    pub(crate) fn prepare_adopted_resident_return(
        &self,
        returned: Box<crate::main_window::MainWindowFailedResidentAdoptionReturn>,
        window: beryl_state::SessionWindowRecord,
        selector: syndic_storage::DraftEditorCurrentSelectorV1,
    ) -> (
        Box<crate::main_window::MainWindowFailedResidentCandidateSource>,
        Box<crate::main_window::MainWindowFailedResidentCapture>,
    ) {
        returned.into_source(self.syndic.clone(), self.state.clone(), window, selector)
    }

    pub(crate) async fn settle_adopted_resident_return(
        &mut self,
        source: &mut crate::main_window::MainWindowFailedResidentCandidateSource,
        capsules: &[crate::main_window::MainWindowRetiredPrepublicationCleanup],
        executor: gpui::BackgroundExecutor,
    ) -> Result<(), String> {
        let (candidate, _) = self
            .services
            .as_mut()
            .and_then(|services| services.cas.as_mut())
            .and_then(|cas| cas.app_preparation_parts())
            .ok_or("adopted return original candidate is unavailable")?;
        source
            .settle_adopted_service(candidate, capsules, executor)
            .await
    }
}
