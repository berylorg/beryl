use std::sync::{Arc, Mutex};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use syndic_storage::SyndicStorage;

#[cfg(test)]
use super::provider_broker::fail_next_provider_broker_join_for_test;
use super::{
    ConnectionEpochIdentity, ConnectionServiceEpoch, EventRouter, ForwardingEpochEndpoint,
    ProjectionConnection,
    driver::{
        AdoptedDriverParkToken, DriverParkBindError, DriverParkError, DriverParkErrorReason,
        ParkedDriver,
    },
    forwarding_hub::ForwardingHubEpochGuard,
    persistent_failure::PersistentFailureDriverSlot,
    provider_broker::{
        PreparedProviderBroker, ProviderBroker, ProviderBrokerAdoptionStopped,
        ProviderBrokerStartToken, ProviderBrokerStopped, StartBlockedProviderBrokerIngester,
    },
};
use crate::cas_projection::{
    LiveCommandAuthorizer, PersistentFailureNotification, ProjectionCoordinatorError,
    accepted_input_scheduler::AcceptedInputSchedulerSignal,
    context_compaction::ContextCompactionCoordinator,
    persistent_failure::{PersistentFailureCutIdentity, PersistentFailureProjectionRetainer},
    service_config::{ProjectionWorkerPermit, ProjectionWorkerPermitPair},
    service_startup::ServiceStartupGate,
    stop::StopCoordinator,
};

/// Complete service-generation context used to prepare one dormant replacement epoch.
pub(in crate::cas_projection) struct ConnectionReplacementContext {
    pub(in crate::cas_projection) home: Arc<HomeStore>,
    pub(in crate::cas_projection) home_id: BerylHomeId,
    pub(in crate::cas_projection) home_generation: HomeGeneration,
    pub(in crate::cas_projection) storage: SyndicStorage,
    pub(in crate::cas_projection) commands: LiveCommandAuthorizer,
    pub(in crate::cas_projection) stop: Arc<StopCoordinator>,
    pub(in crate::cas_projection) compaction: Arc<ContextCompactionCoordinator>,
    pub(in crate::cas_projection) scheduler: AcceptedInputSchedulerSignal,
    pub(in crate::cas_projection) failure_notification: PersistentFailureNotification,
    pub(in crate::cas_projection) retainer: PersistentFailureProjectionRetainer,
    pub(in crate::cas_projection) startup: Arc<ServiceStartupGate>,
}

/// Prepared replacement resources for one stable connection, still outside its forwarding hub.
pub(in crate::cas_projection) struct PreparedConnectionEpoch {
    connection: Arc<ProjectionConnection>,
    epoch: Arc<ConnectionServiceEpoch>,
    endpoint: Option<ForwardingEpochEndpoint>,
    ingester: Option<StartBlockedProviderBrokerIngester>,
    start: Option<ProviderBrokerStartToken>,
    driver_worker: Option<ProjectionWorkerPermit>,
    startup: Arc<ServiceStartupGate>,
}

/// Owning preparation failure retaining every worker admission acquired for this connection.
pub(in crate::cas_projection) struct PreparedConnectionEpochError {
    connection: Arc<ProjectionConnection>,
    failure: ProjectionCoordinatorError,
    driver_worker: Option<ProjectionWorkerPermit>,
    ingester_worker: Option<ProjectionWorkerPermit>,
    router: Option<Arc<EventRouter>>,
    broker_failure: Option<Box<dyn std::error::Error + Send + 'static>>,
    #[cfg(test)]
    broker_spawn_resources_retained: bool,
}

pub(in crate::cas_projection) struct BoundConnectionEpoch {
    prepared: PreparedConnectionEpoch,
    old_driver_worker: Option<ProjectionWorkerPermit>,
    driver_token: Option<AdoptedDriverParkToken>,
}

pub(in crate::cas_projection) struct PreparedConnectionEpochBindError {
    prepared: PreparedConnectionEpoch,
    old_driver_worker: Option<ProjectionWorkerPermit>,
    bind: Option<DriverParkBindError>,
}

pub(in crate::cas_projection) struct AdoptedConnectionEpochAttachment {
    pub(in crate::cas_projection) connection: Arc<ProjectionConnection>,
    new_epoch: Arc<ConnectionServiceEpoch>,
    old_endpoint: Option<ForwardingEpochEndpoint>,
    old_driver_worker: Option<ProjectionWorkerPermit>,
    old_ingester: Option<ProviderBrokerAdoptionStopped>,
    new_ingester: Option<StartBlockedProviderBrokerIngester>,
    new_start: Option<ProviderBrokerStartToken>,
    driver_token: Option<AdoptedDriverParkToken>,
    startup: Arc<ServiceStartupGate>,
    old_epoch_retired: bool,
    publication_armed: bool,
    #[cfg(test)]
    fail_ingester_arm: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum RecoveredConnectionPublicationReason {
    TopologyMismatch,
    OldEpochUnavailable,
    DriverArm(DriverParkErrorReason),
    IngesterArm,
    IngesterRegistryUnavailable,
}

pub(in crate::cas_projection) struct OldConnectionIngesterJoinError {
    failure: ProjectionCoordinatorError,
    stopped: Option<ProviderBrokerStopped>,
    receipt_failed: bool,
}

pub(in crate::cas_projection) struct ConnectionEpochAdoptionBarrier<'a> {
    connection: &'a Arc<ProjectionConnection>,
    guard: ForwardingHubEpochGuard<'a>,
    armed: bool,
}

/// Read-only final-seal barrier over one adopted forwarding epoch.
pub(in crate::cas_projection) struct CandidateSetSealEpochBarrier<'a> {
    guard: ForwardingHubEpochGuard<'a>,
}

/// Final-publication barrier retaining one exact adopted forwarding epoch.
pub(in crate::cas_projection) struct RecoveryPublicationEpochBarrier<'a> {
    guard: ForwardingHubEpochGuard<'a>,
}

#[must_use = "an inert adoption owner must retain the exact detached epoch attachment"]
pub(in crate::cas_projection) struct InertConnectionEpochAttachment {
    endpoint: Option<ForwardingEpochEndpoint>,
}

fn validate_provider_ingester_join(
    stopped: &ProviderBrokerStopped,
    identity: ConnectionEpochIdentity,
) -> Result<(), ProjectionCoordinatorError> {
    stopped
        .validate_receipt(identity.service_generation(), identity.home_generation())
        .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)
}

impl ProjectionConnection {
    #[cfg(test)]
    pub(in crate::cas_projection) fn epoch_identity_for_adoption_test(
        &self,
    ) -> Result<ConnectionEpochIdentity, ProjectionCoordinatorError> {
        self.current_epoch().map(|epoch| epoch.identity)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn epoch_pointer_for_adoption_test(
        &self,
    ) -> Result<usize, ProjectionCoordinatorError> {
        self.current_epoch()
            .map(|epoch| Arc::as_ptr(&epoch) as usize)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn fail_current_epoch_ingester_join_for_test(
        &self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let identity = self.current_epoch()?.identity;
        fail_next_provider_broker_join_for_test(
            identity.home_id(),
            identity.home_generation(),
            identity.service_generation(),
        );
        Ok(())
    }

    pub(in crate::cas_projection) fn prepare_replacement_epoch(
        self: &Arc<Self>,
        context: &ConnectionReplacementContext,
        workers: ProjectionWorkerPermitPair,
    ) -> Result<PreparedConnectionEpoch, PreparedConnectionEpochError> {
        self.prepare_replacement_epoch_inner(context, workers, false)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn prepare_replacement_epoch_with_broker_spawn_failure_for_test(
        self: &Arc<Self>,
        context: &ConnectionReplacementContext,
        workers: ProjectionWorkerPermitPair,
    ) -> Result<PreparedConnectionEpoch, PreparedConnectionEpochError> {
        self.prepare_replacement_epoch_inner(context, workers, true)
    }

    fn prepare_replacement_epoch_inner(
        self: &Arc<Self>,
        context: &ConnectionReplacementContext,
        workers: ProjectionWorkerPermitPair,
        #[cfg(test)] broker_spawn_failure: bool,
        #[cfg(not(test))] _broker_spawn_failure: bool,
    ) -> Result<PreparedConnectionEpoch, PreparedConnectionEpochError> {
        let (driver_worker, ingester_worker) = workers.into_parts();
        let router = match EventRouter::new_with_process(
            self.runtime_id(),
            self.process_generation(),
            self.authority.generation.get(),
            context.scheduler.clone(),
            context.commands.clone(),
            Some(context.retainer.clone()),
            self.process_fact_observation(),
        ) {
            Ok(router) => Arc::new(router),
            Err(failure) => {
                return Err(PreparedConnectionEpochError {
                    connection: Arc::clone(self),
                    failure,
                    driver_worker: Some(driver_worker),
                    ingester_worker: Some(ingester_worker),
                    router: None,
                    broker_failure: None,
                    #[cfg(test)]
                    broker_spawn_resources_retained: false,
                });
            }
        };
        #[cfg(test)]
        let prepared = if broker_spawn_failure {
            ProviderBroker::prepare_replacement_with_spawn_failure_for_test(
                Arc::clone(&context.home),
                context.home_id,
                context.home_generation,
                Arc::clone(&self.authority),
                Arc::clone(&router),
                Arc::clone(&context.stop),
                Arc::clone(&context.compaction),
                context.commands.clone(),
                context.failure_notification.clone(),
                ingester_worker,
                Arc::clone(&context.startup),
            )
        } else {
            ProviderBroker::prepare_with_startup_gate(
                Arc::clone(&context.home),
                context.home_id,
                context.home_generation,
                Arc::clone(&self.authority),
                Arc::clone(&router),
                Arc::clone(&context.stop),
                Arc::clone(&context.compaction),
                context.commands.clone(),
                context.failure_notification.clone(),
                ingester_worker,
                Arc::clone(&context.startup),
            )
        };
        #[cfg(not(test))]
        let prepared = ProviderBroker::prepare_with_startup_gate(
            Arc::clone(&context.home),
            context.home_id,
            context.home_generation,
            Arc::clone(&self.authority),
            Arc::clone(&router),
            Arc::clone(&context.stop),
            Arc::clone(&context.compaction),
            context.commands.clone(),
            context.failure_notification.clone(),
            ingester_worker,
            Arc::clone(&context.startup),
        );
        let PreparedProviderBroker {
            sink,
            control,
            ingester,
            start,
        } = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                #[cfg(test)]
                let broker_spawn_resources_retained =
                    error.retains_complete_unstarted_resources_for_test();
                let message = error.to_string();
                return Err(PreparedConnectionEpochError {
                    connection: Arc::clone(self),
                    failure: ProjectionCoordinatorError::ProviderBrokerAdmission { message },
                    driver_worker: Some(driver_worker),
                    ingester_worker: None,
                    router: Some(router),
                    broker_failure: Some(Box::new(error)),
                    #[cfg(test)]
                    broker_spawn_resources_retained,
                });
            }
        };
        let identity = ConnectionEpochIdentity::new(
            context.home_id,
            context.home_generation,
            context.commands.service_generation(),
        );
        let epoch = Arc::new(ConnectionServiceEpoch {
            identity,
            home: Arc::clone(&context.home),
            storage: context.storage,
            router,
            broker: control,
            ingester: Mutex::new(None),
            commands: context.commands.clone(),
            persistent_failure: Arc::new(PersistentFailureDriverSlot::new()),
            stop_coordinator: Arc::clone(&context.stop),
            context_compaction: Arc::clone(&context.compaction),
            scheduler_signal: context.scheduler.clone(),
            failure_notification: context.failure_notification.clone(),
            projection_retainer: context.retainer.clone(),
        });
        Ok(PreparedConnectionEpoch {
            connection: Arc::clone(self),
            endpoint: Some(ForwardingEpochEndpoint::new(Arc::clone(&epoch), sink)),
            epoch,
            ingester: Some(ingester),
            start: Some(start),
            driver_worker: Some(driver_worker),
            startup: Arc::clone(&context.startup),
        })
    }

    pub(in crate::cas_projection) fn park_driver_for_adoption(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<ParkedDriver, DriverParkError> {
        self.park_stable_driver_for_adoption(cut)
    }

    pub(in crate::cas_projection) fn join_old_ingester_for_adoption(
        &self,
        old_identity: ConnectionEpochIdentity,
        cut: PersistentFailureCutIdentity,
    ) -> Result<ProviderBrokerAdoptionStopped, OldConnectionIngesterJoinError> {
        let epoch = self
            .current_epoch()
            .map_err(|failure| OldConnectionIngesterJoinError {
                failure,
                stopped: None,
                receipt_failed: false,
            })?;
        if epoch.identity != old_identity {
            return Err(OldConnectionIngesterJoinError {
                failure: ProjectionCoordinatorError::ProjectionWorkerStopped,
                stopped: None,
                receipt_failed: false,
            });
        }
        if let Err(failure) = epoch.arm_ingester_worker_retention_for_adoption(cut) {
            return Err(OldConnectionIngesterJoinError {
                failure,
                stopped: None,
                receipt_failed: false,
            });
        }
        let stopped =
            epoch
                .stop_and_join_ingester()
                .map_err(|failure| OldConnectionIngesterJoinError {
                    failure,
                    stopped: None,
                    receipt_failed: false,
                })?;
        match stopped.into_adoption(cut) {
            Ok(stopped) => Ok(stopped),
            Err(stopped) => {
                let receipt_failed = stopped
                    .validate_receipt(cut.service_generation, cut.home_generation)
                    .is_err();
                Err(OldConnectionIngesterJoinError {
                    failure: ProjectionCoordinatorError::ProjectionWorkerStopped,
                    stopped: Some(stopped),
                    receipt_failed,
                })
            }
        }
    }

    pub(in crate::cas_projection) fn lock_epoch_for_adoption(
        self: &Arc<Self>,
    ) -> Result<ConnectionEpochAdoptionBarrier<'_>, ProjectionCoordinatorError> {
        Ok(ConnectionEpochAdoptionBarrier {
            connection: self,
            guard: self.lock_forwarding_epoch_for_adoption()?,
            armed: true,
        })
    }

    pub(in crate::cas_projection) fn lock_epoch_for_candidate_set_seal(
        &self,
    ) -> Result<CandidateSetSealEpochBarrier<'_>, ProjectionCoordinatorError> {
        Ok(CandidateSetSealEpochBarrier {
            guard: self.lock_forwarding_epoch_for_adoption()?,
        })
    }

    pub(in crate::cas_projection) fn lock_epoch_for_recovery_publication(
        &self,
    ) -> Result<RecoveryPublicationEpochBarrier<'_>, ProjectionCoordinatorError> {
        Ok(RecoveryPublicationEpochBarrier {
            guard: self.lock_forwarding_epoch_for_adoption()?,
        })
    }

    pub(in crate::cas_projection) fn make_adoption_inert(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> InertConnectionEpochAttachment {
        self.make_adoption_inert_in_place(cut)
    }

    pub(in crate::cas_projection) fn make_adoption_inert_in_place(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> InertConnectionEpochAttachment {
        self.disable_stable_driver_for_adoption(cut);
        let endpoint = self.detach_forwarding_epoch_for_inert_adoption();
        if let Some(endpoint) = endpoint.as_ref() {
            endpoint.epoch().request_ingester_cancel();
        }
        InertConnectionEpochAttachment { endpoint }
    }

    /// Terminalizes this stable core without allocating or detaching its epoch owner.
    pub(in crate::cas_projection) fn make_adoption_inert_retaining_epoch_in_place(
        &self,
        cut: PersistentFailureCutIdentity,
    ) {
        self.disable_stable_driver_for_adoption(cut);
        self.mark_forwarding_epoch_inert_in_place_for_adoption_failure();
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn forwarding_epoch_is_inert_and_attached_for_test(
        &self,
    ) -> bool {
        self.forwarding_epoch_is_inert_and_attached_after_adoption_failure_for_test()
    }
}

impl PreparedConnectionEpoch {
    pub(in crate::cas_projection) fn connection(&self) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    pub(in crate::cas_projection) fn identity(&self) -> ConnectionEpochIdentity {
        self.epoch.identity
    }

    pub(in crate::cas_projection) fn bind_parked_driver(
        mut self,
        parked: ParkedDriver,
    ) -> Result<BoundConnectionEpoch, PreparedConnectionEpochBindError> {
        let (old_driver_worker, token) = parked.into_parts();
        let driver_worker = self
            .driver_worker
            .take()
            .expect("a prepared connection epoch retains its replacement driver admission");
        let token = match token.bind_replacement(
            self.epoch.identity,
            driver_worker,
            Arc::clone(&self.startup),
        ) {
            Ok(token) => token,
            Err(bind) => {
                return Err(PreparedConnectionEpochBindError {
                    prepared: self,
                    old_driver_worker: Some(old_driver_worker),
                    bind: Some(bind),
                });
            }
        };
        Ok(BoundConnectionEpoch {
            prepared: self,
            old_driver_worker: Some(old_driver_worker),
            driver_token: Some(token),
        })
    }

    pub(in crate::cas_projection) fn dispose_after_adoption_failure(
        mut self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let identity = self.epoch.identity;
        match (self.ingester.take(), self.start.take()) {
            (Some(ingester), Some(start)) => {
                let stopped = ingester.cancel_and_join(start);
                validate_provider_ingester_join(&stopped, identity)
            }
            (None, None) => Ok(()),
            (ingester, start) => {
                drop((ingester, start));
                Err(ProjectionCoordinatorError::ProjectionWorkerStopped)
            }
        }
    }
}

impl PreparedConnectionEpochError {
    pub(in crate::cas_projection) fn connection(&self) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    pub(in crate::cas_projection) fn failure(&self) -> &ProjectionCoordinatorError {
        &self.failure
    }

    pub(in crate::cas_projection) fn dispose_after_adoption_failure(self) {
        drop(self);
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn broker_spawn_resources_retained_for_test(&self) -> bool {
        self.broker_spawn_resources_retained
    }
}

impl PreparedConnectionEpochBindError {
    pub(in crate::cas_projection) fn reason(&self) -> super::driver::DriverParkErrorReason {
        self.bind
            .as_ref()
            .expect("a connection epoch bind failure retains its exact bind error")
            .reason()
    }

    pub(in crate::cas_projection) fn dispose_after_adoption_failure(
        self,
    ) -> Result<(), ProjectionCoordinatorError> {
        self.prepared.dispose_after_adoption_failure()
    }
}

impl OldConnectionIngesterJoinError {
    pub(in crate::cas_projection) fn failure(&self) -> &ProjectionCoordinatorError {
        &self.failure
    }

    pub(in crate::cas_projection) fn dispose_after_adoption_failure(
        self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let receipt_failed = self.receipt_failed;
        drop(self.stopped);
        if receipt_failed {
            Err(ProjectionCoordinatorError::ProjectionWorkerStopped)
        } else {
            Ok(())
        }
    }
}

impl InertConnectionEpochAttachment {
    pub(in crate::cas_projection) fn is_empty(&self) -> bool {
        self.endpoint.is_none()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn dispose(self) {
        self.dispose_after_adoption_failure()
            .expect("the test inert attachment joins its provider ingester cleanly");
    }

    pub(in crate::cas_projection) fn dispose_after_adoption_failure(
        mut self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let Some(endpoint) = self.endpoint.take() else {
            return Ok(());
        };
        let epoch = endpoint.epoch();
        let identity = epoch.identity;
        epoch.request_ingester_cancel();
        let ingester = epoch
            .ingester
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        if let Some(ingester) = ingester {
            let stopped = ingester.stop_and_join();
            validate_provider_ingester_join(&stopped, identity)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn retains_epoch_pointer_for_test(
        &self,
        expected: usize,
    ) -> bool {
        self.endpoint
            .as_ref()
            .is_some_and(|endpoint| Arc::as_ptr(endpoint.epoch()) as usize == expected)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn ingester_is_finished_for_test(&self) -> bool {
        self.endpoint
            .as_ref()
            .is_none_or(|endpoint| endpoint.epoch().ingester_is_finished())
    }
}

impl BoundConnectionEpoch {
    pub(in crate::cas_projection) fn connection(&self) -> &Arc<ProjectionConnection> {
        &self.prepared.connection
    }

    pub(in crate::cas_projection) fn identity(&self) -> ConnectionEpochIdentity {
        self.prepared.epoch.identity
    }

    pub(in crate::cas_projection) fn dispose_after_adoption_failure(
        self,
    ) -> Result<(), ProjectionCoordinatorError> {
        self.prepared.dispose_after_adoption_failure()
    }
}

impl AdoptedConnectionEpochAttachment {
    pub(in crate::cas_projection) fn connection(&self) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    pub(in crate::cas_projection) fn identity(&self) -> ConnectionEpochIdentity {
        self.new_epoch.identity
    }

    pub(in crate::cas_projection) fn preflight_recovery_publication(
        &self,
        connection: &Arc<ProjectionConnection>,
        expected_home: &Arc<HomeStore>,
        expected_epoch: ConnectionEpochIdentity,
        startup: &Arc<ServiceStartupGate>,
    ) -> bool {
        Arc::ptr_eq(&self.connection, connection)
            && self.new_epoch.identity == expected_epoch
            && Arc::ptr_eq(&self.new_epoch.home, expected_home)
            && Arc::ptr_eq(&self.startup, startup)
            && startup.is_closed()
            && !self.old_epoch_retired
            && !self.publication_armed
            && self.old_endpoint.is_some()
            && self.old_driver_worker.is_some()
            && self.old_ingester.is_some()
            && self.new_ingester.is_some()
            && self.new_start.is_some()
            && self.driver_token.is_some()
            && self
                .new_epoch
                .ingester
                .lock()
                .is_ok_and(|ingester| ingester.is_none())
    }

    pub(in crate::cas_projection) fn retire_closed_old_epoch(
        &mut self,
    ) -> Result<(), RecoveredConnectionPublicationReason> {
        if self.old_epoch_retired
            || self.old_endpoint.is_none()
            || self.old_driver_worker.is_none()
            || self.old_ingester.is_none()
        {
            return Err(RecoveredConnectionPublicationReason::OldEpochUnavailable);
        }
        drop(self.old_endpoint.take());
        drop(self.old_driver_worker.take());
        drop(self.old_ingester.take());
        self.old_epoch_retired = true;
        Ok(())
    }

    pub(in crate::cas_projection) fn arm_replacement_workers(
        &mut self,
        startup: &Arc<ServiceStartupGate>,
    ) -> Result<(), RecoveredConnectionPublicationReason> {
        if !self.old_epoch_retired
            || self.publication_armed
            || !Arc::ptr_eq(&self.startup, startup)
            || !startup.is_closed()
        {
            return Err(RecoveredConnectionPublicationReason::TopologyMismatch);
        }
        let driver = self
            .driver_token
            .take()
            .ok_or(RecoveredConnectionPublicationReason::TopologyMismatch)?;
        driver
            .arm_for_publication(startup)
            .map_err(|error| RecoveredConnectionPublicationReason::DriverArm(error.reason()))?;
        #[cfg(test)]
        if self.fail_ingester_arm {
            self.fail_ingester_arm = false;
            return Err(RecoveredConnectionPublicationReason::IngesterArm);
        }

        let mut running = self
            .new_epoch
            .ingester
            .lock()
            .map_err(|_| RecoveredConnectionPublicationReason::IngesterRegistryUnavailable)?;
        if running.is_some() {
            return Err(RecoveredConnectionPublicationReason::TopologyMismatch);
        }
        let ingester = self
            .new_ingester
            .take()
            .ok_or(RecoveredConnectionPublicationReason::TopologyMismatch)?;
        let start = self
            .new_start
            .take()
            .ok_or(RecoveredConnectionPublicationReason::TopologyMismatch)?;
        match ingester.arm_for_publication(start, startup) {
            Ok(ingester) => *running = Some(ingester),
            Err((ingester, start)) => {
                self.new_ingester = Some(ingester);
                self.new_start = Some(start);
                return Err(RecoveredConnectionPublicationReason::IngesterArm);
            }
        }
        self.publication_armed = true;
        Ok(())
    }

    pub(in crate::cas_projection) fn validates_retired_unarmed_publication(
        &self,
        connection: &Arc<ProjectionConnection>,
        expected_home: &Arc<HomeStore>,
        expected_epoch: ConnectionEpochIdentity,
        startup: &Arc<ServiceStartupGate>,
    ) -> bool {
        self.old_epoch_retired
            && !self.publication_armed
            && Arc::ptr_eq(&self.connection, connection)
            && self.new_epoch.identity == expected_epoch
            && Arc::ptr_eq(&self.new_epoch.home, expected_home)
            && Arc::ptr_eq(&self.startup, startup)
            && self.old_endpoint.is_none()
            && self.old_driver_worker.is_none()
            && self.old_ingester.is_none()
            && self.new_ingester.is_some()
            && self.new_start.is_some()
            && self.driver_token.is_some()
            && self
                .new_epoch
                .ingester
                .lock()
                .is_ok_and(|ingester| ingester.is_none())
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn fail_replacement_ingester_arm_for_test(&mut self) {
        self.fail_ingester_arm = true;
    }

    pub(in crate::cas_projection) fn validates_armed_publication(
        &self,
        connection: &Arc<ProjectionConnection>,
        expected_home: &Arc<HomeStore>,
        expected_epoch: ConnectionEpochIdentity,
        startup: &Arc<ServiceStartupGate>,
    ) -> bool {
        self.old_epoch_retired
            && self.publication_armed
            && Arc::ptr_eq(&self.connection, connection)
            && self.new_epoch.identity == expected_epoch
            && Arc::ptr_eq(&self.new_epoch.home, expected_home)
            && Arc::ptr_eq(&self.startup, startup)
            && self.old_endpoint.is_none()
            && self.old_driver_worker.is_none()
            && self.old_ingester.is_none()
            && self.new_ingester.is_none()
            && self.new_start.is_none()
            && self.driver_token.is_none()
    }

    pub(in crate::cas_projection) fn finish_publication(self) {
        drop(self);
    }

    pub(in crate::cas_projection) fn dispose_after_adoption_failure(
        mut self,
    ) -> Result<(), ProjectionCoordinatorError> {
        let identity = self.new_epoch.identity;
        match (self.new_ingester.take(), self.new_start.take()) {
            (Some(ingester), Some(start)) => {
                let stopped = ingester.cancel_and_join(start);
                validate_provider_ingester_join(&stopped, identity)
            }
            (None, None) => Ok(()),
            (ingester, start) => {
                drop((ingester, start));
                Err(ProjectionCoordinatorError::ProjectionWorkerStopped)
            }
        }
    }
}

impl CandidateSetSealEpochBarrier<'_> {
    pub(in crate::cas_projection) fn validates(
        &self,
        expected_home: &Arc<HomeStore>,
        expected_epoch: ConnectionEpochIdentity,
    ) -> bool {
        !self.guard.is_inert()
            && self.guard.epoch().is_some_and(|epoch| {
                epoch.identity == expected_epoch && Arc::ptr_eq(&epoch.home, expected_home)
            })
    }
}

impl RecoveryPublicationEpochBarrier<'_> {
    pub(in crate::cas_projection) fn validates(
        &self,
        expected_home: &Arc<HomeStore>,
        expected_epoch: ConnectionEpochIdentity,
    ) -> bool {
        !self.guard.is_inert()
            && self.guard.epoch().is_some_and(|epoch| {
                epoch.identity == expected_epoch && Arc::ptr_eq(&epoch.home, expected_home)
            })
    }
}

impl ConnectionEpochAdoptionBarrier<'_> {
    pub(in crate::cas_projection) fn validates(
        &self,
        cut: PersistentFailureCutIdentity,
        replacement: &BoundConnectionEpoch,
    ) -> bool {
        !self.guard.is_inert()
            && Arc::ptr_eq(self.connection, replacement.connection())
            && self.guard.epoch().is_some_and(|epoch| {
                epoch.identity.home_id() == cut.home_id
                    && epoch.identity.home_generation() == cut.home_generation
                    && epoch.identity.service_generation() == cut.service_generation
            })
            && replacement.identity().home_id() == cut.home_id
            && replacement.identity().home_generation() > cut.home_generation
            && replacement.identity().service_generation() > cut.service_generation
    }

    pub(in crate::cas_projection) fn commit(
        &mut self,
        mut replacement: BoundConnectionEpoch,
        old_ingester: ProviderBrokerAdoptionStopped,
    ) -> AdoptedConnectionEpochAttachment {
        debug_assert!(Arc::ptr_eq(
            self.connection,
            &replacement.prepared.connection
        ));
        let endpoint = replacement
            .prepared
            .endpoint
            .take()
            .expect("preflighted replacement retains one forwarding endpoint");
        let old_endpoint = self.guard.replace(endpoint);
        debug_assert!(old_endpoint.is_some());
        self.armed = false;
        let startup = Arc::clone(&replacement.prepared.startup);
        AdoptedConnectionEpochAttachment {
            connection: Arc::clone(self.connection),
            new_epoch: Arc::clone(&replacement.prepared.epoch),
            old_endpoint,
            old_driver_worker: replacement.old_driver_worker.take(),
            old_ingester: Some(old_ingester),
            new_ingester: replacement.prepared.ingester.take(),
            new_start: replacement.prepared.start.take(),
            driver_token: replacement.driver_token.take(),
            startup,
            old_epoch_retired: false,
            publication_armed: false,
            #[cfg(test)]
            fail_ingester_arm: false,
        }
    }

    pub(in crate::cas_projection) fn mark_inert(&mut self) -> InertConnectionEpochAttachment {
        self.armed = false;
        InertConnectionEpochAttachment {
            endpoint: self.guard.mark_inert(),
        }
    }
}

impl Drop for ConnectionEpochAdoptionBarrier<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.guard.mark_inert_in_place();
            self.armed = false;
        }
    }
}

impl std::fmt::Debug for PreparedConnectionEpochError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedConnectionEpochError")
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}
