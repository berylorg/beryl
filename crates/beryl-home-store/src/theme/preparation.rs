use crate::{
    HomeGeneration, HomeHealthState, HomeServiceReference, candidate_access::StoreOperationAccess,
};

use super::{ThemeWatchError, ThemeWatchLimits, ThemeWatchSubscription};

pub struct PreparedThemeWatchSubscription {
    subscription: ThemeWatchSubscription,
    home: HomeServiceReference,
}

impl PreparedThemeWatchSubscription {
    pub(crate) fn prepare(
        home: HomeServiceReference,
        state: HomeHealthState,
        generation: HomeGeneration,
        limits: ThemeWatchLimits,
    ) -> Result<Self, ThemeWatchError> {
        let subscription = home.subscribe_theme_changes_with_access(
            limits,
            StoreOperationAccess::Candidate { state, generation },
            true,
        )?;
        Ok(Self { subscription, home })
    }

    pub fn release(self) -> Result<ThemeWatchSubscription, ThemeWatchError> {
        let admission = self
            .home
            .health
            .admit_generation(self.home.admitted_generation)?;
        admission.confirm()?;
        self.subscription.shared.release()?;
        Ok(self.subscription)
    }
}
