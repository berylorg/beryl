use super::*;
use crate::{
    activity_service::PreparedActivityService,
    cas_projection::{
        PreparedRecoveryCasServices, RecoveryCasCloseFailure, RecoveryCasPreparationFailure,
    },
    composer_marker_seal::initial_preparation::PreparedMarkerServices,
};

pub(crate) struct PreparedRecoveryAppServices {
    pub(super) cas: Option<PreparedRecoveryCasServices>,
    pub(super) catalog_source: Option<CatalogSourceCoordinator>,
    pub(super) marker: Option<PreparedMarkerServices>,
    pub(super) activity: Option<PreparedActivityService>,
    pub(super) theme: Option<PreparedThemeRuntime>,
}

pub(crate) struct RecoveryAppServicePreparationFailure {
    error: AppServiceOpenError,
    cas: RecoveryCasPreparationFailure,
}

#[derive(Debug)]
pub(crate) struct RecoveryAppServiceCloseFailure {
    _preparation: AppServiceOpenError,
    _cas: RecoveryCasCloseFailure,
}

impl std::fmt::Debug for RecoveryAppServicePreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryAppServicePreparationFailure")
            .field("error", &self.error)
            .field("cas", &self.cas.error())
            .finish_non_exhaustive()
    }
}

impl RecoveryAppServicePreparationFailure {
    pub(crate) fn retry_home_custody(&mut self) -> Option<&mut Option<HomeStore>> {
        self.cas.retry_home_custody()
    }

    pub(crate) fn close(self) -> Result<(), RecoveryAppServiceCloseFailure> {
        self.cas
            .close()
            .map_err(|cas| RecoveryAppServiceCloseFailure {
                _preparation: self.error,
                _cas: cas,
            })
    }

    pub(crate) fn error(&self) -> &AppServiceOpenError {
        &self.error
    }

    pub(crate) fn into_retry_parts(
        self,
    ) -> Result<(HomeStore, AppServiceOpenError, CasPreparationError), Self> {
        match self.cas.into_retry_parts() {
            Ok((home, cas)) => Ok((home, self.error, cas)),
            Err(cas) => Err(Self {
                error: self.error,
                cas,
            }),
        }
    }
}

impl PreparedRecoveryAppServices {
    pub(super) fn composer_recovery_read(
        &mut self,
        source: &crate::main_window::MainWindowComposerCandidateSource,
        effect: &gpui_text_input::RangePrepublicationEffect,
    ) -> Result<crate::main_window::MainWindowComposerCandidateRead, String> {
        let (candidate, _) = self
            .cas
            .as_mut()
            .expect("prepared recovery CAS custody")
            .app_preparation_parts()
            .ok_or("recovery handoff is unavailable")?;
        source.read_prepublication(candidate, effect)
    }

    pub(super) fn composer_recovery_source(
        &mut self,
        state: &BerylState,
        syndic: SyndicStorage,
        retired: crate::main_window::MainWindowComposerRetiredClose,
        seed: gpui_text_input::RangeRestorationSeed,
        recovered_window: Option<&beryl_state::SessionWindowRemovalEvidence>,
    ) -> Result<
        crate::main_window::MainWindowComposerCandidateSource,
        (crate::main_window::MainWindowComposerRetiredClose, String),
    > {
        let Some((candidate, _)) = self
            .cas
            .as_mut()
            .expect("prepared recovery CAS custody")
            .app_preparation_parts()
        else {
            return Err((retired, "recovery handoff is unavailable".into()));
        };
        crate::main_window::MainWindowComposerCandidateSource::new_for_recovery(
            candidate,
            retired,
            syndic,
            state,
            seed,
            recovered_window,
        )
    }

    pub(super) fn appearance(&self) -> Arc<crate::theme_runtime::AppearanceGeneration> {
        self.theme
            .as_ref()
            .expect("prepared recovery theme")
            .current()
    }

    pub(super) fn matches_candidate(
        &mut self,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
    ) -> bool {
        self.cas
            .as_mut()
            .expect("prepared recovery CAS custody")
            .app_preparation_parts()
            .is_some_and(|(candidate, _)| {
                candidate.home_id() == home && candidate.generation() == generation
            })
    }

    pub(super) fn threadless_recovery_window(
        &mut self,
        state: &BerylState,
        retired_home: beryl_model::BerylHomeId,
        retired_generation: beryl_home_store::HomeGeneration,
        window: beryl_model::WindowId,
    ) -> Result<super::recovery_threadless::ThreadlessRecoveryWindow, String> {
        let (candidate, _) = self
            .cas
            .as_mut()
            .expect("prepared recovery CAS custody")
            .app_preparation_parts()
            .ok_or("recovery handoff is unavailable")?;
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        super::recovery_threadless::ThreadlessRecoveryWindow::prepare(
            &access,
            state,
            retired_home,
            retired_generation,
            window,
        )
    }

    pub(super) fn composer_recovery_adapters(
        &mut self,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        assets: beryl_state::AssetState,
        requirement: beryl_home_store::TurnStartAdmissionRequirement,
    ) -> Result<super::recovery_composer::PreparedComposerRecoveryAdapters, String> {
        let (candidate, cas) = self
            .cas
            .as_mut()
            .expect("prepared recovery CAS custody")
            .app_preparation_parts()
            .ok_or_else(|| "recovery handoff is unavailable".to_owned())?;
        if candidate.home_id() != home || candidate.generation() != generation {
            return Err("composer recovery request belongs to another candidate".into());
        }
        Ok(
            super::recovery_composer::PreparedComposerRecoveryAdapters::from_prepared(
                candidate,
                assets,
                cas,
                self.marker
                    .as_ref()
                    .expect("prepared recovery marker custody")
                    .service(),
                requirement,
            ),
        )
    }

    pub(crate) fn prepare(
        cas: PreparedRecoveryCasServices,
        state: &BerylState,
        syndic: SyndicStorage,
        configuration: AppServiceConfiguration,
        cancellation: &CommandCancellation,
    ) -> Result<Self, RecoveryAppServicePreparationFailure> {
        let mut prepared = Self {
            cas: Some(cas),
            catalog_source: None,
            marker: None,
            activity: None,
            theme: None,
        };
        let result: Result<(), AppServiceOpenError> = (|| {
            check_cancellation(cancellation)?;
            let (candidate, cas) = prepared
                .cas
                .as_mut()
                .expect("prepared recovery CAS custody")
                .app_preparation_parts()
                .ok_or(AppServiceOpenError::HandoffUnavailable)?;
            let runtime = cas
                .activity_read_source()
                .ok_or(AppServiceOpenError::RuntimeUnavailable)?;
            let catalog_source = CatalogSourceCoordinator::prepare(
                Arc::new(candidate.service_reference()),
                syndic.clone(),
                state.clone(),
                cas.catalog_source_start_gate(),
            )?;
            cas.install_catalog_source_waker(catalog_source.waker());
            prepared.catalog_source = Some(catalog_source);
            prepared.marker = Some(PreparedMarkerServices::prepare_recovery(
                candidate,
                syndic.clone(),
                state.assets(),
                configuration.marker,
            )?);
            check_cancellation(cancellation)?;
            prepared.activity = Some(PreparedActivityService::prepare_recovery(
                candidate,
                syndic,
                runtime,
                configuration.activity,
            )?);
            check_cancellation(cancellation)?;
            prepared.theme = Some(PreparedThemeRuntime::prepare_recovery(
                candidate,
                state.themes(),
                &state.settings(),
                configuration.theme,
            )?);
            check_cancellation(cancellation)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(prepared),
            Err(error) => Err(prepared.fail(error)),
        }
    }

    pub(crate) fn cancel(self) -> RecoveryAppServicePreparationFailure {
        self.fail(AppServiceOpenError::Cancelled)
    }

    fn fail(mut self, error: AppServiceOpenError) -> RecoveryAppServicePreparationFailure {
        self.join_ancillary_services();
        let cas = self
            .cas
            .take()
            .expect("prepared recovery CAS custody")
            .cancel();
        RecoveryAppServicePreparationFailure { error, cas }
    }

    fn join_ancillary_services(&mut self) {
        drop(self.catalog_source.take());
        drop(self.theme.take());
        drop(self.activity.take());
        drop(self.marker.take());
    }
}

impl Drop for PreparedRecoveryAppServices {
    fn drop(&mut self) {
        self.join_ancillary_services();
        drop(self.cas.take());
    }
}
