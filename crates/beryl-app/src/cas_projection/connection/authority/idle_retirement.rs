use super::*;

impl ConnectionRegistryAuthority {
    pub(in crate::cas_projection) fn try_elect_shutdown_projection_release<T>(
        &self,
        owns_only_lease: impl FnOnce() -> Result<bool, ProjectionCoordinatorError>,
        publish: impl FnOnce() -> T,
    ) -> Result<Option<T>, ProjectionCoordinatorError> {
        let Some(state) = self.try_lock_for_shutdown()? else {
            return Ok(None);
        };
        if self.is_retired()
            || !state.session_owner_live
            || state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
            || !owns_only_lease()?
        {
            return Ok(None);
        }
        Ok(Some(publish()))
    }

    pub(in crate::cas_projection) fn try_retire_shutdown_session_owner(
        &self,
        elect: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let Some(mut state) = self.try_lock_for_shutdown()? else {
            return Ok(false);
        };
        if self.is_retired()
            || !state.session_owner_live
            || state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
            || registry::try_connection_has_authority(self.generation)? != Some(false)
            || !elect()
        {
            return Ok(false);
        }
        state.session_owner_live = false;
        self.retired.store(true, Ordering::Release);
        state.retirement_complete = true;
        self.retirement_changed.notify_all();
        Ok(true)
    }

    fn try_lock_for_shutdown(
        &self,
    ) -> Result<
        Option<std::sync::MutexGuard<'_, ConnectionAuthorityState>>,
        ProjectionCoordinatorError,
    > {
        match self.gate.try_lock() {
            Ok(state) => Ok(Some(state)),
            Err(std::sync::TryLockError::WouldBlock) => Ok(None),
            Err(std::sync::TryLockError::Poisoned(_)) => Err(Self::authority_state_error()),
        }
    }

    pub(in crate::cas_projection) fn elect_idle_projection_release(
        &self,
        owns_only_lease: impl FnOnce() -> Result<bool, ProjectionCoordinatorError>,
        elect: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let _change = self.work_boundary.begin_change();
        let state = self.lock()?;
        if self.is_retired()
            || !state.session_owner_live
            || state.scheduled_promotion.is_some()
            || !state.cleanup_owners.is_empty()
            || !owns_only_lease()?
        {
            return Ok(false);
        }
        Ok(elect())
    }

    pub(in crate::cas_projection) fn settlement_owners_clear(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let state = self.lock()?;
        Ok(!self.is_retired()
            && state.scheduled_promotion.is_none()
            && state.cleanup_owners.is_empty()
            && !registry::thread_has_authority(self.generation, thread_id)?)
    }

    pub(in crate::cas_projection) fn try_retire_session_owner(
        &self,
        elect_ordinary_retirement: impl FnOnce() -> bool,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let _change = self.work_boundary.begin_change();
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
