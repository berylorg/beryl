use std::{
    io,
    sync::mpsc::{Receiver, SyncSender},
};

use super::DiagnosticChildSupervisorError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AcceptanceStartupFailureStage {
    JobCreate,
    JobConfigure,
    JobAssign,
    WriterSpawn,
    StdoutReaderSpawn,
    StderrReaderSpawn,
    GateWrite,
    GateReady,
    Handshake,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AcceptanceTestObservation {
    JobConfigured {
        pid: u32,
    },
    StartupFailureConsumed {
        pid: u32,
        stage: AcceptanceStartupFailureStage,
    },
    GateWriteCompleted {
        pid: u32,
    },
    CleanupAttempt {
        pid: u32,
        ordinal: usize,
        forced_failure: bool,
        remaining_forced_failures: usize,
    },
    FailSafeRelease {
        pid: u32,
    },
}

pub(crate) struct AcceptanceTestPlan {
    spawn_barrier: Option<AcceptanceStageBarrier>,
    job_assignment: Option<AcceptanceJobAssignmentDirective>,
    startup_failures: Vec<AcceptanceStartupFailureStage>,
    cleanup_failures: usize,
    observer: Option<SyncSender<AcceptanceTestObservation>>,
}

struct AcceptanceStageBarrier {
    reached: SyncSender<u32>,
    release: Receiver<()>,
}

struct AcceptanceJobAssignmentDirective {
    reached: SyncSender<u32>,
    release: Option<Receiver<()>>,
    fail_after_assignment: bool,
}

pub(super) struct AcceptanceTestControl {
    plan: AcceptanceTestPlan,
    cleanup_attempts: usize,
}

impl Default for AcceptanceTestPlan {
    fn default() -> Self {
        Self {
            spawn_barrier: None,
            job_assignment: None,
            startup_failures: Vec::new(),
            cleanup_failures: 0,
            observer: None,
        }
    }
}

impl AcceptanceTestPlan {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_spawn_barrier(
        mut self,
        reached: SyncSender<u32>,
        release: Receiver<()>,
    ) -> Self {
        self.spawn_barrier = Some(AcceptanceStageBarrier { reached, release });
        self
    }

    pub(crate) fn with_job_assignment(
        mut self,
        reached: SyncSender<u32>,
        release: Option<Receiver<()>>,
        fail_after_assignment: bool,
    ) -> Self {
        self.job_assignment = Some(AcceptanceJobAssignmentDirective {
            reached,
            release,
            fail_after_assignment,
        });
        self
    }

    pub(crate) fn with_startup_failure(mut self, stage: AcceptanceStartupFailureStage) -> Self {
        self.startup_failures.push(stage);
        self
    }

    pub(crate) fn with_cleanup_failures(mut self, cleanup_failures: usize) -> Self {
        self.cleanup_failures = cleanup_failures;
        self
    }

    pub(crate) fn with_observer(mut self, observer: SyncSender<AcceptanceTestObservation>) -> Self {
        self.observer = Some(observer);
        self
    }
}

impl AcceptanceTestControl {
    pub(super) fn new(plan: AcceptanceTestPlan) -> Self {
        Self {
            plan,
            cleanup_attempts: 0,
        }
    }

    pub(super) fn run_spawn_barrier(&mut self, pid: u32) {
        let Some(barrier) = self.plan.spawn_barrier.take() else {
            return;
        };
        let _ = barrier.reached.send(pid);
        let _ = barrier.release.recv();
    }

    pub(super) fn run_job_assignment(
        &mut self,
        pid: u32,
    ) -> Result<(), DiagnosticChildSupervisorError> {
        let Some(directive) = self.plan.job_assignment.take() else {
            return Ok(());
        };
        let _ = directive.reached.send(pid);
        if let Some(release) = directive.release {
            let _ = release.recv();
        }
        if directive.fail_after_assignment {
            return Err(DiagnosticChildSupervisorError::AssignProcessToJob {
                source: io::Error::other("forced post-assignment setup failure for test"),
            });
        }
        Ok(())
    }

    pub(super) fn observe_job_configured(&self, pid: u32) {
        self.observe(AcceptanceTestObservation::JobConfigured { pid });
    }

    pub(super) fn observe_gate_write_completed(&self, pid: u32) {
        self.observe(AcceptanceTestObservation::GateWriteCompleted { pid });
    }

    pub(super) fn force_startup_failure(
        &mut self,
        pid: u32,
        stage: AcceptanceStartupFailureStage,
    ) -> bool {
        let Some(position) = self
            .plan
            .startup_failures
            .iter()
            .position(|configured| *configured == stage)
        else {
            return false;
        };
        self.plan.startup_failures.remove(position);
        self.observe(AcceptanceTestObservation::StartupFailureConsumed { pid, stage });
        true
    }

    pub(super) fn begin_cleanup_attempt(&mut self, pid: u32) -> bool {
        self.cleanup_attempts += 1;
        let forced_failure = self.plan.cleanup_failures > 0;
        if forced_failure {
            self.plan.cleanup_failures -= 1;
        }
        self.observe(AcceptanceTestObservation::CleanupAttempt {
            pid,
            ordinal: self.cleanup_attempts,
            forced_failure,
            remaining_forced_failures: self.plan.cleanup_failures,
        });
        forced_failure
    }

    pub(super) fn observe_fail_safe_release(&self, pid: u32) {
        self.observe(AcceptanceTestObservation::FailSafeRelease { pid });
    }

    fn observe(&self, observation: AcceptanceTestObservation) {
        if let Some(observer) = &self.plan.observer {
            let _ = observer.try_send(observation);
        }
    }
}
