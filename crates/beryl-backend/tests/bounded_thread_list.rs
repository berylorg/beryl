use std::{
    io::{ErrorKind, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendLaunchSpec, BackendWebSocketEndpoint, ManagedBackendError, ManagedBackendSession,
    ThreadListBudget, ThreadListBudgetError, ThreadListCollection, ThreadListCollectionError,
    ThreadListCollectionStatus, ThreadListOptions, ThreadListTruncationReason,
};
use beryl_model::workspace::RuntimeMode;
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

#[cfg(feature = "lifecycle-test-support")]
use beryl_backend::ThreadForkFailure;
#[cfg(feature = "lifecycle-test-support")]
use beryl_backend::lifecycle_test_support::{
    fail_next_websocket_write_after_header, next_request_id,
    pause_before_next_transport_close_classification,
    pause_websocket_after_next_control_write_header, pause_websocket_after_next_write_header,
    pause_websocket_before_next_write, pause_websocket_before_write_after,
    websocket_close_frame_attempts,
};

const OUTER_WATCHDOG: Duration = Duration::from_secs(5);

fn thread_summary(id: &str) -> Value {
    json!({
        "id": id,
        "cwd": test_runtime_path_text(),
        "preview": id,
        "createdAt": 1,
        "updatedAt": 2,
        "modelProvider": "openai",
        "ephemeral": false
    })
}

fn test_runtime_path_text() -> String {
    std::env::current_dir()
        .expect("resolve bounded-list test working directory")
        .to_string_lossy()
        .into_owned()
}

fn connect_test_client<F>(handler: F) -> (ManagedBackendSession, thread::JoinHandle<()>)
where
    F: FnOnce(WebSocket<TcpStream>) + Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = accept_hdr(
            stream,
            |request: &tungstenite::handshake::server::Request, response| {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .unwrap()
                        .to_str()
                        .unwrap(),
                    "Bearer test-token"
                );
                Ok(response)
            },
        )
        .unwrap();
        expect_initialize(&mut socket);
        handler(socket);
    });
    let launch = BackendLaunchSpec::managed_websocket(
        RuntimeMode::HostWindows,
        std::env::current_dir().expect("resolve bounded-list test working directory"),
        endpoint.clone(),
        std::env::temp_dir().join("beryl-bounded-list-unused-token.txt"),
    );
    let client = ManagedBackendSession::connect_websocket(
        launch,
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();
    (client, server)
}

fn expect_initialize(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket);
    assert_eq!(request["method"], json!("initialize"));
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": {
                    "userAgent": "codex-cli test",
                    "codexHome": std::env::temp_dir().join("beryl-bounded-list-codex-home"),
                    "platformFamily": "windows",
                    "platformOs": "windows"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let initialized = read_json(socket);
    assert_eq!(initialized["method"], json!("initialized"));
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Value {
    serde_json::from_slice(&socket.read().unwrap().into_data()).unwrap()
}

fn list_threads_with_watchdog(
    mut client: ManagedBackendSession,
    options: ThreadListOptions,
    budget: ThreadListBudget,
) -> Result<ThreadListCollection, ThreadListCollectionError> {
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = thread::spawn(move || {
        let result = client.list_threads_bounded(options, budget);
        result_tx
            .send(result)
            .expect("test must remain available for the bounded listing result");
    });
    let result = result_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("bounded thread listing exceeded the outer test watchdog");
    client_thread.join().unwrap();
    result
}

fn write_server_frame(
    socket: &mut WebSocket<TcpStream>,
    final_fragment: bool,
    opcode: u8,
    payload: &[u8],
) {
    try_write_server_frame(socket, final_fragment, opcode, payload).unwrap();
}

fn try_write_server_frame(
    socket: &mut WebSocket<TcpStream>,
    final_fragment: bool,
    opcode: u8,
    payload: &[u8],
) -> std::io::Result<()> {
    assert!(
        payload.len() <= 125,
        "the deterministic fixture only emits short WebSocket frames"
    );
    let first_byte = (u8::from(final_fragment) << 7) | opcode;
    socket
        .get_mut()
        .write_all(&[first_byte, payload.len() as u8])?;
    socket.get_mut().write_all(payload)?;
    socket.get_mut().flush()
}

fn await_client_pong(socket: &mut WebSocket<TcpStream>, expected: &[u8]) {
    socket
        .get_mut()
        .set_read_timeout(Some(OUTER_WATCHDOG))
        .unwrap();
    loop {
        match socket.read() {
            Ok(Message::Pong(payload)) => {
                assert_eq!(payload.as_ref(), expected);
                return;
            }
            Ok(Message::Ping(_)) => {}
            Ok(other) => panic!("unexpected client frame before Pong: {other:?}"),
            Err(error) => panic!("client did not emit the expected Pong: {error:?}"),
        }
    }
}

fn await_client_close(socket: &mut WebSocket<TcpStream>) -> usize {
    socket
        .get_mut()
        .set_read_timeout(Some(OUTER_WATCHDOG))
        .unwrap();
    let mut pong_count = 0;
    loop {
        match socket.read() {
            Ok(Message::Pong(_)) => pong_count += 1,
            Ok(Message::Ping(_)) => {}
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return pong_count;
            }
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset
                ) =>
            {
                return pong_count;
            }
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) =>
            {
                panic!("client did not close before the outer test watchdog");
            }
            other => panic!("unexpected frame while waiting for client shutdown: {other:?}"),
        }
    }
}

#[cfg(feature = "lifecycle-test-support")]
fn await_client_raw_abort_without_close(socket: &mut WebSocket<TcpStream>) {
    socket
        .get_mut()
        .set_read_timeout(Some(OUTER_WATCHDOG))
        .unwrap();
    loop {
        match socket.read() {
            Ok(Message::Close(frame)) => {
                panic!("poisoned WebSocket emitted a close frame: {frame:?}")
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) =>
            {
                panic!("poisoned WebSocket did not raw-abort before the outer watchdog")
            }
            Err(_) => return,
        }
    }
}

#[test]
fn thread_list_budget_rejects_unbounded_zero_dimensions() {
    assert_eq!(
        ThreadListBudget::new(Duration::ZERO, 1, 1),
        Err(ThreadListBudgetError::ZeroAggregateTimeout)
    );
    assert_eq!(
        ThreadListBudget::new(Duration::from_secs(1), 0, 1),
        Err(ThreadListBudgetError::ZeroMaxPages)
    );
    assert_eq!(
        ThreadListBudget::new(Duration::from_secs(1), 1, 0),
        Err(ThreadListBudgetError::ZeroMaxResults)
    );
}

#[test]
fn bounded_listing_caps_each_request_to_remaining_result_capacity() {
    let (mut client, server) = connect_test_client(|mut socket| {
        let first = read_json(&mut socket);
        assert_eq!(first["method"], json!("thread/list"));
        assert_eq!(first["params"]["limit"], json!(3));
        assert_eq!(first["params"]["cwd"], json!([test_runtime_path_text()]));
        assert_eq!(first["params"]["sortKey"], json!("updated_at"));
        assert_eq!(first["params"]["sortDirection"], json!("desc"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": first["id"],
                    "result": {
                        "data": [thread_summary("thread_1"), thread_summary("thread_2")],
                        "nextCursor": "cursor_2"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let second = read_json(&mut socket);
        assert_eq!(second["params"]["cursor"], json!("cursor_2"));
        assert_eq!(second["params"]["limit"], json!(1));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": second["id"],
                    "result": {
                        "data": [thread_summary("thread_3")],
                        "nextCursor": "cursor_3"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let options = ThreadListOptions::page(100)
        .with_cwd(test_runtime_path_text())
        .updated_descending();
    let budget = ThreadListBudget::new(Duration::from_secs(2), 5, 3).unwrap();

    let collection = client.list_threads_bounded(options, budget).unwrap();

    assert_eq!(collection.data.len(), 3);
    assert_eq!(collection.pages_collected, 2);
    assert_eq!(collection.next_cursor.as_deref(), Some("cursor_3"));
    assert_eq!(
        collection.status,
        ThreadListCollectionStatus::Truncated(ThreadListTruncationReason::ResultLimit)
    );
    server.join().unwrap();
}

#[test]
fn bounded_listing_distinguishes_page_truncation_from_completion() {
    let (mut truncated_client, truncated_server) = connect_test_client(|mut socket| {
        let request = read_json(&mut socket);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": {
                        "data": [thread_summary("thread_1")],
                        "nextCursor": "cursor_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let budget = ThreadListBudget::new(Duration::from_secs(2), 1, 10).unwrap();
    let truncated = truncated_client
        .list_threads_bounded(ThreadListOptions::page(5), budget)
        .unwrap();

    assert_eq!(truncated.pages_collected, 1);
    assert_eq!(truncated.next_cursor.as_deref(), Some("cursor_1"));
    assert_eq!(
        truncated.status,
        ThreadListCollectionStatus::Truncated(ThreadListTruncationReason::PageLimit)
    );
    truncated_server.join().unwrap();

    let (mut complete_client, complete_server) = connect_test_client(|mut socket| {
        let request = read_json(&mut socket);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": { "data": [thread_summary("thread_1")] }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let complete = complete_client
        .list_threads_bounded(
            ThreadListOptions::page(5),
            ThreadListBudget::new(Duration::from_secs(2), 1, 1).unwrap(),
        )
        .unwrap();

    assert_eq!(complete.pages_collected, 1);
    assert_eq!(complete.next_cursor, None);
    assert_eq!(complete.status, ThreadListCollectionStatus::Complete);
    complete_server.join().unwrap();
}

#[test]
fn bounded_listing_rejects_repeated_cursor_and_retains_prior_pages() {
    let (mut client, server) = connect_test_client(|mut socket| {
        let first = read_json(&mut socket);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": first["id"],
                    "result": {
                        "data": [thread_summary("thread_1")],
                        "nextCursor": "cursor_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let second = read_json(&mut socket);
        assert_eq!(second["params"]["cursor"], json!("cursor_1"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": second["id"],
                    "result": {
                        "data": [thread_summary("untrusted_thread")],
                        "nextCursor": "cursor_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let error = client
        .list_threads_bounded(
            ThreadListOptions::page(2),
            ThreadListBudget::new(Duration::from_secs(2), 5, 10).unwrap(),
        )
        .unwrap_err();

    assert_eq!(error.data.len(), 1);
    assert_eq!(error.data[0].id, "thread_1");
    assert_eq!(error.pages_collected, 1);
    assert_eq!(error.next_cursor.as_deref(), Some("cursor_1"));
    assert!(matches!(
        error.source,
        ManagedBackendError::ThreadListCursorRepeated { ref cursor }
            if cursor == "cursor_1"
    ));
    server.join().unwrap();
}

#[test]
fn bounded_listing_keeps_request_failure_as_error_with_trusted_partial_data() {
    let (mut client, server) = connect_test_client(|mut socket| {
        let first = read_json(&mut socket);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": first["id"],
                    "result": {
                        "data": [thread_summary("thread_1")],
                        "nextCursor": "cursor_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let second = read_json(&mut socket);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": second["id"],
                    "error": { "code": -32000, "message": "fixture failure" }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let error = client
        .list_threads_bounded(
            ThreadListOptions::page(1),
            ThreadListBudget::new(Duration::from_secs(2), 5, 10).unwrap(),
        )
        .unwrap_err();

    assert_eq!(error.data.len(), 1);
    assert_eq!(error.pages_collected, 1);
    assert_eq!(error.next_cursor.as_deref(), Some("cursor_1"));
    assert!(matches!(
        error.source,
        ManagedBackendError::RequestFailed { ref method, .. } if method == "thread/list"
    ));
    server.join().unwrap();
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn bounded_listing_preserves_close_observed_before_the_aggregate_deadline() {
    const AGGREGATE_TIMEOUT: Duration = Duration::from_secs(1);
    const CLOSE_BEFORE_DEADLINE: Duration = Duration::from_millis(250);
    let (close_observed_tx, close_observed_rx) = mpsc::sync_channel(1);

    let (mut client, server) = connect_test_client(move |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/list"));
        thread::sleep(CLOSE_BEFORE_DEADLINE);
        socket.send(Message::Close(None)).unwrap();
        assert_eq!(await_client_close(&mut socket), 0);
        close_observed_tx
            .send(())
            .expect("client-close observation latch must remain available");
    });

    let (classification_entered, release_classification) =
        pause_before_next_transport_close_classification(&mut client)
            .expect("install deterministic transport-close classification gate");
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    let started = std::time::Instant::now();
    let client_thread = thread::spawn(move || {
        let result = client.list_threads_bounded(
            ThreadListOptions::page(1),
            ThreadListBudget::new(AGGREGATE_TIMEOUT, 5, 10).unwrap(),
        );
        result_tx
            .send(result)
            .expect("close-classification result receiver must remain available");
    });
    classification_entered
        .recv_timeout(OUTER_WATCHDOG)
        .expect("transport close must reach the request-layer classification gate");
    close_observed_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("server must observe the client's close acknowledgement");
    if let Some(remaining) = AGGREGATE_TIMEOUT.checked_sub(started.elapsed()) {
        thread::sleep(remaining);
    }
    release_classification
        .send(())
        .expect("release transport-close classification after the aggregate deadline");
    let error = result_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("close classification exceeded the outer watchdog")
        .unwrap_err();
    client_thread.join().unwrap();

    assert!(error.data.is_empty());
    assert_eq!(error.pages_collected, 0);
    assert_eq!(error.next_cursor, None);
    assert!(matches!(
        error.source,
        ManagedBackendError::TransportClosed { ref method } if method == "thread/list"
    ));
    server.join().unwrap();
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn bounded_listing_partial_websocket_write_is_terminal() {
    const AGGREGATE_TIMEOUT: Duration = Duration::from_millis(500);
    let (mut client, server) = connect_test_client(move |mut socket| {
        await_client_raw_abort_without_close(&mut socket);
    });
    assert_eq!(client.process_id(), None);
    let (header_entered, release_write) = pause_websocket_after_next_write_header(&mut client)
        .expect("install deterministic partial WebSocket write gate");
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = thread::spawn(move || {
        let first = client.list_threads_bounded(
            ThreadListOptions::page(1),
            ThreadListBudget::new(AGGREGATE_TIMEOUT, 1, 1).unwrap(),
        );
        let second = client.list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(1));
        client
            .shutdown()
            .expect("poisoned WebSocket shutdown should remain an idempotent raw abort");
        let request_id = next_request_id(&client);
        let close_attempts = websocket_close_frame_attempts(&client).unwrap();
        result_tx
            .send((first, second, request_id, close_attempts))
            .expect("blocked-write result receiver must remain available");
    });

    header_entered
        .recv_timeout(OUTER_WATCHDOG)
        .expect("client must commit the WebSocket frame header before the gate");
    thread::sleep(AGGREGATE_TIMEOUT);
    release_write
        .send(())
        .expect("release partial WebSocket write after its deadline");
    let (first, second, request_id, close_attempts) = result_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("partial WebSocket write exceeded the outer watchdog");
    client_thread.join().unwrap();
    server.join().unwrap();

    let first = first.expect_err("partial WebSocket write must expire");
    assert!(matches!(
        first.source,
        ManagedBackendError::RequestTimeout { ref method, timeout }
            if method == "thread/list" && timeout == AGGREGATE_TIMEOUT
    ));
    assert!(matches!(
        second,
        Err(ManagedBackendError::SessionPoisoned { ref method }) if method == "thread/list"
    ));
    assert_eq!(request_id, 3);
    assert_eq!(close_attempts, 0);
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn pre_first_byte_websocket_expiry_is_reusable_without_burning_request_id() {
    const REQUEST_TIMEOUT: Duration = Duration::from_millis(250);
    let (mut client, server) = connect_test_client(move |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/list"));
        assert_eq!(request["id"], json!(2));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": { "data": [], "nextCursor": null }
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(await_client_close(&mut socket), 0);
    });
    assert_eq!(client.process_id(), None);
    let (write_entered, release_write) = pause_websocket_before_next_write(&mut client)
        .expect("install deterministic pre-first-byte write gate");
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = thread::spawn(move || {
        let first = client.list_threads_bounded(
            ThreadListOptions::page(1),
            ThreadListBudget::new(REQUEST_TIMEOUT, 1, 1).unwrap(),
        );
        let second = client.list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(1));
        client.shutdown().unwrap();
        result_tx
            .send((
                first,
                second,
                next_request_id(&client),
                websocket_close_frame_attempts(&client).unwrap(),
            ))
            .unwrap();
    });

    write_entered
        .recv_timeout(OUTER_WATCHDOG)
        .expect("request must reach the pre-first-byte gate");
    thread::sleep(REQUEST_TIMEOUT);
    release_write.send(()).unwrap();
    let (first, second, request_id, close_attempts) = result_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("pre-first-byte expiry exceeded the outer watchdog");
    client_thread.join().unwrap();
    server.join().unwrap();

    let first = first.expect_err("the gated request must expire before dispatch");
    assert!(matches!(
        first.source,
        ManagedBackendError::RequestTimeout { ref method, timeout }
            if method == "thread/list" && timeout == REQUEST_TIMEOUT
    ));
    assert!(
        second.is_ok(),
        "the session must remain reusable: {second:?}"
    );
    assert_eq!(request_id, 3);
    assert_eq!(close_attempts, 1);
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn pre_dispatch_fork_deadline_is_definitively_not_committed() {
    const REQUEST_TIMEOUT: Duration = Duration::from_millis(200);
    let (mut client, server) = connect_test_client(move |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/list"));
        assert_eq!(request["id"], json!(2));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": { "data": [], "nextCursor": null }
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(await_client_close(&mut socket), 0);
    });
    let (write_entered, release_write) = pause_websocket_before_next_write(&mut client).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = thread::spawn(move || {
        let fork = client.fork_thread_with_commitment("source", REQUEST_TIMEOUT);
        let list = client.list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(1));
        client.shutdown().unwrap();
        result_tx
            .send((fork, list, next_request_id(&client)))
            .unwrap();
    });
    write_entered.recv_timeout(OUTER_WATCHDOG).unwrap();
    thread::sleep(REQUEST_TIMEOUT);
    release_write.send(()).unwrap();
    let (fork, list, request_id) = result_rx.recv_timeout(OUTER_WATCHDOG).unwrap();
    client_thread.join().unwrap();
    server.join().unwrap();

    assert!(matches!(
        fork,
        Err(ThreadForkFailure::NotCommitted {
            source: ManagedBackendError::RequestTimeout { ref method, timeout }
        }) if method == "thread/fork" && timeout == REQUEST_TIMEOUT
    ));
    assert!(list.is_ok());
    assert_eq!(request_id, 3);
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn streaming_parser_pong_partial_write_expiry_poisoning_is_preserved() {
    const REQUEST_TIMEOUT: Duration = Duration::from_millis(250);
    let (mut client, server) = connect_test_client(move |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/list"));
        write_server_frame(
            &mut socket,
            false,
            0x1,
            br#"{"jsonrpc":"2.0","id":2,"result":{"data":["#,
        );
        write_server_frame(&mut socket, true, 0x9, b"x");
        await_client_raw_abort_without_close(&mut socket);
    });
    assert_eq!(client.process_id(), None);
    let (pong_header_entered, release_pong) =
        pause_websocket_after_next_control_write_header(&mut client)
            .expect("install deterministic Pong partial-write gate");
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = thread::spawn(move || {
        let first = client.list_threads_bounded(
            ThreadListOptions::page(1),
            ThreadListBudget::new(REQUEST_TIMEOUT, 1, 1).unwrap(),
        );
        let second = client.list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(1));
        client
            .shutdown()
            .expect("poisoned parser-control path should raw-abort idempotently");
        result_tx
            .send((
                first,
                second,
                next_request_id(&client),
                websocket_close_frame_attempts(&client).unwrap(),
            ))
            .unwrap();
    });

    pong_header_entered
        .recv_timeout(OUTER_WATCHDOG)
        .expect("streaming parser must commit the Pong header");
    thread::sleep(REQUEST_TIMEOUT);
    release_pong.send(()).unwrap();
    let (first, second, request_id, close_attempts) = result_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("Pong partial-write expiry exceeded the outer watchdog");
    client_thread.join().unwrap();
    server.join().unwrap();

    let first = first.expect_err("partial Pong write must expire the request");
    assert!(matches!(
        first.source,
        ManagedBackendError::RequestTimeout { ref method, timeout }
            if method == "thread/list" && timeout == REQUEST_TIMEOUT
    ));
    assert!(matches!(
        second,
        Err(ManagedBackendError::SessionPoisoned { ref method }) if method == "thread/list"
    ));
    assert_eq!(request_id, 3);
    assert_eq!(close_attempts, 0);
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn post_commit_websocket_io_error_is_preserved_and_terminal() {
    let (mut client, server) = connect_test_client(move |mut socket| {
        await_client_raw_abort_without_close(&mut socket);
    });
    assert_eq!(client.process_id(), None);
    fail_next_websocket_write_after_header(&mut client, ErrorKind::BrokenPipe)
        .expect("inject transport-owned post-header I/O failure");

    let first = client
        .list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(1))
        .expect_err("post-commit I/O failure must be returned");
    match first {
        ManagedBackendError::WebSocketTransport { source, .. } => {
            assert_eq!(source.io_error_kind(), Some(ErrorKind::BrokenPipe));
        }
        other => panic!("ordinary post-commit I/O error provenance was lost: {other:?}"),
    }
    let second = client.list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(1));
    assert!(matches!(
        second,
        Err(ManagedBackendError::SessionPoisoned { ref method }) if method == "thread/list"
    ));
    assert_eq!(next_request_id(&client), 3);
    client
        .shutdown()
        .expect("poisoned WebSocket shutdown should remain a raw abort");
    assert_eq!(websocket_close_frame_attempts(&client).unwrap(), 0);
    server.join().unwrap();
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn bounded_listing_uses_one_aggregate_deadline_across_paginated_requests() {
    const AGGREGATE_TIMEOUT: Duration = Duration::from_millis(300);

    let (mut client, server) = connect_test_client(|mut socket| {
        let first = read_json(&mut socket);
        assert_eq!(first["method"], json!("thread/list"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": first["id"],
                    "result": {
                        "data": [thread_summary("thread_1")],
                        "nextCursor": "cursor_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(await_client_close(&mut socket), 0);
    });
    let (second_write_entered, release_second_write) =
        pause_websocket_before_write_after(&mut client, 1)
            .expect("install deterministic second-page pre-write gate");
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = thread::spawn(move || {
        let result = client.list_threads_bounded(
            ThreadListOptions::page(1),
            ThreadListBudget::new(AGGREGATE_TIMEOUT, 5, 10).unwrap(),
        );
        client.shutdown().unwrap();
        result_tx.send((result, next_request_id(&client))).unwrap();
    });
    second_write_entered
        .recv_timeout(OUTER_WATCHDOG)
        .expect("pagination must reach the second-page write gate");
    thread::sleep(AGGREGATE_TIMEOUT);
    release_second_write.send(()).unwrap();
    let (result, request_id) = result_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("aggregate deadline gate exceeded the outer watchdog");
    let error = result.unwrap_err();
    client_thread.join().unwrap();

    assert_eq!(error.data.len(), 1);
    assert_eq!(error.data[0].id, "thread_1");
    assert_eq!(error.pages_collected, 1);
    assert_eq!(error.next_cursor.as_deref(), Some("cursor_1"));
    match &error.source {
        ManagedBackendError::RequestTimeout { method, .. } => {
            assert_eq!(method, "thread/list");
        }
        other => panic!("a later page must not receive a rebased aggregate deadline: {other:?}"),
    }
    assert_eq!(request_id, 3);
    server.join().unwrap();
}

#[test]
fn bounded_listing_reports_explicit_expiry_with_trusted_partial_data() {
    const AGGREGATE_TIMEOUT: Duration = Duration::from_secs(1);

    let (client, server) = connect_test_client(|mut socket| {
        let first = read_json(&mut socket);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": first["id"],
                    "result": {
                        "data": [thread_summary("thread_1")],
                        "nextCursor": "cursor_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let second = read_json(&mut socket);
        assert_eq!(second["method"], json!("thread/list"));
        assert_eq!(second["params"]["cursor"], json!("cursor_1"));
        assert_eq!(await_client_close(&mut socket), 0);
    });

    let error = list_threads_with_watchdog(
        client,
        ThreadListOptions::page(1),
        ThreadListBudget::new(AGGREGATE_TIMEOUT, 5, 10).unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.data.len(), 1);
    assert_eq!(error.data[0].id, "thread_1");
    assert_eq!(error.pages_collected, 1);
    assert_eq!(error.next_cursor.as_deref(), Some("cursor_1"));
    match error.source {
        ManagedBackendError::RequestTimeout { method, .. } => {
            assert_eq!(method, "thread/list");
        }
        other => panic!("an unanswered page must report explicit deadline expiry, got {other:?}"),
    }
    server.join().unwrap();
}

#[test]
fn bounded_listing_fragmented_payload_and_control_frames_do_not_extend_the_deadline() {
    const AGGREGATE_TIMEOUT: Duration = Duration::from_secs(1);
    const TRAFFIC_INTERVAL: Duration = Duration::from_millis(600);
    const FIRST_FRAGMENT_END: usize = 80;
    const SECOND_FRAGMENT_END: usize = 160;
    let (first_pong_tx, first_pong_rx) = mpsc::sync_channel(1);
    let (release_second_tx, release_second_rx) = mpsc::sync_channel(1);
    let (second_pong_tx, second_pong_rx) = mpsc::sync_channel(1);
    let (release_final_tx, release_final_rx) = mpsc::sync_channel(1);

    let (mut client, server) = connect_test_client(move |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("thread/list"));
        let response = json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": { "data": [thread_summary("thread_1")] }
        })
        .to_string();
        let bytes = response.as_bytes();
        assert!(bytes.len() > SECOND_FRAGMENT_END);

        write_server_frame(&mut socket, false, 0x1, &bytes[..FIRST_FRAGMENT_END]);
        write_server_frame(&mut socket, true, 0x9, b"first-ping");
        await_client_pong(&mut socket, b"first-ping");
        first_pong_tx.send(()).unwrap();
        release_second_rx
            .recv_timeout(OUTER_WATCHDOG)
            .expect("second-fragment release gate exceeded the outer watchdog");

        write_server_frame(
            &mut socket,
            false,
            0x0,
            &bytes[FIRST_FRAGMENT_END..SECOND_FRAGMENT_END],
        );
        write_server_frame(&mut socket, true, 0x9, b"second-ping");
        await_client_pong(&mut socket, b"second-ping");
        second_pong_tx.send(()).unwrap();
        release_final_rx
            .recv_timeout(OUTER_WATCHDOG)
            .expect("final-fragment release gate exceeded the outer watchdog");

        // This completes a valid response after the original deadline, but
        // before a regression that refreshed the full timeout on the second
        // fragment or Ping would expire.
        let _ = try_write_server_frame(&mut socket, true, 0x0, &bytes[SECOND_FRAGMENT_END..]);
    });
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client_thread = thread::spawn(move || {
        result_tx
            .send(client.list_threads_bounded(
                ThreadListOptions::page(1),
                ThreadListBudget::new(AGGREGATE_TIMEOUT, 5, 10).unwrap(),
            ))
            .unwrap();
    });

    first_pong_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("first Pong gate exceeded the outer watchdog");
    thread::sleep(TRAFFIC_INTERVAL);
    release_second_tx.send(()).unwrap();
    second_pong_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("second Pong gate exceeded the outer watchdog");
    thread::sleep(TRAFFIC_INTERVAL);
    release_final_tx.send(()).unwrap();
    let error = result_rx
        .recv_timeout(OUTER_WATCHDOG)
        .expect("fragmented deadline test exceeded its outer leak watchdog")
        .unwrap_err();
    client_thread.join().unwrap();

    assert!(error.data.is_empty());
    assert_eq!(error.pages_collected, 0);
    assert_eq!(error.next_cursor, None);
    assert!(matches!(
        error.source,
        ManagedBackendError::RequestTimeout { ref method, .. } if method == "thread/list"
    ));
    server.join().unwrap();
}

#[test]
fn bounded_listing_rejects_a_page_larger_than_the_requested_limit() {
    let (mut client, server) = connect_test_client(|mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["params"]["limit"], json!(1));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": {
                        "data": [thread_summary("thread_1"), thread_summary("thread_2")]
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let error = client
        .list_threads_bounded(
            ThreadListOptions::page(100),
            ThreadListBudget::new(Duration::from_secs(2), 1, 1).unwrap(),
        )
        .unwrap_err();

    assert!(error.data.is_empty());
    assert_eq!(error.pages_collected, 0);
    assert!(matches!(
        error.source,
        ManagedBackendError::ThreadListPageLimitExceeded {
            requested_limit: 1,
            returned: 2
        }
    ));
    server.join().unwrap();
}
