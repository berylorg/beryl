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
        ProjectionServiceConfig::try_new(8, 4).unwrap(),
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
        Some(ManagedBackendError::CompatibilityManagedLaunchProvenanceMissing)
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
