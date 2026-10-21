use super::*;

#[cfg(test)]
#[path = "../../tests/unit/generated_source_authority.rs"]
mod tests;
pub(crate) struct LoadedSourceSink<'a> {
    pub(crate) sink: &'a mut dyn crate::OrderedTurnStreamSink,
    pub(crate) authority: Option<Arc<LoadedSourceAuthority>>,
}
impl crate::OrderedTurnStreamSink for LoadedSourceSink<'_> {
    fn steering_user_message_capture_mode(
        &mut self,
    ) -> Result<crate::SteeringUserMessageCaptureMode, crate::OrderedTurnStreamSubmitCause> {
        self.sink.steering_user_message_capture_mode()
    }
    fn submit_unverified_steering_user_message(
        &mut self,
        message: crate::UnverifiedSteeringUserMessage,
    ) -> Result<(), crate::OrderedTurnStreamSubmitCause> {
        self.sink.submit_unverified_steering_user_message(message)
    }
    fn submit(
        &mut self,
        operation: crate::OrderedTurnStreamOperation,
    ) -> Result<crate::OrderedTurnStreamCompletion, crate::OrderedTurnStreamSubmitError> {
        if let crate::OrderedTurnStreamOperation::ThreadClosed(closed) = &operation {
            if let Some(authority) = &self.authority {
                if &authority.thread == closed.thread_id() {
                    authority.live.store(false, Ordering::Release);
                }
            }
        }
        self.sink.submit(operation)
    }
    fn select_steering_user_message(
        &mut self,
        selection: crate::SteeringUserMessageSelection,
    ) -> Result<crate::SteeringUserMessageSource, crate::SteeringUserMessageSelectionError> {
        self.sink.select_steering_user_message(selection)
    }
    fn submit_checked_steering_user_message(
        &mut self,
        message: crate::CheckedSteeringUserMessage,
    ) -> Result<(), crate::CheckedSteeringUserMessageSubmitError> {
        self.sink.submit_checked_steering_user_message(message)
    }
    fn abandon_steering_user_message(
        &mut self,
        reason: crate::SteeringUserMessageAbandonReason,
    ) -> Result<(), crate::OrderedTurnStreamSubmitCause> {
        self.sink.abandon_steering_user_message(reason)
    }
}
impl ManagedBackendSession {
    pub(crate) fn register_generated_loaded_source(&mut self, loaded: &mut LoadedThreadSession) {
        if let Some(previous) = self.generated_loaded_source.take() {
            previous.live.store(false, Ordering::Release);
        }
        let authority = Arc::new(LoadedSourceAuthority {
            thread: loaded.thread_id().clone(),
            live: AtomicBool::new(true),
        });
        loaded.generated_source_authority = Some(authority.clone());
        self.generated_loaded_source = Some(authority);
    }
    pub(crate) fn retire_generated_loaded_source(&mut self) {
        if let Some(previous) = self.generated_loaded_source.take() {
            previous.live.store(false, Ordering::Release);
        }
    }
    pub(crate) fn retire_generated_thread(&mut self, thread: &CasThreadId) {
        if self
            .generated_loaded_source
            .as_ref()
            .is_some_and(|authority| &authority.thread == thread)
        {
            self.retire_generated_loaded_source();
        }
    }
    pub fn authorize_generated_output(
        &self,
        release: &ManagedBackendReleaseAdmission,
        loaded: &LoadedThreadSession,
        target: GeneratedOutputTarget,
    ) -> Result<GeneratedOutputPermit, GeneratedOutputError> {
        let Some(ManagedLaunchProvenance::Production(launch)) = &self.managed_launch_provenance
        else {
            return Err(GeneratedOutputError::LostSourceAuthority);
        };
        if !launch.same_instance(release.launch_identity()) || !self.generated_profile_is_live() {
            return Err(GeneratedOutputError::LostSourceAuthority);
        }
        let authority = loaded
            .generated_source_authority
            .as_ref()
            .ok_or(GeneratedOutputError::LostSourceAuthority)?;
        if !authority.live.load(Ordering::Acquire)
            || !self
                .generated_loaded_source
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, authority))
            || authority.thread != *loaded.thread_id()
        {
            return Err(GeneratedOutputError::LostSourceAuthority);
        }
        Ok(GeneratedOutputPermit {
            launch: launch.clone(),
            loaded: authority.clone(),
            target,
        })
    }
}
