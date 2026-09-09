use super::*;

impl ProjectionConnection {
    pub(in crate::cas_projection) fn elect_idle_session_retirement(
        &self,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let attachment = self.current_attachment()?;
        let Ok(command) = attachment.commands.authorize() else {
            return Ok(false);
        };
        self.authority
            .try_retire_session_owner(|| self.elect_ordinary_retirement(&command))
    }

    pub(in crate::cas_projection) fn signal_idle_session_retirement(&self) {
        self.signal_ordinary_retirement();
    }

    pub(in crate::cas_projection) fn try_reap_ordinary_retirement(
        &self,
    ) -> Result<bool, ProjectionCoordinatorError> {
        if !self.authority.is_retired() || !self.authority.try_retirement_complete()? {
            return Ok(false);
        }
        let mut settlement = match self.shutdown_settlement.try_lock() {
            Ok(settlement) => settlement,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(ProjectionCoordinatorError::RegistryPoisoned {
                    registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
                });
            }
        };
        let Some(hub) = self.forwarding_hub.try_lock_attachment()? else {
            return Ok(false);
        };
        match *settlement {
            ConnectionShutdownSettlement::Clean => return Ok(hub.attachment().is_none()),
            ConnectionShutdownSettlement::Failed => {
                return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
            }
            ConnectionShutdownSettlement::Unsettled => {}
        }
        let attachment = hub.attachment().cloned();
        drop(hub);
        let broker_finished = match attachment {
            Some(attachment) => attachment.try_ingester_is_finished()?,
            None => true,
        };
        let driver_finished = match self.runtime.try_lock() {
            Ok(runtime) => runtime
                .as_ref()
                .is_none_or(|runtime| runtime.driver.is_finished()),
            Err(std::sync::TryLockError::WouldBlock) => false,
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(ProjectionCoordinatorError::RegistryPoisoned {
                    registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
                });
            }
        };
        if !broker_finished || !driver_finished {
            return Ok(false);
        }
        self.settle_ordinary_shutdown_locked(&mut settlement)?;
        Ok(self.is_detached())
    }

    pub(in crate::cas_projection) fn shutdown_for_runtime_retirement(
        &self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let shutdown = self.shutdown();
        let settlement = self.shutdown_after_ordinary_retirement();
        if shutdown.is_ok() && settlement.is_ok() {
            return Ok(());
        }
        let mut owner = self
            .shutdown_settlement
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _ = self.execute_terminal_shutdown();
        *owner = ConnectionShutdownSettlement::Failed;
        shutdown.and(settlement)
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn with_shutdown_settlement_for_test<T>(
        &self,
        inspect: impl FnOnce() -> T,
    ) -> T {
        let _settlement = self
            .shutdown_settlement
            .lock()
            .expect("settlement is healthy");
        inspect()
    }
}
