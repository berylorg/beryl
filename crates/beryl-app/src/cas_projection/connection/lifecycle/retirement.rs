use super::*;

impl ProjectionConnection {
    pub(in crate::cas_projection) fn begin_shutdown_cleanup_change(
        &self,
    ) -> crate::cas_projection::connection_work::ConnectionWorkMutation {
        self.authority.work_boundary.begin_change()
    }

    pub(in crate::cas_projection) fn settle_shutdown_authority_error(
        &self,
        error: &ProjectionCoordinatorError,
    ) {
        if matches!(
            error,
            ProjectionCoordinatorError::RegistryPoisoned {
                registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection
            }
        ) {
            drop(self.authority.lock());
        }
    }

    pub(in crate::cas_projection) fn elect_shutdown_projection_release<T>(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
        owns_only_lease: impl FnOnce() -> Result<bool, ProjectionCoordinatorError>,
        publish: impl FnOnce() -> T,
    ) -> Result<Option<T>, ProjectionCoordinatorError> {
        let Some(hub) = self.forwarding_hub.try_lock_attachment()? else {
            return Ok(None);
        };
        let Some(attachment) = hub.attachment().cloned() else {
            return Ok(None);
        };
        drop(hub);
        if attachment.identity.service_generation() != command.service_generation() {
            return Ok(None);
        }
        self.authority
            .try_elect_shutdown_projection_release(owns_only_lease, publish)
    }

    pub(in crate::cas_projection) fn elect_shutdown_session_retirement(
        &self,
        command: &crate::cas_projection::LiveCommandPermit,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let Some(hub) = self.forwarding_hub.try_lock_attachment()? else {
            return Ok(false);
        };
        let Some(attachment) = hub.attachment().cloned() else {
            return Ok(false);
        };
        drop(hub);
        if attachment.identity.service_generation() != command.service_generation() {
            return Ok(false);
        }
        self.authority.try_retire_shutdown_session_owner(|| {
            attachment
                .persistent_failure
                .elect_ordinary_retirement(None)
        })
    }

    pub(in crate::cas_projection) fn elect_idle_projection_release(
        &self,
        owner: &crate::cas_projection::runtime_interest::RuntimeInterestOwner,
        binding: &beryl_model::ExecutionBinding,
        observation: &beryl_home_store::HomeMutationObservation,
        owns_only_lease: impl FnOnce() -> Result<bool, ProjectionCoordinatorError>,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let attachment = self.current_attachment()?;
        let Ok(command) = attachment.commands.authorize() else {
            return Ok(false);
        };
        self.authority
            .elect_idle_projection_release(owns_only_lease, || {
                owner
                    .with_no_view_interest(binding, self.process_generation(), || {
                        command
                            .commit_if_current(|| observation.try_elect(|| true).unwrap_or(false))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
    }

    pub(in crate::cas_projection) fn settlement_cleanup_complete(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<bool, ProjectionCoordinatorError> {
        if self.authority.is_retired() {
            self.try_reap_ordinary_retirement()
        } else {
            self.authority.settlement_owners_clear(thread_id)
        }
    }

    pub(in crate::cas_projection) fn elect_idle_session_retirement(
        &self,
    ) -> Result<bool, ProjectionCoordinatorError> {
        self.elect_idle_retirement(None)
    }

    pub(in crate::cas_projection) fn elect_unviewed_session_retirement(
        &self,
        owner: &crate::cas_projection::runtime_interest::RuntimeInterestOwner,
        binding: &beryl_model::ExecutionBinding,
        observation: &beryl_home_store::HomeMutationObservation,
    ) -> Result<bool, ProjectionCoordinatorError> {
        self.elect_idle_retirement(Some((owner, binding, observation)))
    }

    fn elect_idle_retirement(
        &self,
        view_owner: Option<(
            &crate::cas_projection::runtime_interest::RuntimeInterestOwner,
            &beryl_model::ExecutionBinding,
            &beryl_home_store::HomeMutationObservation,
        )>,
    ) -> Result<bool, ProjectionCoordinatorError> {
        let attachment = self.current_attachment()?;
        let Ok(command) = attachment.commands.authorize() else {
            return Ok(false);
        };
        self.authority.try_retire_session_owner(|| {
            let elect = || {
                command
                    .commit_if_current(|| {
                        attachment.persistent_failure.elect_ordinary_retirement(
                            view_owner.map(|(_, _, observation)| observation),
                        )
                    })
                    .unwrap_or(false)
            };
            match view_owner {
                Some((owner, binding, _)) => owner
                    .with_no_view_interest(binding, self.process_generation(), elect)
                    .unwrap_or(false),
                None => elect(),
            }
        })
    }

    pub(in crate::cas_projection) fn signal_idle_session_retirement(&self) {
        self.ingress_cancellation.request();
        if let Ok(attachment) = self.current_attachment() {
            attachment.signal_elected_ordinary_retirement();
        }
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
