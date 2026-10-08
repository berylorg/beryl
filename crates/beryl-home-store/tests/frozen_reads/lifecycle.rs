use std::{sync::mpsc, thread, time::Duration};

use super::*;

#[test]
fn release_drains_admitted_request_through_confirmation() {
    let fixture = Fixture::new();
    put(&fixture.store, &fixture.domain, 1, b"admitted");
    let read = fixture.capture();
    let reading = fixture.store.service_reference();
    let read_domain = fixture.domain.clone();
    let read_capability = read.clone();
    let blocked = fixture
        .faults
        .block_next(FaultPoint::BeforeReadConfirmation);
    let reader = thread::spawn(move || {
        reading.read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &read_capability,
            &read_domain,
            &1,
            PointReadLimit::new(128).unwrap(),
        )
    });
    assert!(blocked.wait_until_reached(Duration::from_secs(10)));
    let releasing = fixture.store.service_reference();
    let release_capability = read.clone();
    let (sender, receiver) = mpsc::channel();
    let releaser = thread::spawn(move || {
        sender
            .send(releasing.release_frozen_read(&release_capability))
            .unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
    blocked.release();
    assert_eq!(reader.join().unwrap().unwrap(), Some(b"admitted".to_vec()));
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    releaser.join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    assert!(fixture.point(&read, 1).is_err());
}

#[test]
fn draining_release_remains_charged_until_snapshot_and_admitted_request_are_gone() {
    let fixture = Fixture::new();
    set_frozen_read_limits_for_test(&fixture.store, 1, 1).unwrap();
    put(&fixture.store, &fixture.domain, 1, b"held");
    let read = fixture.capture();
    let reading = fixture.store.service_reference();
    let domain = fixture.domain.clone();
    let capability = read.clone();
    let blocked = fixture
        .faults
        .block_next(FaultPoint::BeforeReadConfirmation);
    let reader = thread::spawn(move || {
        reading.read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &capability,
            &domain,
            &1,
            PointReadLimit::new(128).unwrap(),
        )
    });
    assert!(blocked.wait_until_reached(Duration::from_secs(10)));
    let releasing = fixture.store.service_reference();
    let release_capability = read.clone();
    let releaser = thread::spawn(move || releasing.release_frozen_read(&release_capability));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !matches!(
        fixture.point(&read, 1),
        Err(ReadError::FrozenRead(FrozenReadAccessError::Released))
    ) {
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    assert!(matches!(
        fixture
            .store
            .capture_frozen_read(&CommandCancellation::new()),
        Err(ReadError::FrozenRead(FrozenReadAccessError::Saturated {
            maximum: 1
        }))
    ));
    blocked.release();
    assert_eq!(reader.join().unwrap().unwrap(), Some(b"held".to_vec()));
    releaser.join().unwrap().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    let next = fixture.capture();
    assert_ne!(next, read);
    fixture.store.release_frozen_read(&next).unwrap();
}

#[test]
fn retirement_drains_admitted_request_and_escaped_capabilities_cannot_pin_database() {
    let fixture = Fixture::new();
    put(&fixture.store, &fixture.domain, 1, b"retiring");
    let read = fixture.capture();
    let retained = fixture
        .store
        .retain_frozen_read(&read, &CommandCancellation::new())
        .unwrap();
    let escaped = fixture.store.service_reference();
    let reading = fixture.store.service_reference();
    let read_domain = fixture.domain.clone();
    let read_capability = retained.clone();
    let blocked = fixture
        .faults
        .block_next(FaultPoint::BeforeReadConfirmation);
    let reader = thread::spawn(move || {
        reading.read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &read_capability,
            &read_domain,
            &1,
            PointReadLimit::new(128).unwrap(),
        )
    });
    assert!(blocked.wait_until_reached(Duration::from_secs(10)));
    let (sender, receiver) = mpsc::channel();
    let owner = fixture.store;
    let closer = thread::spawn(move || {
        sender.send(owner.close()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
    blocked.release();
    let _read_result = reader.join().unwrap();
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    closer.join().unwrap();
    assert_eq!(escaped.retained_frozen_read_count().unwrap(), 0);
    assert!(
        escaped
            .read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &retained,
                &fixture.domain,
                &1,
                PointReadLimit::new(128).unwrap(),
            )
            .is_err()
    );
    let (reopened, domain) = open(fixture.directory.path(), FaultController::new());
    assert!(matches!(
        reopened.read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &read,
            &domain,
            &1,
            PointReadLimit::new(128).unwrap(),
        ),
        Err(ReadError::FrozenRead(FrozenReadAccessError::Foreign))
    ));
    let fresh = reopened
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        reopened
            .read_frozen_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
                &fresh,
                &domain,
                &1,
                PointReadLimit::new(128).unwrap(),
            )
            .unwrap(),
        Some(b"retiring".to_vec())
    );
    reopened.release_frozen_read(&fresh).unwrap();
}

#[test]
fn repeated_retention_release_cycles_leave_no_owned_reads() {
    let fixture = Fixture::new();
    for _ in 0..32 {
        let read = fixture.capture();
        let retained = fixture
            .store
            .retain_frozen_read(&read, &CommandCancellation::new())
            .unwrap();
        fixture.store.release_frozen_read(&read).unwrap();
        fixture.store.release_frozen_read(&retained).unwrap();
        assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    }
}
