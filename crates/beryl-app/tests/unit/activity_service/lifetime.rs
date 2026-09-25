use super::*;
use beryl_home_store::test_faults::FaultPoint;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn failed_initial_request_retries_only_its_original_runtime_period() {
    use beryl_home_store::{CommandOutcome, HomeCommand};
    use beryl_model::SyndicItemId;
    use support::{draft_id, timestamp};
    use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1};

    let f = Fixture::new(true);
    let mut command = HomeCommand::new(f.home.home_revision().unwrap());
    command
        .add(f.storage.create_thread(
            f.storage.revision(&f.home).unwrap(),
            CreateThread::ordinary(
                id(88),
                draft_id(89),
                f.binding.clone(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        f.home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let service = f.service(limits(1, 1, 1));
    let request = service
        .prepare_collection(id(88), f.binding.runtime_id())
        .unwrap();
    assert!(matches!(request.open(), Err(ActivityReadError::Unenrolled)));
    request.with_current(|| ()).unwrap();
    let turn = support::exact_cas::submit_current_draft(
        &f.home,
        f.storage.clone(),
        id(88),
        draft_id(90),
        SyndicItemId::from_bytes([91; 16]),
        "other",
        timestamp(101),
    );
    f.interest
        .as_ref()
        .unwrap()
        .enroll_activity_for_test(&f.home, &f.storage, id(88), turn)
        .unwrap();
    let collection = request.open().unwrap();
    let expected = f
        .storage
        .activity_query_head(&f.home, id(30), point_limit())
        .unwrap()
        .unwrap()
        .work_period();
    collection
        .with_current_head(|head| {
            assert_eq!(head.work_period(), expected);
            assert_eq!(head.thread_id(), id(88));
            assert_eq!(head.logical_row_count(), 0);
        })
        .unwrap();
}

#[test]
fn persisted_activity_cannot_supply_unenrolled_or_ended_runtime_authority() {
    let mut f = Fixture::new(true);
    let source = f.activate();
    f.add_command(&source, 230);
    let before = f
        .storage
        .activity_query_head(&f.home, id(30), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(before.logical_row_count(), 1);
    let service = f.service(limits(2, 2, 1));
    f.interest.take();
    assert!(f.probe.wait_for_disposal(TIMEOUT));
    f.acquire(false);
    assert!(matches!(
        service.prepare_collection(id(30), f.binding.runtime_id()),
        Err(ActivityReadError::Runtime(
            RuntimeActivityReadError::Unenrolled
        ))
    ));
    f.interest.take();
    assert!(f.probe.wait_for_disposal(TIMEOUT));
    assert!(matches!(
        service.prepare_collection(id(30), f.binding.runtime_id()),
        Err(ActivityReadError::Runtime(
            RuntimeActivityReadError::RuntimeUnavailable
        ))
    ));
    assert_eq!(
        f.storage
            .activity_query_head(&f.home, id(30), point_limit())
            .unwrap(),
        Some(before)
    );
}

#[test]
fn failed_initial_request_retains_exact_scope_and_cannot_retry_after_runtime_retirement() {
    let mut f = Fixture::new(true);
    let service = f.service(limits(2, 1, 1));
    let request = service
        .prepare_collection(id(250), f.binding.runtime_id())
        .unwrap();
    assert!(matches!(
        request.open(),
        Err(ActivityReadError::ThreadUnavailable)
    ));
    request.with_current(|| ()).unwrap();
    f.interest.take();
    assert!(f.probe.wait_for_disposal(TIMEOUT));
    assert!(
        request
            .with_current(|| panic!("retired error callback"))
            .is_err()
    );
    assert!(request.open().is_err());
    f.acquire(true);
    assert!(request.open().is_err());
    assert!(
        request
            .with_current(|| panic!("replacement cannot revive old request"))
            .is_err()
    );
    let fresh = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    drop(fresh.open().unwrap());
}

#[test]
fn runtime_retirement_during_read_excludes_completion_and_retained_page_callbacks() {
    let mut f = Fixture::new(true);
    let service = f.service(limits(1, 2, 1));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let collection = request.open().unwrap();
    let page = collection.read_page(None).unwrap();
    let blocked = f.faults.block_next(FaultPoint::BeforeReadConfirmation);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| collection.read_page(None));
        if !blocked.wait_until_reached(TIMEOUT) {
            blocked.release();
            panic!("Activity reader did not reach confirmation");
        }
        let duplicate = collection.read_page(None);
        f.interest.take();
        let retired = f.probe.wait_for_disposal(TIMEOUT);
        blocked.release();
        let completion = worker.join().unwrap();
        assert!(matches!(duplicate, Err(ActivityReadError::RequestPending)));
        assert!(retired);
        assert!(completion.is_err());
    });
    let published = AtomicBool::new(false);
    assert!(
        page.with_current(|_, _| published.store(true, Ordering::SeqCst))
            .is_err()
    );
    assert!(!published.load(Ordering::SeqCst));
    assert!(page.continuation().is_err());
    assert!(
        collection
            .with_current_head(|_| panic!("retired head callback"))
            .is_err()
    );
}

#[test]
fn initial_open_is_single_flight_and_its_late_completion_cannot_publish() {
    let mut f = Fixture::new(true);
    let service = f.service(limits(1, 1, 1));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let blocked = f.faults.block_next(FaultPoint::BeforeReadConfirmation);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| request.open());
        if !blocked.wait_until_reached(TIMEOUT) {
            blocked.release();
            panic!("initial Activity reader did not reach confirmation");
        }
        let duplicate = request.open();
        f.interest.take();
        let retired = f.probe.wait_for_disposal(TIMEOUT);
        blocked.release();
        let completion = worker.join().unwrap();
        assert!(matches!(duplicate, Err(ActivityReadError::RequestPending)));
        assert!(retired);
        assert!(completion.is_err());
    });
    assert!(
        request
            .with_current(|| panic!("late initial callback"))
            .is_err()
    );
}

#[test]
fn service_retirement_disposes_resources_and_rejects_retained_queries() {
    let f = Fixture::new(true);
    let service = f.service(limits(2, 1, 1));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let collection = request.open().unwrap();
    let page = collection.read_page(None).unwrap();
    let pending = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    service.retire();
    assert!(service.shared.state.lock().unwrap().resources.is_none());
    assert!(matches!(
        pending.open(),
        Err(ActivityReadError::Unavailable)
    ));
    assert!(matches!(
        request.with_current(|| ()),
        Err(ActivityReadError::Unavailable)
    ));
    assert!(matches!(
        collection.read_page(None),
        Err(ActivityReadError::Unavailable)
    ));
    assert!(matches!(
        page.with_current(|_, _| panic!("retired service callback")),
        Err(ActivityReadError::Unavailable)
    ));
    drop(service);
    drop(page);
    drop(collection);
    drop(request);
}

#[test]
fn service_retirement_waits_for_the_blocked_reader_and_releases_its_resources() {
    let f = Fixture::new(true);
    let service = f.service(limits(1, 1, 1));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let collection = request.open().unwrap();
    let blocked = f.faults.block_next(FaultPoint::BeforeReadConfirmation);
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| collection.read_page(None));
        if !blocked.wait_until_reached(TIMEOUT) {
            blocked.release();
            panic!("Activity reader did not reach retirement cut");
        }
        let retire = scope.spawn(|| {
            service.retire();
            finished_tx.send(()).unwrap();
        });
        let deadline = std::time::Instant::now() + TIMEOUT;
        let state_at_cut = loop {
            let state = service.shared.state.lock().unwrap();
            if !state.live || std::time::Instant::now() >= deadline {
                break (state.live, state.active, state.resources.is_some());
            }
            drop(state);
            std::thread::yield_now();
        };
        let prematurely_finished = finished_rx.try_recv().is_ok();
        blocked.release();
        let result = reader.join().unwrap();
        retire.join().unwrap();
        assert_eq!(state_at_cut, (false, 1, true));
        assert!(!prematurely_finished);
        assert!(matches!(result, Err(ActivityReadError::Unavailable)));
    });
    assert!(finished_rx.recv_timeout(TIMEOUT).is_ok());
    let state = service.shared.state.lock().unwrap();
    assert_eq!(state.active, 0);
    assert!(state.resources.is_none());
}

#[test]
fn page_publication_is_busy_during_a_producer_commit_and_retries_after_release() {
    let mut f = Fixture::new(true);
    let service = f.service(limits(1, 1, 1));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let collection = request.open().unwrap();
    let page = collection.read_page(None).unwrap();
    let interest = Arc::clone(f.interest.as_ref().unwrap());
    let source = syndic_storage::ActivityQuerySource::new(id(30), f.turn);
    let published = AtomicBool::new(false);
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let producer = scope.spawn(move || {
            interest
                .with_activity_for_test(source, |_| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                })
                .unwrap();
        });
        if entered_rx.recv_timeout(TIMEOUT).is_err() {
            let _ = release_tx.send(());
            producer.join().unwrap();
            panic!("producer did not enter Activity publication");
        }
        let publisher = scope.spawn(|| {
            result_tx
                .send(page.with_current(|_, _| published.store(true, Ordering::SeqCst)))
                .unwrap();
        });
        let result = result_rx.recv_timeout(TIMEOUT);
        release_tx.send(()).unwrap();
        producer.join().unwrap();
        publisher.join().unwrap();
        assert!(matches!(
            result,
            Ok(Err(ActivityReadError::Runtime(
                RuntimeActivityReadError::Busy
            )))
        ));
        assert!(!published.load(Ordering::SeqCst));
    });
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match page.with_current(|_, _| published.store(true, Ordering::SeqCst)) {
            Ok(()) => break,
            Err(ActivityReadError::Runtime(RuntimeActivityReadError::Busy))
                if std::time::Instant::now() < deadline =>
            {
                std::thread::yield_now()
            }
            Err(error) => panic!("released publication: {error:?}"),
        }
    }
    assert!(published.load(Ordering::SeqCst));
    f.interest.take();
    assert!(f.probe.wait_for_disposal(TIMEOUT));
    assert!(
        page.with_current(|_, _| panic!("retired publication"))
            .is_err()
    );
}
