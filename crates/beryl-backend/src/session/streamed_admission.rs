use super::{BackendClientTransport, ManagedBackendError, ManagedBackendSession};
use crate::incoming_json::ResponseFamily;

pub(super) struct StreamedRequestReservation {
    pub(super) request_id: u64,
    pub(super) next_request_id: u64,
}

impl ManagedBackendSession {
    pub(super) fn reserve_streamed_request(
        &mut self,
        method: &'static str,
        response_family: ResponseFamily,
    ) -> Result<StreamedRequestReservation, ManagedBackendError> {
        if !matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) {
            let transport = match &self.transport {
                BackendClientTransport::RequestOnlyWebSocket(_) => "request-only websocket",
                BackendClientTransport::ForegroundWebSocket(_) => unreachable!(),
                #[cfg(feature = "lifecycle-test-support")]
                BackendClientTransport::Unsupported => "unsupported",
            };
            return Err(ManagedBackendError::StreamedInputTransportUnsupported {
                method: method.to_string(),
                transport,
            });
        }
        if self.initialize.is_none() {
            return Err(ManagedBackendError::ClientNotInitialized);
        }
        if !self.has_full_turn_stream() {
            return Err(ManagedBackendError::RequestProfileMismatch {
                method,
                required_profile: "full turn stream",
            });
        }
        if self.ordered_turn_stream_sink.is_none() {
            return Err(ManagedBackendError::OrderedTurnStreamSinkUnbound);
        }
        if self.transport.is_closed() {
            return Err(ManagedBackendError::TransportClosed {
                method: method.to_string(),
            });
        }

        let Some(next_request_id) = self.next_request_id.checked_add(1) else {
            return Err(ManagedBackendError::RequestIdExhausted { method });
        };
        let request_id = self.next_request_id;
        if self
            .response_expectation
            .install_fixed(request_id, response_family)
            .is_err()
        {
            return Err(ManagedBackendError::ResponseExpectationUnavailable { method });
        }

        Ok(StreamedRequestReservation {
            request_id,
            next_request_id,
        })
    }
}
