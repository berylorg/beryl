use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream},
    time::{Duration, Instant},
};

use serde_json::Value;
use soketto::{
    Parsing,
    base::{Codec, Header, OpCode},
    handshake::client::{Client, Header as HandshakeHeader},
};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};
use tracing::debug;

use crate::{
    BackendWebSocketEndpoint, incoming_json,
    session::{ManagedBackendError, ManagedWebSocketError, TransportWriteFailure},
};

mod message;
mod reader;

use message::{MessagePayload, PayloadRead};
use reader::{PayloadReaderState, WebSocketPayloadReader};

const READ_CHUNK_BYTES: usize = 8 * 1024;
const WEBSOCKET_FRAME_PAYLOAD_BUDGET: usize = 64 * 1024 * 1024;
const WEBSOCKET_TEXT_MESSAGE_BUDGET: usize = 64 * 1024 * 1024;
const WEBSOCKET_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const WEBSOCKET_HANDSHAKE_READ_AHEAD_BUDGET: usize = 4 * 1024;

pub(crate) struct WebSocketClientTransport {
    endpoint: String,
    stream: TcpStream,
    read_codec: Codec,
    write_codec: Codec,
    pending_read: VecDeque<u8>,
    last_ingress_stats: Option<WebSocketIngressStats>,
    closed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WebSocketIngressStats {
    pub(crate) message_bytes: usize,
    pub(crate) maximum_transport_chunk_bytes: usize,
    pub(crate) maximum_parser_buffer_bytes: usize,
    pub(crate) discarded_image_result_bytes: usize,
    pub(crate) retained_item_result_present: bool,
}

impl WebSocketClientTransport {
    pub(crate) fn connect(
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
        read_codec.set_max_data_size(WEBSOCKET_FRAME_PAYLOAD_BUDGET);

        Ok(Self {
            endpoint: endpoint_label,
            stream: stream.stream,
            read_codec,
            write_codec: Codec::new(),
            pending_read: stream.pending_read,
            last_ingress_stats: None,
            closed: false,
        })
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub(crate) fn last_ingress_stats(&self) -> Option<WebSocketIngressStats> {
        self.last_ingress_stats
    }

    pub(crate) fn write_message(
        &mut self,
        method: &str,
        line: &str,
    ) -> Result<(), TransportWriteFailure> {
        if self.closed {
            return Err(TransportWriteFailure::ProvenNotDispatched(
                ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                },
            ));
        }
        self.write_frame_payload(OpCode::Text, line.as_bytes())
            .map_err(|failure| match failure {
                FrameWriteFailure::ProvenNotDispatched(source) => {
                    TransportWriteFailure::ProvenNotDispatched(self.transport_error(method, source))
                }
                FrameWriteFailure::MayHaveDispatched(source) => {
                    TransportWriteFailure::MayHaveDispatched(self.transport_error(method, source))
                }
            })
    }

    pub(crate) fn recv_json_value_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Result<Option<Value>, ManagedBackendError> {
        self.set_read_timeout(Some(timeout), method)?;
        let receive_started = Instant::now();
        let state = PayloadReaderState::default();
        let reader =
            WebSocketPayloadReader::new(self, method, WEBSOCKET_TEXT_MESSAGE_BUDGET, state.clone());
        let decoded = incoming_json::decode_reader(reader, READ_CHUNK_BYTES);

        if let Some(error) = state.take_failure() {
            self.close();
            return Err(error);
        }
        if !state.started() {
            return Ok(None);
        }
        let decoded = match decoded {
            Ok(decoded) => decoded,
            Err(source) => {
                self.close();
                return Err(ManagedBackendError::InvalidJsonLine {
                    line: incoming_json::redacted_invalid_json(),
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
            retained_item_result_present: decoded
                .value
                .pointer("/params/item")
                .and_then(Value::as_object)
                .is_some_and(|item| item.contains_key("result")),
        };
        self.last_ingress_stats = Some(ingress_stats);

        debug!(
            method,
            response_bytes = ingress_stats.message_bytes,
            maximum_transport_chunk_bytes = ingress_stats.maximum_transport_chunk_bytes,
            maximum_parser_buffer_bytes = ingress_stats.maximum_parser_buffer_bytes,
            discarded_image_result_bytes = ingress_stats.discarded_image_result_bytes,
            retained_item_result_present = ingress_stats.retained_item_result_present,
            wait_first_frame_ms = state.first_frame_after().map(elapsed_ms),
            wait_first_payload_ms = state.first_payload_after().map(elapsed_ms),
            full_message_ms = elapsed_ms(receive_started.elapsed()),
            "received and parsed backend WebSocket JSON message"
        );
        Ok(Some(decoded.value))
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

enum HeaderRead {
    Idle,
    Header(Header),
}

enum FrameWriteFailure {
    ProvenNotDispatched(ManagedWebSocketError),
    MayHaveDispatched(ManagedWebSocketError),
}

impl FrameWriteFailure {
    fn into_error(self) -> ManagedWebSocketError {
        match self {
            Self::ProvenNotDispatched(error) | Self::MayHaveDispatched(error) => error,
        }
    }
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
            .map_err(FrameWriteFailure::into_error)
            .map_err(|source| self.transport_error(method, source))
    }

    fn write_frame_payload(
        &mut self,
        opcode: OpCode,
        payload: &[u8],
    ) -> Result<(), FrameWriteFailure> {
        let mut header = Header::new(opcode);
        let mut mask = [0_u8; 4];
        getrandom::fill(&mut mask)
            .map_err(ManagedWebSocketError::from_mask_generation)
            .map_err(FrameWriteFailure::ProvenNotDispatched)?;
        header
            .set_masked(true)
            .set_mask(u32::from_be_bytes(mask))
            .set_payload_len(payload.len());

        let header_bytes = self.write_codec.encode_header(&header);
        let mut masked_payload = payload.to_vec();
        Codec::apply_mask(&header, &mut masked_payload);
        self.stream
            .write_all(header_bytes)
            .and_then(|()| self.stream.write_all(&masked_payload))
            .and_then(|()| self.stream.flush())
            .map_err(ManagedWebSocketError::from_io)
            .map_err(FrameWriteFailure::MayHaveDispatched)
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
