use std::{
    fmt,
    ops::{Deref, DerefMut},
    sync::{Mutex, MutexGuard},
};

use beryl_model::CasThreadId;

use super::{
    super::StreamedInputSource, state::StreamedUserMessageVerifier,
    types::StreamedUserMessageCorrelationError,
};

pub(crate) type StreamedUserMessageVerifierHandle<'a> = &'a StreamedUserMessageVerifierSlot;

#[derive(Default)]
pub(crate) struct StreamedUserMessageVerifierSlot {
    active: Mutex<Option<StreamedUserMessageVerifier>>,
}

pub(crate) struct StreamedUserMessageVerifierGuard<'a>(
    MutexGuard<'a, Option<StreamedUserMessageVerifier>>,
);

impl Deref for StreamedUserMessageVerifierGuard<'_> {
    type Target = StreamedUserMessageVerifier;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("guard exists only for an active verifier")
    }
}

impl DerefMut for StreamedUserMessageVerifierGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
            .as_mut()
            .expect("guard exists only for an active verifier")
    }
}

impl fmt::Debug for StreamedUserMessageVerifierSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let active = match self.active.lock() {
            Ok(active) => active.is_some().to_string(),
            Err(_) => "poisoned".to_string(),
        };
        formatter
            .debug_struct("StreamedUserMessageVerifierSlot")
            .field("active", &active)
            .finish()
    }
}

impl StreamedUserMessageVerifierSlot {
    pub(crate) fn install(
        &self,
        request_scope: u64,
        target_thread_id: CasThreadId,
        source: Box<dyn StreamedInputSource>,
    ) -> Result<(), StreamedUserMessageCorrelationError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        if active.is_some() {
            return Err(StreamedUserMessageCorrelationError::VerifierAlreadyInstalled);
        }
        *active = Some(StreamedUserMessageVerifier::new(
            request_scope,
            target_thread_id,
            source,
        ));
        Ok(())
    }

    pub(crate) fn active_handle(
        &self,
    ) -> Result<Option<StreamedUserMessageVerifierHandle<'_>>, StreamedUserMessageCorrelationError>
    {
        let active = self
            .active
            .lock()
            .map_err(|_| StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        Ok(active.is_some().then_some(self))
    }

    pub(crate) fn lock(
        &self,
    ) -> Result<StreamedUserMessageVerifierGuard<'_>, StreamedUserMessageCorrelationError> {
        let active = self
            .active
            .lock()
            .map_err(|_| StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        if active.is_none() {
            return Err(StreamedUserMessageCorrelationError::VerifierUnavailable);
        }
        Ok(StreamedUserMessageVerifierGuard(active))
    }

    pub(crate) fn remove(
        &self,
        expected_scope: u64,
    ) -> Result<StreamedUserMessageVerifier, StreamedUserMessageCorrelationError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        let Some(installed) = active.take() else {
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        };
        if installed.request_scope() != expected_scope {
            *active = Some(installed);
            return Err(StreamedUserMessageCorrelationError::VerifierScopeDisagreement);
        }
        Ok(installed)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn poison_for_lifecycle_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _active = self.active.lock().expect("test slot starts unpoisoned");
            panic!("poison streamed user-message verifier slot for lifecycle test");
        }));
    }
}
