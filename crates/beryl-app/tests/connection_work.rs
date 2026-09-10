#![cfg(feature = "test-faults")]

#[path = "connection_work/process_work.rs"]
mod process_work;
#[allow(dead_code)]
#[path = "normal_terminal/server.rs"]
mod protocol;
#[path = "connection_work/server.rs"]
mod server;
#[path = "projection/syndic.rs"]
mod syndic;

use beryl_app::cas_projection::*;
use beryl_backend::{DynamicToolCallResponse, ManagedBackendClientConnector, ThreadStartOptions};
use beryl_model::{CasProcessGeneration, CasTurnId};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};
use syndic::{Fixture, execution_binding};
use syndic_storage::SyndicTimestamp;

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";
const TIMEOUT: Duration = Duration::from_secs(10);

fn limits(records: usize) -> ConnectionWorkPageLimits {
    ConnectionWorkPageLimits::new(records, 65_536).unwrap()
}

fn page(service: &ProjectionConnectionService) -> ConnectionWorkPage {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let result = service
            .connection_work_revision()
            .and_then(|revision| service.connection_work_page(&revision, None, limits(256)));
        match result {
            Ok(page) => return page,
            Err(ConnectionWorkError::StaleRevision) if Instant::now() < deadline => {
                thread::yield_now()
            }
            other => panic!("connection work read failed: {other:?}"),
        }
    }
}

fn wait_page(
    service: &ProjectionConnectionService,
    predicate: impl Fn(&ConnectionWorkPage) -> bool,
) -> ConnectionWorkPage {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let page = page(service);
        if predicate(&page) {
            return page;
        }
        assert!(
            Instant::now() < deadline,
            "connection facts did not settle: {page:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn live_fixture(
    seed: u8,
) -> (
    Fixture,
    server::Server,
    AdmittedProjectionSession,
    LiveEventTarget,
    ScheduledExecutionSessions,
) {
    let (mut fixture, sessions) = process_work::fixture(seed);
    fixture.submit_text(" connection work request");
    let server = server::Server::spawn();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        protocol::AUTHORIZATION,
    );
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(92_000 + u64::from(seed)).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(92_219),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &request,
            &fixture.cancellation,
        )
        .unwrap();
    let target = projection
        .into_active_live_event_target(CasTurnId::new(protocol::CAS_TURN_ID).unwrap())
        .unwrap();
    (fixture, server, session, target, sessions)
}

#[test]
fn connection_request_pages_track_queue_handoff_write_and_disposal() {
    let (fixture, server, session, target, sessions) = live_fixture(219);
    let registration = process_work::register(&fixture, &sessions, session);
    let executing = page(&fixture.store);
    let ConnectionWorkRecord::Target(fact) = &executing.records()[0] else {
        panic!("missing target")
    };
    assert_eq!(fact.state(), ConnectionTargetWorkState::Executing);
    assert_eq!(fact.identity().thread_id(), fixture.thread);
    assert_eq!(fact.queued_operations(), 0);

    server.send_requests(2);
    let queued = wait_page(&fixture.store, |page| {
        page.records().iter().filter(|row|
        matches!(row, ConnectionWorkRecord::Request(fact) if fact.stage() == ConnectionRequestWorkStage::Queued)).count() == 2
    });
    assert_eq!(queued.records().len(), 3);
    process_work::assert_request_work(&fixture, &sessions, true);
    let revision = queued.revision();
    let first = fixture
        .store
        .connection_work_page(revision, None, limits(1))
        .unwrap();
    let second = fixture
        .store
        .connection_work_page(revision, first.next_cursor(), limits(1))
        .unwrap();
    let third = fixture
        .store
        .connection_work_page(revision, second.next_cursor(), limits(1))
        .unwrap();
    assert!(matches!(
        first.records()[0],
        ConnectionWorkRecord::Target(_)
    ));
    let ConnectionWorkRecord::Request(second_request) = &second.records()[0] else {
        panic!("missing request")
    };
    let ConnectionWorkRecord::Request(third_request) = &third.records()[0] else {
        panic!("missing request")
    };
    assert!(second_request.request_serial() < third_request.request_serial());
    assert!(third.next_cursor().is_none());
    assert_eq!(page(&fixture.store), queued);
    assert_eq!(
        fixture
            .store
            .connection_work_page(
                revision,
                None,
                ConnectionWorkPageLimits::new(256, first.bytes()).unwrap()
            )
            .unwrap(),
        first
    );
    assert_eq!(
        fixture.store.connection_work_page(
            revision,
            None,
            ConnectionWorkPageLimits::new(256, first.bytes() - 1).unwrap()
        ),
        Err(ConnectionWorkError::ByteLimit)
    );

    let LiveEventPoll::DynamicTool(call) = target.poll(TIMEOUT) else {
        panic!("missing first call")
    };
    assert_eq!(
        fixture.store.validate_connection_work_revision(revision),
        Err(ConnectionWorkError::StaleRevision)
    );
    let handling = page(&fixture.store);
    process_work::assert_request_work(&fixture, &sessions, true);
    assert!(handling.records().iter().any(|row| matches!(row, ConnectionWorkRecord::Request(fact)
        if fact.stage() == ConnectionRequestWorkStage::Handling && fact.response().retained_capabilities() != 0)));
    let barrier = test_faults::install_response_write_barrier(fixture.thread);
    let writer = thread::spawn(move || {
        test_faults::respond_routed_dynamic_tool(
            &target,
            call,
            DynamicToolCallResponse::success_text("complete"),
        )
        .unwrap();
        target
    });
    barrier.wait();
    let admitted = page(&fixture.store);
    assert_ne!(admitted.revision(), handling.revision());
    assert!(admitted.records().iter().any(|row| matches!(row, ConnectionWorkRecord::Request(fact)
        if fact.stage() == ConnectionRequestWorkStage::ResponseAdmitted
            && !fact.response().response_written() && fact.response().retained_capabilities() != 0)));
    assert_eq!(page(&fixture.store), admitted);
    process_work::assert_request_work(&fixture, &sessions, true);
    barrier.release();
    let target = writer.join().unwrap();
    server.wait_for_response();
    let written = wait_page(&fixture.store, |page| {
        page.records().iter().any(|row|
        matches!(row, ConnectionWorkRecord::Request(fact) if fact.response().response_written()))
    });
    assert_ne!(written.revision(), admitted.revision());
    process_work::assert_request_work(&fixture, &sessions, true);
    let LiveEventPoll::DynamicTool(call) = target.poll(TIMEOUT) else {
        panic!("missing second call")
    };
    let before_drop = page(&fixture.store);
    drop(call);
    let disposed = wait_page(&fixture.store, |page| {
        page.records().iter().any(|row|
        matches!(row, ConnectionWorkRecord::Request(fact) if !fact.response().response_written() && fact.response().retained_capabilities() == 0))
    });
    assert_ne!(before_drop.revision(), disposed.revision());
    process_work::assert_request_work(&fixture, &sessions, false);
    assert_eq!(
        fixture
            .store
            .connection_work_page(disposed.revision(), first.next_cursor(), limits(1)),
        Err(ConnectionWorkError::ForeignRevision)
    );
    drop(target);
    process_work::assert_request_work(&fixture, &sessions, false);
    assert!(sessions.retire(registration));
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}

#[test]
fn connection_work_revisions_reject_foreign_owners_and_zero_limits() {
    let first = Fixture::new(220);
    let second = Fixture::new(221);
    let revision = first.store.connection_work_revision().unwrap();
    assert_eq!(
        second.store.validate_connection_work_revision(&revision),
        Err(ConnectionWorkError::ForeignRevision)
    );
    assert_eq!(
        second
            .store
            .connection_work_page(&revision, None, limits(1)),
        Err(ConnectionWorkError::ForeignRevision)
    );
    assert_eq!(
        ConnectionWorkPageLimits::new(0, 1),
        Err(ConnectionWorkError::InvalidLimits)
    );
    assert_eq!(
        ConnectionWorkPageLimits::new(1, 0),
        Err(ConnectionWorkError::InvalidLimits)
    );
    assert!(page(&first.store).records().is_empty());
    let stop_revision = first.store.stop_work_revision().unwrap();
    assert_eq!(
        second.store.validate_stop_work_revision(&stop_revision),
        Err(StopWorkError::ForeignRevision)
    );
    assert_eq!(
        second
            .store
            .stop_work_page(&stop_revision, None, StopWorkPageLimits::new(1, 1).unwrap()),
        Err(StopWorkError::ForeignRevision)
    );
    assert_eq!(
        StopWorkPageLimits::new(0, 1),
        Err(StopWorkError::InvalidLimits)
    );
    assert_eq!(
        StopWorkPageLimits::new(1, 0),
        Err(StopWorkError::InvalidLimits)
    );
    assert!(
        first
            .store
            .stop_work_page(
                &stop_revision,
                None,
                StopWorkPageLimits::new(256, 65_536).unwrap()
            )
            .unwrap()
            .records()
            .is_empty()
    );
}

#[test]
fn full_request_queue_pages_obey_hard_limits_and_retirement_invalidates_cursor() {
    let (fixture, server, session, target, _sessions) = live_fixture(222);
    let empty = page(&fixture.store);
    server.send_requests(256);
    let full = wait_page(&fixture.store, |page| {
        matches!(&page.records()[0],
        ConnectionWorkRecord::Target(fact) if fact.queued_operations() == 256)
    });
    assert_eq!(
        fixture
            .store
            .validate_connection_work_revision(empty.revision()),
        Err(ConnectionWorkError::StaleRevision)
    );
    let oversized = ConnectionWorkPageLimits::new(usize::MAX, usize::MAX).unwrap();
    let first = fixture
        .store
        .connection_work_page(full.revision(), None, oversized)
        .unwrap();
    assert!(first.records().len() <= 256);
    assert!(first.bytes() <= 65_536);
    assert!(first.next_cursor().is_some());
    let mut count = first.records().len();
    let mut cursor = first.next_cursor().cloned();
    let mut last_serial = 0;
    for row in first.records() {
        if let ConnectionWorkRecord::Request(request) = row {
            assert!(request.request_serial() > last_serial);
            last_serial = request.request_serial();
        }
    }
    while let Some(next) = cursor {
        let page = fixture
            .store
            .connection_work_page(full.revision(), Some(&next), oversized)
            .unwrap();
        assert!(page.records().len() <= 256);
        assert!(page.bytes() <= 65_536);
        for row in page.records() {
            let ConnectionWorkRecord::Request(request) = row else {
                panic!("duplicate target")
            };
            assert!(request.request_serial() > last_serial);
            last_serial = request.request_serial();
        }
        count += page.records().len();
        assert!(count <= 257);
        cursor = page.next_cursor().cloned();
    }
    assert_eq!(count, 257);
    assert_eq!(
        fixture.store.connection_work_revision().unwrap(),
        *full.revision()
    );
    drop(target);
    assert_eq!(
        fixture
            .store
            .validate_connection_work_revision(full.revision()),
        Err(ConnectionWorkError::StaleRevision)
    );
    drop(session);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}

#[test]
fn approval_response_completion_and_presentation_custody_are_independent() {
    let (fixture, server, session, target, sessions) = live_fixture(223);
    let registration = process_work::register(&fixture, &sessions, session);
    server.send_approval();
    server.wait_for_response();
    let queued = wait_page(&fixture.store, |page| {
        page.records().iter().any(|row|
        matches!(row, ConnectionWorkRecord::Request(fact)
            if fact.kind() == ConnectionRequestWorkKind::Approval(beryl_backend::ApprovalRequestKind::CommandExecution)
                && fact.stage() == ConnectionRequestWorkStage::Queued && fact.response().response_written()))
    });
    let LiveEventPoll::Approval(approval) = target.poll(TIMEOUT) else {
        panic!("missing approval")
    };
    process_work::assert_request_work(&fixture, &sessions, false);
    let handling = page(&fixture.store);
    assert_ne!(queued.revision(), handling.revision());
    let ConnectionWorkRecord::Request(fact) = &handling.records()[1] else {
        panic!("missing approval fact")
    };
    assert_eq!(fact.stage(), ConnectionRequestWorkStage::Handling);
    assert!(fact.response().response_written());
    assert_eq!(fact.response().retained_capabilities(), 1);
    drop(approval);
    let disposed = page(&fixture.store);
    assert_ne!(handling.revision(), disposed.revision());
    let ConnectionWorkRecord::Request(fact) = &disposed.records()[1] else {
        panic!("missing disposed approval fact")
    };
    assert!(fact.response().response_written());
    assert_eq!(fact.response().retained_capabilities(), 0);
    process_work::assert_request_work(&fixture, &sessions, false);
    drop(target);
    process_work::assert_request_work(&fixture, &sessions, false);
    assert!(sessions.retire(registration));
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}

#[test]
fn removed_target_keeps_only_unwritten_response_custody_required() {
    let (fixture, server, session, target, sessions) = live_fixture(224);
    let registration = process_work::register(&fixture, &sessions, session);
    server.send_requests(1);
    let LiveEventPoll::DynamicTool(call) = target.poll(TIMEOUT) else {
        panic!("missing call");
    };
    process_work::assert_request_work(&fixture, &sessions, true);
    drop(target);
    let retained = wait_page(&fixture.store, |page| {
        page.records().iter().any(|row| {
            matches!(row, ConnectionWorkRecord::Request(fact)
            if !fact.target_registered() && !fact.response().response_written()
                && fact.response().retained_capabilities() != 0)
        })
    });
    process_work::assert_request_work(&fixture, &sessions, true);
    drop(call);
    let disposed = page(&fixture.store);
    assert_ne!(retained.revision(), disposed.revision());
    process_work::assert_request_work(&fixture, &sessions, false);
    assert!(sessions.retire(registration));
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}
