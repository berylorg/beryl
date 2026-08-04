use std::{sync::Arc, thread, time::Duration};

use beryl_backend::ManagedBackendError;

use crate::cas_projection::connection::ConnectionRequestSession;
use crate::cas_projection::{
    AdmittedProjectionSession, ProjectionCancellationToken, ProjectionExecutionError,
};

const NATIVE_SOURCE_ATTEMPT_LIMIT: u8 = 3;
const NATIVE_SOURCE_RETRY_DELAYS: [Duration; 2] =
    [Duration::from_millis(50), Duration::from_millis(150)];

pub(in crate::cas_projection) enum NativeCallFailure {
    Terminal(ProjectionExecutionError),
    RetryExhausted {
        failed_attempts: u8,
        last_failure: Box<ManagedBackendError>,
    },
}

pub(in crate::cas_projection) fn call_native_with_retry<T, F>(
    session: &AdmittedProjectionSession,
    cancellation: &ProjectionCancellationToken,
    operation: F,
) -> Result<T, NativeCallFailure>
where
    T: Send + 'static,
    F: Fn(&mut ConnectionRequestSession<'_>) -> Result<T, ManagedBackendError>
        + Send
        + Sync
        + 'static,
{
    let operation = Arc::new(operation);
    for attempt in 1..=NATIVE_SOURCE_ATTEMPT_LIMIT {
        if cancellation.is_cancelled() {
            return Err(NativeCallFailure::Terminal(
                ProjectionExecutionError::Cancelled,
            ));
        }
        let operation = Arc::clone(&operation);
        match session.call(move |backend| operation(backend)) {
            Ok(value) => return Ok(value),
            Err(ProjectionExecutionError::Backend(error))
                if matches!(&*error, ManagedBackendError::RequestFailed { .. }) =>
            {
                if attempt == NATIVE_SOURCE_ATTEMPT_LIMIT {
                    return Err(NativeCallFailure::RetryExhausted {
                        failed_attempts: attempt,
                        last_failure: error,
                    });
                }
                if let Err(error) = wait_for_native_retry(attempt, cancellation) {
                    return Err(NativeCallFailure::Terminal(error));
                }
            }
            Err(error) => return Err(NativeCallFailure::Terminal(error)),
        }
    }
    unreachable!("the positive native retry limit always returns from the attempt loop")
}

fn wait_for_native_retry(
    failed_attempt: u8,
    cancellation: &ProjectionCancellationToken,
) -> Result<(), ProjectionExecutionError> {
    let mut remaining = NATIVE_SOURCE_RETRY_DELAYS[usize::from(failed_attempt - 1)];
    let slice = Duration::from_millis(10);
    while !remaining.is_zero() {
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        let sleep_for = remaining.min(slice);
        thread::sleep(sleep_for);
        remaining = remaining.saturating_sub(sleep_for);
    }
    Ok(())
}
