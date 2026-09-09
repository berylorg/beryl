use super::*;

impl ProjectionWorkerPool {
    pub(in crate::cas_projection) fn try_acquire_cold_preparation_or_arm(
        &self,
    ) -> Result<(ProjectionWorkerPermitPair, ProjectionWorkerPermitPair), ProjectionWorkerPermitError>
    {
        self.reserve_preparation(true)?;
        Ok((
            self.reserved_connection_pair(),
            self.reserved_connection_pair(),
        ))
    }

    pub(in crate::cas_projection) fn try_acquire_warm_preparation_or_arm(
        &self,
    ) -> Result<ProjectionWorkerPermitPair, ProjectionWorkerPermitError> {
        self.reserve_preparation(false)?;
        Ok(self.reserved_connection_pair())
    }

    fn reserve_preparation(&self, cold: bool) -> Result<(), ProjectionWorkerPermitError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ProjectionWorkerPermitError::Poisoned)?;
        let permits = CONNECTION_WORKER_PERMITS * if cold { 2 } else { 1 };
        if !state.noncritical_role_fits(permits) {
            state.denied_pairs = state.denied_pairs.saturating_add(1);
            if cold {
                state.release_waiter.cold_preparation = true;
            } else {
                state.release_waiter.warm_preparation = true;
            }
            return Err(ProjectionWorkerPermitError::CapacityFull {
                available: state.available,
            });
        }
        if cold {
            state.release_waiter.cold_preparation = false;
        } else {
            state.release_waiter.warm_preparation = false;
        }
        state.record_acquisition(permits);
        Ok(())
    }

    fn reserved_connection_pair(&self) -> ProjectionWorkerPermitPair {
        ProjectionWorkerPermitPair {
            driver: Some(ProjectionWorkerPermit::new(
                self.clone(),
                ProjectionWorkerRole::Connection,
            )),
            ingester: Some(ProjectionWorkerPermit::new(
                self.clone(),
                ProjectionWorkerRole::Connection,
            )),
        }
    }
}
