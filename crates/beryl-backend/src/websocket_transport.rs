use std::{
    collections::VecDeque,
    io::{self, Read},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream},
    time::Duration,
};

use serde::Serialize;
use soketto::{
    Parsing,
    base::{Codec, Header, OpCode},
    handshake::client::{Client, Header as HandshakeHeader},
};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use crate::{
    BackendWebSocketEndpoint,
    session::{
        ManagedBackendError, ManagedWebSocketError, TransportWriteFailure,
        outbound::{OutboundWriteFailure, OutboundWriteMetrics, write_json},
    },
    thread_injection::{
        ThreadInjectionSourceFailureSlot, ThreadInjectionWriteFailure, write_injection_source_json,
    },
    turn::{
        StreamedInputJsonWriteFailure, StreamedInputSourceFailureSlot, write_source_aware_json,
    },
};

#[cfg(feature = "lifecycle-test-support")]
pub(crate) mod diagnostics;
mod message;
mod provider;
mod reader;
mod writer;

#[cfg(feature = "lifecycle-test-support")]
pub(crate) use diagnostics::WebSocketDiagnostics;

use message::{MessagePayload, PayloadRead};
use reader::{PayloadReaderState, WebSocketPayloadReader};
use writer::{OUTBOUND_FRAME_PAYLOAD_BYTES, WebSocketMessageWriter, write_control_frame};

const WEBSOCKET_PROTOCOL_PAYLOAD_BUDGET: usize = usize::MAX;
const WEBSOCKET_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const WEBSOCKET_HANDSHAKE_READ_AHEAD_BUDGET: usize = 4 * 1024;

struct WebSocketClientTransport {
    endpoint: String,
    stream: TcpStream,
    read_codec: Codec,
    write_codec: Codec,
    outbound_payload: Box<[u8]>,
    pending_read: VecDeque<u8>,
    last_ingress_stats: Option<WebSocketIngressStats>,
    #[cfg(feature = "lifecycle-test-support")]
    diagnostics: WebSocketDiagnostics,
    #[cfg(feature = "lifecycle-test-support")]
    fail_next_write_before_dispatch: bool,
    closed: bool,
}

/// Immutable foreground candidate selected before the authenticated handshake reads any byte.
pub(crate) struct ForegroundWebSocketCandidate {
    endpoint: BackendWebSocketEndpoint,
    authorization_header_value: String,
    config: Option<crate::ForegroundSessionConfig>,
}

/// Connected WebSocket whose only ingress policy is the foreground incremental machine.
pub(crate) struct ForegroundWebSocketTransport {
    inner: WebSocketClientTransport,
    config: crate::ForegroundSessionConfig,
}

/// Immutable request-only candidate selected before the authenticated handshake reads any byte.
pub(crate) struct RequestOnlyWebSocketCandidate {
    endpoint: BackendWebSocketEndpoint,
    authorization_header_value: String,
}

/// Connected request-only WebSocket that uses incremental response ingress without foreground
/// verifier, ordered-sink, or compact-control capabilities.
pub(crate) struct RequestOnlyWebSocketTransport {
    inner: WebSocketClientTransport,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WebSocketIngressStats {
    pub(crate) message_bytes: usize,
    pub(crate) maximum_transport_chunk_bytes: usize,
    pub(crate) maximum_parser_buffer_bytes: usize,
    pub(crate) discarded_image_result_bytes: usize,
    pub(crate) verified_user_text_wire_bytes: usize,
    pub(crate) retained_item_result_present: bool,
}

impl ForegroundWebSocketCandidate {
    pub(crate) const fn new(
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
        config: crate::ForegroundSessionConfig,
    ) -> Self {
        Self {
            endpoint,
            authorization_header_value,
            config: Some(config),
        }
    }

    pub(crate) fn try_connect(
        &mut self,
    ) -> Result<ForegroundWebSocketTransport, ManagedBackendError> {
        let inner = WebSocketClientTransport::connect_profiled(
            &self.endpoint,
            self.authorization_header_value.clone(),
        )?;
        Ok(ForegroundWebSocketTransport {
            inner,
            config: self
                .config
                .take()
                .expect("a foreground candidate connects at most once"),
        })
    }
}

impl RequestOnlyWebSocketCandidate {
    pub(crate) const fn new(
        endpoint: BackendWebSocketEndpoint,
        authorization_header_value: String,
    ) -> Self {
        Self {
            endpoint,
            authorization_header_value,
        }
    }

    pub(crate) fn try_connect(&self) -> Result<RequestOnlyWebSocketTransport, ManagedBackendError> {
        WebSocketClientTransport::connect_profiled(
            &self.endpoint,
            self.authorization_header_value.clone(),
        )
        .map(|inner| RequestOnlyWebSocketTransport { inner })
    }
}

impl WebSocketClientTransport {
    fn connect_profiled(
        endpoint: &BackendWebSocketEndpoint,
        authorization_header_value: String,
    ) -> Result<Self, ManagedBackendError> {
        let endpoint_label = endpoint.listen_url();
        let stream = connect_handshake(endpoint, authorization_header_value).map_err(|source| {
            ManagedBackendError::ConnectWebSocket {
                endpoint: endpoint_label.clone(),
                source,
            }
        })?;

        let mut read_codec = Codec::new();
        read_codec.set_max_data_size(WEBSOCKET_PROTOCOL_PAYLOAD_BUDGET);

        let outbound_payload = vec![0; OUTBOUND_FRAME_PAYLOAD_BYTES].into_boxed_slice();
        #[cfg(feature = "lifecycle-test-support")]
        let diagnostics = {
            let diagnostics = WebSocketDiagnostics::default();
            diagnostics.record_outbound_buffer_capacity(outbound_payload.len());
            diagnostics
        };
        Ok(Self {
            endpoint: endpoint_label,
            stream: stream.stream,
            read_codec,
            write_codec: Codec::new(),
            outbound_payload,
            pending_read: stream.pending_read,
            last_ingress_stats: None,
            #[cfg(feature = "lifecycle-test-support")]
            diagnostics,
            #[cfg(feature = "lifecycle-test-support")]
            fail_next_write_before_dispatch: false,
            closed: false,
        })
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub(crate) fn last_ingress_stats(&self) -> Option<WebSocketIngressStats> {
        self.last_ingress_stats
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn diagnostics(&self) -> WebSocketDiagnostics {
        self.diagnostics.clone()
    }

    pub(crate) const fn is_closed(&self) -> bool {
        self.closed
    }

    pub(crate) fn write_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
    ) -> Result<OutboundWriteMetrics, TransportWriteFailure> {
        if self.closed {
            return Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                },
            ));
        }
        #[cfg(feature = "lifecycle-test-support")]
        if std::mem::replace(&mut self.fail_next_write_before_dispatch, false) {
            return Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::WriteRequest {
                    method: method.to_string(),
                    source: io::Error::other("forced pre-dispatch lifecycle write failure"),
                },
            ));
        }
        let result = {
            let mut writer = WebSocketMessageWriter::new(
                &mut self.stream,
                &mut self.write_codec,
                &mut self.outbound_payload,
                #[cfg(feature = "lifecycle-test-support")]
                self.diagnostics.clone(),
            );
            write_json(&mut writer, message)
        };
        match result {
            Ok(metrics) => Ok(metrics),
            Err(failure) => {
                let progress = failure.progress();
                let error = match failure {
                    OutboundWriteFailure::Serialize { source, .. } if progress.some_bytes() => self
                        .transport_error(
                            method,
                            ManagedWebSocketError::from_io(io::Error::other(source)),
                        ),
                    OutboundWriteFailure::Serialize { source, .. } => {
                        ManagedBackendError::SerializeRequest {
                            method: method.to_string(),
                            source,
                        }
                    }
                    OutboundWriteFailure::Transport { source, .. } => {
                        self.transport_error(method, source)
                    }
                };
                if progress.some_bytes() {
                    self.poison();
                }
                Err(TransportWriteFailure::from_progress(progress, error))
            }
        }
    }

    pub(crate) fn write_streamed_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
        source_failure: &StreamedInputSourceFailureSlot,
    ) -> Result<OutboundWriteMetrics, TransportWriteFailure> {
        if self.closed {
            return Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                },
            ));
        }
        #[cfg(feature = "lifecycle-test-support")]
        if std::mem::replace(&mut self.fail_next_write_before_dispatch, false) {
            return Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::WriteRequest {
                    method: method.to_string(),
                    source: io::Error::other("forced pre-dispatch lifecycle write failure"),
                },
            ));
        }
        let result = {
            let mut writer = WebSocketMessageWriter::new(
                &mut self.stream,
                &mut self.write_codec,
                &mut self.outbound_payload,
                #[cfg(feature = "lifecycle-test-support")]
                self.diagnostics.clone(),
            );
            write_source_aware_json(&mut writer, message, source_failure)
        };
        match result {
            Ok(metrics) => Ok(metrics),
            Err(failure) => {
                let progress = failure.progress();
                let error =
                    match failure {
                        StreamedInputJsonWriteFailure::Source { source, .. } => {
                            ManagedBackendError::StreamedInputSource {
                                method: method.to_string(),
                                source,
                                transport_bytes_written: progress.some_bytes(),
                            }
                        }
                        StreamedInputJsonWriteFailure::Outbound(
                            OutboundWriteFailure::Serialize { source, .. },
                        ) if progress.some_bytes() => self.transport_error(
                            method,
                            ManagedWebSocketError::from_io(io::Error::other(source)),
                        ),
                        StreamedInputJsonWriteFailure::Outbound(
                            OutboundWriteFailure::Serialize { source, .. },
                        ) => ManagedBackendError::SerializeRequest {
                            method: method.to_string(),
                            source,
                        },
                        StreamedInputJsonWriteFailure::Outbound(
                            OutboundWriteFailure::Transport { source, .. },
                        ) => self.transport_error(method, source),
                    };
                if progress.some_bytes() {
                    self.poison();
                }
                Err(TransportWriteFailure::from_progress(progress, error))
            }
        }
    }

    pub(crate) fn write_injection_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
        source_failure: &ThreadInjectionSourceFailureSlot,
    ) -> Result<OutboundWriteMetrics, TransportWriteFailure> {
        if self.closed {
            return Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                },
            ));
        }
        #[cfg(feature = "lifecycle-test-support")]
        if std::mem::replace(&mut self.fail_next_write_before_dispatch, false) {
            return Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::WriteRequest {
                    method: method.to_string(),
                    source: io::Error::other("forced pre-dispatch lifecycle write failure"),
                },
            ));
        }
        let result = {
            let mut writer = WebSocketMessageWriter::new(
                &mut self.stream,
                &mut self.write_codec,
                &mut self.outbound_payload,
                #[cfg(feature = "lifecycle-test-support")]
                self.diagnostics.clone(),
            );
            write_injection_source_json(&mut writer, message, source_failure)
        };
        match result {
            Ok(metrics) => Ok(metrics),
            Err(failure) => {
                let progress = failure.progress();
                let error =
                    match failure {
                        ThreadInjectionWriteFailure::Source { source, .. } => {
                            ManagedBackendError::ThreadInjectionSource {
                                method: method.to_string(),
                                source,
                                transport_bytes_written: progress.some_bytes(),
                            }
                        }
                        ThreadInjectionWriteFailure::Outbound(
                            OutboundWriteFailure::Serialize { source, .. },
                        ) if progress.some_bytes() => self.transport_error(
                            method,
                            ManagedWebSocketError::from_io(io::Error::other(source)),
                        ),
                        ThreadInjectionWriteFailure::Outbound(
                            OutboundWriteFailure::Serialize { source, .. },
                        ) => ManagedBackendError::SerializeRequest {
                            method: method.to_string(),
                            source,
                        },
                        ThreadInjectionWriteFailure::Outbound(
                            OutboundWriteFailure::Transport { source, .. },
                        ) => self.transport_error(method, source),
                    };
                if progress.some_bytes() {
                    self.poison();
                }
                Err(TransportWriteFailure::from_progress(progress, error))
            }
        }
    }

    pub(crate) fn close(&mut self) {
        if self.closed {
            return;
        }
        let _ = self.write_close_frame("close");
        let _ = self.stream.shutdown(Shutdown::Both);
        self.closed = true;
    }
}

impl ForegroundWebSocketTransport {
    pub(crate) fn endpoint(&self) -> &str {
        self.inner.endpoint()
    }

    pub(crate) fn last_ingress_stats(&self) -> Option<WebSocketIngressStats> {
        self.inner.last_ingress_stats()
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn diagnostics(&self) -> WebSocketDiagnostics {
        self.inner.diagnostics()
    }

    pub(crate) const fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    pub(crate) fn write_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
    ) -> Result<OutboundWriteMetrics, TransportWriteFailure> {
        self.inner.write_message(method, message)
    }

    pub(crate) fn write_streamed_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
        source_failure: &StreamedInputSourceFailureSlot,
    ) -> Result<OutboundWriteMetrics, TransportWriteFailure> {
        self.inner
            .write_streamed_message(method, message, source_failure)
    }

    pub(crate) fn write_injection_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
        source_failure: &ThreadInjectionSourceFailureSlot,
    ) -> Result<OutboundWriteMetrics, TransportWriteFailure> {
        self.inner
            .write_injection_message(method, message, source_failure)
    }

    pub(crate) fn recv_json_value_timeout<'a>(
        &mut self,
        method: &str,
        timeout: Duration,
        verifier: Option<crate::turn::StreamedUserMessageVerifierHandle<'a>>,
        ordered_sink: Option<&'a mut dyn crate::OrderedTurnStreamSink>,
        response_authority_generation: u64,
        response_expectation: &mut crate::incoming_json::ResponseExpectationSlot,
    ) -> Result<Option<crate::incoming_json::DecodedIncoming>, ManagedBackendError> {
        self.inner.recv_json_value_timeout(
            method,
            timeout,
            verifier,
            ordered_sink,
            response_authority_generation,
            response_expectation,
        )
    }

    pub(crate) const fn config(&self) -> crate::ForegroundSessionConfig {
        self.config
    }

    pub(crate) fn close(&mut self) {
        self.inner.close();
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn fail_next_write_before_dispatch_for_lifecycle_test(&mut self) {
        self.inner.fail_next_write_before_dispatch = true;
    }
}

impl RequestOnlyWebSocketTransport {
    pub(crate) fn endpoint(&self) -> &str {
        self.inner.endpoint()
    }

    pub(crate) const fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    pub(crate) fn write_message<T: Serialize + ?Sized>(
        &mut self,
        method: &str,
        message: &T,
    ) -> Result<OutboundWriteMetrics, TransportWriteFailure> {
        self.inner.write_message(method, message)
    }

    pub(crate) fn close(&mut self) {
        self.inner.close();
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn fail_next_write_before_dispatch_for_lifecycle_test(&mut self) {
        self.inner.fail_next_write_before_dispatch = true;
    }
}

enum HeaderRead {
    Idle,
    Header(Header),
}

impl WebSocketClientTransport {
    fn read_header(&mut self) -> Result<HeaderRead, ManagedWebSocketError> {
        let mut bytes = Vec::with_capacity(14);
        loop {
            match self
                .read_codec
                .decode_header(&bytes)
                .map_err(ManagedWebSocketError::from_frame)?
            {
                Parsing::Done { value, .. } => return Ok(HeaderRead::Header(value)),
                Parsing::NeedMore(count) => {
                    for _ in 0..count {
                        match self.read_header_byte()? {
                            Some(byte) => bytes.push(byte),
                            None if bytes.is_empty() => return Ok(HeaderRead::Idle),
                            None => {
                                return Err(ManagedWebSocketError::protocol(
                                    "timed out while reading incomplete WebSocket frame header",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    fn read_header_byte(&mut self) -> Result<Option<u8>, ManagedWebSocketError> {
        if let Some(byte) = self.pending_read.pop_front() {
            return Ok(Some(byte));
        }

        let mut byte = [0_u8; 1];
        match self.stream.read(&mut byte) {
            Ok(0) => Err(ManagedWebSocketError::protocol(
                "unexpected EOF while reading WebSocket frame header",
            )),
            Ok(_) => Ok(Some(byte[0])),
            Err(error) if is_timeout_io_error(&error) => Ok(None),
            Err(error) => Err(ManagedWebSocketError::from_io(error)),
        }
    }

    fn read_payload_chunk(
        &mut self,
        remaining: usize,
        output: &mut [u8],
    ) -> Result<usize, ManagedWebSocketError> {
        if remaining == 0 || output.is_empty() {
            return Ok(0);
        }

        let target = remaining.min(output.len());
        let mut written = 0;
        while written < target {
            let Some(byte) = self.pending_read.pop_front() else {
                break;
            };
            output[written] = byte;
            written += 1;
        }
        if written == target {
            return Ok(written);
        }

        match self.stream.read(&mut output[written..target]) {
            Ok(0) => Err(ManagedWebSocketError::protocol(
                "unexpected EOF while reading WebSocket frame payload",
            )),
            Ok(count) => Ok(written + count),
            Err(error) if is_timeout_io_error(&error) => {
                if written > 0 {
                    Ok(written)
                } else {
                    Err(ManagedWebSocketError::protocol(
                        "timed out while reading WebSocket frame payload",
                    ))
                }
            }
            Err(error) => Err(ManagedWebSocketError::from_io(error)),
        }
    }

    fn read_control_payload(&mut self, header: &Header) -> Result<Vec<u8>, ManagedWebSocketError> {
        let mut payload = vec![0_u8; header.payload_len()];
        let mut offset = 0;
        while offset < payload.len() {
            let count = self.read_payload_chunk(payload.len() - offset, &mut payload[offset..])?;
            if count == 0 {
                return Err(ManagedWebSocketError::protocol(
                    "timed out while reading WebSocket control payload",
                ));
            }
            offset += count;
        }
        Ok(payload)
    }

    fn write_close_frame(&mut self, method: &str) -> Result<(), ManagedBackendError> {
        self.write_frame_payload(OpCode::Close, &1000_u16.to_be_bytes())
            .map_err(|source| self.transport_error(method, source))
    }

    fn write_frame_payload(
        &mut self,
        opcode: OpCode,
        payload: &[u8],
    ) -> Result<(), ManagedWebSocketError> {
        write_control_frame(&mut self.stream, &mut self.write_codec, opcode, payload)
    }

    fn poison(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
        self.closed = true;
    }

    fn set_read_timeout(
        &mut self,
        timeout: Option<Duration>,
        method: &str,
    ) -> Result<(), ManagedBackendError> {
        self.stream
            .set_read_timeout(timeout)
            .map_err(ManagedWebSocketError::from_io)
            .map_err(|source| self.transport_error(method, source))
    }

    fn transport_error(&self, method: &str, source: ManagedWebSocketError) -> ManagedBackendError {
        ManagedBackendError::WebSocketTransport {
            method: method.to_string(),
            endpoint: self.endpoint.clone(),
            source,
        }
    }
}

fn is_timeout_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

struct HandshakenStream {
    stream: TcpStream,
    pending_read: VecDeque<u8>,
}

fn connect_handshake(
    endpoint: &BackendWebSocketEndpoint,
    authorization_header_value: String,
) -> Result<HandshakenStream, ManagedWebSocketError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(ManagedWebSocketError::from_io)?;
    runtime.block_on(connect_handshake_async(
        endpoint.port(),
        authorization_header_value,
    ))
}

async fn connect_handshake_async(
    port: u16,
    authorization_header_value: String,
) -> Result<HandshakenStream, ManagedWebSocketError> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = tokio::time::timeout(
        WEBSOCKET_HANDSHAKE_TIMEOUT,
        tokio::net::TcpStream::connect(address),
    )
    .await
    .map_err(|_| ManagedWebSocketError::protocol("timed out connecting WebSocket endpoint"))?
    .map_err(ManagedWebSocketError::from_io)?;
    let host = format!("127.0.0.1:{port}");
    let headers = [HandshakeHeader {
        name: "Authorization",
        value: authorization_header_value.as_bytes(),
    }];
    let mut client = Client::new(stream.compat(), &host, "/");
    client.set_headers(&headers);
    tokio::time::timeout(WEBSOCKET_HANDSHAKE_TIMEOUT, client.handshake())
        .await
        .map_err(|_| ManagedWebSocketError::protocol("timed out during WebSocket handshake"))?
        .map_err(ManagedWebSocketError::from_handshake)?;

    let buffered = client.take_buffer();
    if buffered.len() > WEBSOCKET_HANDSHAKE_READ_AHEAD_BUDGET {
        return Err(ManagedWebSocketError::protocol(format!(
            "WebSocket handshake read-ahead exceeded {} byte budget",
            WEBSOCKET_HANDSHAKE_READ_AHEAD_BUDGET
        )));
    }
    let pending_read = VecDeque::from(buffered.to_vec());
    let compat_stream: Compat<tokio::net::TcpStream> = client.into_inner();
    let stream = compat_stream
        .into_inner()
        .into_std()
        .map_err(ManagedWebSocketError::from_io)?;
    stream
        .set_nonblocking(false)
        .map_err(ManagedWebSocketError::from_io)?;

    Ok(HandshakenStream {
        stream,
        pending_read,
    })
}
