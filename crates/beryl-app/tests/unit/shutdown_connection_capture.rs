use super::*;
use crate::cas_projection::{ProcessWorkPageLimits, ProjectionCancellationToken};

#[test]
fn generic_cleanup_without_thread_is_retained_and_cleanup_aba_is_stale() {
    let (directory, service, _state, sessions) = owned_service(6);
    let (session, server) = admitted_session(&service, 72_120);
    let connection = Arc::clone(session.connection());
    let before = service.shutdown_work_revision(&sessions).unwrap();
    assert!(!before.requires_connection_cleanup());
    assert!(
        !service
            .observe_shutdown_work(&sessions, &ProjectionCancellationToken::new())
            .unwrap()
            .has_work()
    );
    let cleanup = connection.acquire_cleanup_owner().unwrap().unwrap();
    assert!(
        service
            .validate_shutdown_work_revision(&sessions, &before)
            .is_err()
    );
    let revision = service.shutdown_work_revision(&sessions).unwrap();
    assert!(revision.requires_connection_cleanup());
    assert!(
        service
            .observe_shutdown_work(&sessions, &ProjectionCancellationToken::new())
            .unwrap()
            .has_work()
    );
    let page = service
        .shutdown_work_page(
            &sessions,
            &revision,
            None,
            ProcessWorkPageLimits::new(4, 65_536).unwrap(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert!(page.records.is_empty());
    drop(cleanup);
    assert!(
        service
            .validate_shutdown_work_revision(&sessions, &before)
            .is_err()
    );
    assert!(
        service
            .validate_shutdown_work_revision(&sessions, &revision)
            .is_err()
    );
    assert!(
        !service
            .shutdown_work_revision(&sessions)
            .unwrap()
            .requires_connection_cleanup()
    );
    drop(session);
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}

#[test]
fn shared_loaded_projection_remains_captured_after_one_lease_returns() {
    let (directory, service, _state, sessions) = owned_service(6);
    let server = NormalTerminalServer::spawn_unsubscribe_failure();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([91; 16]),
            CasProcessGeneration::new(72_121).unwrap(),
            Path::new(r"C:\work\scheduled-ordinary"),
            TIMEOUT,
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let thread = SyndicThreadId::from_bytes([218; 16]);
    let home = service.home_for_shutdown_test();
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command
        .add(service.storage().create_thread(
            service.storage().revision(home).unwrap(),
            syndic_storage::CreateThread::ordinary(
                thread,
                beryl_model::SyndicDraftId::from_bytes([218; 16]),
                execution_binding(RuntimeId::from_bytes([91; 16])),
                syndic_storage::SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let cas_thread = beryl_model::CasThreadId::new("shared-shutdown-capture").unwrap();
    let first = connection
        .register_new(
            cas_thread.clone(),
            beryl_backend::ThreadSessionMetadata::default(),
            thread,
            TIMEOUT,
        )
        .unwrap();
    let crate::cas_projection::connection::ExistingLease::Exact(second) = connection
        .acquire_existing(&cas_thread, thread, TIMEOUT)
        .unwrap()
    else {
        panic!("shared lease unavailable")
    };
    let revision = service.shutdown_work_revision(&sessions).unwrap();
    first.release().unwrap();
    assert!(
        service
            .validate_shutdown_work_revision(&sessions, &revision)
            .is_err()
    );
    let revision = service.shutdown_work_revision(&sessions).unwrap();
    let page = service
        .shutdown_work_page(
            &sessions,
            &revision,
            None,
            ProcessWorkPageLimits::new(4, 65_536).unwrap(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].thread_id, thread);
    assert_eq!(page.records[0].current_turn_id, None);
    assert!(page.records[0].loaded_projection);
    assert!(
        !service
            .observe_shutdown_work(&sessions, &ProjectionCancellationToken::new())
            .unwrap()
            .has_work()
    );
    assert!(second.release().is_err());
    assert!(
        service
            .validate_shutdown_work_revision(&sessions, &revision)
            .is_err()
    );
    drop(session);
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}
