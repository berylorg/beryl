use crate::cas_projection::persistent_failure::{MasterCommandGate, ProjectionServiceGeneration};

use super::*;

pub struct RuntimeInterestTestHarness {
    owner: RuntimeInterestOwner,
    gate: MasterCommandGate,
}

impl RuntimeInterestTestHarness {
    pub fn new(config: RuntimeInterestConfig) -> Self {
        let gate = MasterCommandGate::new(ProjectionServiceGeneration::allocate().unwrap(), None);
        Self {
            owner: RuntimeInterestOwner::new(config, gate.authorizer()),
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

    pub fn shutdown(&mut self) -> bool {
        self.gate.close_for_shutdown();
        self.owner.shutdown()
    }

    pub fn retained_counts(&self) -> (usize, usize) {
        let state = self.owner.shared.lock();
        (state.runtimes.len(), state.interest_count)
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
