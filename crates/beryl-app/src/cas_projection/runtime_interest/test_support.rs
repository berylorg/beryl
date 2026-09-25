use crate::cas_projection::persistent_failure::{MasterCommandGate, ProjectionServiceGeneration};

use super::*;

#[cfg(test)]
#[path = "../../../tests/unit/runtime_acquisition_cleanup.rs"]
mod acquisition_cleanup_tests;

pub struct RuntimeInterestTestHarness {
    owner: RuntimeInterestOwner,
    gate: MasterCommandGate,
}

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn acquire_lifecycle_test_interest(
        &self,
        binding: ExecutionBinding,
        generation: CasProcessGeneration,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeNativePath};
        let host_executable = format!(r"C:\test-runtimes\{}\codex.exe", binding.runtime_id());
        let native_executable = match binding.root_path().flavor() {
            PathFlavor::Windows => host_executable.clone(),
            PathFlavor::Posix => format!("/test-runtimes/{}/codex", binding.runtime_id()),
        };
        let native_tokens = match binding.root_path().flavor() {
            PathFlavor::Windows => r"C:\test-tokens",
            PathFlavor::Posix => "/test-tokens",
        };
        let native = |path: &str| {
            RuntimeNativePath::from_admitted(
                binding.root_path().mode().clone(),
                binding.root_path().flavor(),
                path,
            )
            .expect("fixture native runtime path")
        };
        let spec = ManagedBackendLaunchSpec::new(
            binding.runtime_id(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, &host_executable)
                .expect("fixture executable path"),
            binding.root_path().mode().clone(),
            native(&native_executable),
            binding.root_path().clone(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\test-tokens")
                .expect("fixture token path"),
            native(native_tokens),
        )
        .expect("fixture launch specification");
        let probe = RuntimeInterestTestProbe::new(generation);
        self.acquire(spec, binding, RuntimeInterestKind::RequiredWork, || {
            Ok(Box::new(move || probe.launch()))
        })
    }
}

impl RuntimeInterestTestHarness {
    pub fn new(config: RuntimeInterestConfig) -> Self {
        Self::for_home(config, beryl_model::BerylHomeId::from_bytes([1; 16]))
    }

    pub fn for_home(config: RuntimeInterestConfig, home: beryl_model::BerylHomeId) -> Self {
        Self::with_enrollments(
            config,
            crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
                home,
                config.runtime_capacity,
            ),
        )
    }

    pub fn with_enrollments(
        config: RuntimeInterestConfig,
        enrollments: crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations,
    ) -> Self {
        let gate = MasterCommandGate::new(
            Default::default(),
            ProjectionServiceGeneration::allocate().unwrap(),
            None,
        );
        Self {
            owner: RuntimeInterestOwner::new(
                config,
                gate.authorizer(),
                crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(
                ),
                enrollments,
            ),
            gate,
        }
    }

    pub fn acquire(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        kind: RuntimeInterestKind,
        probe: RuntimeInterestTestProbe,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        self.owner
            .acquire(spec, binding, kind, || Ok(Box::new(move || probe.launch())))
    }

    pub fn lose_generation(&self) {
        self.gate.close_for_shutdown();
    }

    pub fn publication_readiness(
        &self,
        interest: &RuntimeInterest,
        ready: RuntimeReadiness,
    ) -> Result<(), RuntimeSessionAdmissionError> {
        if !Arc::ptr_eq(&self.owner.shared, &interest.shared) {
            return Err(RuntimeSessionAdmissionError::OwnerMismatch);
        }
        interest.publication_readiness(&self.owner.shared.lock(), ready)
    }

    pub fn acquire_scheduled(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        probe: RuntimeInterestTestProbe,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        let acquisition = crate::cas_projection::acquisition::ProjectionAcquisition::admit(
            &self.gate.authorizer(),
        )
        .map_err(|_| RuntimeInterestError::Closed)?;
        self.owner.acquire_with_retry(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            None,
            true,
            &acquisition,
            || Ok(Box::new(move || probe.launch())),
        )
    }

    pub fn preparation_wake_pending(&self) -> bool {
        self.owner
            .shared
            .scheduler_signal
            .wake_pending_for_test(crate::cas_projection::accepted_input_scheduler::AcceptedInputWakeReason::ExecutionReady)
    }

    pub fn shutdown(&mut self) -> bool {
        self.gate.close_for_shutdown();
        self.owner.shutdown()
    }

    pub fn retained_counts(&self) -> (usize, usize) {
        let state = self.owner.shared.lock();
        (state.runtimes.len(), state.interest_count)
    }
}

impl RuntimeInterest {
    pub fn enroll_activity_for_test(
        &self,
        home: &beryl_home_store::HomeStore,
        storage: &syndic_storage::SyndicStorage,
        thread: SyndicThreadId,
        turn: beryl_model::SyndicTurnId,
    ) -> Result<(), crate::cas_projection::OrdinaryTurnExecutionError> {
        self.enroll_activity(
            home,
            storage,
            thread,
            turn,
            &crate::cas_projection::ProjectionCancellationToken::new(),
        )
    }

    pub fn with_activity_for_test<T>(
        &self,
        source: syndic_storage::ActivityQuerySource,
        publish: impl FnOnce(syndic_storage::ActivitySourceQualification) -> T,
    ) -> Option<T> {
        self.with_activity(source, publish)
    }
}

#[derive(Clone)]
pub struct RuntimeInterestTestProbe {
    shared: Arc<(Mutex<ProbeState>, Condvar)>,
    process_generation: CasProcessGeneration,
}

struct ProbeState {
    launches: usize,
    retirements: usize,
    disposed: usize,
    launch_allowed: bool,
    retirement_allowed: bool,
    admission_failure: Option<RuntimeFailure>,
    retirement_failure: Option<RuntimeFailure>,
    health_failure: Option<RuntimeFailure>,
}

impl RuntimeInterestTestProbe {
    pub fn new(process_generation: CasProcessGeneration) -> Self {
        Self {
            shared: Arc::new((
                Mutex::new(ProbeState {
                    launches: 0,
                    retirements: 0,
                    disposed: 0,
                    launch_allowed: true,
                    retirement_allowed: true,
                    admission_failure: None,
                    retirement_failure: None,
                    health_failure: None,
                }),
                Condvar::new(),
            )),
            process_generation,
        }
    }

    pub fn allow_launch(&self, allowed: bool) {
        self.shared.0.lock().unwrap().launch_allowed = allowed;
        self.shared.1.notify_all();
    }

    pub fn allow_retirement(&self, allowed: bool) {
        self.shared.0.lock().unwrap().retirement_allowed = allowed;
        self.shared.1.notify_all();
    }

    pub fn fail_admission(&self, failure: RuntimeFailure) {
        self.shared.0.lock().unwrap().admission_failure = Some(failure);
    }

    pub fn fail_retirement(&self, failure: RuntimeFailure) {
        self.shared.0.lock().unwrap().retirement_failure = Some(failure);
    }

    pub fn fail_health(&self, failure: RuntimeFailure) {
        self.shared.0.lock().unwrap().health_failure = Some(failure);
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let state = self.shared.0.lock().unwrap();
        (state.launches, state.retirements, state.disposed)
    }

    pub fn wait_for_launch(&self, timeout: Duration) -> bool {
        let (state, _) = self
            .shared
            .1
            .wait_timeout_while(self.shared.0.lock().unwrap(), timeout, |state| {
                state.launches == 0
            })
            .unwrap();
        state.launches != 0
    }

    pub fn wait_for_retirement(&self, timeout: Duration) -> bool {
        let (state, _) = self
            .shared
            .1
            .wait_timeout_while(self.shared.0.lock().unwrap(), timeout, |state| {
                state.retirements == 0
            })
            .unwrap();
        state.retirements != 0
    }

    pub fn wait_for_disposal(&self, timeout: Duration) -> bool {
        let (state, _) = self
            .shared
            .1
            .wait_timeout_while(self.shared.0.lock().unwrap(), timeout, |state| {
                state.disposed == 0
            })
            .unwrap();
        state.disposed != 0
    }

    fn launch(self) -> Result<Box<dyn RunningRuntime>, RuntimeFailure> {
        let mut state = self.shared.0.lock().unwrap();
        state.launches += 1;
        self.shared.1.notify_all();
        let (state, _) = self
            .shared
            .1
            .wait_timeout_while(state, Duration::from_secs(5), |state| !state.launch_allowed)
            .unwrap();
        if !state.launch_allowed {
            return Err(RuntimeFailure::Admission);
        }
        if let Some(failure) = state.admission_failure {
            return Err(failure);
        }
        drop(state);
        Ok(Box::new(self))
    }
}

impl RunningRuntime for RuntimeInterestTestProbe {
    fn process_generation(&self) -> CasProcessGeneration {
        self.process_generation
    }

    fn poll_health(&mut self) -> Result<(), RuntimeFailure> {
        self.shared
            .0
            .lock()
            .unwrap()
            .health_failure
            .map_or(Ok(()), Err)
    }

    fn retire(&mut self) -> Result<(), RuntimeFailure> {
        let mut state = self.shared.0.lock().unwrap();
        state.retirements += 1;
        self.shared.1.notify_all();
        let (mut state, _) = self
            .shared
            .1
            .wait_timeout_while(state, Duration::from_secs(5), |state| {
                !state.retirement_allowed
            })
            .unwrap();
        if !state.retirement_allowed {
            return Err(RuntimeFailure::AppRetirement);
        }
        state.disposed += 1;
        self.shared.1.notify_all();
        state.retirement_failure.map_or(Ok(()), Err)
    }
}
