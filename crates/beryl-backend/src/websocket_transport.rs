use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream},
    time::{Duration, Instant},
};

use soketto::{
    Parsing,
    base::{Codec, Header, OpCode},
    handshake::client::{Client, Header as HandshakeHeader},
};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};
use tracing::debug;

use crate::{
    BackendWebSocketEndpoint,
    session::{ManagedBackendError, ManagedWebSocketError},
};

mod message;

use message::{MessagePayload, MessagePrime, PayloadRead, WebSocketTextMessageReader};

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
        })
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub(crate) fn write_message(
        &mut self,
        method: &str,
        line: &str,
    ) -> Result<(), ManagedBackendError> {
        self.write_frame_payload(OpCode::Text, line.as_bytes())
            .map_err(|source| self.transport_error(method, source))
    }

    pub(crate) fn recv_text_message_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Result<Option<String>, ManagedBackendError> {
        self.set_read_timeout(Some(timeout), method)?;
        let receive_started = Instant::now();
        let mut payload = MessagePayload::new(method, WEBSOCKET_TEXT_MESSAGE_BUDGET);
        let mut chunk = [0_u8; READ_CHUNK_BYTES];
        let mut bytes = Vec::new();
        let mut saw_message_byte = false;
        let mut first_frame_after = None;
        let mut first_payload_after = None;

        loop {
            let was_started = payload.started;
            match self.read_message_payload_chunk(method, &mut payload, &mut chunk) {
                Ok(PayloadRead::Idle) if !saw_message_byte && !payload.started => {
                    return Ok(None);
                }
                Ok(PayloadRead::Idle) => {
                    return Err(self.transport_error(
                        method,
                        ManagedWebSocketError::protocol(
                            "timed out while reading WebSocket message",
                        ),
                    ));
                }
                Ok(PayloadRead::Bytes(count)) => {
                    if !was_started && payload.started && first_frame_after.is_none() {
                        first_frame_after = Some(receive_started.elapsed());
                    }
                    if first_payload_after.is_none() {
                        first_payload_after = Some(receive_started.elapsed());
                    }
                    saw_message_byte = true;
                    bytes.extend_from_slice(&chunk[..count]);
                }
                Ok(PayloadRead::Complete) => {
                    debug!(
                        method,
                        response_bytes = bytes.len(),
                        wait_first_frame_ms = first_frame_after.map(elapsed_ms),
                        wait_first_payload_ms = first_payload_after.map(elapsed_ms),
                        full_message_ms = elapsed_ms(receive_started.elapsed()),
                        "received backend WebSocket text message"
                    );
                    return String::from_utf8(bytes).map(Some).map_err(|source| {
                        self.transport_error(method, ManagedWebSocketError::from_utf8(source))
                    });
                }
                Ok(PayloadRead::Pong) => {}
                Ok(PayloadRead::Close) => {
                    return Err(ManagedBackendError::TransportClosed {
                        method: method.to_string(),
                    });
                }
                Ok(PayloadRead::Binary) => return Err(ManagedBackendError::UnexpectedMessageShape),
                Err(error) => return Err(self.transport_error(method, error)),
            }
        }
    }

    pub(crate) fn recv_text_message_timeout_with_parser<T>(
        &mut self,
        method: &str,
        timeout: Duration,
        parse: impl FnOnce(&mut dyn Read) -> Result<T, serde_json::Error>,
    ) -> Result<Option<Result<T, serde_json::Error>>, ManagedBackendError> {
        self.set_read_timeout(Some(timeout), method)?;
        let mut reader = WebSocketTextMessageReader::new(self, method);

        let parse_started = Instant::now();
        let prime_started = Instant::now();
        let prime = match reader.prime() {
            Ok(prime) => prime,
            Err(source) => return Err(reader.transport.transport_error(method, source)),
        };
        let prime_elapsed = prime_started.elapsed();
        let reader_wait_after_prime = reader.read_wait();

        match prime {
            MessagePrime::Idle => return Ok(None),
            MessagePrime::Close => {
                return Err(ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                });
            }
            MessagePrime::Binary => return Err(ManagedBackendError::UnexpectedMessageShape),
            MessagePrime::Ready => {}
        }

        let parse_fn_started = Instant::now();
        let parsed = parse(&mut reader);
        let parse_fn_elapsed = parse_fn_started.elapsed();
        let parse_elapsed = parse_started.elapsed();
        let reader_wait_after_parse = reader.read_wait();
        let parse_fn_reader_wait = reader_wait_after_parse.saturating_sub(reader_wait_after_prime);
        let parse_fn_cpu_estimate = parse_fn_elapsed.saturating_sub(parse_fn_reader_wait);
        match parsed {
            Ok(value) => {
                let discard_started = Instant::now();
                let reader_wait_before_discard = reader.read_wait();
                reader
                    .discard_to_end()
                    .map_err(|source| reader.transport.transport_error(method, source))?;
                let discard_elapsed = discard_started.elapsed();
                let discard_reader_wait = reader
                    .read_wait()
                    .saturating_sub(reader_wait_before_discard);
                let stream_total_elapsed = parse_started.elapsed();
                let post_first_payload_reader_wait =
                    reader.first_payload_after().map(|first_payload_after| {
                        reader.read_wait().saturating_sub(first_payload_after)
                    });
                debug!(
                    method,
                    response_bytes = reader.bytes_read(),
                    wait_first_frame_ms = reader.first_frame_after().map(elapsed_ms),
                    wait_first_payload_ms = reader.first_payload_after().map(elapsed_ms),
                    parser_wall_ms = elapsed_ms(parse_elapsed),
                    stream_total_ms = elapsed_ms(stream_total_elapsed),
                    prime_ms = elapsed_ms(prime_elapsed),
                    parse_fn_ms = elapsed_ms(parse_fn_elapsed),
                    parse_fn_reader_wait_ms = elapsed_ms(parse_fn_reader_wait),
                    parse_fn_cpu_estimate_ms = elapsed_ms(parse_fn_cpu_estimate),
                    discard_to_end_ms = elapsed_ms(discard_elapsed),
                    discard_reader_wait_ms = elapsed_ms(discard_reader_wait),
                    reader_wait_ms = elapsed_ms(reader.read_wait()),
                    reader_wait_after_prime_ms = elapsed_ms(reader_wait_after_prime),
                    reader_wait_after_parse_ms = elapsed_ms(reader_wait_after_parse),
                    post_first_payload_reader_wait_ms =
                        post_first_payload_reader_wait.map(elapsed_ms),
                    parser_cpu_estimate_ms =
                        elapsed_ms(parse_elapsed.saturating_sub(reader_wait_after_parse)),
                    reader_fill_buffer_calls = reader.fill_buffer_calls(),
                    reader_payload_chunk_count = reader.payload_chunk_count(),
                    reader_max_payload_chunk_bytes = reader.max_payload_chunk_bytes(),
                    reader_text_frame_count = reader.text_frame_count(),
                    reader_continuation_frame_count = reader.continuation_frame_count(),
                    reader_control_frame_count = reader.control_frame_count(),
                    "stream-parsed backend WebSocket text message"
                );
                Ok(Some(Ok(value)))
            }
            Err(source) if source.io_error_kind() == Some(io::ErrorKind::ConnectionAborted) => {
                Err(ManagedBackendError::TransportClosed {
                    method: method.to_string(),
                })
            }
            Err(source) if source.is_io() => {
                debug!(
                    method,
                    response_bytes = reader.bytes_read(),
                    wait_first_frame_ms = reader.first_frame_after().map(elapsed_ms),
                    wait_first_payload_ms = reader.first_payload_after().map(elapsed_ms),
                    parser_wall_ms = elapsed_ms(parse_elapsed),
                    prime_ms = elapsed_ms(prime_elapsed),
                    parse_fn_ms = elapsed_ms(parse_fn_elapsed),
                    parse_fn_reader_wait_ms = elapsed_ms(parse_fn_reader_wait),
                    parse_fn_cpu_estimate_ms = elapsed_ms(parse_fn_cpu_estimate),
                    reader_wait_ms = elapsed_ms(reader.read_wait()),
                    reader_wait_after_prime_ms = elapsed_ms(reader_wait_after_prime),
                    reader_wait_after_parse_ms = elapsed_ms(reader_wait_after_parse),
                    parser_cpu_estimate_ms =
                        elapsed_ms(parse_elapsed.saturating_sub(reader_wait_after_parse)),
                    reader_fill_buffer_calls = reader.fill_buffer_calls(),
                    reader_payload_chunk_count = reader.payload_chunk_count(),
                    reader_max_payload_chunk_bytes = reader.max_payload_chunk_bytes(),
                    reader_text_frame_count = reader.text_frame_count(),
                    reader_continuation_frame_count = reader.continuation_frame_count(),
                    reader_control_frame_count = reader.control_frame_count(),
                    "stream parser failed while reading backend WebSocket text message"
                );
                Err(reader.transport.transport_error(
                    method,
                    ManagedWebSocketError::protocol(format!(
                        "failed while streaming WebSocket text message: {source}"
                    )),
                ))
            }
            Err(source) => {
                debug!(
                    method,
                    response_bytes = reader.bytes_read(),
                    wait_first_frame_ms = reader.first_frame_after().map(elapsed_ms),
                    wait_first_payload_ms = reader.first_payload_after().map(elapsed_ms),
                    parser_wall_ms = elapsed_ms(parse_elapsed),
                    prime_ms = elapsed_ms(prime_elapsed),
                    parse_fn_ms = elapsed_ms(parse_fn_elapsed),
                    parse_fn_reader_wait_ms = elapsed_ms(parse_fn_reader_wait),
                    parse_fn_cpu_estimate_ms = elapsed_ms(parse_fn_cpu_estimate),
                    reader_wait_ms = elapsed_ms(reader.read_wait()),
                    reader_wait_after_prime_ms = elapsed_ms(reader_wait_after_prime),
                    reader_wait_after_parse_ms = elapsed_ms(reader_wait_after_parse),
                    parser_cpu_estimate_ms =
                        elapsed_ms(parse_elapsed.saturating_sub(reader_wait_after_parse)),
                    reader_fill_buffer_calls = reader.fill_buffer_calls(),
                    reader_payload_chunk_count = reader.payload_chunk_count(),
                    reader_max_payload_chunk_bytes = reader.max_payload_chunk_bytes(),
                    reader_text_frame_count = reader.text_frame_count(),
                    reader_continuation_frame_count = reader.continuation_frame_count(),
                    reader_control_frame_count = reader.control_frame_count(),
                    "stream parser rejected backend WebSocket text message"
                );
                Ok(Some(Err(source)))
            }
        }
    }

    pub(crate) fn close(&mut self) {
        let _ = self.write_close_frame("close");
        let _ = self.stream.shutdown(Shutdown::Both);
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
        let code = 1000_u16.to_be_bytes();
        self.write_frame_payload(OpCode::Close, &code)
            .map_err(|source| self.transport_error(method, source))
    }

    fn write_frame_payload(
        &mut self,
        opcode: OpCode,
        payload: &[u8],
    ) -> Result<(), ManagedWebSocketError> {
        let mut header = Header::new(opcode);
        let mut mask = [0_u8; 4];
        getrandom::fill(&mut mask).map_err(ManagedWebSocketError::from_mask_generation)?;
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
