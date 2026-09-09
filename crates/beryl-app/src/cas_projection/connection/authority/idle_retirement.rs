use super::*;

impl ConnectionRegistryAuthority {
    pub(in crate::cas_projection) fn try_retire_session_owner(
        &self,
        elect_ordinary_retirement: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let mut state = self.lock()?;
        if self.is_retired()
            || !state.session_owner_live
            || state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
            || registry::connection_has_authority(self.generation)?
            || !elect_ordinary_retirement()
        {
            return Ok(false);
        }
        state.session_owner_live = false;
        self.retired.store(true, Ordering::Release);
        // The gate excludes new leases, and the exact live-lease count is already zero.
        state.retirement_complete = true;
        self.retirement_changed.notify_all();
        Ok(true)
    }
}
