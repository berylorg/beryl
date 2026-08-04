#[path = "phase10_projection/syndic.rs"]
mod syndic;

use std::{
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};

use beryl_app::cas_projection::ProjectionSessionAdmissionError;
use beryl_backend::{BackendWebSocketEndpoint, ManagedBackendClientConnector, ManagedBackendError};
use beryl_model::CasProcessGeneration;
use tungstenite::{Message, WebSocket, accept_hdr};

use syndic::{Fixture, execution_binding};

const AUTHORIZATION: &str = "Bearer phase28-projection-admission";
const TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[test]
fn projection_admission_propagates_foreground_initialize_timeout() {
    let fixture = Fixture::new(35);
    let (endpoint, server) = spawn_initialize_timeout_server();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let runtime_id = execution_binding().runtime_id();
    let process_generation = CasProcessGeneration::new(35).unwrap();

    let error = fixture
        .store
        .admit(
            &connector,
            runtime_id,
            process_generation,
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap_err();

    assert_eq!(error.runtime_id(), runtime_id);
    assert_eq!(error.process_generation(), process_generation);
    assert!(
        matches!(
            &error,
            ProjectionSessionAdmissionError::Initialization { source, .. }
                if matches!(
                    source.as_ref(),
                    ManagedBackendError::RequestTimeout { method, .. }
                        if method == "initialize"
                )
        ),
        "unexpected projection admission error: {error:?}"
    );
    let workers = fixture.store.worker_pool_diagnostics();
    assert_eq!(workers.available(), workers.capacity());
    assert_eq!(workers.active(), 0);
    assert!(
        (2..=3).contains(&workers.high_water()),
        "the connection pair may overlap the one-permit startup scheduler scan"
    );
    assert_eq!(workers.denied_pairs(), 0);
    server.join().unwrap();
}

fn spawn_initialize_timeout_server() -> (BackendWebSocketEndpoint, thread::JoinHandle<()>) {
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
                    AUTHORIZATION,
                );
                Ok(response)
            },
        )
        .unwrap();
        socket.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
        let Message::Text(request) = socket.read().unwrap() else {
            panic!("foreground initialization must send one JSON text request");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], "initialize");
        assert_eq!(request["params"]["capabilities"]["experimentalApi"], true);
        assert!(
            request["params"]["capabilities"]
                .get("optOutNotificationMethods")
                .is_none()
        );
        assert_no_json_frame(&mut socket);
    });
    (endpoint, server)
}

fn assert_no_json_frame(socket: &mut WebSocket<TcpStream>) {
    match socket.read() {
        Ok(Message::Text(text)) => panic!("unexpected outbound JSON text: {text}"),
        Ok(Message::Binary(bytes)) => panic!("unexpected outbound binary frame: {bytes:?}"),
        Ok(Message::Close(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
        Err(tungstenite::Error::Io(source))
            if matches!(
                source.kind(),
                std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::UnexpectedEof
            ) => {}
        Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {}
        Err(error) => panic!("unexpected test-server WebSocket failure: {error}"),
    }
}
