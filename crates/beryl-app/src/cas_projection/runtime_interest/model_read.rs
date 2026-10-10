use std::{
    path::Path,
    sync::{Arc, TryLockError, Weak},
    time::Duration,
};

use super::{RuntimeInterestOwner, RuntimeInterestShared, RuntimeInterestStatus, RuntimeReadiness};
use beryl_backend::{BackendConfigDefaults, ManagedBackendError, ModelListOptions, ModelPage};
use beryl_model::ExecutionBinding;

#[derive(Debug, thiserror::Error)]
pub(crate) enum RuntimeModelReadError {
    #[error("the original runtime is unavailable")]
    Unavailable,
    #[error("runtime model publication is busy")]
    Busy,
    #[error("model request timeout must be positive")]
    Timeout,
    #[error("bounded backend model request failed: {0}")]
    Backend(#[from] ManagedBackendError),
}

pub(crate) struct RuntimeModelRead {
    shared: Weak<RuntimeInterestShared>,
    binding: ExecutionBinding,
    attempt: u64,
    readiness: RuntimeReadiness,
}

impl RuntimeInterestOwner {
    #[cfg(all(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn attach_model_connector_for_test(
        &self,
        binding: &ExecutionBinding,
        generation: beryl_model::CasProcessGeneration,
        connector: beryl_backend::ManagedBackendClientConnector,
    ) {
        let mut state = self.shared.state.lock().unwrap();
        let entry = state
            .runtimes
            .get_mut(&binding.runtime_id())
            .expect("existing admitted runtime");
        assert!(
            matches!(entry.status, RuntimeInterestStatus::Ready(ready) if ready.process_generation() == generation)
        );
        assert!(
            entry
                .interests
                .values()
                .any(|interest| &interest.binding == binding)
        );
        entry.connector = Some(connector);
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn retire_model_readiness_for_test(
        &self,
        runtime: beryl_model::RuntimeId,
    ) {
        let mut state = self.shared.state.lock().unwrap();
        let entry = state
            .runtimes
            .get_mut(&runtime)
            .expect("existing admitted runtime");
        assert!(matches!(entry.status, RuntimeInterestStatus::Ready(_)));
        entry.connector = None;
        entry.status = RuntimeInterestStatus::Unavailable(super::RuntimeFailure::ConnectionLost);
        self.shared.changed.notify_all();
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn retry_model_interest_for_test(
        &self,
        binding: ExecutionBinding,
        generation: beryl_model::CasProcessGeneration,
        timeout: Duration,
    ) -> Result<super::RuntimeInterest, super::RuntimeInterestError> {
        use super::{RuntimeInterestError, RuntimeInterestKind, RuntimeInterestTestProbe};
        let deadline = std::time::Instant::now()
            .checked_add(timeout)
            .filter(|_| !timeout.is_zero())
            .ok_or(RuntimeInterestError::InvalidTimeout)?;
        let snapshot = loop {
            let snapshot = self
                .failure_snapshot(binding.runtime_id())
                .ok_or(RuntimeInterestError::RetryMismatch)?;
            if snapshot.retry_ready() {
                break snapshot;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(RuntimeInterestError::Retiring);
            }
            let state = self
                .shared
                .state
                .lock()
                .map_err(|_| RuntimeInterestError::Closed)?;
            drop(
                self.shared
                    .changed
                    .wait_timeout(state, remaining.min(Duration::from_millis(25)))
                    .map_err(|_| RuntimeInterestError::Closed)?,
            );
        };
        let spec = self
            .shared
            .state
            .lock()
            .map_err(|_| RuntimeInterestError::Closed)?
            .runtimes
            .get(&binding.runtime_id())
            .ok_or(RuntimeInterestError::RetryMismatch)?
            .spec
            .clone();
        let acquisition =
            crate::cas_projection::acquisition::ProjectionAcquisition::admit(&self.shared.commands)
                .map_err(|_| RuntimeInterestError::Closed)?;
        let probe = RuntimeInterestTestProbe::new(generation);
        self.acquire_with_retry(
            spec,
            binding,
            RuntimeInterestKind::RequiredWork,
            Some(snapshot),
            false,
            &acquisition,
            || Ok(Box::new(move || probe.launch())),
        )
    }

    pub(in crate::cas_projection) fn model_read(
        &self,
        binding: ExecutionBinding,
    ) -> Result<RuntimeModelRead, RuntimeModelReadError> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| RuntimeModelReadError::Unavailable)?;
        if state.closed || !self.shared.commands.is_open() {
            return Err(RuntimeModelReadError::Unavailable);
        }
        let entry = state
            .runtimes
            .get(&binding.runtime_id())
            .ok_or(RuntimeModelReadError::Unavailable)?;
        let RuntimeInterestStatus::Ready(readiness) = entry.status else {
            return Err(RuntimeModelReadError::Unavailable);
        };
        if entry.spec.runtime_mode() != binding.root_path().mode() || entry.connector.is_none() {
            return Err(RuntimeModelReadError::Unavailable);
        }
        Ok(RuntimeModelRead {
            shared: Arc::downgrade(&self.shared),
            binding,
            attempt: entry.attempt,
            readiness,
        })
    }
}

impl RuntimeModelRead {
    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, RuntimeModelReadError> {
        let shared = self
            .shared
            .upgrade()
            .ok_or(RuntimeModelReadError::Unavailable)?;
        let state = shared.state.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => RuntimeModelReadError::Busy,
            TryLockError::Poisoned(_) => RuntimeModelReadError::Unavailable,
        })?;
        let entry = state
            .runtimes
            .get(&self.binding.runtime_id())
            .ok_or(RuntimeModelReadError::Unavailable)?;
        if state.closed
            || entry.attempt != self.attempt
            || entry.status != RuntimeInterestStatus::Ready(self.readiness)
            || entry.spec.runtime_mode() != self.binding.root_path().mode()
            || entry.connector.is_none()
        {
            return Err(RuntimeModelReadError::Unavailable);
        }
        shared
            .commands
            .try_with_work_open(publish)
            .ok_or(RuntimeModelReadError::Unavailable)
    }

    fn request<T>(
        &self,
        timeout: Duration,
        read: impl FnOnce(&mut beryl_backend::ManagedBackendSession) -> Result<T, ManagedBackendError>,
    ) -> Result<T, RuntimeModelReadError> {
        if timeout.is_zero() {
            return Err(RuntimeModelReadError::Timeout);
        }
        self.with_current(|| ())?;
        let shared = self
            .shared
            .upgrade()
            .ok_or(RuntimeModelReadError::Unavailable)?;
        let connector = {
            let state = shared
                .state
                .lock()
                .map_err(|_| RuntimeModelReadError::Unavailable)?;
            let entry = state
                .runtimes
                .get(&self.binding.runtime_id())
                .filter(|entry| {
                    entry.attempt == self.attempt
                        && entry.status == RuntimeInterestStatus::Ready(self.readiness)
                })
                .ok_or(RuntimeModelReadError::Unavailable)?;
            entry
                .connector
                .clone()
                .ok_or(RuntimeModelReadError::Unavailable)?
        };
        let mut session = connector.connect_request_client(timeout)?;
        let result = read(&mut session);
        session.shutdown()?;
        let result = result?;
        self.with_current(|| result)
    }

    pub(crate) fn read_defaults(
        &self,
        timeout: Duration,
    ) -> Result<BackendConfigDefaults, RuntimeModelReadError> {
        self.request(timeout, |session| {
            session
                .read_config(Path::new(self.binding.root_path().as_str()), timeout)
                .map(|response| response.into_defaults())
        })
    }

    pub(crate) fn read_page(
        &self,
        options: &ModelListOptions,
        timeout: Duration,
    ) -> Result<Box<ModelPage>, RuntimeModelReadError> {
        self.request(timeout, |session| session.list_model_page(options, timeout))
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/runtime_model_read.rs"
    ));
}
