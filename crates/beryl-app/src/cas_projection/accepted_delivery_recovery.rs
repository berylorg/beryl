use std::time::{SystemTime, UNIX_EPOCH};

use beryl_home_store::{CursorReadLimits, HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
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
    publication,
};

const STALE_REASON: &str = "restart lost active CAS projection authority";
const POINT_READ_BYTES: usize = 512 * 1024;

pub(super) fn recover_startup(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
) -> Result<StartupRecoveryDiagnostics, ProjectionCoordinatorError> {
    let mut diagnostics = StartupRecoveryDiagnostics::default();
    let mut cursor = None;
    let mut source_restart_used = false;
    'scan: loop {
        let page = storage
            .delivery_recovery_startup_page(home, cursor, startup_page_limits())
            .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)?;
        diagnostics.page_reads = diagnostics.page_reads.saturating_add(1);
        for source in page.records() {
            diagnostics.cases = diagnostics.cases.saturating_add(1);
            let case = match storage.classify_delivery_recovery(home, source, point_limit()) {
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
            converge_case(
                home,
                home_id,
                home_generation,
                &storage,
                case,
                &mut diagnostics,
            )?;
        }
        match page.next_cursor() {
            Some(next) => cursor = Some(next),
            None => return Ok(diagnostics),
        }
    }
}

fn converge_case(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
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
            publication::abandon_active_reconciled(
                home,
                home_id,
                home_generation,
                storage,
                &request,
                point_limit(),
            )
            .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
            diagnostics.active_convergences = diagnostics.active_convergences.saturating_add(1);
            publish_source_less_terminal(
                home,
                home_id,
                home_generation,
                storage,
                active.thread_id(),
                active.turn_id(),
                observed_at,
            )?;
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
            publication::abandon_stop_reconciled(
                home,
                home_id,
                home_generation,
                storage,
                &request,
                point_limit(),
            )
            .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
            diagnostics.active_convergences = diagnostics.active_convergences.saturating_add(1);
            if !provider_operation {
                converge_terminal_history(
                    home,
                    storage,
                    stopping.target().thread_id(),
                    stopping.target().turn_id(),
                    observed_at,
                    point_limit(),
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
            publish_source_less_terminal(
                home,
                home_id,
                home_generation,
                storage,
                thread_id,
                turn_id,
                minimum_timestamp,
            )?;
            diagnostics.terminal_convergences = diagnostics.terminal_convergences.saturating_add(1);
        }
        DeliveryRecoveryCase::FinalizingHistory {
            thread_id,
            turn_id,
            minimum_timestamp,
        } => {
            converge_terminal_history(
                home,
                storage,
                thread_id,
                turn_id,
                minimum_timestamp,
                point_limit(),
            )
            .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
            diagnostics.terminal_convergences = diagnostics.terminal_convergences.saturating_add(1);
        }
        DeliveryRecoveryCase::DeferredCompaction { thread_id, turn_id } => {
            converge_compaction_restart(home, storage, thread_id, turn_id)?;
            diagnostics.deferred_compactions = diagnostics.deferred_compactions.saturating_add(1);
        }
        DeliveryRecoveryCase::Settled { .. } => {}
    }
    Ok(())
}

fn converge_compaction_restart(
    home: &HomeStore,
    storage: &SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
) -> Result<(), ProjectionCoordinatorError> {
    let operation = match storage
        .compaction_admission_read(home, thread_id, point_limit())
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
    let recovery = storage
        .compaction_recovery_read(home, operation.id(), point_limit())
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
        } => {}
        beryl_home_store::CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
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
    let settled = storage
        .compaction_recovery_read(home, operation_id, point_limit())
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
