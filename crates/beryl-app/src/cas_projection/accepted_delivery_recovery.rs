use std::time::{SystemTime, UNIX_EPOCH};

mod compaction_access;
mod startup_access;

use beryl_home_store::{CursorReadLimits, HomeCandidateRecoveryAccess, HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use compaction_access::CompactionAccess;
use startup_access::StartupAccess;
use syndic_storage::{
    AbandonCompactionOperation, AbandonStopOperation, CompactionAbandonmentReason,
    CompactionAdmissionRead, CompactionOperationState, CompactionRecoveryCase,
    CompactionSettlement, DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
    DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS, DeliveryRecoveryCase,
    DeliveryRecoveryClassificationError, ProviderOperationKind, SettleCompactionOperation,
    SourceEventPayload, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnEndStatus,
    TurnIncompleteReason, TurnKind,
};

use super::{
    ProjectionCoordinatorError,
    accepted_input_scheduler::StartupRecoveryDiagnostics,
    live_source::{LiveSourceFrontier, LiveSourceTarget, publish_reconciled},
    ordinary::converge_terminal_history,
};

const STALE_REASON: &str = "restart lost active CAS projection authority";
const POINT_READ_BYTES: usize = 512 * 1024;

pub(super) fn recover_startup(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
) -> Result<StartupRecoveryDiagnostics, ProjectionCoordinatorError> {
    recover_startup_with_access(
        StartupAccess::Ordinary(home, home_id, home_generation),
        storage,
    )
}

pub(super) fn recover_startup_candidate(
    home: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
) -> Result<StartupRecoveryDiagnostics, ProjectionCoordinatorError> {
    recover_startup_with_access(StartupAccess::Candidate(home), storage)
}

fn recover_startup_with_access(
    home: StartupAccess<'_>,
    storage: &SyndicStorage,
) -> Result<StartupRecoveryDiagnostics, ProjectionCoordinatorError> {
    let mut diagnostics = StartupRecoveryDiagnostics::default();
    let mut cursor = None;
    let mut source_restart_used = false;
    'scan: loop {
        let page = match home.page(storage, cursor) {
            Ok(page) => page,
            Err(
                syndic_storage::SyndicReadError::StaleNonIdleGateSourceScan
                | syndic_storage::SyndicReadError::ConcurrentChange { .. },
            ) if !source_restart_used => {
                source_restart_used = true;
                cursor = None;
                continue 'scan;
            }
            Err(syndic_storage::SyndicReadError::Invariant(_)) => {
                return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant);
            }
            Err(_) => return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead),
        };
        diagnostics.page_reads = diagnostics.page_reads.saturating_add(1);
        for source in page.records() {
            diagnostics.cases = diagnostics.cases.saturating_add(1);
            #[cfg(feature = "test-faults")]
            super::test_faults::pause_startup_classification(
                source.thread_id(),
                diagnostics.page_reads,
            );
            let case = match home.classify(storage, source) {
                Ok(case) => case,
                Err(DeliveryRecoveryClassificationError::SourceDrift) if !source_restart_used => {
                    source_restart_used = true;
                    cursor = None;
                    continue 'scan;
                }
                Err(DeliveryRecoveryClassificationError::SourceDrift) => {
                    return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead);
                }
                Err(DeliveryRecoveryClassificationError::Corruption(_)) => {
                    return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant);
                }
                Err(DeliveryRecoveryClassificationError::Read(_)) => {
                    return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead);
                }
            };
            converge_case(home, storage, case, &mut diagnostics)?;
        }
        match page.next_cursor() {
            Some(next) => {
                cursor = Some(
                    home.rebase(storage, next)
                        .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)?,
                );
            }
            None => return Ok(diagnostics),
        }
    }
}

fn converge_case(
    home: StartupAccess<'_>,
    storage: &SyndicStorage,
    case: DeliveryRecoveryCase,
    diagnostics: &mut StartupRecoveryDiagnostics,
) -> Result<(), ProjectionCoordinatorError> {
    match case {
        DeliveryRecoveryCase::Pending { .. } => {
            diagnostics.pending_turns = diagnostics.pending_turns.saturating_add(1);
        }
        DeliveryRecoveryCase::Active(active) => {
            let observed_at = system_timestamp_at_least(active.minimum_timestamp())?;
            let request = active
                .generic_abandonment(STALE_REASON, observed_at)
                .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant)?;
            home.abandon_active(storage, &request)
                .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
            diagnostics.active_convergences = diagnostics.active_convergences.saturating_add(1);
            home.publish_terminal(storage, active.thread_id(), active.turn_id(), observed_at)?;
            diagnostics.terminal_convergences = diagnostics.terminal_convergences.saturating_add(1);
        }
        DeliveryRecoveryCase::Stopping(stopping) => {
            let provider_operation = matches!(
                stopping.target().turn_kind(),
                TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction)
            );
            let observed_at = system_timestamp_at_least(stopping.minimum_timestamp())?;
            let stale = stopping
                .startup_stale_binding(STALE_REASON, observed_at)
                .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant)?;
            let request = AbandonStopOperation::new(
                stopping.operation_id(),
                stopping.target().clone(),
                stopping.current_gate_revision(),
                stopping.stop_revision(),
                stopping.current_state_revision(),
                stopping.startup_abandonment_reason(),
                stale,
            );
            home.abandon_stop(storage, &request)
                .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
            diagnostics.active_convergences = diagnostics.active_convergences.saturating_add(1);
            if !provider_operation {
                home.converge_history(
                    storage,
                    stopping.target().thread_id(),
                    stopping.target().turn_id(),
                    observed_at,
                )
                .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
            }
            diagnostics.terminal_convergences = diagnostics.terminal_convergences.saturating_add(1);
        }
        DeliveryRecoveryCase::PostAbandonment {
            thread_id,
            turn_id,
            minimum_timestamp,
        } => {
            home.publish_terminal(storage, thread_id, turn_id, minimum_timestamp)?;
            diagnostics.terminal_convergences = diagnostics.terminal_convergences.saturating_add(1);
        }
        DeliveryRecoveryCase::FinalizingHistory {
            thread_id,
            turn_id,
            minimum_timestamp,
        } => {
            home.converge_history(storage, thread_id, turn_id, minimum_timestamp)
                .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
            diagnostics.terminal_convergences = diagnostics.terminal_convergences.saturating_add(1);
        }
        DeliveryRecoveryCase::DeferredCompaction { thread_id, turn_id } => {
            home.converge_compaction(storage, thread_id, turn_id)?;
            diagnostics.deferred_compactions = diagnostics.deferred_compactions.saturating_add(1);
        }
        DeliveryRecoveryCase::Settled { .. } => {}
    }
    Ok(())
}

pub(super) fn converge_compaction_restart(
    home: &HomeStore,
    storage: &SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
) -> Result<(), ProjectionCoordinatorError> {
    converge_compaction_with_access(
        CompactionAccess::Ordinary(home),
        storage,
        thread_id,
        turn_id,
    )
}

pub(super) fn converge_compaction_restart_candidate(
    home: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
) -> Result<(), ProjectionCoordinatorError> {
    converge_compaction_with_access(
        CompactionAccess::Candidate(home),
        storage,
        thread_id,
        turn_id,
    )
}

fn converge_compaction_with_access(
    home: CompactionAccess<'_>,
    storage: &SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
) -> Result<(), ProjectionCoordinatorError> {
    let operation = match home
        .admission(storage, thread_id, point_limit())
        .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)?
    {
        CompactionAdmissionRead::Existing(operation) if operation.target().turn_id() == turn_id => {
            operation
        }
        CompactionAdmissionRead::Existing(_)
        | CompactionAdmissionRead::Admissible(_)
        | CompactionAdmissionRead::Ineligible(_) => {
            return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant);
        }
    };
    let recovery = home
        .recovery(storage, operation.id(), point_limit())
        .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)?
        .ok_or(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant)?;
    let operation_id = operation.id();
    let command = match recovery {
        CompactionRecoveryCase::CancelBeforeDispatch(operation) => storage
            .current_settle_compaction_operation(SettleCompactionOperation::new(
                operation.id(),
                operation.revision(),
                CompactionSettlement::CancelledBeforeDispatch,
            )),
        CompactionRecoveryCase::FinishLocalNondispatch(operation) => storage
            .current_settle_compaction_operation(SettleCompactionOperation::new(
                operation.id(),
                operation.revision(),
                CompactionSettlement::LocalNondispatch,
            )),
        CompactionRecoveryCase::RetireRejectedTarget(operation) => storage
            .current_abandon_compaction_operation(AbandonCompactionOperation::new(
                operation.id(),
                operation.revision(),
                CompactionAbandonmentReason::ProviderRejectedBeforeCore,
            )),
        CompactionRecoveryCase::PossibleDispatch(operation) => storage
            .current_abandon_compaction_operation(AbandonCompactionOperation::new(
                operation.id(),
                operation.revision(),
                CompactionAbandonmentReason::StartupProcessGenerationLost,
            )),
        CompactionRecoveryCase::FinalizeSuccess(operation) => storage
            .current_settle_compaction_operation(SettleCompactionOperation::new(
                operation.id(),
                operation.revision(),
                CompactionSettlement::ManualSuccess,
            )),
        CompactionRecoveryCase::FinalizeInterruptedWithIdleEvidence(operation)
        | CompactionRecoveryCase::FinalizeFailure(operation) => storage
            .current_settle_compaction_operation(SettleCompactionOperation::new(
                operation.id(),
                operation.revision(),
                CompactionSettlement::ManualFailure,
            )),
        CompactionRecoveryCase::Settled(operation)
            if matches!(operation.state(), CompactionOperationState::Consumed(_)) =>
        {
            return Ok(());
        }
        CompactionRecoveryCase::Stopping(_) | CompactionRecoveryCase::Settled(_) => {
            return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant);
        }
    };
    match home.execute_current(command) {
        beryl_home_store::CommandOutcome::NotCommitted { evidence } => {
            return Err(ProjectionCoordinatorError::CommandNotCommitted(evidence));
        }
        beryl_home_store::CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
            local_finalization: _,
        } => {}
        beryl_home_store::CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
            local_finalization: _,
        } => {
            return Err(ProjectionCoordinatorError::CommandCommitted {
                receipt,
                later_failure,
            });
        }
        beryl_home_store::CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            return Err(ProjectionCoordinatorError::CommandIndeterminate { failure });
        }
    }
    #[cfg(feature = "test-faults")]
    crate::cas_projection::test_faults::pause_compaction_recovery_confirmation(thread_id);
    let settled = home
        .recovery(storage, operation_id, point_limit())
        .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)?
        .ok_or(ProjectionCoordinatorError::AcceptedDeliveryRecoveryInvariant)?;
    if !matches!(settled, CompactionRecoveryCase::Settled(_)) {
        return Err(ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication);
    }
    Ok(())
}

fn publish_source_less_terminal(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
    minimum_observed_at: SyndicTimestamp,
) -> Result<(), ProjectionCoordinatorError> {
    let target = LiveSourceTarget::new_source_less(thread_id, turn_id);
    let frontier = LiveSourceFrontier::read_at_least(
        home,
        storage,
        &target,
        minimum_observed_at,
        point_limit(),
    )
    .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
    let terminal = frontier
        .event(
            &target,
            None,
            SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
                TurnIncompleteReason::AuthorityLost,
            )),
        )
        .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
    publish_reconciled(
        home,
        home_id,
        home_generation,
        storage,
        &terminal,
        point_limit(),
    )
    .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
    converge_terminal_history(
        home,
        storage,
        thread_id,
        turn_id,
        minimum_observed_at,
        point_limit(),
        None,
    )
    .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)
}

fn startup_page_limits() -> CursorReadLimits {
    CursorReadLimits::new(
        DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
        DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
    )
    .expect("accepted-delivery recovery startup bounds are nonzero")
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_BYTES)
        .expect("accepted-delivery recovery point bound is nonzero")
}

fn system_timestamp_at_least(
    minimum: SyndicTimestamp,
) -> Result<SyndicTimestamp, ProjectionCoordinatorError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryClock)?;
    let millis = u64::try_from(elapsed.as_millis())
        .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryClock)?;
    Ok(SyndicTimestamp::from_unix_millis(
        millis.max(minimum.unix_millis()),
    ))
}
