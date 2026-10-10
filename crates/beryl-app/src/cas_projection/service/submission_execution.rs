use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use syndic_storage::FirstAcceptanceKind;

use super::super::{
    ProjectionConnectionService, accepted_input_scheduler::AcceptedInputSchedulerSignal,
    persistent_failure::LiveCommandAuthorizer,
};

#[derive(Clone)]
pub struct SubmissionExecutionWake {
    target: Option<SubmissionExecutionTarget>,
    window: Option<(
        crate::app_services::PublishedExactStopWorker,
        Option<crate::main_window::MainWindowComposerSelectionIdentity>,
    )>,
}

#[derive(Clone)]
struct SubmissionExecutionTarget {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    authorizer: LiveCommandAuthorizer,
    signal: AcceptedInputSchedulerSignal,
    submission: Option<super::stop_worker::ExactStopWorker>,
}

impl std::fmt::Debug for SubmissionExecutionWake {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SubmissionExecutionWake")
            .field("bound", &self.target.is_some())
            .field("selected_window", &self.window.is_some())
            .finish()
    }
}

impl ProjectionConnectionService {
    pub fn submission_execution_wake(&self) -> SubmissionExecutionWake {
        SubmissionExecutionWake {
            target: Some(SubmissionExecutionTarget {
                home_id: self.home_id(),
                home_generation: self.home_generation(),
                authorizer: self.command_authorizer.clone(),
                signal: self.scheduler_signal.clone(),
                submission: Some(self.exact_stop_worker()),
            }),
            window: None,
        }
    }
}

impl SubmissionExecutionWake {
    pub(crate) fn with_window_worker(
        mut self,
        worker: crate::app_services::PublishedExactStopWorker,
    ) -> Self {
        self.window = Some((worker, None));
        self
    }

    pub(crate) fn for_selection(
        mut self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
    ) -> Self {
        if let Some((_, original)) = self.window.as_mut() {
            *original = Some(selection);
        }
        self
    }

    pub(crate) fn guard_command(
        &self,
        store: &HomeStore,
        thread: beryl_model::SyndicThreadId,
        command: beryl_home_store::HomeCommand,
    ) -> Result<beryl_home_store::HomeCommand, super::SubmissionUnavailable> {
        if let Some((worker, selection)) = &self.window {
            return worker.guard_selected_submission(
                store,
                selection.ok_or(super::SubmissionUnavailable::AuthorityUnavailable)?,
                command,
            );
        }
        let Some(worker) = self
            .target
            .as_ref()
            .and_then(|target| target.submission.as_ref())
        else {
            return Ok(command);
        };
        let proof = worker.selected_submission_proof(thread)?;
        Ok(command.with_local_admission(move || proof.elect_local()))
    }

    pub(crate) fn submission_unavailable(
        &self,
        thread: beryl_model::SyndicThreadId,
    ) -> Option<super::SubmissionUnavailable> {
        if let Some((worker, selection)) = &self.window {
            return selection.map_or(
                Some(super::SubmissionUnavailable::AuthorityUnavailable),
                |selection| worker.selected_submission_availability(selection).err(),
            );
        }
        self.target
            .as_ref()
            .and_then(|target| target.submission.as_ref())
            .and_then(|worker| worker.selected_submission_availability(thread).err())
    }

    pub(crate) fn execution_candidate(
        &self,
    ) -> Result<
        Option<super::super::LiveExecutionCandidate>,
        crate::process_admission::ProcessExecutionAdmissionError,
    > {
        self.target
            .as_ref()
            .map(|target| target.authorizer.execution_candidate())
            .transpose()
    }

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
        Self {
            target: None,
            window: None,
        }
    }

    #[cfg(any(test, feature = "test-faults"))]
    pub fn test_for_home(
        home: &HomeStore,
        process: crate::process_admission::ProcessAdmissionGate,
    ) -> (Self, SubmissionExecutionWakeTestProbe) {
        use super::super::persistent_failure::{MasterCommandGate, ProjectionServiceGeneration};
        let gate = MasterCommandGate::new(
            process,
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
                submission: None,
            }),
            window: None,
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
