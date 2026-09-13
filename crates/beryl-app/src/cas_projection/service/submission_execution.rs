use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use syndic_storage::FirstAcceptanceKind;

use super::super::{
    ProjectionConnectionService, accepted_input_scheduler::AcceptedInputSchedulerSignal,
    persistent_failure::LiveCommandAuthorizer,
};

#[derive(Clone, Debug)]
pub struct SubmissionExecutionWake {
    target: Option<SubmissionExecutionTarget>,
}

#[derive(Clone, Debug)]
struct SubmissionExecutionTarget {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    authorizer: LiveCommandAuthorizer,
    signal: AcceptedInputSchedulerSignal,
}

impl ProjectionConnectionService {
    pub fn submission_execution_wake(&self) -> SubmissionExecutionWake {
        SubmissionExecutionWake {
            target: Some(SubmissionExecutionTarget {
                home_id: self.home_id(),
                home_generation: self.home_generation(),
                authorizer: self.command_authorizer.clone(),
                signal: self.scheduler_signal.clone(),
            }),
        }
    }
}

impl SubmissionExecutionWake {
    pub(crate) fn matches_binding(
        &self,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> bool {
        self.target.as_ref().is_none_or(|target| {
            target.home_id == home_id && target.home_generation == home_generation
        })
    }

    pub(crate) fn matches_home(&self, home: &HomeStore) -> bool {
        self.target.as_ref().is_none_or(|target| {
            target.home_id == home.home_id()
                && Some(target.home_generation) == home.health().generation()
        })
    }

    pub(crate) fn accepted(&self, kind: FirstAcceptanceKind) {
        let Some(target) = &self.target else {
            return;
        };
        if !target.authorizer.is_open() {
            return;
        }
        target.signal.wake_submission(kind);
    }

    #[cfg(any(test, feature = "test-faults"))]
    pub fn storage_only_for_test() -> Self {
        Self { target: None }
    }

    #[cfg(any(test, feature = "test-faults"))]
    pub fn test_for_home(home: &HomeStore) -> (Self, SubmissionExecutionWakeTestProbe) {
        use super::super::persistent_failure::{MasterCommandGate, ProjectionServiceGeneration};
        let gate = MasterCommandGate::new(
            Default::default(),
            ProjectionServiceGeneration::allocate().unwrap(),
            None,
        );
        let signal = AcceptedInputSchedulerSignal::new();
        let wake = Self {
            target: Some(SubmissionExecutionTarget {
                home_id: home.home_id(),
                home_generation: home.health().generation().unwrap(),
                authorizer: gate.authorizer(),
                signal: signal.clone(),
            }),
        };
        (wake, SubmissionExecutionWakeTestProbe { gate, signal })
    }
}

#[cfg(any(test, feature = "test-faults"))]
pub struct SubmissionExecutionWakeTestProbe {
    gate: super::super::persistent_failure::MasterCommandGate,
    signal: AcceptedInputSchedulerSignal,
}

#[cfg(any(test, feature = "test-faults"))]
impl SubmissionExecutionWakeTestProbe {
    pub fn wake_count(&self) -> u64 {
        self.signal.diagnostics().wake_count()
    }

    pub fn retire(&self) {
        self.gate.close_for_shutdown();
    }
}
