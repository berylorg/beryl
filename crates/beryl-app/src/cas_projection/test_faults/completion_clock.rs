use std::{
    sync::{Mutex, OnceLock, atomic::Ordering},
    time::{Duration, UNIX_EPOCH},
};

use beryl_model::{BerylHomeId, CasProcessGeneration, CasThreadId, SyndicThreadId};

use crate::cas_projection::ProjectionExecutionError;

#[derive(Clone, Copy, Debug)]
pub enum ProjectionCompletionClockFault {
    BeforeUnixEpoch,
    OutOfRange,
}

struct ClockFault {
    token: u64,
    home: BerylHomeId,
    thread: SyndicThreadId,
    cas_thread: CasThreadId,
    process: CasProcessGeneration,
    fault: ProjectionCompletionClockFault,
}

pub struct ProjectionCompletionClockFaultGuard {
    token: u64,
}

pub fn install_projection_completion_clock_fault(
    home: BerylHomeId,
    thread: SyndicThreadId,
    cas_thread: CasThreadId,
    process: CasProcessGeneration,
    fault: ProjectionCompletionClockFault,
) -> ProjectionCompletionClockFaultGuard {
    let token = super::NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let mut slot = slot().lock().unwrap();
    assert!(
        slot.is_none(),
        "one projection completion clock fault may be installed"
    );
    *slot = Some(ClockFault {
        token,
        home,
        thread,
        cas_thread,
        process,
        fault,
    });
    ProjectionCompletionClockFaultGuard { token }
}

pub(crate) fn take_projection_completion_clock_fault(
    home: BerylHomeId,
    thread: SyndicThreadId,
    cas_thread: &CasThreadId,
    process: CasProcessGeneration,
) -> Option<ProjectionExecutionError> {
    let mut slot = slot().lock().unwrap();
    if !slot.as_ref().is_some_and(|entry| {
        entry.home == home
            && entry.thread == thread
            && entry.cas_thread == *cas_thread
            && entry.process == process
    }) {
        return None;
    }
    let fault = slot.take().unwrap().fault;
    Some(match fault {
        ProjectionCompletionClockFault::BeforeUnixEpoch => {
            ProjectionExecutionError::SystemClockBeforeUnixEpoch(
                UNIX_EPOCH
                    .duration_since(UNIX_EPOCH + Duration::from_millis(1))
                    .unwrap_err(),
            )
        }
        ProjectionCompletionClockFault::OutOfRange => {
            ProjectionExecutionError::SystemClockOutOfRange
        }
    })
}

impl Drop for ProjectionCompletionClockFaultGuard {
    fn drop(&mut self) {
        let mut slot = slot().lock().unwrap();
        if slot.as_ref().is_some_and(|entry| entry.token == self.token) {
            *slot = None;
        }
    }
}

fn slot() -> &'static Mutex<Option<ClockFault>> {
    static SLOT: OnceLock<Mutex<Option<ClockFault>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}
