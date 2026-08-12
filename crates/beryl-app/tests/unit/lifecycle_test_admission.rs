use std::{io::ErrorKind, net::TcpListener, path::Path, sync::mpsc, thread, time::Duration};

use beryl_backend::{BackendWebSocketEndpoint, ManagedBackendClientConnector, ManagedBackendError};
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{CasProcessGeneration, RuntimeId};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;
use tungstenite::{Message, accept};

use crate::cas_projection::{
    ProjectionConnectionService, ProjectionServiceConfig, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
    MinimumTurnCaptureReserve,
};

struct RejectingScheduledOrdinaryProvider;

impl ScheduledOrdinaryExecutionProvider for RejectingScheduledOrdinaryProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

#[test]
fn public_admit_rejects_lifecycle_connector_without_consuming_another_fixture_server() {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    BerylState::register(&mut home).unwrap();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(RejectingScheduledOrdinaryProvider),
    )
    .unwrap();

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let later_fixture_listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    later_fixture_listener.set_nonblocking(true).unwrap();
    let (initialized, initialized_observer) = mpsc::sync_channel(1);
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = accept(stream).unwrap();
        let initialize = socket.read().unwrap().into_text().unwrap();
        assert!(initialize.contains("\"method\":\"initialize\""));
        socket
            .send(Message::text(
                r#"{"id":1,"result":{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}"#,
            ))
            .unwrap();
        let notification = socket.read().unwrap().into_text().unwrap();
        assert!(notification.contains("\"method\":\"initialized\""));
        initialized.send(()).unwrap();
        while matches!(socket.read(), Ok(message) if !message.is_close()) {}
    });

    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, "Bearer test");
    let error = service
        .admit(
            &connector,
            RuntimeId::from_bytes([85; 16]),
            CasProcessGeneration::new(85_001).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(5),
        )
        .unwrap_err();
    assert!(matches!(
        error.backend_error(),
        Some(ManagedBackendError::ReleaseAdmissionManagedLaunchProvenanceMissing)
    ));
    initialized_observer
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    server.join().unwrap();
    assert!(matches!(
        later_fixture_listener.accept(),
        Err(error) if error.kind() == ErrorKind::WouldBlock
    ));
}

#[test]
fn lifecycle_release_admission_sends_only_initialize_initialized_and_config_read() {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    BerylState::register(&mut home).unwrap();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(RejectingScheduledOrdinaryProvider),
    )
    .unwrap();

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let (admitted, admitted_observer) = mpsc::sync_channel(1);
    let (release, release_observer) = mpsc::sync_channel(1);
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = accept(stream).unwrap();
        let initialize = socket.read().unwrap().into_text().unwrap();
        assert!(initialize.contains("\"method\":\"initialize\""));
        socket
            .send(Message::text(
                r#"{"id":1,"result":{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}"#,
            ))
            .unwrap();
        let initialized = socket.read().unwrap().into_text().unwrap();
        assert!(initialized.contains("\"method\":\"initialized\""));
        let config = socket.read().unwrap().into_text().unwrap();
        assert!(config.contains("\"method\":\"config/read\""));
        assert!(config.contains(r#""cwd":"C:\\work\\beryl""#));
        socket
            .send(Message::text(
                r#"{"id":2,"result":{"config":{"model":"gpt-5.6","model_reasoning_effort":"high","features":{"multi_agent_v2":{"enabled":true,"expose_spawn_agent_model_overrides":true}}},"origins":{"features.multi_agent_v2.enabled":{"name":{"type":"sessionFlags"},"version":"0"},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{"name":{"type":"sessionFlags"},"version":"0"}}}}"#,
            ))
            .unwrap();
        socket
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        assert!(matches!(
            socket.read(),
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)
        ));
        admitted.send(()).unwrap();
        release_observer.recv().unwrap();
    });

    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, "Bearer test");
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([86; 16]),
            CasProcessGeneration::new(85_002).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(5),
        )
        .unwrap();
    admitted_observer.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(session);
    release.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn app_admission_source_uses_release_admission_terminology_only() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let error = std::fs::read_to_string(crate_root.join("src/cas_projection/error.rs"))
        .expect("projection admission error source is readable");
    let admission = std::fs::read_to_string(crate_root.join("src/cas_projection/service/admission.rs"))
        .expect("projection admission source is readable");

    assert!(error.contains("ReleaseAdmission"));
    assert!(error.contains("release_admission("));
    assert!(error.contains("CAS release admission failed"));
    assert!(error.contains("release admission"));
    assert!(admission.contains(".admit_release("));
    assert!(
        admission.contains(".admit_release_non_authorizing_for_lifecycle_test(")
    );
    for obsolete in [
        "ProjectionSessionAdmissionError::Compatibility",
        "Self::Compatibility",
        "fn compatibility(",
        "CAS compatibility admission",
        "or probing",
    ] {
        assert!(
            !error.contains(obsolete) && !admission.contains(obsolete),
            "app release-admission boundary retained obsolete terminology {obsolete}"
        );
    }
}
