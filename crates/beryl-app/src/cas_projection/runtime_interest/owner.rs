use std::thread;

use super::*;

impl RuntimeInterestOwner {
    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn interest_count_for_test(
        &self,
        runtime_id: RuntimeId,
        kind: RuntimeInterestKind,
    ) -> usize {
        self.shared
            .lock()
            .runtimes
            .get(&runtime_id)
            .map_or(0, |entry| {
                entry
                    .interests
                    .values()
                    .filter(|current| **current == kind)
                    .count()
            })
    }

    pub(in crate::cas_projection) fn configuration(&self) -> RuntimeInterestConfig {
        self.shared.config
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn preparation_waits(
        &self,
        runtime_id: RuntimeId,
    ) -> (bool, bool, bool) {
        let state = self.shared.lock();
        (
            state.interest_capacity_waiter,
            state.runtime_capacity_waiter,
            state
                .runtimes
                .get(&runtime_id)
                .is_some_and(|entry| entry.retirement_waiter),
        )
    }
    pub(in crate::cas_projection) fn new(
        config: RuntimeInterestConfig,
        commands: LiveCommandAuthorizer,
        scheduler_signal: crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal,
    ) -> Self {
        Self {
            shared: Arc::new(RuntimeInterestShared {
                state: Mutex::new(RuntimeInterestState {
                    closed: false,
                    next_identity: 1,
                    interest_count: 0,
                    runtimes: HashMap::new(),
                    interest_capacity_waiter: false,
                    runtime_capacity_waiter: false,
                }),
                changed: Condvar::new(),
                commands,
                config,
                scheduler_signal,
            }),
        }
    }

    pub(super) fn acquire(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        kind: RuntimeInterestKind,
        prepare: impl FnOnce() -> Result<
            Box<dyn FnOnce() -> Result<Box<dyn RunningRuntime>, RuntimeFailure> + Send>,
            RuntimeInterestError,
        >,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        self.acquire_with_retry(spec, binding, kind, None, false, prepare)
    }

    pub(super) fn acquire_with_retry(
        &self,
        spec: ManagedBackendLaunchSpec,
        binding: ExecutionBinding,
        kind: RuntimeInterestKind,
        retry: Option<RuntimeFailureSnapshot>,
        wake_on_release: bool,
        prepare: impl FnOnce() -> Result<
            Box<dyn FnOnce() -> Result<Box<dyn RunningRuntime>, RuntimeFailure> + Send>,
            RuntimeInterestError,
        >,
    ) -> Result<RuntimeInterest, RuntimeInterestError> {
        if spec.runtime_id() != binding.runtime_id()
            || spec.working_directory() != binding.root_path()
        {
            return Err(RuntimeInterestError::ConfigurationMismatch);
        }
        let command = self
            .shared
            .commands
            .authorize()
            .map_err(|_| RuntimeInterestError::Closed)?;
        let mut state = self.shared.lock();
        self.reap_finished(&mut state);
        if state.closed {
            return Err(RuntimeInterestError::Closed);
        }
        let runtime_id = spec.runtime_id();
        let interest = state.next_identity;
        let next = interest
            .checked_add(1)
            .ok_or(RuntimeInterestError::IdentityExhausted)?;
        let mut previous = if let Some(retry) = retry {
            let entry = state
                .runtimes
                .get(&runtime_id)
                .ok_or(RuntimeInterestError::RetryMismatch)?;
            if retry.runtime_id != runtime_id
                || retry.service_generation != self.shared.commands.service_generation()
                || retry.attempt != entry.attempt
                || !matches!(entry.status, RuntimeInterestStatus::Unavailable(_))
            {
                return Err(RuntimeInterestError::RetryMismatch);
            }
            if !same_runtime_configuration(&entry.spec, &spec) {
                return Err(RuntimeInterestError::ConfigurationMismatch);
            }
            if entry.worker.is_some() || !entry.cleanup_complete {
                return Err(RuntimeInterestError::Retiring);
            }
            if state.interest_count - entry.interests.len()
                >= self.shared.config.interest_capacity.get()
            {
                state.interest_capacity_waiter |= wake_on_release;
                return Err(RuntimeInterestError::InterestCapacity);
            }
            let entry = state
                .runtimes
                .remove(&runtime_id)
                .expect("exact failed runtime");
            state.interest_count -= entry.interests.len();
            Some(entry)
        } else {
            if let Some(entry) = state.runtimes.get(&runtime_id)
                && let RuntimeInterestStatus::Unavailable(failure) = entry.status
            {
                return Err(RuntimeInterestError::Unavailable(failure));
            }
            None
        };
        if state.interest_count == self.shared.config.interest_capacity.get() {
            state.interest_capacity_waiter |= wake_on_release;
            return Err(RuntimeInterestError::InterestCapacity);
        }
        if let Some(entry) = state.runtimes.get_mut(&runtime_id) {
            if !same_runtime_configuration(&entry.spec, &spec) {
                return Err(RuntimeInterestError::ConfigurationMismatch);
            }
            if entry.interests.is_empty() || entry.status == RuntimeInterestStatus::Retiring {
                entry.retirement_waiter |= wake_on_release;
                return Err(RuntimeInterestError::Retiring);
            }
            let attempt = entry.attempt;
            command
                .commit_if_current(|| {
                    entry.interests.insert(interest, kind);
                })
                .map_err(|_| RuntimeInterestError::Closed)?;
            state.next_identity = next;
            state.interest_count += 1;
            return Ok(RuntimeInterest {
                shared: Arc::clone(&self.shared),
                runtime_id,
                attempt,
                interest,
                binding,
                kind,
            });
        }
        if state.runtimes.len() == self.shared.config.runtime_capacity.get() {
            state.runtime_capacity_waiter |= wake_on_release;
            return Err(RuntimeInterestError::RuntimeCapacity);
        }
        if state.runtimes.values().any(|entry| {
            entry.spec.canonical_executable() == spec.canonical_executable()
                && entry.spec.runtime_mode() == spec.runtime_mode()
        }) {
            return Err(RuntimeInterestError::ConfigurationMismatch);
        }
        command
            .commit_if_current(|| {
                state.runtimes.insert(
                    runtime_id,
                    RuntimeEntry {
                        spec,
                        attempt: interest,
                        interests: HashMap::from([(interest, kind)]),
                        status: RuntimeInterestStatus::Starting,
                        worker: None,
                        cleanup_complete: false,
                        connector: None,
                        retry: None,
                        retirement_waiter: false,
                    },
                );
                state.interest_count += 1;
                state.next_identity = next;
            })
            .map_err(|_| RuntimeInterestError::Closed)?;
        let launch = match prepare() {
            Ok(launch) => launch,
            Err(error) => {
                state.runtimes.remove(&runtime_id);
                state.interest_count -= 1;
                if let Some(previous) = previous.take() {
                    state.interest_count += previous.interests.len();
                    state.runtimes.insert(runtime_id, previous);
                }
                return Err(error);
            }
        };
        let shared = Arc::clone(&self.shared);
        let worker = match thread::Builder::new()
            .name("beryl-runtime-owner".to_owned())
            .spawn(move || worker::run(shared, runtime_id, interest, launch))
        {
            Ok(worker) => worker,
            Err(_) => {
                state.runtimes.remove(&runtime_id);
                state.interest_count -= 1;
                if let Some(previous) = previous.take() {
                    state.interest_count += previous.interests.len();
                    state.runtimes.insert(runtime_id, previous);
                }
                return Err(RuntimeInterestError::WorkerStart);
            }
        };
        state
            .runtimes
            .get_mut(&runtime_id)
            .expect("reserved runtime")
            .worker = Some(worker);
        Ok(RuntimeInterest {
            shared: Arc::clone(&self.shared),
            runtime_id,
            attempt: interest,
            interest,
            binding,
            kind,
        })
    }

    pub(super) fn reap_finished(&self, state: &mut RuntimeInterestState) {
        state.runtimes.retain(|_, entry| {
            if entry
                .worker
                .as_ref()
                .is_some_and(|worker| worker.is_finished() || entry.cleanup_complete)
            {
                let clean = entry
                    .worker
                    .take()
                    .expect("finished worker")
                    .join()
                    .unwrap_or(false);
                entry.cleanup_complete = clean;
                if !clean && !matches!(entry.status, RuntimeInterestStatus::Unavailable(_)) {
                    entry.status =
                        RuntimeInterestStatus::Unavailable(RuntimeFailure::WorkerPanicked);
                }
            }
            !(entry.worker.is_none()
                && entry.cleanup_complete
                && entry.interests.is_empty()
                && !matches!(entry.status, RuntimeInterestStatus::Unavailable(_)))
        });
    }

    pub(in crate::cas_projection) fn failure_snapshot(
        &self,
        runtime_id: RuntimeId,
    ) -> Option<RuntimeFailureSnapshot> {
        let mut state = self.shared.lock();
        self.reap_finished(&mut state);
        if state.closed || !self.shared.commands.is_open() {
            return None;
        }
        let entry = state.runtimes.get(&runtime_id)?;
        let RuntimeInterestStatus::Unavailable(failure) = entry.status else {
            return None;
        };
        Some(RuntimeFailureSnapshot {
            runtime_id,
            service_generation: self.shared.commands.service_generation(),
            attempt: entry.attempt,
            failure,
            retry_ready: entry.worker.is_none() && entry.cleanup_complete && entry.retry.is_none(),
        })
    }

    pub(in crate::cas_projection) fn shutdown(&self) -> bool {
        let workers = {
            let mut state = self.shared.lock();
            state.closed = true;
            let workers = state
                .runtimes
                .values_mut()
                .filter_map(|entry| {
                    entry.status = RuntimeInterestStatus::Retiring;
                    entry.worker.take()
                })
                .collect::<Vec<_>>();
            self.shared.changed.notify_all();
            workers
        };
        let mut clean = true;
        for worker in workers {
            clean &= worker.join().unwrap_or(false);
        }
        let mut state = self.shared.lock();
        clean &= state.runtimes.values().all(|entry| entry.cleanup_complete);
        state.runtimes.clear();
        state.interest_count = 0;
        self.shared.changed.notify_all();
        clean
    }

    pub(in crate::cas_projection) fn request_shutdown(&self) {
        let mut state = self.shared.lock();
        state.closed = true;
        self.shared.changed.notify_all();
    }
}

impl Drop for RuntimeInterestOwner {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

fn same_runtime_configuration(
    left: &ManagedBackendLaunchSpec,
    right: &ManagedBackendLaunchSpec,
) -> bool {
    left.runtime_id() == right.runtime_id()
        && left.canonical_executable() == right.canonical_executable()
        && left.runtime_mode() == right.runtime_mode()
        && left.runtime_native_executable() == right.runtime_native_executable()
        && left.host_token_directory() == right.host_token_directory()
        && left.runtime_token_directory() == right.runtime_token_directory()
}
