use std::{
    path::Path,
    time::{Duration, Instant},
};

use beryl_model::CasThreadId;

use super::{
    BackendClientTransport, IncomingMessage, InitializedNotificationProfile, ManagedBackendError,
    ManagedBackendSession, ReceiveOutcome, TransportWriteFailure,
};
use crate::{
    BoundedResponseResult, ConfigReadResponse, JsonRpcError, ModelListOptions, ModelPage,
    ThreadUnsubscribeResponse, incoming_json::ResponseFamily,
};

mod compatibility;
mod lineage;
mod thread_read;
mod wire;

use wire::{
    ConfigReadParams, InitializeParams, InitializedNotification, JsonRpcRequest, ModelListParams,
    RequestSpec, ThreadUnsubscribeParams,
};

enum RequestCompletion {
    Response(BoundedResponseResult),
    Rejection(JsonRpcError),
}

impl ManagedBackendSession {
    /// Initializes one immutable full-profile foreground candidate.
    pub fn initialize_foreground(&mut self, timeout: Duration) -> Result<(), ManagedBackendError> {
        if !matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) {
            return Err(ManagedBackendError::InitializationProfileMismatch {
                profile: "foreground",
            });
        }
        self.initialize_with(
            &InitializeParams::foreground(),
            InitializedNotificationProfile::FullTurnStream,
            timeout,
        )
    }

    pub(crate) fn initialize_request_only(
        &mut self,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        if !matches!(
            self.transport,
            BackendClientTransport::RequestOnlyWebSocket(_)
        ) {
            return Err(ManagedBackendError::InitializationProfileMismatch {
                profile: "request-only",
            });
        }
        self.initialize_with(
            &InitializeParams::request_only(),
            InitializedNotificationProfile::OptedOut,
            timeout,
        )
    }

    fn initialize_with(
        &mut self,
        params: &InitializeParams,
        profile: InitializedNotificationProfile,
        timeout: Duration,
    ) -> Result<(), ManagedBackendError> {
        if self.initialize.is_some() || self.initialized_notification_profile.is_some() {
            return Err(ManagedBackendError::ClientAlreadyInitialized);
        }

        let completion = self.dispatch_request(params, timeout)?;
        let exact = self.exact_response(completion, ResponseFamily::Initialize.method())?;
        let BoundedResponseResult::Initialize(initialize) = exact.result else {
            return self.fail_unexpected_response(ResponseFamily::Initialize.method());
        };
        if let Err(source) = initialize.validate_required_app_server_version() {
            self.retire_connection();
            return Err(ManagedBackendError::Compatibility(source));
        }

        if let Err(failure) = self
            .transport
            .write_message("initialized", &InitializedNotification::new())
        {
            self.retire_connection();
            return Err(failure.into_error());
        }

        self.initialize = Some(initialize);
        self.initialized_notification_profile = Some(profile);
        self.experimental_api_negotiated = true;
        Ok(())
    }

    /// Requests exactly one bounded page with the fixed maximum record count.
    pub fn list_models(
        &mut self,
        timeout: Duration,
    ) -> Result<Box<ModelPage>, ManagedBackendError> {
        self.list_model_page(&ModelListOptions::default(), timeout)
    }

    /// Requests exactly one bounded page and never follows its continuation cursor.
    pub fn list_model_page(
        &mut self,
        options: &ModelListOptions,
        timeout: Duration,
    ) -> Result<Box<ModelPage>, ManagedBackendError> {
        let params = ModelListParams::new(options);
        let completion = self.dispatch_request(&params, timeout)?;
        let exact = self.exact_response(completion, ResponseFamily::ModelList.method())?;
        let BoundedResponseResult::ModelList(page) = exact.result else {
            return self.fail_unexpected_response(ResponseFamily::ModelList.method());
        };
        Ok(page)
    }

    /// Reads only the fixed defaults projection from one bounded config response.
    pub fn read_config(
        &mut self,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ConfigReadResponse, ManagedBackendError> {
        let params = ConfigReadParams::new(cwd);
        let completion = self.dispatch_request(&params, timeout)?;
        let exact = self.exact_response(completion, ResponseFamily::ConfigRead.method())?;
        let BoundedResponseResult::ConfigRead(config) = exact.result else {
            return self.fail_unexpected_response(ResponseFamily::ConfigRead.method());
        };
        Ok(config)
    }

    /// Unloads one exact thread from this foreground connection.
    pub fn unsubscribe_thread(
        &mut self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, ManagedBackendError> {
        let family = ResponseFamily::ThreadUnsubscribe;
        self.require_foreground_request(family.method())?;
        let params = ThreadUnsubscribeParams::new(thread_id);
        let completion = self.dispatch_request(&params, timeout)?;
        let exact = self.exact_response(completion, family.method())?;
        let BoundedResponseResult::ThreadUnsubscribe(status) = exact.result else {
            return self.fail_unexpected_response(family.method());
        };
        Ok(ThreadUnsubscribeResponse::new(status))
    }

    fn require_foreground_request(&self, method: &'static str) -> Result<(), ManagedBackendError> {
        if !matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) {
            return Err(ManagedBackendError::RequestProfileMismatch {
                method,
                required_profile: "foreground",
            });
        }
        if self.initialize.is_none() {
            return Err(ManagedBackendError::ClientNotInitialized);
        }
        if !matches!(
            self.initialized_notification_profile,
            Some(InitializedNotificationProfile::FullTurnStream)
        ) {
            return Err(ManagedBackendError::RequestProfileMismatch {
                method,
                required_profile: "foreground",
            });
        }
        Ok(())
    }

    fn dispatch_request<P: RequestSpec>(
        &mut self,
        params: &P,
        timeout: Duration,
    ) -> Result<RequestCompletion, ManagedBackendError> {
        let family = params.response_family();
        let method = family.method();
        if family != ResponseFamily::Initialize && self.initialize.is_none() {
            return Err(ManagedBackendError::ClientNotInitialized);
        }
        let next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(ManagedBackendError::RequestIdExhausted { method })?;
        let request_id = self.next_request_id;

        self.response_expectation
            .install_fixed(request_id, family)
            .map_err(|_| ManagedBackendError::ResponseExpectationUnavailable { method })?;

        let request = JsonRpcRequest::new(request_id, params);
        match self.transport.write_message(method, &request) {
            Ok(_) => self.next_request_id = next_request_id,
            Err(TransportWriteFailure::ProvenNotDispatched(error)) => {
                if !self.response_expectation.cancel(request_id) {
                    self.retire_connection();
                    return Err(ManagedBackendError::ResponseExpectationUnavailable { method });
                }
                return Err(error);
            }
            Err(TransportWriteFailure::MayHaveDispatched(error)) => {
                self.retire_connection();
                return Err(error);
            }
        }

        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                self.retire_connection();
                return Err(ManagedBackendError::RequestTimeout {
                    method: method.to_string(),
                    timeout,
                });
            };
            match self.recv_message_timeout(method, remaining)? {
                ReceiveOutcome::Quiet => {
                    self.retire_connection();
                    return Err(ManagedBackendError::RequestTimeout {
                        method: method.to_string(),
                        timeout,
                    });
                }
                ReceiveOutcome::OrderedProgress => {}
                ReceiveOutcome::Message(message @ IncomingMessage::Approval { .. }) => {
                    match self.submit_approval_message(method, message) {
                        Ok(_) | Err(ManagedBackendError::ApprovalTargetFailed { .. }) => {}
                        Err(error) => return Err(error),
                    }
                }
                ReceiveOutcome::Response(result) => {
                    return Ok(RequestCompletion::Response(result));
                }
                ReceiveOutcome::Rejection(error) => {
                    return Ok(RequestCompletion::Rejection(error));
                }
            }
        }
    }

    fn exact_response(
        &mut self,
        completion: RequestCompletion,
        method: &'static str,
    ) -> Result<ExactResponse, ManagedBackendError> {
        match completion {
            RequestCompletion::Response(result) => Ok(ExactResponse { result }),
            RequestCompletion::Rejection(error) => Err(ManagedBackendError::RequestFailed {
                method: method.to_string(),
                error: Box::new(error),
            }),
        }
    }

    fn fail_unexpected_response<T>(
        &mut self,
        method: &'static str,
    ) -> Result<T, ManagedBackendError> {
        self.retire_connection();
        Err(ManagedBackendError::UnexpectedBoundedResponse { method })
    }
}

struct ExactResponse {
    result: BoundedResponseResult,
}

impl TransportWriteFailure {
    fn into_error(self) -> ManagedBackendError {
        match self {
            Self::ProvenNotDispatched(error) | Self::MayHaveDispatched(error) => error,
        }
    }
}
