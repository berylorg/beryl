use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

fn limits() -> ProcessWorkPageLimits {
    ProcessWorkPageLimits::new(256, 65_536).unwrap()
}

fn create_idle_thread(fixture: &Fixture, id: u8) -> SyndicThreadId {
    create_idle_thread_with_id(fixture, [id; 16])
}

fn create_idle_thread_with_id(fixture: &Fixture, id: [u8; 16]) -> SyndicThreadId {
    let home = fixture.service.home.as_deref().unwrap();
    let thread = SyndicThreadId::from_bytes(id);
    let mut draft_id = id;
    draft_id[0] ^= 0xff;
    let binding = ExecutionBinding::new(
        RuntimeId::from_bytes([71; 16]),
        RootId::from_bytes([72; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl",
        )
        .unwrap(),
    );
    execute(
        home,
        fixture.service.storage.create_thread(
            fixture.service.storage.revision(home).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes(draft_id),
                binding,
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    thread
}

#[test]
fn capture_pages_live_idle_threads_with_count_and_byte_bounds() {
    let fixture = Fixture::new();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let threads: Vec<_> = (1_u16..=270)
        .map(|id| {
            let mut bytes = [0; 16];
            bytes[..2].copy_from_slice(&id.to_be_bytes());
            create_idle_thread_with_id(&fixture, bytes)
        })
        .collect();
    let flights: Vec<_> = threads
        .iter()
        .rev()
        .map(|thread| coordinator.begin_projection(*thread).unwrap())
        .collect();
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    for (count, bytes, expected_len) in [
        (256, 65_536, 256),
        (20, std::mem::size_of::<ShutdownWorkRecord>() * 2, 2),
    ] {
        let mut cursor = None;
        let mut found = Vec::new();
        loop {
            let page = fixture
                .service
                .shutdown_work_page(
                    &fixture.sessions,
                    &revision,
                    cursor.as_ref(),
                    ProcessWorkPageLimits::new(count, bytes).unwrap(),
                    &ProjectionCancellationToken::new(),
                )
                .unwrap();
            assert!(page.records.len() <= expected_len);
            assert!(page.bytes <= bytes);
            assert_eq!(page.revision, revision);
            for row in &page.records {
                assert!(row.projection_flight);
                assert_eq!(row.current_turn_id, None);
                found.push(row.thread_id);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(found, threads);
    }
    drop(flights);
}

#[test]
fn durable_pending_work_without_live_custody_does_not_fill_capture() {
    let fixture = Fixture::new();
    for id in 1..=12 {
        let thread = create_idle_thread(&fixture, id);
        let text = format!("preserve backlog {id}");
        submission_fixture::submit_atoms(
            fixture.service.home.as_deref().unwrap(),
            fixture.service.storage.clone(),
            fixture.assets.clone(),
            thread,
            SyndicDraftId::from_bytes([id + 100; 16]),
            SyndicItemId::from_bytes([id + 140; 16]),
            &[submission_fixture::Atom::Text(&text)],
            id * 16,
            SyndicTimestamp::from_unix_millis(3),
        );
    }
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let page = fixture
        .service
        .shutdown_work_page(
            &fixture.sessions,
            &revision,
            None,
            limits(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert!(page.records.is_empty());
    assert!(page.next_cursor.is_none());
    assert_eq!(page.bytes, 0);
    assert!(!revision.requires_connection_cleanup());
}

#[test]
fn foreign_service_sources_cursors_and_durable_drift_cannot_return_capture() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    assert!(matches!(
        fixture.service.shutdown_work_revision(&foreign.sessions),
        Err(ProcessWorkError::ForeignSources)
    ));
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let foreign_revision = foreign
        .service
        .shutdown_work_revision(&foreign.sessions)
        .unwrap();
    assert!(matches!(
        fixture
            .service
            .validate_shutdown_work_revision(&fixture.sessions, &foreign_revision),
        Err(ProcessWorkError::ForeignSources)
    ));
    let cursor = ShutdownWorkCursor {
        revision: foreign_revision,
        after: fixture.thread,
    };
    assert!(matches!(
        fixture.service.shutdown_work_page(
            &fixture.sessions,
            &revision,
            Some(&cursor),
            limits(),
            &ProjectionCancellationToken::new()
        ),
        Err(ProcessWorkError::ForeignCursor)
    ));
    let result = fixture.service.work_read().read_shutdown_work_page(
        &fixture.sessions,
        &revision,
        None,
        limits(),
        &ProjectionCancellationToken::new(),
        || {
            create_idle_thread(&fixture, 1);
        },
    );
    assert!(matches!(result, Err(ProcessWorkError::StaleRevision)));
}

#[test]
fn projection_flight_is_captured_and_acquire_release_invalidates_revision() {
    let fixture = Fixture::new();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let before = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let flight = coordinator.begin_projection(fixture.thread).unwrap();
    assert!(matches!(
        fixture
            .service
            .validate_shutdown_work_revision(&fixture.sessions, &before),
        Err(ProcessWorkError::StaleRevision)
    ));
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let page = fixture
        .service
        .shutdown_work_page(
            &fixture.sessions,
            &revision,
            None,
            limits(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(page.records.len(), 1);
    let row = &page.records[0];
    assert_eq!(row.thread_id, fixture.thread);
    assert_eq!(row.current_turn_id, Some(fixture.turn));
    assert!(row.projection_flight);
    assert!(!row.session_registered);
    assert!(!row.preparation_retained);
    assert!(!row.loaded_projection);
    drop(flight);
    assert!(matches!(
        fixture
            .service
            .validate_shutdown_work_revision(&fixture.sessions, &revision),
        Err(ProcessWorkError::StaleRevision)
    ));
    assert!(matches!(
        fixture
            .service
            .validate_shutdown_work_revision(&fixture.sessions, &before),
        Err(ProcessWorkError::StaleRevision)
    ));
}

#[test]
fn stable_missing_flight_thread_is_an_invariant_but_concurrent_release_is_stale() {
    let fixture = Fixture::new();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let flight = coordinator
        .begin_projection(SyndicThreadId::from_bytes([199; 16]))
        .unwrap();
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    assert!(matches!(
        fixture.service.shutdown_work_page(
            &fixture.sessions,
            &revision,
            None,
            limits(),
            &ProjectionCancellationToken::new()
        ),
        Err(ProcessWorkError::Durable(SyndicReadError::Invariant(_)))
    ));
    let result = fixture.service.work_read().read_shutdown_work_page(
        &fixture.sessions,
        &revision,
        None,
        limits(),
        &ProjectionCancellationToken::new(),
        || drop(flight),
    );
    assert!(matches!(result, Err(ProcessWorkError::StaleRevision)));
}

#[test]
fn terminal_slot_capture_retries_creation_and_release_without_retargeting_observers() {
    let fixture = Fixture::idle();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let flight = coordinator.begin_projection(fixture.thread).unwrap();
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let mut publisher = None;
    let result = fixture.service.work_read().read_shutdown_work_page(
        &fixture.sessions,
        &revision,
        None,
        limits(),
        &ProjectionCancellationToken::new(),
        || {
            publisher = Some(flight.bind_terminal_completion(fixture.turn).unwrap());
        },
    );
    assert!(matches!(result, Err(ProcessWorkError::StaleRevision)));
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let page = fixture
        .service
        .shutdown_work_page(
            &fixture.sessions,
            &revision,
            None,
            limits(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let observer = page.records[0].terminal_completion.clone().unwrap();
    assert_eq!(observer.turn_id(), fixture.turn);
    let result = fixture.service.work_read().read_shutdown_work_page(
        &fixture.sessions,
        &revision,
        None,
        limits(),
        &ProjectionCancellationToken::new(),
        || {
            drop(publisher);
            drop(flight);
        },
    );
    assert!(matches!(result, Err(ProcessWorkError::StaleRevision)));
    let replacement = coordinator.begin_projection(fixture.thread).unwrap();
    let next = SyndicTurnId::from_bytes([199; 16]);
    let _publisher = replacement.bind_terminal_completion(next).unwrap();
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let page = fixture
        .service
        .shutdown_work_page(
            &fixture.sessions,
            &revision,
            None,
            limits(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let next_observer = page.records[0].terminal_completion.as_ref().unwrap();
    assert_ne!(&observer, next_observer);
    assert_eq!(observer.turn_id(), fixture.turn);
    assert_eq!(next_observer.turn_id(), next);
}

#[test]
fn captured_completion_rejects_foreign_identity_service_generation_and_closed_service() {
    let fixture = Fixture::idle();
    let foreign = Fixture::idle();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let flight = coordinator.begin_projection(fixture.thread).unwrap();
    let publisher = flight.bind_terminal_completion(fixture.turn).unwrap();
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let page = fixture
        .service
        .shutdown_work_page(
            &fixture.sessions,
            &revision,
            None,
            limits(),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let captured = page.records[0].terminal_completion.as_ref().unwrap();
    assert_eq!(
        captured
            .completion(&fixture.service, fixture.thread, fixture.turn)
            .unwrap(),
        None
    );
    assert!(matches!(
        captured.completion(&foreign.service, fixture.thread, fixture.turn),
        Err(ProcessWorkError::ForeignSources)
    ));
    assert!(matches!(
        captured.completion(
            &fixture.service,
            fixture.thread,
            SyndicTurnId::from_bytes([199; 16])
        ),
        Err(ProcessWorkError::ForeignSources)
    ));
    assert!(matches!(
        captured.completion(
            &fixture.service,
            SyndicThreadId::from_bytes([198; 16]),
            fixture.turn
        ),
        Err(ProcessWorkError::ForeignSources)
    ));
    let mut old_service = captured.clone();
    old_service.service_generation = foreign.service.service_generation;
    assert!(matches!(
        old_service.completion(&fixture.service, fixture.thread, fixture.turn),
        Err(ProcessWorkError::ForeignSources)
    ));
    drop(publisher);
    drop(flight);
    fixture.service.command_gate.close_for_shutdown();
    assert!(matches!(
        captured.completion(&fixture.service, fixture.thread, fixture.turn),
        Err(ProcessWorkError::Closed)
    ));
}

#[test]
fn cancellation_and_byte_limits_fail_before_returning_capture() {
    let fixture = Fixture::new();
    let revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let cancel = ProjectionCancellationToken::new();
    assert!(matches!(
        fixture.service.shutdown_work_page(
            &fixture.sessions,
            &revision,
            None,
            ProcessWorkPageLimits::new(1, 1).unwrap(),
            &cancel
        ),
        Err(ProcessWorkError::ByteLimit)
    ));
    let result = fixture.service.work_read().read_shutdown_work_page(
        &fixture.sessions,
        &revision,
        None,
        limits(),
        &cancel,
        || cancel.cancel(),
    );
    assert!(matches!(result, Err(ProcessWorkError::Cancelled)));
}
