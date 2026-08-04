use std::time::{SystemTime, UNIX_EPOCH};

use beryl_home_store::CommandError;
use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AcceptedInputPromotionStatus, AcceptedNextCandidate, PromoteAcceptedInput, SelectedPathProof,
    SyndicReadError, SyndicTimestamp,
};

use super::{
    super::{
        AcceptedInputSchedulerSignal, AcceptedInputWakeReason, SchedulerFailure, SchedulerRuntime,
        WorkerCompletion, WorkerDisposition, failure,
    },
    authority::{
        LeaseValidationAuthority, expected_admission_drift, expected_coordinator_drift,
        obsolete_admission_generation, obsolete_coordinator_generation,
    },
};
use crate::{
    cas_projection::{
        CasProjectionCoordinator, CasProjectionRequest, LiveEventTargetRegistrationError,
        LoadedProjectionReleaseError, OrdinaryNotStartedProjection, OrdinaryTurnCaptureLoss,
        OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionOutcome,
        ProjectionCancellationToken, ProjectionCoordinatorError, ProjectionExecutionError,
        ScheduledOrdinaryExecutionLease, connection::ConnectionPromotionReleaseOutcome,
    },
    input_admission::{accepted_input_promotion_command, accepted_input_promotion_status},
};

pub(super) fn spawn_worker(
    runtime: &mut SchedulerRuntime,
    candidate: AcceptedNextCandidate,
    mut lease: ScheduledOrdinaryExecutionLease,
) -> Result<(), SchedulerFailure> {
    let syndic_thread_id = candidate.thread_id();
    let Some(command) = failure::authorize(&runtime.context)? else {
        return Ok(());
    };
    let validator = runtime.context.lease_validator(command);
    let storage = runtime.context.storage;
    let cancellation = runtime.context.ordinary_cancellation.clone();
    let signal = runtime.context.signal.clone();
    let completions = runtime.completions.clone();
    let handle = std::thread::Builder::new()
        .name("beryl-scheduled-ordinary-execution".to_owned())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute_candidate(
                    &validator,
                    storage,
                    &signal,
                    &cancellation,
                    candidate,
                    &mut lease,
                )
            }));
            let disposition = result.unwrap_or(WorkerDisposition::Fatal);
            completions.publish(WorkerCompletion {
                thread_id: std::thread::current().id(),
                disposition,
            });
            drop(lease);
            signal.wake(AcceptedInputWakeReason::WorkerCompleted);
            disposition
        })
        .map_err(|_| SchedulerFailure::Fatal)?;
    runtime.register_next_worker(handle, syndic_thread_id);
    Ok(())
}

fn execute_candidate(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    signal: &AcceptedInputSchedulerSignal,
    cancellation: &ProjectionCancellationToken,
    candidate: AcceptedNextCandidate,
    lease: &mut ScheduledOrdinaryExecutionLease,
) -> WorkerDisposition {
    if cancellation.is_cancelled() {
        return WorkerDisposition::NextParked;
    }
    if let Err(error) = validator.validate(lease) {
        pause_obsolete_generation(candidate.thread_id(), obsolete_admission_generation(&error));
        return if failure::is_verification_pending_admission(&error, validator.home_generation()) {
            WorkerDisposition::VerificationPending
        } else if failure::is_cut_correlated_admission(&error, validator.home_generation()) {
            WorkerDisposition::PersistentHomeFailure
        } else if expected_admission_drift(&error) {
            WorkerDisposition::NextParked
        } else {
            WorkerDisposition::Fatal
        };
    }
    let promoted_at = match current_timestamp(candidate.minimum_promotion_timestamp()) {
        Ok(timestamp) => timestamp,
        Err(()) => return WorkerDisposition::Fatal,
    };
    let successor_turn_id = match fresh_turn_id() {
        Ok(id) => id,
        Err(()) => return WorkerDisposition::Fatal,
    };
    let successor_item_id = match fresh_item_id() {
        Ok(id) => id,
        Err(()) => return WorkerDisposition::Fatal,
    };
    let promotion =
        PromoteAcceptedInput::new(candidate, successor_turn_id, successor_item_id, promoted_at);
    let command = match accepted_input_promotion_command(
        &validator.home,
        storage,
        lease.assets(),
        promotion.clone(),
    ) {
        Ok(command) => command,
        Err(_) => {
            return classify_unbuilt_promotion(validator, storage, &promotion);
        }
    };
    if cancellation.is_cancelled() {
        return WorkerDisposition::NextParked;
    }
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_scheduled_promotion_reservation(
        promotion.thread_id(),
    );
    let reservation = match validator.reserve_promotion(lease) {
        Ok(Some(reservation)) => reservation,
        Ok(None) => return WorkerDisposition::NextParked,
        Err(error) if failure::is_cut_correlated_admission(&error, validator.home_generation()) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(error)
            if failure::is_verification_pending_admission(&error, validator.home_generation()) =>
        {
            return WorkerDisposition::VerificationPending;
        }
        Err(error) if expected_admission_drift(&error) => {
            return WorkerDisposition::NextParked;
        }
        Err(error) => {
            pause_obsolete_generation(promotion.thread_id(), obsolete_admission_generation(&error));
            return WorkerDisposition::Fatal;
        }
    };
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_scheduled_promotion(promotion.thread_id());
    let dispatch = validator.home.execute(command);
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_scheduled_promotion_reconciliation(
        promotion.thread_id(),
    );
    let dispatch_verification_pending = dispatch.as_ref().err().is_some_and(|source| {
        failure::is_verification_pending_command(source, validator.home_generation())
    });
    let dispatch_cut_correlated = dispatch.as_ref().err().is_some_and(|source| {
        failure::is_cut_correlated_command(source, validator.home_generation())
    });
    let promotion_result = if matches!(dispatch, Err(CommandError::Conflict { .. })) {
        Ok(None)
    } else {
        reconcile_promotion(validator, storage, lease.assets(), &promotion)
            .map(|status| Some((dispatch.is_ok(), status)))
    };
    match reservation.release() {
        Ok(ConnectionPromotionReleaseOutcome::Ordinary) => {}
        Ok(ConnectionPromotionReleaseOutcome::PersistentFailure) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Ok(ConnectionPromotionReleaseOutcome::Closed) => {
            return WorkerDisposition::NextParked;
        }
        Err(_) => return WorkerDisposition::Fatal,
    }
    let Some((dispatch_succeeded, status)) = (match promotion_result {
        Ok(result) => result,
        Err(SchedulerFailure::PersistentHomeFailure) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(SchedulerFailure::VerificationPending) => {
            return WorkerDisposition::VerificationPending;
        }
        Err(SchedulerFailure::Fatal) => return WorkerDisposition::Fatal,
    }) else {
        signal.wake(AcceptedInputWakeReason::AcceptedNextReady);
        return WorkerDisposition::NextContinue;
    };
    if !dispatch_succeeded && status == AcceptedInputPromotionStatus::Prior {
        if dispatch_verification_pending {
            return WorkerDisposition::VerificationPending;
        }
        if dispatch_cut_correlated {
            return WorkerDisposition::PersistentHomeFailure;
        }
    }
    match (dispatch_succeeded, status) {
        (_, AcceptedInputPromotionStatus::Exact) => {}
        (false, AcceptedInputPromotionStatus::Prior) => {
            return WorkerDisposition::NextParked;
        }
        (true, AcceptedInputPromotionStatus::Prior)
        | (_, AcceptedInputPromotionStatus::Collision) => return WorkerDisposition::Fatal,
    }

    signal.wake(AcceptedInputWakeReason::AcceptedNextReady);
    let selected_path = match current_selected_path(
        &validator.home,
        storage,
        promotion.thread_id(),
        validator.home_generation(),
    ) {
        Ok(path) => path,
        Err(SchedulerFailure::PersistentHomeFailure) => {
            return WorkerDisposition::PersistentHomeFailure;
        }
        Err(SchedulerFailure::VerificationPending) => {
            return WorkerDisposition::VerificationPending;
        }
        Err(SchedulerFailure::Fatal) => return WorkerDisposition::Fatal,
    };
    match execute_pending_turn(
        validator,
        storage,
        cancellation,
        promoted_at,
        selected_path,
        lease,
    ) {
        PendingTurnExecutionDisposition::Settled => WorkerDisposition::NextContinue,
        PendingTurnExecutionDisposition::ExpectedInterruption => WorkerDisposition::NextParked,
        PendingTurnExecutionDisposition::PersistentHomeFailure => {
            WorkerDisposition::PersistentHomeFailure
        }
        PendingTurnExecutionDisposition::VerificationPending => {
            WorkerDisposition::VerificationPending
        }
        PendingTurnExecutionDisposition::ProjectionRefused => WorkerDisposition::Fatal,
    }
}

fn classify_unbuilt_promotion(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    promotion: &PromoteAcceptedInput,
) -> WorkerDisposition {
    if let Err(error) = validator.ensure_current() {
        pause_obsolete_generation(
            promotion.thread_id(),
            obsolete_coordinator_generation(&error),
        );
        return if failure::is_verification_pending_coordinator(&error, validator.home_generation())
        {
            WorkerDisposition::VerificationPending
        } else if failure::is_cut_correlated_coordinator(&error, validator.home_generation()) {
            WorkerDisposition::PersistentHomeFailure
        } else if expected_coordinator_drift(&error) {
            WorkerDisposition::NextParked
        } else {
            WorkerDisposition::Fatal
        };
    }
    match storage.accepted_input_promotion_status(
        &validator.home,
        promotion,
        crate::cas_projection::input_replay::point_limit(),
    ) {
        Ok(AcceptedInputPromotionStatus::Collision)
        | Err(SyndicReadError::ConcurrentChange { .. }) => WorkerDisposition::NextContinue,
        Err(error) => match failure::from_syndic_read(&error, validator.home_generation()) {
            SchedulerFailure::VerificationPending => WorkerDisposition::VerificationPending,
            SchedulerFailure::PersistentHomeFailure => WorkerDisposition::PersistentHomeFailure,
            SchedulerFailure::Fatal => WorkerDisposition::Fatal,
        },
        Ok(AcceptedInputPromotionStatus::Prior | AcceptedInputPromotionStatus::Exact) => {
            WorkerDisposition::Fatal
        }
    }
}

fn pause_obsolete_generation(thread_id: SyndicThreadId, obsolete: bool) {
    #[cfg(feature = "test-faults")]
    if obsolete {
        crate::cas_projection::test_faults::pause_scheduled_generation_invalidation(thread_id);
    }
    #[cfg(not(feature = "test-faults"))]
    let _ = (thread_id, obsolete);
}

fn reconcile_promotion(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    assets: beryl_state::AssetState,
    promotion: &PromoteAcceptedInput,
) -> Result<AcceptedInputPromotionStatus, SchedulerFailure> {
    let read = || {
        accepted_input_promotion_status(
            &validator.home,
            storage,
            assets,
            promotion,
            crate::cas_projection::input_replay::point_limit(),
        )
    };
    match read() {
        Ok(status) => Ok(status),
        Err(_) => {
            validator
                .ensure_current()
                .map_err(|error| failure::from_coordinator(&error, validator.home_generation()))?;
            read().map_err(|error| {
                failure::from_input_admission_build(&error, validator.home_generation())
            })
        }
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn current_selected_path(
    home: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    thread_id: SyndicThreadId,
    home_generation: beryl_home_store::HomeGeneration,
) -> Result<SelectedPathProof, SchedulerFailure> {
    let thread = storage
        .thread(
            home,
            thread_id,
            crate::cas_projection::input_replay::point_limit(),
        )
        .map_err(|error| failure::from_syndic_read(&error, home_generation))?
        .ok_or(SchedulerFailure::Fatal)?;
    Ok(SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::accepted_input_scheduler) enum PendingTurnExecutionDisposition {
    Settled,
    ExpectedInterruption,
    VerificationPending,
    PersistentHomeFailure,
    ProjectionRefused,
}

pub(in crate::cas_projection::accepted_input_scheduler) fn execute_pending_turn(
    validator: &LeaseValidationAuthority,
    storage: syndic_storage::SyndicStorage,
    cancellation: &ProjectionCancellationToken,
    observed_at: SyndicTimestamp,
    selected_path: SelectedPathProof,
    lease: &mut ScheduledOrdinaryExecutionLease,
) -> PendingTurnExecutionDisposition {
    let thread_id = lease.thread_id();
    let execution_binding = lease.execution_binding().clone();
    let coordinator = match CasProjectionCoordinator::for_healthy_home(&validator.home) {
        Ok(coordinator) => coordinator,
        Err(error)
            if failure::is_verification_pending_coordinator(
                &error,
                validator.home_generation(),
            ) =>
        {
            return PendingTurnExecutionDisposition::VerificationPending;
        }
        Err(error)
            if failure::is_cut_correlated_coordinator(&error, validator.home_generation()) =>
        {
            return PendingTurnExecutionDisposition::PersistentHomeFailure;
        }
        Err(error) if expected_coordinator_drift(&error) => {
            return PendingTurnExecutionDisposition::ExpectedInterruption;
        }
        Err(_) => return PendingTurnExecutionDisposition::ProjectionRefused,
    };
    lease.with_execution_authority(|session, policy, assets, tools, flight| {
        let projection_request = CasProjectionRequest::new(
            thread_id,
            selected_path,
            execution_binding,
            policy.thread_options().clone(),
            policy.model_context_window_tokens(),
            observed_at,
            policy.projection_timeout(),
        );
        let projection = match coordinator.obtain_projection_in_flight(
            &validator.home,
            storage,
            session,
            &projection_request,
            cancellation,
            flight,
        ) {
            Ok(projection) => projection,
            Err(error) => {
                return classify_projection_error(error, validator.home_generation());
            }
        };
        let outcome = coordinator.execute_ordinary_turn_in_flight(
            &validator.home,
            storage,
            assets,
            projection,
            cancellation,
            policy.turn(),
            tools,
            flight,
        );
        match settle_ordinary_outcome(validator, outcome) {
            OrdinaryTurnSettlement::Settled => PendingTurnExecutionDisposition::Settled,
            OrdinaryTurnSettlement::VerificationPending => {
                PendingTurnExecutionDisposition::VerificationPending
            }
            OrdinaryTurnSettlement::PersistentHomeFailure => {
                PendingTurnExecutionDisposition::PersistentHomeFailure
            }
        }
    })
}

fn classify_projection_error(
    error: ProjectionExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> PendingTurnExecutionDisposition {
    if projection_error_verification_pending(&error, home_generation) {
        return PendingTurnExecutionDisposition::VerificationPending;
    }
    if projection_error_cut_correlated(&error, home_generation) {
        return PendingTurnExecutionDisposition::PersistentHomeFailure;
    }
    match error {
        ProjectionExecutionError::Cancelled => {
            PendingTurnExecutionDisposition::ExpectedInterruption
        }
        ProjectionExecutionError::Coordinator(error)
            if failure::is_verification_pending_coordinator(&error, home_generation) =>
        {
            PendingTurnExecutionDisposition::VerificationPending
        }
        ProjectionExecutionError::Coordinator(error)
            if failure::is_cut_correlated_coordinator(&error, home_generation) =>
        {
            PendingTurnExecutionDisposition::PersistentHomeFailure
        }
        ProjectionExecutionError::Coordinator(error) if expected_coordinator_drift(&error) => {
            PendingTurnExecutionDisposition::ExpectedInterruption
        }
        ProjectionExecutionError::Coordinator(
            ProjectionCoordinatorError::ProjectionConnectionUnavailable { .. },
        ) => PendingTurnExecutionDisposition::ExpectedInterruption,
        _ => PendingTurnExecutionDisposition::ProjectionRefused,
    }
}

fn projection_error_cut_correlated(
    error: &ProjectionExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        ProjectionExecutionError::Coordinator(source) => {
            failure::is_cut_correlated_coordinator(source, home_generation)
        }
        ProjectionExecutionError::SyndicRead(syndic_storage::SyndicReadError::Read(source))
        | ProjectionExecutionError::NativePlanning(syndic_storage::NativeProjectionError::Read(
            source,
        ))
        | ProjectionExecutionError::RecoveryProjection(
            syndic_storage::RecoveryProjectionError::Read(source),
        ) => failure::is_cut_correlated_read(source, home_generation),
        ProjectionExecutionError::Publication(source) => {
            failure::is_cut_correlated_publication(source, home_generation)
        }
        ProjectionExecutionError::LeaseRelease(source) => matches!(
            source.as_ref(),
            crate::cas_projection::LoadedProjectionReleaseError::Registry(source)
                if failure::is_cut_correlated_coordinator(source, home_generation)
        ),
        ProjectionExecutionError::AbandonmentFailed {
            primary,
            release,
            publication,
        } => {
            projection_error_cut_correlated(primary, home_generation)
                || release.as_deref().is_some_and(|source| {
                    matches!(
                        source,
                        crate::cas_projection::LoadedProjectionReleaseError::Registry(source)
                            if failure::is_cut_correlated_coordinator(source, home_generation)
                    )
                })
                || publication.as_deref().is_some_and(|source| {
                    failure::is_cut_correlated_publication(source, home_generation)
                })
        }
        _ => false,
    }
}

fn projection_error_verification_pending(
    error: &ProjectionExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        ProjectionExecutionError::Coordinator(source) => {
            failure::is_verification_pending_coordinator(source, home_generation)
        }
        ProjectionExecutionError::SyndicRead(syndic_storage::SyndicReadError::Read(source))
        | ProjectionExecutionError::NativePlanning(syndic_storage::NativeProjectionError::Read(
            source,
        ))
        | ProjectionExecutionError::RecoveryProjection(
            syndic_storage::RecoveryProjectionError::Read(source),
        ) => failure::is_verification_pending_read(source, home_generation),
        ProjectionExecutionError::Publication(source) => {
            failure::is_verification_pending_publication(source, home_generation)
        }
        ProjectionExecutionError::LeaseRelease(source) => matches!(
            source.as_ref(),
            crate::cas_projection::LoadedProjectionReleaseError::Registry(source)
                if failure::is_verification_pending_coordinator(source, home_generation)
        ),
        ProjectionExecutionError::AbandonmentFailed {
            primary,
            release,
            publication,
        } => {
            projection_error_verification_pending(primary, home_generation)
                || release.as_deref().is_some_and(|source| {
                    matches!(
                        source,
                        crate::cas_projection::LoadedProjectionReleaseError::Registry(source)
                            if failure::is_verification_pending_coordinator(source, home_generation)
                    )
                })
                || publication.as_deref().is_some_and(|source| {
                    failure::is_verification_pending_publication(source, home_generation)
                })
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn settle_ordinary_outcome(
    validator: &LeaseValidationAuthority,
    outcome: Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
) -> OrdinaryTurnSettlement {
    let verification_pending =
        ordinary_outcome_verification_pending(&outcome, validator.home_generation());
    let cut_correlated = ordinary_outcome_cut_correlated(&outcome, validator.home_generation());
    match outcome {
        Ok(OrdinaryTurnExecutionOutcome::NotStarted {
            projection: OrdinaryNotStartedProjection::Retained(projection),
            ..
        }) => {
            release_or_retain_projection(validator, projection);
        }
        Ok(OrdinaryTurnExecutionOutcome::NotStarted {
            projection: OrdinaryNotStartedProjection::Unavailable { .. },
            ..
        }) => {}
        Ok(OrdinaryTurnExecutionOutcome::Terminal { projection, .. }) => {
            release_or_retain_projection(validator, projection);
        }
        Ok(OrdinaryTurnExecutionOutcome::ReacquisitionRequired { anchor, .. }) => {
            if validator.observe_persistent_failure() {
                validator.retain_failed_reacquisition_anchor(*anchor);
            }
        }
        Ok(OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled { .. }) => {}
        Ok(OrdinaryTurnExecutionOutcome::Incomplete { .. }) => {}
        Err(OrdinaryTurnExecutionFailure::PreActivation { projection, .. }) => {
            release_or_retain_projection(validator, projection);
        }
        Err(
            OrdinaryTurnExecutionFailure::Activation { .. }
            | OrdinaryTurnExecutionFailure::AfterActivation { .. },
        ) => {}
    }
    let settlement = ordinary_typed_settlement(verification_pending, cut_correlated);
    if settlement != OrdinaryTurnSettlement::Settled {
        let _ = validator.observe_persistent_failure();
    }
    settlement
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::accepted_input_scheduler) enum OrdinaryTurnSettlement {
    Settled,
    VerificationPending,
    PersistentHomeFailure,
}

fn ordinary_typed_settlement(
    verification_pending: bool,
    cut_correlated: bool,
) -> OrdinaryTurnSettlement {
    if verification_pending {
        OrdinaryTurnSettlement::VerificationPending
    } else if cut_correlated {
        OrdinaryTurnSettlement::PersistentHomeFailure
    } else {
        OrdinaryTurnSettlement::Settled
    }
}

fn ordinary_outcome_verification_pending(
    outcome: &Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match outcome {
        Ok(OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::StartAuthorityLost(source),
        }) => projection_error_verification_pending(source, home_generation),
        Err(OrdinaryTurnExecutionFailure::PreActivation { source, .. })
        | Err(OrdinaryTurnExecutionFailure::Activation { source })
        | Err(OrdinaryTurnExecutionFailure::AfterActivation { source }) => {
            ordinary_error_verification_pending(source, home_generation)
        }
        _ => false,
    }
}

fn ordinary_outcome_cut_correlated(
    outcome: &Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure>,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match outcome {
        Ok(OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::StartAuthorityLost(source),
        }) => projection_error_cut_correlated(source, home_generation),
        Err(OrdinaryTurnExecutionFailure::PreActivation { source, .. })
        | Err(OrdinaryTurnExecutionFailure::Activation { source })
        | Err(OrdinaryTurnExecutionFailure::AfterActivation { source }) => {
            ordinary_error_cut_correlated(source, home_generation)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn ordinary_error_verification_pending(
    error: &OrdinaryTurnExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        OrdinaryTurnExecutionError::Coordinator(source) => {
            failure::is_verification_pending_coordinator(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeRead(source) => {
            failure::is_verification_pending_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeCommand(source) => {
            failure::is_verification_pending_command(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputReplayHomeNotHealthy {
            state: beryl_home_store::HomeHealthState::Verifying,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            expected_home_id == actual_home_id
                && *expected_generation == home_generation
                && *actual_generation == home_generation
        }
        OrdinaryTurnExecutionError::Read(SyndicReadError::Read(source)) => {
            failure::is_verification_pending_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::AssetRead(beryl_state::AssetReadError::Read(source)) => {
            failure::is_verification_pending_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::TargetRegistration(
            LiveEventTargetRegistrationError::ProjectionRegistry(source),
        ) => failure::is_verification_pending_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::ProjectionExecution(source) => {
            projection_error_verification_pending(source, home_generation)
        }
        OrdinaryTurnExecutionError::ReacquisitionAnchor(
            LoadedProjectionReleaseError::Registry(source),
        ) => failure::is_verification_pending_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::Publication(source) => {
            failure::is_verification_pending_publication(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputAssetSidecar(source) => {
            failure::is_verification_pending_sidecar(source, home_generation)
        }
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn ordinary_error_cut_correlated(
    error: &OrdinaryTurnExecutionError,
    home_generation: beryl_home_store::HomeGeneration,
) -> bool {
    match error {
        OrdinaryTurnExecutionError::Coordinator(source) => {
            failure::is_cut_correlated_coordinator(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeRead(source) => {
            failure::is_cut_correlated_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::HomeCommand(source) => {
            failure::is_cut_correlated_command(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputReplayHomeNotHealthy {
            state: beryl_home_store::HomeHealthState::Failed,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            expected_home_id == actual_home_id
                && *expected_generation == home_generation
                && *actual_generation == home_generation
        }
        OrdinaryTurnExecutionError::Read(SyndicReadError::Read(source)) => {
            failure::is_cut_correlated_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::AssetRead(beryl_state::AssetReadError::Read(source)) => {
            failure::is_cut_correlated_read(source, home_generation)
        }
        OrdinaryTurnExecutionError::TargetRegistration(
            LiveEventTargetRegistrationError::ProjectionRegistry(source),
        ) => failure::is_cut_correlated_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::ProjectionExecution(source) => {
            projection_error_cut_correlated(source, home_generation)
        }
        OrdinaryTurnExecutionError::ReacquisitionAnchor(
            LoadedProjectionReleaseError::Registry(source),
        ) => failure::is_cut_correlated_coordinator(source, home_generation),
        OrdinaryTurnExecutionError::Publication(source) => {
            failure::is_cut_correlated_publication(source, home_generation)
        }
        OrdinaryTurnExecutionError::InputAssetSidecar(source) => {
            failure::is_cut_correlated_sidecar(source, home_generation)
        }
        _ => false,
    }
}

fn release_or_retain_projection(
    validator: &LeaseValidationAuthority,
    projection: Box<crate::cas_projection::LoadedCasProjection>,
) {
    if validator.observe_persistent_failure() {
        validator.retain_failed_projection(*projection);
    } else {
        let _ = projection.release();
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn current_timestamp(
    floor: SyndicTimestamp,
) -> Result<SyndicTimestamp, ()> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ())?
        .as_millis();
    let millis = u64::try_from(millis).map_err(|_| ())?;
    Ok(SyndicTimestamp::from_unix_millis(
        millis.max(floor.unix_millis()),
    ))
}

fn fresh_turn_id() -> Result<SyndicTurnId, ()> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(SyndicTurnId::from_bytes(bytes))
}

fn fresh_item_id() -> Result<SyndicItemId, ()> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(SyndicItemId::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/ordinary_turn_settlement.rs"
    ));
}
