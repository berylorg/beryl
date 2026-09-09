use std::thread;

use super::*;

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn new(
        config: RuntimeInterestConfig,
        commands: LiveCommandAuthorizer,
    ) -> Self {
        Self {
            shared: Arc::new(RuntimeInterestShared {
                state: Mutex::new(RuntimeInterestState {
                    closed: false,
                    next_identity: 1,
                    interest_count: 0,
                    runtimes: HashMap::new(),
                }),
                changed: Condvar::new(),
                commands,
                config,
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
        if state.interest_count == self.shared.config.interest_capacity.get() {
            return Err(RuntimeInterestError::InterestCapacity);
        }
        let runtime_id = spec.runtime_id();
        let interest = state.next_identity;
        let next = interest
            .checked_add(1)
            .ok_or(RuntimeInterestError::IdentityExhausted)?;
        if let Some(entry) = state.runtimes.get_mut(&runtime_id) {
            if !same_runtime_configuration(&entry.spec, &spec) {
                return Err(RuntimeInterestError::ConfigurationMismatch);
            }
            if entry.interests.is_empty() || entry.status == RuntimeInterestStatus::Retiring {
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

    fn reap_finished(&self, state: &mut RuntimeInterestState) {
        state.runtimes.retain(|_, entry| {
            if entry.worker.as_ref().is_some_and(JoinHandle::is_finished) {
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
            !(entry.worker.is_none() && entry.cleanup_complete && entry.interests.is_empty())
        });
    }

    pub(in crate::cas_projection) fn shutdown(&mut self) -> bool {
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
}

impl Drop for RuntimeInterestOwner {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.closed = true;
        self.shared.changed.notify_all();
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
