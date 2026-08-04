use std::{
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread,
    time::Duration,
};

use beryl_backend::{BackendWebSocketEndpoint, CompatibilityProbe, ManagedBackendClientConnector};
use beryl_model::{CasProcessGeneration, CasThreadId, RuntimeId};
use serde_json::Value;
use tungstenite::{Message, WebSocket, accept_hdr};

use super::*;

const FORWARDING_AUTHORIZATION: &str = "Bearer phase82-forwarding-order";
const FORWARDING_TIMEOUT: Duration = Duration::from_secs(10);

enum ForwardingServerCommand {
    ThreadClosed(CasThreadId, SyncSender<()>),
    Close,
}

struct ForwardingOrderServer {
    endpoint: BackendWebSocketEndpoint,
    commands: SyncSender<ForwardingServerCommand>,
    ready: Receiver<()>,
    closed: Receiver<()>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ForwardingOrderServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (commands, command_receiver) = mpsc::sync_channel(1);
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let (closed_sender, closed) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("phase82-forwarding-order-server".to_owned())
            .spawn(move || {
                run_forwarding_server(listener, command_receiver, ready_sender, closed_sender);
            })
            .unwrap();
        Self {
            endpoint,
            commands,
            ready,
            closed,
            handle: Some(handle),
        }
    }

    fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    fn wait_ready(&self) {
        self.ready
            .recv_timeout(FORWARDING_TIMEOUT)
            .expect("the forwarding-order server must finish admission");
    }

    fn send_thread_closed(&self, thread_id: CasThreadId) {
        let (sent, observed) = mpsc::sync_channel(1);
        self.commands
            .send(ForwardingServerCommand::ThreadClosed(thread_id, sent))
            .unwrap();
        observed
            .recv_timeout(FORWARDING_TIMEOUT)
            .expect("the forwarding-order server must write thread/closed");
    }

    fn close(mut self) {
        self.commands.send(ForwardingServerCommand::Close).unwrap();
        self.closed
            .recv_timeout(FORWARDING_TIMEOUT)
            .expect("the forwarding-order server must close");
        self.handle.take().unwrap().join().unwrap();
    }
}

fn run_forwarding_server(
    listener: TcpListener,
    commands: Receiver<ForwardingServerCommand>,
    ready: SyncSender<()>,
    closed: SyncSender<()>,
) {
    let (stream, _) = listener.accept().unwrap();
    let mut socket = accept_hdr(
        stream,
        |request: &tungstenite::handshake::server::Request, response| {
            assert_eq!(
                request.headers().get("authorization").unwrap(),
                FORWARDING_AUTHORIZATION
            );
            Ok(response)
        },
    )
    .unwrap();
    socket
        .get_mut()
        .set_read_timeout(Some(FORWARDING_TIMEOUT))
        .unwrap();
    complete_forwarding_admission(&mut socket);
    ready.send(()).unwrap();
    while let Ok(command) = commands.recv() {
        match command {
            ForwardingServerCommand::ThreadClosed(thread_id, sent) => {
                send_forwarding_json(
                    &mut socket,
                    &format!(
                        r#"{{"method":"thread/closed","params":{{"threadId":"{}"}}}}"#,
                        thread_id.as_str()
                    ),
                );
                sent.send(()).unwrap();
            }
            ForwardingServerCommand::Close => break,
        }
    }
    drop(socket);
    closed.send(()).unwrap();
}

fn complete_forwarding_admission(socket: &mut WebSocket<TcpStream>) {
    let initialize = read_forwarding_json(socket);
    assert_eq!(initialize["method"], "initialize");
    let initialize_id = initialize["id"].as_u64().unwrap();
    send_forwarding_json(
        socket,
        &format!(
            r#"{{"id":{initialize_id},"result":{{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}}}"#,
        ),
    );
    let initialized = read_forwarding_json(socket);
    assert_eq!(initialized["method"], "initialized");

    for probe in CompatibilityProbe::ALL {
        let request = read_forwarding_json(socket);
        assert_eq!(request["method"], probe.method());
        let id = request["id"].as_u64().unwrap();
        let response = match probe {
            CompatibilityProbe::ConfigRead => format!(
                r#"{{"id":{id},"result":{{"config":{{"model":"gpt-5.6","model_reasoning_effort":"high","features":{{"multi_agent_v2":{{"enabled":true,"expose_spawn_agent_model_overrides":true}}}}}},"origins":{{"features.multi_agent_v2.enabled":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{{"name":{{"type":"sessionFlags"}},"version":"0"}}}}}}}}"#,
            ),
            CompatibilityProbe::ModelList => {
                format!(r#"{{"id":{id},"result":{{"data":[],"nextCursor":null}}}}"#)
            }
            CompatibilityProbe::ThreadUnsubscribe => {
                format!(r#"{{"id":{id},"result":{{"status":"notLoaded"}}}}"#)
            }
            _ => format!(r#"{{"error":{{"code":-32600,"message":"recognized"}},"id":{id}}}"#,),
        };
        send_forwarding_json(socket, &response);
    }
}

fn read_forwarding_json(socket: &mut WebSocket<TcpStream>) -> Value {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return serde_json::from_str(&text).unwrap(),
            Message::Ping(payload) => socket.send(Message::Pong(payload)).unwrap(),
            Message::Pong(_) | Message::Frame(_) => {}
            Message::Binary(bytes) => panic!("unexpected binary admission frame: {bytes:?}"),
            Message::Close(close) => panic!("unexpected admission close: {close:?}"),
        }
    }
}

fn send_forwarding_json(socket: &mut WebSocket<TcpStream>, value: &str) {
    socket.send(Message::Text(value.into())).unwrap();
}

struct ForwardingAdoptionFixture {
    _directory: tempfile::TempDir,
    server: ForwardingOrderServer,
    connection: Arc<crate::cas_projection::connection::ProjectionConnection>,
    quarantine: crate::cas_projection::PersistentFailurePendingProjectionQuarantine,
    replacement: UnpublishedProjectionConnectionService,
}

struct ForwardingLiveFixture {
    directory: tempfile::TempDir,
    faults: beryl_home_store::test_faults::FaultController,
    state: beryl_state::BerylState,
    service: ProjectionConnectionService,
    server: ForwardingOrderServer,
    session: crate::cas_projection::AdmittedProjectionSession,
    connection: Arc<crate::cas_projection::connection::ProjectionConnection>,
}

fn forwarding_live_fixture(seed: u8) -> ForwardingLiveFixture {
    let (directory, faults, state, _shutdowns, service) = service();
    let server = ForwardingOrderServer::spawn();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        FORWARDING_AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([seed; 16]),
            CasProcessGeneration::new(82_000 + u64::from(seed)).unwrap(),
            Path::new(r"C:\work\beryl"),
            FORWARDING_TIMEOUT,
        )
        .unwrap();
    server.wait_ready();
    let connection = Arc::clone(session.connection());

    ForwardingLiveFixture {
        directory,
        faults,
        state,
        service,
        server,
        session,
        connection,
    }
}

fn finish_forwarding_failure(
    fixture: ForwardingLiveFixture,
    cut_armed: SyncSender<()>,
) -> ForwardingAdoptionFixture {
    let ForwardingLiveFixture {
        directory,
        faults,
        state,
        service,
        server,
        session,
        connection,
    } = fixture;

    fail_home_through_live_command(&service, state, &faults);
    cut_armed.send(()).unwrap();
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("the forwarding-order failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the forwarding-order connection must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }),
    )
    .unwrap();

    ForwardingAdoptionFixture {
        _directory: directory,
        server,
        connection,
        quarantine,
        replacement,
    }
}

fn dispose_forwarding_fixture(
    _directory: tempfile::TempDir,
    server: ForwardingOrderServer,
    connection: Arc<crate::cas_projection::connection::ProjectionConnection>,
    adopted: AdoptedUnpublishedProjectionConnectionService,
) {
    server.close();
    drop(adopted);
    connection
        .dispose_inert_driver_after_adoption_failure()
        .unwrap();
}

#[test]
fn phase82_thread_closed_selection_straddles_the_hub_cut_without_old_router_capture() {
    let live = forwarding_live_fixture(187);
    let before_thread = CasThreadId::new("phase-82-forwarding-before-cut").unwrap();
    let before_pause = live
        .connection
        .pause_next_thread_closed_after_router_for_test(before_thread.clone());
    live.server.send_thread_closed(before_thread);
    let old_router = before_pause.wait();

    let (failure_started_sender, failure_started_receiver) = mpsc::sync_channel(1);
    let (failure_sender, failure_receiver) = mpsc::sync_channel(1);
    let failure = thread::spawn(move || {
        failure_sender
            .send(finish_forwarding_failure(live, failure_started_sender))
            .unwrap();
    });
    failure_started_receiver
        .recv_timeout(FORWARDING_TIMEOUT)
        .expect("persistent-failure progression must enter its consuming call");
    assert!(matches!(
        failure_receiver.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Timeout)
    ));

    before_pause.release();
    let ForwardingAdoptionFixture {
        _directory,
        server,
        connection,
        quarantine,
        replacement,
    } = failure_receiver
        .recv_timeout(FORWARDING_TIMEOUT)
        .expect("failure quarantine must finish after the old close settles");
    failure.join().unwrap();

    let hub_attempt = connection.observe_next_forwarding_hub_lock_attempt_for_test();
    let (adoption_sender, adoption_receiver) = mpsc::sync_channel(1);
    let adoption = thread::spawn(move || {
        adoption_sender
            .send(quarantine.adopt_unpublished_service(replacement))
            .unwrap();
    });
    hub_attempt.wait();
    let adopted = adoption_receiver
        .recv_timeout(FORWARDING_TIMEOUT)
        .expect("adoption must finish after the selected old close settles")
        .unwrap();
    adoption.join().unwrap();

    let after_thread = CasThreadId::new("phase-82-forwarding-after-cut").unwrap();
    let after_pause =
        connection.pause_next_thread_closed_after_router_for_test(after_thread.clone());
    let after_connection = Arc::clone(&connection);
    let after_close = thread::spawn(move || after_connection.record_thread_closed(&after_thread));
    let replacement_router = after_pause.wait();
    assert_ne!(replacement_router, old_router);
    after_pause.release();
    after_close.join().unwrap().unwrap();

    let repeated_thread = CasThreadId::new("phase-82-forwarding-repeated-after-cut").unwrap();
    let repeated_pause =
        connection.pause_next_thread_closed_after_router_for_test(repeated_thread.clone());
    let repeated_connection = Arc::clone(&connection);
    let repeated_close =
        thread::spawn(move || repeated_connection.record_thread_closed(&repeated_thread));
    assert_eq!(repeated_pause.wait(), replacement_router);
    repeated_pause.release();
    repeated_close.join().unwrap().unwrap();

    dispose_forwarding_fixture(_directory, server, connection, adopted);
}
