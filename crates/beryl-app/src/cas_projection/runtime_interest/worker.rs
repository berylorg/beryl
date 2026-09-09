use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;

const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(50);
static NEXT_ACTIVITY_PERIOD: AtomicU64 = AtomicU64::new(1);

pub(super) fn run(
    shared: Arc<RuntimeInterestShared>,
    runtime_id: RuntimeId,
    attempt: u64,
    launch: Box<dyn FnOnce() -> Result<Box<dyn RunningRuntime>, RuntimeFailure> + Send>,
) -> bool {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        run_inner(&shared, runtime_id, attempt, launch)
    }));
    let (clean, failure) = match outcome {
        Ok(result) => result,
        Err(_) => (false, Some(RuntimeFailure::WorkerPanicked)),
    };
    let mut state = shared.lock();
    if let Some(entry) = state.runtimes.get_mut(&runtime_id)
        && entry.attempt == attempt
    {
        entry.cleanup_complete = clean;
        entry.status = failure.map_or(
            RuntimeInterestStatus::Retired,
            RuntimeInterestStatus::Unavailable,
        );
    }
    shared.changed.notify_all();
    clean
}

fn run_inner(
    shared: &RuntimeInterestShared,
    runtime_id: RuntimeId,
    attempt: u64,
    launch: Box<dyn FnOnce() -> Result<Box<dyn RunningRuntime>, RuntimeFailure> + Send>,
) -> (bool, Option<RuntimeFailure>) {
    if !wanted(shared, &shared.lock(), runtime_id, attempt) {
        return (true, None);
    }
    let mut runtime = match launch() {
        Ok(runtime) => runtime,
        Err(failure) => {
            return (
                !matches!(
                    failure,
                    RuntimeFailure::BackendDisposal
                        | RuntimeFailure::AppRetirement
                        | RuntimeFailure::WorkerPanicked
                ),
                Some(failure),
            );
        }
    };
    let mut failure = runtime.poll_health().err();
    if failure.is_none() {
        let period =
            NEXT_ACTIVITY_PERIOD.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            });
        match period {
            Ok(period) => {
                let mut state = shared.lock();
                if wanted(shared, &state, runtime_id, attempt) {
                    if let Ok(command) = shared.commands.authorize() {
                        let _ = command.commit_if_current(|| {
                            state
                                .runtimes
                                .get_mut(&runtime_id)
                                .expect("current runtime")
                                .status = RuntimeInterestStatus::Ready(RuntimeReadiness {
                                process_generation: runtime.process_generation(),
                                activity_period: RuntimeActivityPeriod(period),
                            });
                        });
                    }
                    shared.changed.notify_all();
                }
            }
            Err(_) => failure = Some(RuntimeFailure::IdentityExhausted),
        }
    }
    while failure.is_none() {
        let state = shared.lock();
        if !wanted(shared, &state, runtime_id, attempt) {
            break;
        }
        let (state, _) = shared
            .changed
            .wait_timeout(state, HEALTH_POLL_INTERVAL)
            .unwrap_or_else(|poison| poison.into_inner());
        if !wanted(shared, &state, runtime_id, attempt) {
            break;
        }
        drop(state);
        failure = runtime.poll_health().err();
    }
    {
        let mut state = shared.lock();
        if let Some(entry) = state.runtimes.get_mut(&runtime_id)
            && entry.attempt == attempt
        {
            entry.status = failure.map_or(
                RuntimeInterestStatus::Retiring,
                RuntimeInterestStatus::Unavailable,
            );
        }
        shared.changed.notify_all();
    }
    match runtime.retire() {
        Ok(()) => (true, failure),
        Err(retirement_failure) => (false, Some(retirement_failure)),
    }
}

fn wanted(
    shared: &RuntimeInterestShared,
    state: &RuntimeInterestState,
    runtime_id: RuntimeId,
    attempt: u64,
) -> bool {
    !state.closed
        && shared.commands.is_open()
        && state.runtimes.get(&runtime_id).is_some_and(|entry| {
            entry.attempt == attempt
                && !entry.interests.is_empty()
                && matches!(
                    entry.status,
                    RuntimeInterestStatus::Starting | RuntimeInterestStatus::Ready(_)
                )
        })
}
