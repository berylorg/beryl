use std::time::Duration;

use beryl_model::CasThreadId;

use super::{
    BackendClientTransport, InitializedNotificationProfile, ManagedBackendError,
    ManagedBackendSession, wire::ThreadReadParams,
};
use crate::{BoundedResponseResult, ThreadReadMetadata, incoming_json::ResponseFamily};

impl ManagedBackendSession {
    /// Reads only compact bounded metadata for one exact thread on a maintenance session.
    pub fn read_thread_metadata(
        &mut self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<ThreadReadMetadata, ManagedBackendError> {
        let family = ResponseFamily::ThreadRead;
        self.require_request_only_thread_read(family.method())?;
        let params = ThreadReadParams::new(thread_id);
        let completion = self.dispatch_request(&params, timeout)?;
        let exact = self.exact_response(completion, family.method())?;
        let BoundedResponseResult::ThreadRead(metadata) = exact.result else {
            return self.fail_unexpected_response(family.method());
        };
        if metadata.thread_id() != thread_id {
            let actual = metadata.thread_id().clone();
            self.retire_connection();
            return Err(ManagedBackendError::ThreadResponseIdentityMismatch {
                method: family.method().to_owned(),
                expected: thread_id.clone(),
                actual,
            });
        }
        Ok(metadata)
    }

    fn require_request_only_thread_read(
        &self,
        method: &'static str,
    ) -> Result<(), ManagedBackendError> {
        if !matches!(
            self.transport,
            BackendClientTransport::RequestOnlyWebSocket(_)
        ) {
            return Err(ManagedBackendError::RequestProfileMismatch {
                method,
                required_profile: "request-only",
            });
        }
        if self.initialize.is_none() {
            return Err(ManagedBackendError::ClientNotInitialized);
        }
        if !matches!(
            self.initialized_notification_profile,
            Some(InitializedNotificationProfile::OptedOut)
        ) {
            return Err(ManagedBackendError::RequestProfileMismatch {
                method,
                required_profile: "request-only",
            });
        }
        Ok(())
    }
}
