use super::*;
use beryl_model::JobRevision;
use beryl_state::BranchHandoffJobLifecycle;
use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn run(
    service: DiscussionSettlementService,
    limits: HandoffScanLimits,
    signal: Arc<HandoffSignal>,
    cancellation: CommandCancellation,
) -> Result<(), HandoffCoordinatorError> {
    while signal.wait() && !cancellation.is_cancelled() {
        scan(&service, limits, &signal, &cancellation)?;
        #[cfg(feature = "test-faults")]
        signal.test_update(|probe| probe.passes += 1);
    }
    Ok(())
}

fn scan(
    service: &DiscussionSettlementService,
    limits: HandoffScanLimits,
    signal: &Arc<HandoffSignal>,
    cancellation: &CommandCancellation,
) -> Result<(), HandoffCoordinatorError> {
    let mut after = None;
    let mut ready = VecDeque::<(JobId, JobRevision)>::new();
    let mut waited = false;
    loop {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        while !service.operations.capacity_ready() {
            waited = true;
            #[cfg(feature = "test-faults")]
            signal.test_update(|probe| probe.capacity_waits += 1);
            service.store.home_revision()?;
            if !signal.wait() || cancellation.is_cancelled() {
                return Ok(());
            }
        }
        let before = service.store.home_revision()?;
        let page = service
            .state
            .durable_jobs()
            .list_live(&service.store, after, limits.page())?;
        if service.store.home_revision()? != before {
            signal.wake_by_ref();
            continue;
        }
        let count = page.records().len().min(limits.ready_job_items().get());
        let more = page.has_more() || count < page.records().len();
        for job in page.records().iter().take(count) {
            ready.push_back((job.job_id(), job.revision()));
        }
        drop(page);
        #[cfg(feature = "test-faults")]
        signal.test_update(|probe| probe.maximum_ready = probe.maximum_ready.max(ready.len()));
        while let Some(&(job_id, revision)) = ready.front() {
            if cancellation.is_cancelled() {
                return Ok(());
            }
            match reconcile(service, job_id, revision, cancellation.clone(), signal) {
                Ok(()) => {}
                Err(HandoffCoordinatorError::Settlement(DiscussionSettlementError::Capacity)) => {
                    waited = true;
                    if !signal.wait() {
                        return Ok(());
                    }
                    continue;
                }
                Err(HandoffCoordinatorError::Settlement(
                    DiscussionSettlementError::DuplicateIdentity,
                )) => {}
                Err(HandoffCoordinatorError::Settlement(ref error)) if concurrent(error) => {
                    signal.wake_by_ref();
                }
                Err(HandoffCoordinatorError::Settlement(
                    DiscussionSettlementError::IdentityMismatch,
                )) if service
                    .state
                    .durable_jobs()
                    .job(&service.store, job_id)?
                    .is_some_and(|job| job.revision() != revision) =>
                {
                    signal.wake_by_ref();
                }
                Err(HandoffCoordinatorError::Settlement(
                    DiscussionSettlementError::Cancelled
                    | DiscussionSettlementError::Command(CommandError::CancelledBeforeAdmission),
                )) if cancellation.is_cancelled() => return Ok(()),
                Err(error) => return Err(error),
            }
            ready.pop_front();
            after = Some(job_id);
            #[cfg(feature = "test-faults")]
            signal.test_checkpoint(job_id, false);
        }
        if !more {
            if waited {
                signal.wake_by_ref();
            }
            return Ok(());
        }
    }
}

fn reconcile(
    service: &DiscussionSettlementService,
    job_id: JobId,
    revision: JobRevision,
    cancellation: CommandCancellation,
    signal: &Arc<HandoffSignal>,
) -> Result<(), HandoffCoordinatorError> {
    let Some(job) = service.state.durable_jobs().job(&service.store, job_id)? else {
        return Ok(());
    };
    if job.revision() != revision {
        return Err(DiscussionSettlementError::ConcurrentChange.into());
    }
    #[cfg(feature = "test-faults")]
    signal.test_checkpoint(job_id, true);
    #[cfg(not(feature = "test-faults"))]
    let _ = signal;
    let prepared = match job.lifecycle() {
        BranchHandoffJobLifecycle::WaitingResolvingTurn => service.prepare(job_id, cancellation)?,
        BranchHandoffJobLifecycle::WaitingParent => {
            let mut turn = [0; 16];
            let mut item = [0; 16];
            getrandom::fill(&mut turn).map_err(|_| HandoffCoordinatorError::Identity)?;
            getrandom::fill(&mut item).map_err(|_| HandoffCoordinatorError::Identity)?;
            service.prepare_parent_input(
                job_id,
                DiscussionParentInputRequest {
                    turn_id: SyndicTurnId::from_bytes(turn),
                    item_id: SyndicItemId::from_bytes(item),
                    admitted_at: timestamp()?,
                },
                cancellation,
            )?
        }
        BranchHandoffJobLifecycle::StartingParent | BranchHandoffJobLifecycle::ParentActive => {
            service.prepare_parent_execution(job_id, timestamp()?, cancellation)?
        }
        _ => None,
    };
    drop(job);
    let Some(prepared) = prepared else {
        return Ok(());
    };
    match prepared.execute() {
        DiscussionSettlementOutcome::Committed {
            later_failure: None,
            local_finalization: None,
            ..
        } => Ok(()),
        DiscussionSettlementOutcome::NotCommitted { evidence } => Err(evidence.into()),
        _ => Err(HandoffCoordinatorError::RecoveryRequired),
    }
}

fn timestamp() -> Result<SyndicTimestamp, HandoffCoordinatorError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| HandoffCoordinatorError::Identity)?
        .as_millis();
    Ok(SyndicTimestamp::from_unix_millis(
        u64::try_from(millis).map_err(|_| HandoffCoordinatorError::Identity)?,
    ))
}

fn concurrent(error: &DiscussionSettlementError) -> bool {
    matches!(
        error,
        DiscussionSettlementError::ConcurrentChange
            | DiscussionSettlementError::Syndic(
                syndic_storage::SyndicReadError::ConcurrentChange { .. }
            )
            | DiscussionSettlementError::Command(CommandError::Conflict { .. })
    )
}
