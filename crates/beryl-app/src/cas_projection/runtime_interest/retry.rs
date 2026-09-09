use super::*;

impl RuntimeInterestOwner {
    pub(in crate::cas_projection) fn pending_retry(
        &self,
        runtime_id: RuntimeId,
    ) -> Option<(SyndicThreadId, ExecutionBinding)> {
        let state = self.shared.lock();
        let retry = state.runtimes.get(&runtime_id)?.retry.as_ref()?;
        Some((retry.thread_id, retry.binding.clone()))
    }

    pub(in crate::cas_projection) fn authorize_retry(
        &self,
        snapshot: RuntimeFailureSnapshot,
        thread_id: SyndicThreadId,
        binding: ExecutionBinding,
    ) -> Result<(), RuntimeInterestError> {
        let command = self
            .shared
            .commands
            .authorize()
            .map_err(|_| RuntimeInterestError::Closed)?;
        let mut state = self.shared.lock();
        self.reap_finished(&mut state);
        let entry = state
            .runtimes
            .get_mut(&snapshot.runtime_id)
            .ok_or(RuntimeInterestError::RetryMismatch)?;
        if snapshot.service_generation != self.shared.commands.service_generation()
            || snapshot.runtime_id != binding.runtime_id()
            || snapshot.attempt != entry.attempt
            || !matches!(entry.status, RuntimeInterestStatus::Unavailable(_))
            || entry.worker.is_some()
            || !entry.cleanup_complete
            || entry.retry.is_some()
        {
            return Err(RuntimeInterestError::RetryMismatch);
        }
        command
            .commit_if_current(|| entry.retry = Some(RuntimeRetryTarget { thread_id, binding }))
            .map_err(|_| RuntimeInterestError::Closed)
    }

    pub(super) fn scheduled_retry(
        &self,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> Option<RuntimeFailureSnapshot> {
        let state = self.shared.lock();
        let entry = state.runtimes.get(&binding.runtime_id())?;
        let retry = entry.retry.as_ref()?;
        if retry.thread_id != thread_id || &retry.binding != binding {
            return None;
        }
        let RuntimeInterestStatus::Unavailable(failure) = entry.status else {
            return None;
        };
        Some(RuntimeFailureSnapshot {
            runtime_id: binding.runtime_id(),
            service_generation: self.shared.commands.service_generation(),
            attempt: entry.attempt,
            failure,
            retry_ready: entry.worker.is_none() && entry.cleanup_complete,
        })
    }

    pub(in crate::cas_projection) fn revoke_retry(
        &self,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
    ) {
        let mut state = self.shared.lock();
        if let Some(entry) = state.runtimes.get_mut(&binding.runtime_id())
            && entry
                .retry
                .as_ref()
                .is_some_and(|retry| retry.thread_id == thread_id && &retry.binding == binding)
        {
            entry.retry = None;
        }
    }
}
