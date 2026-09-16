use beryl_home_store::{HomeGeneration, HomeOpenPublication, ThemeWatchLimits};
use beryl_model::BerylHomeId;
use beryl_state::PreparedThemeChangeSubscription;

use super::*;

pub(crate) struct PreparedThemeRuntime {
    subscription: PreparedThemeChangeSubscription,
    service: ThemeService,
    config: ThemeRuntimeConfig,
    home_id: BerylHomeId,
    generation: HomeGeneration,
}

impl PreparedThemeRuntime {
    pub(crate) fn prepare(
        candidate: &mut HomeOpenPublication,
        service: ThemeService,
        config: ThemeRuntimeConfig,
    ) -> Result<Self, ThemeRuntimeStartError> {
        let limits = ThemeWatchLimits::new(
            config.watch_interval,
            config.watch_queue_capacity,
            config.watch_max_entries_per_poll,
            config.watch_max_file_bytes.get(),
            NonZeroUsize::new(64 * 1024).unwrap(),
        )
        .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        let subscription = service
            .prepare_initial_changes(candidate, limits)
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        Ok(Self {
            subscription,
            service,
            config,
            home_id: candidate.home_id(),
            generation: candidate.generation(),
        })
    }

    pub(crate) fn load_published(
        self,
        store: &HomeStore,
        domain_revision: DomainRevision,
        active_setting: Option<&SettingRecord>,
    ) -> Result<ThemeRuntime, ThemeRuntimeStartError> {
        if store.home_id() != self.home_id || store.health().generation() != Some(self.generation) {
            return Err(start_error(ThemeRuntimeFailureClass::Identity));
        }
        let subscription = self
            .subscription
            .release()
            .map_err(|_| start_error(ThemeRuntimeFailureClass::Subscription))?;
        ThemeRuntime::load_started(
            store,
            self.service,
            domain_revision,
            active_setting,
            self.config,
            subscription,
        )
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/theme_runtime_preparation.rs"
    ));
}
