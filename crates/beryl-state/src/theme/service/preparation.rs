use beryl_home_store::{
    HomeGeneration, HomeOpenPublication, HomeRecoveryCandidate, PreparedThemeWatchSubscription,
    ThemeWatchLimits,
};
use beryl_model::BerylHomeId;

use super::{
    Arc, ThemeActivityGuard, ThemeActivityKind, ThemeChangeSubscription,
    ThemeChangeSubscriptionError, ThemeService, ThemeServiceRuntime,
};

pub struct PreparedThemeChangeSubscription {
    inner: PreparedThemeWatchSubscription,
    runtime: Arc<ThemeServiceRuntime>,
    activity: ThemeActivityGuard,
}

impl ThemeService {
    pub fn prepare_initial_changes(
        &self,
        candidate: &mut HomeOpenPublication,
        limits: ThemeWatchLimits,
    ) -> Result<PreparedThemeChangeSubscription, ThemeChangeSubscriptionError> {
        self.check_subscription_candidate(candidate.home_id(), candidate.generation())?;
        let inner = candidate
            .prepare_theme_changes(limits)
            .map_err(ThemeChangeSubscriptionError::Watcher)?;
        Ok(self.retain_prepared_subscription(inner))
    }

    pub fn prepare_recovered_changes(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        limits: ThemeWatchLimits,
    ) -> Result<PreparedThemeChangeSubscription, ThemeChangeSubscriptionError> {
        self.check_subscription_candidate(candidate.home_id(), candidate.generation())?;
        let inner = candidate
            .prepare_theme_changes(limits)
            .map_err(ThemeChangeSubscriptionError::Watcher)?;
        Ok(self.retain_prepared_subscription(inner))
    }

    fn check_subscription_candidate(
        &self,
        home: BerylHomeId,
        generation: HomeGeneration,
    ) -> Result<(), ThemeChangeSubscriptionError> {
        if home != self.home.home_id() || generation != self.home.home_generation() {
            return Err(ThemeChangeSubscriptionError::Freshness(
                crate::theme::ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        Ok(())
    }

    fn retain_prepared_subscription(
        &self,
        inner: PreparedThemeWatchSubscription,
    ) -> PreparedThemeChangeSubscription {
        PreparedThemeChangeSubscription {
            inner,
            runtime: Arc::clone(&self.runtime),
            activity: self.runtime.begin_activity(ThemeActivityKind::Subscription),
        }
    }
}

impl PreparedThemeChangeSubscription {
    pub fn release(self) -> Result<ThemeChangeSubscription, ThemeChangeSubscriptionError> {
        let inner = self
            .inner
            .release()
            .map_err(ThemeChangeSubscriptionError::Watcher)?;
        Ok(ThemeChangeSubscription {
            inner,
            runtime: self.runtime,
            _activity: self.activity,
        })
    }
}
