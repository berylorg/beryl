use std::{sync::mpsc, time::Instant};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore, RecoveryRetrySchedule};
use beryl_state::{BerylState, BerylStateReacquireError};
use syndic_storage::SyndicStorage;

use super::{provider::ProviderFactoryOwner, slot::RunningServiceSlot};
use crate::cas_projection::{
    AdoptedUnpublishedProjectionConnectionService, PersistentFailureCutHandoff,
    PersistentFailurePendingProjectionQuarantine,
    PersistentFailurePendingProjectionQuarantineError, PersistentFailureServiceAdoptionError,
    ProjectionServiceConfig, ScheduledOrdinaryProviderEpochContext,
    UnpublishedProjectionConnectionService, UnpublishedProjectionConnectionServiceBuildError,
};

/// The exact consuming input from Phase 92 into the mounted Phase 93 adoption boundary.
///
/// This owner exposes no convergence, scheduler, publication, or command capability. Phase 93 may
/// consume it exactly once into adoption after attaching the replacement supervisor notification.
#[must_use = "the Phase 92 owner retains both the failed-cut quarantine and dormant replacement"]
pub(super) struct PersistentFailurePendingProjectionQuarantineDormantReplacement {
    quarantine: PersistentFailurePendingProjectionQuarantine,
    replacement: UnpublishedProjectionConnectionService,
}

/// Owning terminal failure while crossing the Phase 92-to-93 adoption seam.
#[must_use = "the failure retains both Phase 92 inputs or the complete inert adoption attempt"]
pub(super) enum PersistentFailureDormantReplacementAdoptionFailure {
    NotificationAttachment {
        quarantine: PersistentFailurePendingProjectionQuarantine,
        replacement: UnpublishedProjectionConnectionService,
    },
    Adoption(PersistentFailureServiceAdoptionError),
}

pub(super) enum Phase93TerminalDispositionFailure {
    Replacement(crate::cas_projection::ProjectionConnectionServiceCloseError),
    Quarantine,
    Adoption(crate::cas_projection::ProjectionConnectionServiceCloseError),
}

pub(super) enum Phase92FailureTerminalDispositionFailure {
    Inventory,
    Quarantine,
    QuarantineDisposition,
}

/// Owning failure while assembling the Phase 92 unmounted recovery result.
#[must_use = "the failure retains the only remaining failed-cut authority"]
pub(super) enum PersistentFailurePendingProjectionQuarantineDormantReplacementFailure {
    Inventory(crate::cas_projection::PersistentFailureRecoveryInventoryError),
    Quarantine(PersistentFailurePendingProjectionQuarantineError),
    WrongHome {
        quarantine: PersistentFailurePendingProjectionQuarantine,
    },
    RetryShutdown {
        quarantine: PersistentFailurePendingProjectionQuarantine,
    },
    NotNewerHealthyGeneration {
        quarantine: PersistentFailurePendingProjectionQuarantine,
    },
    BerylState {
        quarantine: PersistentFailurePendingProjectionQuarantine,
        error: BerylStateReacquireError,
    },
    SyndicStorage {
        quarantine: PersistentFailurePendingProjectionQuarantine,
        error: beryl_home_store::DomainHandleError,
    },
    Provider {
        quarantine: PersistentFailurePendingProjectionQuarantine,
    },
    Service {
        quarantine: PersistentFailurePendingProjectionQuarantine,
        error: UnpublishedProjectionConnectionServiceBuildError,
    },
}

impl PersistentFailurePendingProjectionQuarantineDormantReplacementFailure {
    pub(super) fn dispose_for_worker_terminal(
        self,
    ) -> Result<(), Phase92FailureTerminalDispositionFailure> {
        match self {
            Self::Inventory(error) => error
                .dispose_for_supervisor_shutdown()
                .map_err(|()| Phase92FailureTerminalDispositionFailure::Inventory),
            Self::Quarantine(error) => error
                .dispose_for_supervisor_shutdown()
                .map_err(|()| Phase92FailureTerminalDispositionFailure::Quarantine),
            Self::WrongHome { quarantine }
            | Self::RetryShutdown { quarantine }
            | Self::NotNewerHealthyGeneration { quarantine }
            | Self::BerylState { quarantine, .. }
            | Self::SyndicStorage { quarantine, .. }
            | Self::Provider { quarantine }
            | Self::Service { quarantine, .. } => quarantine
                .dispose_for_supervisor_shutdown()
                .map_err(|()| Phase92FailureTerminalDispositionFailure::QuarantineDisposition),
        }
    }
}

pub(super) enum RecoveryBuildOutcome {
    Dormant(PersistentFailurePendingProjectionQuarantineDormantReplacement),
    Shutdown,
    Terminal(Option<PersistentFailurePendingProjectionQuarantineDormantReplacementFailure>),
}

impl PersistentFailurePendingProjectionQuarantineDormantReplacement {
    pub(super) fn adopt(
        self,
        signal: mpsc::SyncSender<()>,
    ) -> Result<
        AdoptedUnpublishedProjectionConnectionService,
        PersistentFailureDormantReplacementAdoptionFailure,
    > {
        let Self {
            quarantine,
            replacement,
        } = self;
        if replacement.attach_recovery_supervisor(signal).is_err() {
            return Err(
                PersistentFailureDormantReplacementAdoptionFailure::NotificationAttachment {
                    quarantine,
                    replacement,
                },
            );
        }
        quarantine
            .adopt_unpublished_service(replacement)
            .map_err(PersistentFailureDormantReplacementAdoptionFailure::Adoption)
    }
}

impl PersistentFailureDormantReplacementAdoptionFailure {
    pub(super) fn dispose_for_worker_terminal(
        self,
    ) -> Result<(), Phase93TerminalDispositionFailure> {
        let (quarantine, replacement) = match self {
            Self::Adoption(error) => {
                return error
                    .dispose()
                    .map_err(Phase93TerminalDispositionFailure::Adoption);
            }
            Self::NotificationAttachment {
                quarantine,
                replacement,
            } => (quarantine, replacement),
        };
        let replacement_failure = replacement.dispose_for_supervisor_terminal().err();
        let quarantine_failed = quarantine.dispose_for_supervisor_shutdown().is_err();
        if quarantine_failed {
            Err(Phase93TerminalDispositionFailure::Quarantine)
        } else if let Some(error) = replacement_failure {
            Err(Phase93TerminalDispositionFailure::Replacement(error))
        } else {
            Ok(())
        }
    }
}

/// Converts one fully withdrawn failed service into the unmounted Phase 92 owner.
///
/// The call intentionally stops after the dormant service constructor. In particular, it does not
/// adopt quarantine authority, converge recovered state, dequeue scheduler work, start workers, or
/// publish a replacement service.
pub(super) fn build_dormant_replacement(
    handoff: PersistentFailureCutHandoff,
    home: &std::sync::Arc<HomeStore>,
    config: ProjectionServiceConfig,
    receiver: &mpsc::Receiver<()>,
    slot: &RunningServiceSlot,
    provider_factory: &mut ProviderFactoryOwner,
) -> Result<
    PersistentFailurePendingProjectionQuarantineDormantReplacement,
    PersistentFailurePendingProjectionQuarantineDormantReplacementFailure,
> {
    let expected_home_id = handoff.home_id();
    let failed_generation = handoff.home_generation();
    let inventory = handoff.into_recovery_inventory().map_err(
        PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::Inventory,
    )?;
    let quarantine = inventory.into_pending_projection_quarantine().map_err(
        PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::Quarantine,
    )?;
    if home.home_id() != expected_home_id || quarantine.home_id() != expected_home_id {
        return Err(
            PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::WrongHome {
                quarantine,
            },
        );
    }

    let quarantine =
        reopen_same_home_until_newer(quarantine, home, failed_generation, receiver, slot)?;
    let state = match BerylState::reacquire(home) {
        Ok(state) => state,
        Err(error) => {
            return Err(
                PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::BerylState {
                    quarantine,
                    error,
                },
            );
        }
    };
    let storage = match SyndicStorage::reacquire(home) {
        Ok(storage) => storage,
        Err(error) => {
            return Err(
                PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::SyndicStorage {
                    quarantine,
                    error,
                },
            );
        }
    };
    let Some(generation) = healthy_newer_generation(home, failed_generation) else {
        return Err(
            PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::NotNewerHealthyGeneration {
                quarantine,
            },
        );
    };
    let provider = match provider_factory.create_epoch(ScheduledOrdinaryProviderEpochContext::new(
        expected_home_id,
        generation,
        state,
    )) {
        Ok(provider) => provider,
        Err(_) => {
            return Err(
                PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::Provider {
                    quarantine,
                },
            );
        }
    };
    let replacement = match UnpublishedProjectionConnectionService::from_recovered_handles(
        std::sync::Arc::clone(home),
        state,
        storage,
        config,
        provider,
    ) {
        Ok(replacement) => replacement,
        Err(error) => {
            return Err(
                PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::Service {
                    quarantine,
                    error,
                },
            );
        }
    };
    let owner = PersistentFailurePendingProjectionQuarantineDormantReplacement {
        quarantine,
        replacement,
    };
    Ok(owner)
}

pub(super) fn build_dormant_replacement_for_supervisor(
    handoff: PersistentFailureCutHandoff,
    home: &std::sync::Arc<HomeStore>,
    config: ProjectionServiceConfig,
    receiver: &mpsc::Receiver<()>,
    slot: &RunningServiceSlot,
    provider_factory: &mut ProviderFactoryOwner,
) -> RecoveryBuildOutcome {
    match build_dormant_replacement(handoff, home, config, receiver, slot, provider_factory) {
        Ok(owner) => RecoveryBuildOutcome::Dormant(owner),
        Err(
            PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::RetryShutdown {
                quarantine,
            },
        ) => {
            if quarantine.dispose_for_supervisor_shutdown().is_ok() {
                RecoveryBuildOutcome::Shutdown
            } else {
                RecoveryBuildOutcome::Terminal(None)
            }
        }
        Err(owning_failure) => RecoveryBuildOutcome::Terminal(Some(owning_failure)),
    }
}

fn reopen_same_home_until_newer(
    quarantine: PersistentFailurePendingProjectionQuarantine,
    home: &std::sync::Arc<HomeStore>,
    failed_generation: HomeGeneration,
    receiver: &mpsc::Receiver<()>,
    slot: &RunningServiceSlot,
) -> Result<
    PersistentFailurePendingProjectionQuarantine,
    PersistentFailurePendingProjectionQuarantineDormantReplacementFailure,
> {
    let mut schedule = RecoveryRetrySchedule::default();
    loop {
        if slot.is_shutting_down() {
            return Err(
                PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::RetryShutdown {
                    quarantine,
                },
            );
        }
        match home.recover_same_home() {
            Ok(_) if healthy_newer_generation(home, failed_generation).is_some() => {
                return Ok(quarantine);
            }
            Ok(_) => {
                return Err(
                    PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::NotNewerHealthyGeneration {
                        quarantine,
                    },
                );
            }
            Err(_) if wait_retry_delay(receiver, slot, schedule.next_delay()) => {
                return Err(
                    PersistentFailurePendingProjectionQuarantineDormantReplacementFailure::RetryShutdown {
                        quarantine,
                    },
                );
            }
            Err(_) => {}
        }
    }
}

fn healthy_newer_generation(
    home: &HomeStore,
    failed_generation: HomeGeneration,
) -> Option<HomeGeneration> {
    let health = home.health();
    (health.state() == HomeHealthState::Healthy)
        .then(|| health.generation())
        .flatten()
        .filter(|generation| *generation > failed_generation)
}

fn wait_retry_delay(
    receiver: &mpsc::Receiver<()>,
    slot: &RunningServiceSlot,
    delay: std::time::Duration,
) -> bool {
    let deadline = Instant::now() + delay;
    loop {
        if slot.is_shutting_down() {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(()) if slot.is_shutting_down() => return true,
            Ok(()) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => return false,
            Err(mpsc::RecvTimeoutError::Disconnected) => return true,
        }
    }
}
