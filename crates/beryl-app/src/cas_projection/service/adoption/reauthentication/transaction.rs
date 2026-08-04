use std::sync::Arc;

use beryl_home_store::{HomeHealthState, HomeStore};

use super::super::ServiceAdoptionAttempt;
use super::{
    ProjectionCandidateId, ProjectionCandidateReauthenticationReason,
    TerminalAdoptedProjectionConnectionServiceReason, model::DormantRecoveredProjection,
    validation,
};
use crate::cas_projection::{
    connection::{
        CandidateSetConnectionOwnerSealFailure, CandidateSetConvergedProjectionConnectionOwner,
        ConnectionEpochIdentity, PendingProjectionConnectionOwner, PendingProjectionLeaseOwner,
        StableProjectionConnectionAuthentication, StableProjectionConnectionObservation,
        seal_pending_projection_connection_owners,
    },
    input_replay::point_limit,
    ordinary::{OrdinaryTurnExecutionError, preflight::PendingOrdinaryExecution},
    persistent_failure::{PendingProjectionGroupIdentity, PendingProjectionWitness},
    service::ProjectionConnectionService,
};

use crate::cas_projection::connection::registry::LoadedRegistryRecoveryObservation;

#[cfg(test)]
use super::test_support::{
    CandidateReauthenticationPauseRegistry, CandidateReauthenticationPauseStage,
};

struct ReauthenticationContext<'a> {
    home: &'a Arc<HomeStore>,
    service: &'a ProjectionConnectionService,
    assets: beryl_state::AssetState,
    expected_epoch: ConnectionEpochIdentity,
}

pub(super) enum CandidateSetSealFailure {
    AcceptedCandidate {
        reason: ProjectionCandidateReauthenticationReason,
        candidate_id: ProjectionCandidateId,
    },
    RetryableConnectionOwnerCapacity,
    Terminal(TerminalAdoptedProjectionConnectionServiceReason),
}

fn terminal(
    reason: TerminalAdoptedProjectionConnectionServiceReason,
) -> ProjectionCandidateReauthenticationReason {
    ProjectionCandidateReauthenticationReason::ServiceTerminal(reason)
}

pub(super) fn reauthenticate_pending_candidate(
    attempt: &ServiceAdoptionAttempt,
    candidate_id: ProjectionCandidateId,
    identity: &PendingProjectionGroupIdentity,
    witness: &PendingProjectionWitness,
    owner: &PendingProjectionLeaseOwner,
    #[cfg(test)] pauses: &CandidateReauthenticationPauseRegistry,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    #[cfg(not(test))]
    let _ = candidate_id;
    let context = context(attempt)?;
    #[cfg(test)]
    let fact_fault = pauses.take_fact_fault(candidate_id);
    validation::validate_candidate(
        attempt,
        identity,
        witness,
        owner,
        #[cfg(test)]
        fact_fault,
    )?;
    authenticate_pending(
        &context,
        identity.connection(),
        owner,
        AuthenticationStage::PreRead,
    )?;
    #[cfg(test)]
    pauses.pause_if_requested(
        candidate_id,
        CandidateReauthenticationPauseStage::AfterPreAuth,
    );
    #[cfg(test)]
    let pending = PendingOrdinaryExecution::read_with_confirmation_hook(
        context.home,
        context.service.storage,
        context.assets,
        witness,
        point_limit(),
        || {
            pauses.pause_if_requested(
                candidate_id,
                CandidateReauthenticationPauseStage::BeforeStableReadConfirmation,
            );
        },
    );
    #[cfg(not(test))]
    let pending = PendingOrdinaryExecution::read(
        context.home,
        context.service.storage,
        context.assets,
        witness,
        point_limit(),
    );
    #[cfg(test)]
    pauses.pause_if_requested(
        candidate_id,
        CandidateReauthenticationPauseStage::AfterStableRead,
    );
    authenticate_pending(
        &context,
        identity.connection(),
        owner,
        AuthenticationStage::PostRead,
    )?;
    validate_final_home(attempt, context.home)?;
    pending.map(|_| ()).map_err(classify_pending_read_error)
}

fn classify_pending_read_error(
    error: OrdinaryTurnExecutionError,
) -> ProjectionCandidateReauthenticationReason {
    match error {
        OrdinaryTurnExecutionError::PendingTurnUnavailable { .. } => {
            ProjectionCandidateReauthenticationReason::PendingOrdinaryTurnUnavailable
        }
        OrdinaryTurnExecutionError::ProjectionMismatch { .. } => {
            ProjectionCandidateReauthenticationReason::PendingOrdinaryProjectionMismatch
        }
        OrdinaryTurnExecutionError::ConcurrentChange { .. } => {
            ProjectionCandidateReauthenticationReason::PendingOrdinaryConcurrentChange
        }
        OrdinaryTurnExecutionError::InputContentUnavailable => {
            ProjectionCandidateReauthenticationReason::PendingOrdinaryInputContentUnavailable
        }
        OrdinaryTurnExecutionError::InputAssetReferenceSetMismatch => {
            ProjectionCandidateReauthenticationReason::PendingOrdinaryInputAssetReferenceSetMismatch
        }
        OrdinaryTurnExecutionError::Invariant(_) => {
            ProjectionCandidateReauthenticationReason::PendingOrdinaryInvariant
        }
        _ => ProjectionCandidateReauthenticationReason::PendingOrdinaryReadUnavailable,
    }
}

pub(super) fn authenticate_accepted_candidate(
    attempt: &ServiceAdoptionAttempt,
    accepted: &DormantRecoveredProjection,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    let context = context(attempt)?;
    if accepted.metadata().home_id() != attempt.metadata.home_id()
        || accepted.metadata().home_generation() != attempt.metadata.new_home_generation()
        || accepted.metadata().service_generation() != attempt.metadata.new_service_generation()
        || accepted.owner().stable_connection_observation() != accepted.identity().connection()
    {
        return Err(ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch);
    }
    validation::validate_candidate_identity(attempt, accepted.identity(), accepted.witness())?;
    authenticate_connection(
        &context,
        accepted.owner().stable_connection_observation(),
        AuthenticationStage::Seal,
    )?;
    authenticate_service_membership(
        context.service,
        accepted.owner().stable_connection_observation(),
        AuthenticationStage::Seal,
    )?;
    match accepted.owner().authenticate_live_exact() {
        Ok(true) => validate_final_home(attempt, context.home),
        Ok(false) => Err(ProjectionCandidateReauthenticationReason::SealRegistryTokenMismatch),
        Err(_) => Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::LoadedRegistryAuthenticationUnavailable,
        )),
    }
}

pub(super) fn seal_candidate_set_connections(
    attempt: &ServiceAdoptionAttempt,
    owners: &mut Vec<PendingProjectionConnectionOwner>,
    accepted_candidate_ids: &[ProjectionCandidateId],
    accepted_observations: &[LoadedRegistryRecoveryObservation],
    #[cfg(test)] pauses: &CandidateReauthenticationPauseRegistry,
) -> Result<Vec<CandidateSetConvergedProjectionConnectionOwner>, CandidateSetSealFailure> {
    if accepted_candidate_ids.len() != accepted_observations.len() {
        return Err(CandidateSetSealFailure::Terminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch,
        ));
    }
    let context = context(attempt).map_err(|reason| match reason {
        ProjectionCandidateReauthenticationReason::ServiceTerminal(reason) => {
            CandidateSetSealFailure::Terminal(reason)
        }
        _ => unreachable!("recovered service validation returns only service-terminal failure"),
    })?;
    let mut connections = Vec::new();
    let mut barriers = Vec::new();
    if connections.try_reserve_exact(owners.len()).is_err()
        || barriers.try_reserve_exact(owners.len()).is_err()
    {
        return Err(CandidateSetSealFailure::RetryableConnectionOwnerCapacity);
    }
    connections.extend(
        owners
            .iter()
            .map(|owner| Arc::clone(owner.stable_connection_for_inert_failure())),
    );
    for connection in &connections {
        barriers.push(
            connection
                .lock_epoch_for_candidate_set_seal()
                .map_err(|_| {
                    CandidateSetSealFailure::Terminal(
                        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionAuthenticationUnavailable,
                    )
                })?,
        );
    }
    if barriers
        .iter()
        .any(|barrier| !barrier.validates(context.home, context.expected_epoch))
    {
        return Err(CandidateSetSealFailure::Terminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch,
        ));
    }

    let service_connections = context.service.connections.lock().map_err(|_| {
        CandidateSetSealFailure::Terminal(
            TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipUnavailable,
        )
    })?;
    if service_connections.len() != connections.len()
        || connections.iter().any(|connection| {
            !service_connections.iter().any(|candidate| {
                Arc::ptr_eq(candidate, connection)
                    && candidate.identity_observation() == connection.identity_observation()
            })
        })
    {
        return Err(CandidateSetSealFailure::Terminal(
            TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipMismatch,
        ));
    }
    validate_final_home(attempt, context.home).map_err(|reason| match reason {
        ProjectionCandidateReauthenticationReason::ServiceTerminal(reason) => {
            CandidateSetSealFailure::Terminal(reason)
        }
        _ => unreachable!("final home validation returns only service-terminal failure"),
    })?;
    seal_pending_projection_connection_owners(
        owners,
        accepted_observations,
        #[cfg(test)]
        || pauses.pause_seal_if_requested(),
    )
    .map_err(|failure| classify_candidate_set_seal_failure(failure, accepted_candidate_ids))
}

pub(super) fn validate_recovered_service(
    attempt: &ServiceAdoptionAttempt,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    let context = context(attempt)?;
    validate_final_home(attempt, context.home)
}

fn classify_candidate_set_seal_failure(
    failure: CandidateSetConnectionOwnerSealFailure,
    accepted_candidate_ids: &[ProjectionCandidateId],
) -> CandidateSetSealFailure {
    match failure {
        CandidateSetConnectionOwnerSealFailure::RegistryTokenMismatch { observation_index } => {
            let Some(candidate_id) = accepted_candidate_ids.get(observation_index).copied() else {
                return CandidateSetSealFailure::Terminal(
                    TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch,
                );
            };
            CandidateSetSealFailure::AcceptedCandidate {
                reason: ProjectionCandidateReauthenticationReason::SealRegistryTokenMismatch,
                candidate_id,
            }
        }
        CandidateSetConnectionOwnerSealFailure::RegistryAuthenticationUnavailable => {
            CandidateSetSealFailure::Terminal(
                TerminalAdoptedProjectionConnectionServiceReason::LoadedRegistryAuthenticationUnavailable,
            )
        }
        CandidateSetConnectionOwnerSealFailure::AuthorityUnavailable => {
            CandidateSetSealFailure::Terminal(
                TerminalAdoptedProjectionConnectionServiceReason::StableConnectionAuthenticationUnavailable,
            )
        }
        CandidateSetConnectionOwnerSealFailure::CapacityUnavailable => {
            CandidateSetSealFailure::RetryableConnectionOwnerCapacity
        }
        CandidateSetConnectionOwnerSealFailure::ConnectionRetired => {
            CandidateSetSealFailure::Terminal(
                TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
            )
        }
        CandidateSetConnectionOwnerSealFailure::TopologyMismatch => {
            CandidateSetSealFailure::Terminal(
                TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch,
            )
        }
    }
}

fn context(
    attempt: &ServiceAdoptionAttempt,
) -> Result<ReauthenticationContext<'_>, ProjectionCandidateReauthenticationReason> {
    let service = attempt.adopted_service.as_ref().ok_or_else(|| {
        terminal(TerminalAdoptedProjectionConnectionServiceReason::RecoveredServiceUnavailable)
    })?;
    let beryl_state = attempt.adopted_beryl_state.as_ref().ok_or_else(|| {
        terminal(TerminalAdoptedProjectionConnectionServiceReason::RecoveredServiceUnavailable)
    })?;
    let startup = attempt.adopted_startup_gate.as_ref().ok_or_else(|| {
        terminal(TerminalAdoptedProjectionConnectionServiceReason::RecoveredServiceUnavailable)
    })?;
    let home = service.home.as_ref().ok_or_else(|| {
        terminal(TerminalAdoptedProjectionConnectionServiceReason::RecoveredServiceUnavailable)
    })?;
    let inventory = attempt.inventory.as_ref().ok_or_else(|| {
        terminal(TerminalAdoptedProjectionConnectionServiceReason::RecoveredServiceUnavailable)
    })?;
    if !startup.is_closed() || !Arc::ptr_eq(home, inventory.retained_home()) {
        return Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::RecoveredServiceUnavailable,
        ));
    }
    if home.home_id() != attempt.metadata.home_id()
        || service.home_id() != attempt.metadata.home_id()
        || inventory.home_id() != attempt.metadata.home_id()
    {
        return Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::RecoveredHomeIdentityMismatch,
        ));
    }
    if service.home_generation() != attempt.metadata.new_home_generation()
        || service.service_generation() != attempt.metadata.new_service_generation()
    {
        return Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::RecoveredHomeGenerationMismatch,
        ));
    }
    let health = home.health();
    if health.state() != HomeHealthState::Healthy {
        return Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::RecoveredHomeNotHealthy,
        ));
    }
    if health.generation() != Some(attempt.metadata.new_home_generation()) {
        return Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::RecoveredHomeGenerationMismatch,
        ));
    }
    Ok(ReauthenticationContext {
        home,
        service,
        assets: beryl_state.assets(),
        expected_epoch: ConnectionEpochIdentity::new(
            attempt.metadata.home_id(),
            attempt.metadata.new_home_generation(),
            attempt.metadata.new_service_generation(),
        ),
    })
}

#[derive(Clone, Copy)]
enum AuthenticationStage {
    PreRead,
    PostRead,
    Seal,
}

fn authenticate_pending(
    context: &ReauthenticationContext<'_>,
    connection: &StableProjectionConnectionObservation,
    owner: &PendingProjectionLeaseOwner,
    stage: AuthenticationStage,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    authenticate_connection(context, connection, stage)?;
    authenticate_service_membership(context.service, connection, stage)?;
    match owner.authenticate_live_exact() {
        Ok(true) => Ok(()),
        Ok(false) => Err(match stage {
            AuthenticationStage::PreRead => {
                ProjectionCandidateReauthenticationReason::PreReadRegistryTokenMismatch
            }
            AuthenticationStage::PostRead => {
                ProjectionCandidateReauthenticationReason::PostReadRegistryTokenMismatch
            }
            AuthenticationStage::Seal => {
                ProjectionCandidateReauthenticationReason::SealRegistryTokenMismatch
            }
        }),
        Err(_) => Err(match stage {
            AuthenticationStage::PreRead
            | AuthenticationStage::PostRead
            | AuthenticationStage::Seal => terminal(
                TerminalAdoptedProjectionConnectionServiceReason::LoadedRegistryAuthenticationUnavailable,
            ),
        }),
    }
}

fn authenticate_connection(
    context: &ReauthenticationContext<'_>,
    connection: &StableProjectionConnectionObservation,
    stage: AuthenticationStage,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    let _ = stage;
    match connection.authenticate_current_adopted_epoch(context.home, context.expected_epoch) {
        Ok(StableProjectionConnectionAuthentication::Current) => Ok(()),
        Ok(StableProjectionConnectionAuthentication::Retired) => Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
        )),
        Ok(StableProjectionConnectionAuthentication::Mismatch) => Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch,
        )),
        Err(_) => Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionAuthenticationUnavailable,
        )),
    }
}

fn authenticate_service_membership(
    service: &ProjectionConnectionService,
    connection: &StableProjectionConnectionObservation,
    stage: AuthenticationStage,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    let _ = stage;
    let connections = service.connections.lock().map_err(|_| {
        terminal(TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipUnavailable)
    })?;
    if connections
        .iter()
        .any(|candidate| connection.matches_connection(candidate))
    {
        Ok(())
    } else {
        Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipMismatch,
        ))
    }
}

fn validate_final_home(
    attempt: &ServiceAdoptionAttempt,
    home: &Arc<HomeStore>,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    let health = home.health();
    if home.home_id() == attempt.metadata.home_id()
        && health.state() == HomeHealthState::Healthy
        && health.generation() == Some(attempt.metadata.new_home_generation())
    {
        Ok(())
    } else {
        Err(terminal(
            TerminalAdoptedProjectionConnectionServiceReason::FinalRecoveredHomeMismatch,
        ))
    }
}
