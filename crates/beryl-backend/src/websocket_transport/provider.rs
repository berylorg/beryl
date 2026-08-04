use std::time::{Duration, Instant};

use tracing::debug;

use super::{
    PayloadReaderState, RequestOnlyWebSocketTransport, WEBSOCKET_PROTOCOL_PAYLOAD_BUDGET,
    WebSocketClientTransport, WebSocketIngressStats, WebSocketPayloadReader, elapsed_ms,
};
use crate::{
    incoming_json,
    session::{ManagedBackendError, ManagedWebSocketError},
    turn::StreamedUserMessageVerifierHandle,
};

const READ_CHUNK_BYTES: usize = 8 * 1024;
impl WebSocketClientTransport {
    pub(crate) fn recv_json_value_timeout<'a>(
        &mut self,
        method: &str,
        timeout: Duration,
        verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
        ordered_sink: Option<&'a mut dyn crate::OrderedTurnStreamSink>,
        response_authority_generation: u64,
        response_expectation: &mut incoming_json::ResponseExpectationSlot,
    ) -> Result<Option<incoming_json::DecodedIncoming>, ManagedBackendError> {
        self.set_read_timeout(Some(timeout), method)?;
        #[cfg(feature = "lifecycle-test-support")]
        let diagnostics = self.diagnostics.clone();
        let receive_started = Instant::now();
        let state = PayloadReaderState::default();
        let reader = WebSocketPayloadReader::new(
            self,
            method,
            WEBSOCKET_PROTOCOL_PAYLOAD_BUDGET,
            state.clone(),
        );
        let decoded = incoming_json::decode_reader_with_provider(
            reader,
            READ_CHUNK_BYTES,
            verifier,
            ordered_sink,
            response_authority_generation,
            response_expectation,
        );

        if let Some(error) = state.take_failure() {
            self.close();
            return Err(error);
        }
        if !state.started() {
            return Ok(None);
        }
        let decoded = match decoded {
            Ok(decoded) => decoded,
            Err(incoming_json::DecodeReaderError::Json(source)) => {
                self.close();
                return Err(ManagedBackendError::InvalidJsonLine {
                    line: incoming_json::redacted_invalid_json(),
                    source,
                });
            }
            Err(incoming_json::DecodeReaderError::Correlation(source)) => {
                self.close();
                return Err(ManagedBackendError::StreamedUserMessageCorrelation {
                    method: method.to_string(),
                    source,
                    transport_bytes_written: true,
                });
            }
            Err(incoming_json::DecodeReaderError::Steering(source)) => {
                self.close();
                return Err(ManagedBackendError::SteeringUserMessage {
                    method: method.to_string(),
                    source,
                });
            }
            Err(incoming_json::DecodeReaderError::Approval { kind, source }) => {
                self.close();
                return Err(ManagedBackendError::InvalidApprovalRequest { kind, source });
            }
            Err(incoming_json::DecodeReaderError::Provider(source)) => {
                self.close();
                return Err(ManagedBackendError::ProviderObservation {
                    method: method.to_string(),
                    source,
                });
            }
            Err(incoming_json::DecodeReaderError::DynamicTool(source)) => {
                self.close();
                return Err(ManagedBackendError::DynamicToolCall {
                    method: method.to_string(),
                    source,
                });
            }
            Err(incoming_json::DecodeReaderError::Ordered(source)) => {
                self.close();
                return Err(ManagedBackendError::OrderedTurnStream {
                    method: method.to_string(),
                    source,
                });
            }
            Err(incoming_json::DecodeReaderError::OrderedUnexpectedCompletion) => {
                self.close();
                return Err(ManagedBackendError::OrderedTurnStreamUnexpectedCompletion {
                    method: method.to_string(),
                });
            }
            Err(incoming_json::DecodeReaderError::Envelope(source)) => {
                self.close();
                return Err(ManagedBackendError::ForegroundIngress {
                    method: method.to_string(),
                    source,
                });
            }
        };
        if !state.complete() {
            let error = self.transport_error(
                method,
                ManagedWebSocketError::protocol(
                    "incoming JSON parser stopped before the WebSocket message completed",
                ),
            );
            self.close();
            return Err(error);
        }

        let ingress_stats = WebSocketIngressStats {
            message_bytes: state.bytes_read(),
            maximum_transport_chunk_bytes: state.maximum_chunk_bytes(),
            maximum_parser_buffer_bytes: decoded.stats.maximum_buffered_input_bytes,
            discarded_image_result_bytes: decoded.stats.discarded_image_result_bytes,
            verified_user_text_wire_bytes: decoded.stats.verified_user_text_wire_bytes,
            retained_item_result_present: match &decoded.incoming {
                incoming_json::DecodedIncoming::Approval(_)
                | incoming_json::DecodedIncoming::OrderedHandled
                | incoming_json::DecodedIncoming::DiscardedNotification
                | incoming_json::DecodedIncoming::Response { .. }
                | incoming_json::DecodedIncoming::Rejection { .. } => false,
            },
        };
        self.last_ingress_stats = Some(ingress_stats);
        #[cfg(feature = "lifecycle-test-support")]
        diagnostics.record_decoded_message(
            ingress_stats.message_bytes,
            ingress_stats.maximum_transport_chunk_bytes,
            ingress_stats.maximum_parser_buffer_bytes,
            ingress_stats.verified_user_text_wire_bytes,
        );

        debug!(
            method,
            response_bytes = ingress_stats.message_bytes,
            maximum_transport_chunk_bytes = ingress_stats.maximum_transport_chunk_bytes,
            maximum_parser_buffer_bytes = ingress_stats.maximum_parser_buffer_bytes,
            discarded_image_result_bytes = ingress_stats.discarded_image_result_bytes,
            verified_user_text_wire_bytes = ingress_stats.verified_user_text_wire_bytes,
            retained_item_result_present = ingress_stats.retained_item_result_present,
            wait_first_frame_ms = state.first_frame_after().map(elapsed_ms),
            wait_first_payload_ms = state.first_payload_after().map(elapsed_ms),
            full_message_ms = elapsed_ms(receive_started.elapsed()),
            "received and parsed backend WebSocket JSON message"
        );
        Ok(Some(decoded.incoming))
    }
}

impl RequestOnlyWebSocketTransport {
    pub(crate) fn recv_json_value_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
        response_expectation: &mut incoming_json::ResponseExpectationSlot,
    ) -> Result<Option<incoming_json::DecodedIncoming>, ManagedBackendError> {
        self.inner
            .recv_json_value_timeout(method, timeout, None, None, 0, response_expectation)
    }
}
