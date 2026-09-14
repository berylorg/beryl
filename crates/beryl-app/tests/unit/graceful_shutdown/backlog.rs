use super::*;

fn identity(index: u16, kind: u8) -> [u8; 16] {
    let mut id = [0x50; 16];
    id[1..3].copy_from_slice(&index.to_be_bytes());
    id[15] = kind;
    id
}

fn create(fixture: &Fixture, index: u16) -> SyndicThreadId {
    let home = fixture.service.home.as_deref().unwrap();
    let storage = &fixture.service.storage;
    let thread = SyndicThreadId::from_bytes(identity(index, 1));
    let binding = storage
        .thread_execution(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    execute(
        home,
        storage.create_thread(
            storage.revision(home).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes(identity(index, 2)),
                binding,
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    thread
}

#[test]
fn durable_pending_backlog_crosses_pages_without_accumulating_guards() {
    let fixture = Fixture::new();
    let home = fixture.service.home.as_deref().unwrap();
    for index in 1..=270 {
        let thread = create(&fixture, index);
        let text = format!("shutdown backlog {index}");
        submission_fixture::submit_atoms(
            home,
            fixture.service.storage.clone(),
            fixture.assets.clone(),
            thread,
            SyndicDraftId::from_bytes(identity(index, 3)),
            SyndicItemId::from_bytes(identity(index, 4)),
            &[submission_fixture::Atom::Text(&text)],
            index as u8,
            SyndicTimestamp::from_unix_millis(3),
        );
    }
    let durable = fixture.service.storage.revision(home).unwrap();
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
    assert_eq!(fixture.service.storage.revision(home).unwrap(), durable);
    let owner = fixture.service.graceful_shutdown.lock().unwrap();
    let attempt = owner.attempt.as_ref().unwrap();
    assert_eq!(attempt.execution.ordinary().count(), 0);
    assert_eq!(attempt.execution.compactions().count(), 0);
    assert!(attempt.stops.is_empty());
    let coordinator = CasProjectionCoordinator::for_healthy_home(home).unwrap();
    for index in [1, 256, 270] {
        let thread = SyndicThreadId::from_bytes(identity(index, 1));
        assert!(
            fixture
                .service
                .storage
                .pending_dispatch_evidence(home, thread, point_limit())
                .unwrap()
                .is_some()
        );
        drop(coordinator.begin_projection(thread).unwrap());
    }
}

#[test]
fn late_execution_behind_generic_cursor_is_captured_before_admission_closes() {
    let fixture = Fixture::new();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let flights: Vec<_> = (1..=270)
        .map(|index| {
            coordinator
                .begin_projection(create(&fixture, index))
                .unwrap()
        })
        .collect();
    let ordinary = fixture.acquired_projection_flight(fixture.thread);
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    {
        let owner = fixture.service.graceful_shutdown.lock().unwrap();
        let attempt = owner.attempt.as_ref().unwrap();
        assert!(
            attempt
                .progress_after
                .is_some_and(|after| after > fixture.thread)
        );
        assert_eq!(attempt.execution.ordinary().count(), 0);
    }
    let publisher = ordinary.bind_terminal_completion(fixture.turn).unwrap();
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    assert_eq!(
        fixture
            .service
            .graceful_shutdown
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .execution
            .ordinary()
            .count(),
        1
    );
    drop(publisher);
    drop(ordinary);
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    drop(flights);
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
}
