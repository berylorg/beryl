use beryl_home_store::{
    HomeGeneration, HomeOpenPublication, HomeRecoveryCandidate, ThemeWatchLimits,
};
use beryl_model::BerylHomeId;
use beryl_state::{PreparedThemeChangeSubscription, SettingKey, SettingsState};

use super::*;

pub(crate) struct PreparedThemeRuntime {
    subscription: PreparedThemeChangeSubscription,
    runtime: ThemeRuntime,
    home_id: BerylHomeId,
    generation: HomeGeneration,
}

impl PreparedThemeRuntime {
    pub(crate) fn prepare(
        candidate: &mut HomeOpenPublication,
        service: ThemeService,
        settings: &SettingsState,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let limits = Self::watch_limits(&config)?;
        let subscription = service
            .prepare_initial_changes(candidate, limits)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        let runtime = Self::load_candidate(
            &candidate
                .recovery_access()
                .map_err(|_| start_error(ThemeRuntimeFailureClass::Settings))?,
            service,
            settings,
            config,
        )?;
        Ok(Self {
            subscription,
            runtime,
            home_id: candidate.home_id(),
            generation: candidate.generation(),
        })
    }

    pub(crate) fn prepare_recovery(
        candidate: &mut HomeRecoveryCandidate,
        service: ThemeService,
        settings: &SettingsState,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let limits = Self::watch_limits(&config)?;
        let subscription = service
            .prepare_recovered_changes(candidate, limits)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        let runtime = Self::load_candidate(
            &candidate
                .recovery_access()
                .map_err(|_| start_error(ThemeRuntimeFailureClass::Settings))?,
            service,
            settings,
            config,
        )?;
        Ok(Self {
            subscription,
            runtime,
            home_id: candidate.home_id(),
            generation: candidate.generation(),
        })
    }

    fn watch_limits(
        config: &ThemeRuntimeConfig,
    ) -> Result<ThemeWatchLimits, ThemeRuntimeStartError> {
        ThemeWatchLimits::new(
            config.watch_interval,
            config.watch_queue_capacity,
            config.watch_max_entries_per_poll,
            config.watch_max_file_bytes.get(),
            NonZeroUsize::new(64 * 1024).unwrap(),
        )
        .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))
    }

    fn load_candidate(
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        service: ThemeService,
        settings: &SettingsState,
        config: ThemeRuntimeConfig,
    ) -> Result<ThemeRuntime, ThemeRuntimeStartError> {
        let revision = settings
            .revision_candidate(access)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Settings))?;
        let active = settings
            .setting_candidate(access, SettingKey::ActiveThemeId)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Settings))?;
        let runtime = ThemeRuntime::load_initial(
            load::ThemeLoadAccess::Candidate(access),
            service,
            revision,
            active.as_ref(),
            config,
        )?;
        if settings
            .revision_candidate(access)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Settings))?
            != revision
        {
            return Err(start_error(ThemeRuntimeFailureClass::Settings));
        }
        Ok(runtime)
    }

    pub(crate) fn current(&self) -> Arc<AppearanceGeneration> {
        self.runtime.current().expect("prepared appearance")
    }

    pub(crate) fn release_published(
        self,
        store: &HomeStore,
    ) -> Result<ThemeRuntime, ThemeRuntimeStartError> {
        if store.home_id() != self.home_id || store.health().generation() != Some(self.generation) {
            return Err(start_error(ThemeRuntimeFailureClass::Identity));
        }
        let subscription = self
            .subscription
            .release()
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        let mut runtime = self.runtime;
        runtime.subscription = Some(subscription);
        Ok(runtime)
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/theme_runtime_preparation.rs"
    ));
}
