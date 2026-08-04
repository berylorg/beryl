use std::{
    sync::{Arc, Barrier, Mutex},
    thread,
};

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{CasProcessGeneration, CasThreadId, RuntimeId, SyndicThreadId};

use super::{
    CasProjectionCoordinator, ProjectionCancellationToken, ProjectionCoordinatorError,
    accepted_input_scheduler::AcceptedInputSchedulerSignal,
    connection::{
        ConnectionRegistryAuthority, EventRouter, record_connection_thread_closed,
        registry::{
            ExistingSubscription, LoadedThreadKey, ReleaseDisposition, acquire_existing,
            allocate_connection_generation, contains_exact, contains_quarantined,
            contains_reacquisition_reservation, invalidate_connection_thread, live_entry_count,
            live_reacquisition_reservation_count, quarantine_exact, register_new, release_exact,
            reserve_reacquisition, transfer_quarantined,
        },
    },
};

static REGISTRY_TEST_LOCK: Mutex<()> = Mutex::new(());

fn syndic_thread(byte: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([byte; 16])
}

fn runtime(byte: u8) -> RuntimeId {
    RuntimeId::from_bytes([byte; 16])
}

fn process(value: u64) -> CasProcessGeneration {
    CasProcessGeneration::new(value).expect("test process generation is nonzero")
}

fn cas_thread(value: &str) -> CasThreadId {
    CasThreadId::new(value).expect("test CAS thread identity is valid")
}

fn key(value: &str) -> LoadedThreadKey {
    LoadedThreadKey {
        runtime_id: runtime(41),
        process_generation: process(41),
        cas_thread_id: cas_thread(value),
    }
}

fn event_router(connection_generation: u64) -> EventRouter {
    EventRouter::new(runtime(41), process(41), connection_generation).unwrap()
}

#[test]
fn thousands_of_sequential_leases_leave_no_live_entries() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(41);

    for index in 0..4_096 {
        let key = key(&format!("sequential-{index}"));
        let (generation, token) = register_new(key.clone(), connection, owner).unwrap();
        assert_eq!(
            release_exact(&key, connection, owner, generation, token).unwrap(),
            ReleaseDisposition::Last
        );
    }

    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn another_connection_cannot_acquire_a_live_subscription() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    let first_connection = allocate_connection_generation().unwrap();
    let second_connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(42);
    let key = key("cross-connection");
    let (generation, token) = register_new(key.clone(), first_connection, owner).unwrap();

    assert_eq!(
        acquire_existing(&key, second_connection, owner).unwrap(),
        ExistingSubscription::AnotherConnection
    );
    assert_eq!(
        release_exact(&key, first_connection, owner, generation, token).unwrap(),
        ReleaseDisposition::Last
    );
}

#[test]
fn old_lease_token_cannot_revoke_a_reused_key() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    let connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(43);
    let key = key("aba-key");
    let (old_generation, old_token) = register_new(key.clone(), connection, owner).unwrap();
    assert_eq!(
        release_exact(&key, connection, owner, old_generation, old_token).unwrap(),
        ReleaseDisposition::Last
    );

    let (new_generation, new_token) = register_new(key.clone(), connection, owner).unwrap();
    assert_ne!(old_generation, new_generation);
    assert_eq!(
        release_exact(&key, connection, owner, old_generation, old_token).unwrap(),
        ReleaseDisposition::Stale
    );
    assert!(contains_exact(&key, connection, owner, new_generation, new_token).unwrap());
    assert_eq!(
        release_exact(&key, connection, owner, new_generation, new_token).unwrap(),
        ReleaseDisposition::Last
    );
}

#[test]
fn source_and_fork_child_subscriptions_are_independent() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    let connection = allocate_connection_generation().unwrap();
    let source_owner = syndic_thread(44);
    let child_owner = syndic_thread(45);
    let source_key = key("fork-source");
    let child_key = key("fork-child");
    let (source_generation, source_token) =
        register_new(source_key.clone(), connection, source_owner).unwrap();
    let (child_generation, child_token) =
        register_new(child_key.clone(), connection, child_owner).unwrap();

    assert_eq!(
        release_exact(
            &source_key,
            connection,
            source_owner,
            source_generation,
            source_token,
        )
        .unwrap(),
        ReleaseDisposition::Last
    );
    assert!(
        contains_exact(
            &child_key,
            connection,
            child_owner,
            child_generation,
            child_token,
        )
        .unwrap()
    );
    assert_eq!(
        release_exact(
            &child_key,
            connection,
            child_owner,
            child_generation,
            child_token,
        )
        .unwrap(),
        ReleaseDisposition::Last
    );
}

#[test]
fn last_shared_lease_physically_removes_the_entry() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    let connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(46);
    let key = key("shared");
    let (generation, first_token) = register_new(key.clone(), connection, owner).unwrap();
    let ExistingSubscription::Exact {
        generation: acquired_generation,
        token: second_token,
    } = acquire_existing(&key, connection, owner).unwrap()
    else {
        panic!("same connection must acquire a sibling lease")
    };
    assert_eq!(generation, acquired_generation);
    assert_eq!(
        release_exact(&key, connection, owner, generation, first_token).unwrap(),
        ReleaseDisposition::Shared
    );
    assert!(contains_exact(&key, connection, owner, generation, second_token).unwrap());
    assert_eq!(
        release_exact(&key, connection, owner, generation, second_token).unwrap(),
        ReleaseDisposition::Last
    );
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn quarantine_invalidates_siblings_and_transfers_to_one_fresh_generation() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    let old_connection = allocate_connection_generation().unwrap();
    let new_connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(50);
    let key = key("quarantine-transfer");
    let (old_generation, first_token) = register_new(key.clone(), old_connection, owner).unwrap();
    let ExistingSubscription::Exact {
        generation: sibling_generation,
        token: sibling_token,
    } = acquire_existing(&key, old_connection, owner).unwrap()
    else {
        panic!("old connection must acquire its sibling lease")
    };
    assert_eq!(old_generation, sibling_generation);

    let anchor = quarantine_exact(&key, old_connection, owner, old_generation, first_token)
        .unwrap()
        .expect("exact live lease must become the anchor");
    assert!(contains_quarantined(&key, old_connection, owner, old_generation, anchor).unwrap());
    assert!(!contains_exact(&key, old_connection, owner, old_generation, first_token).unwrap());
    assert!(!contains_exact(&key, old_connection, owner, old_generation, sibling_token).unwrap());
    assert_eq!(
        acquire_existing(&key, old_connection, owner).unwrap(),
        ExistingSubscription::Quarantined
    );
    assert_eq!(
        acquire_existing(&key, new_connection, owner).unwrap(),
        ExistingSubscription::AnotherConnection
    );
    let reservation = reserve_reacquisition(
        &key,
        old_connection,
        new_connection,
        owner,
        old_generation,
        anchor,
    )
    .unwrap()
    .expect("fresh connection must reserve the exact handoff");
    let competing_connection = allocate_connection_generation().unwrap();
    assert!(
        reserve_reacquisition(
            &key,
            old_connection,
            competing_connection,
            owner,
            old_generation,
            anchor,
        )
        .unwrap()
        .is_none()
    );
    assert!(
        reserve_reacquisition(
            &key,
            old_connection,
            new_connection,
            owner,
            old_generation,
            anchor,
        )
        .unwrap()
        .is_none()
    );
    assert!(
        contains_reacquisition_reservation(
            &key,
            old_connection,
            new_connection,
            owner,
            old_generation,
            anchor,
            reservation,
        )
        .unwrap()
    );
    let reserved_other = LoadedThreadKey {
        cas_thread_id: cas_thread("reserved-connection-other"),
        ..key.clone()
    };
    assert!(matches!(
        register_new(reserved_other, new_connection, owner),
        Err(ProjectionCoordinatorError::ProjectionConnectionReservedForReacquisition { .. })
    ));
    assert!(matches!(
        acquire_existing(&key, new_connection, owner),
        Err(ProjectionCoordinatorError::ProjectionConnectionReservedForReacquisition { .. })
    ));

    let (new_generation, new_token) = transfer_quarantined(
        &key,
        old_connection,
        new_connection,
        owner,
        old_generation,
        anchor,
        reservation,
    )
    .unwrap()
    .expect("exact anchor must transfer");
    assert_eq!(new_generation.process(), old_generation.process());
    assert_ne!(new_generation.thread(), old_generation.thread());
    assert!(!contains_quarantined(&key, old_connection, owner, old_generation, anchor).unwrap());
    assert!(contains_exact(&key, new_connection, owner, new_generation, new_token).unwrap());
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    assert_eq!(
        release_exact(&key, new_connection, owner, new_generation, new_token).unwrap(),
        ReleaseDisposition::Last
    );
}

#[test]
fn transfer_rejects_a_reservation_token_from_another_exact_handoff() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old_a = allocate_connection_generation().unwrap();
    let new_a = allocate_connection_generation().unwrap();
    let old_b = allocate_connection_generation().unwrap();
    let new_b = allocate_connection_generation().unwrap();
    let owner_a = syndic_thread(58);
    let owner_b = syndic_thread(59);
    let key_a = key("reservation-token-a");
    let key_b = key("reservation-token-b");

    let (generation_a, lease_a) = register_new(key_a.clone(), old_a, owner_a).unwrap();
    let anchor_a = quarantine_exact(&key_a, old_a, owner_a, generation_a, lease_a)
        .unwrap()
        .expect("first exact lease becomes quarantined");
    let reservation_a =
        reserve_reacquisition(&key_a, old_a, new_a, owner_a, generation_a, anchor_a)
            .unwrap()
            .expect("first exact handoff becomes reserved");

    let (generation_b, lease_b) = register_new(key_b.clone(), old_b, owner_b).unwrap();
    let anchor_b = quarantine_exact(&key_b, old_b, owner_b, generation_b, lease_b)
        .unwrap()
        .expect("second exact lease becomes quarantined");
    let reservation_b =
        reserve_reacquisition(&key_b, old_b, new_b, owner_b, generation_b, anchor_b)
            .unwrap()
            .expect("second exact handoff becomes reserved");

    assert!(
        transfer_quarantined(
            &key_a,
            old_a,
            new_a,
            owner_a,
            generation_a,
            anchor_a,
            reservation_b,
        )
        .unwrap()
        .is_none()
    );
    assert!(
        contains_reacquisition_reservation(
            &key_a,
            old_a,
            new_a,
            owner_a,
            generation_a,
            anchor_a,
            reservation_a,
        )
        .unwrap()
    );
    assert!(
        contains_reacquisition_reservation(
            &key_b,
            old_b,
            new_b,
            owner_b,
            generation_b,
            anchor_b,
            reservation_b,
        )
        .unwrap()
    );

    let (new_generation_a, new_lease_a) = transfer_quarantined(
        &key_a,
        old_a,
        new_a,
        owner_a,
        generation_a,
        anchor_a,
        reservation_a,
    )
    .unwrap()
    .expect("first exact token still transfers");
    let (new_generation_b, new_lease_b) = transfer_quarantined(
        &key_b,
        old_b,
        new_b,
        owner_b,
        generation_b,
        anchor_b,
        reservation_b,
    )
    .unwrap()
    .expect("second exact token still transfers");
    assert_eq!(
        release_exact(&key_a, new_a, owner_a, new_generation_a, new_lease_a).unwrap(),
        ReleaseDisposition::Last
    );
    assert_eq!(
        release_exact(&key_b, new_b, owner_b, new_generation_b, new_lease_b).unwrap(),
        ReleaseDisposition::Last
    );
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
}

#[test]
fn old_side_thread_close_prevents_reserved_handoff() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old_connection = allocate_connection_generation().unwrap();
    let new_connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(51);
    let key = key("old-close-before-transfer");
    let (generation, lease) = register_new(key.clone(), old_connection, owner).unwrap();
    let anchor = quarantine_exact(&key, old_connection, owner, generation, lease)
        .unwrap()
        .expect("exact lease becomes a quarantine anchor");
    let reservation = reserve_reacquisition(
        &key,
        old_connection,
        new_connection,
        owner,
        generation,
        anchor,
    )
    .unwrap()
    .expect("fresh replacement becomes reserved");

    assert!(invalidate_connection_thread(&key, old_connection).unwrap());
    assert!(!contains_quarantined(&key, old_connection, owner, generation, anchor).unwrap());
    assert!(
        !contains_reacquisition_reservation(
            &key,
            old_connection,
            new_connection,
            owner,
            generation,
            anchor,
            reservation,
        )
        .unwrap()
    );
    assert!(
        transfer_quarantined(
            &key,
            old_connection,
            new_connection,
            owner,
            generation,
            anchor,
            reservation,
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
}

#[test]
fn replacement_side_thread_close_poisoning_preserves_only_old_anchor() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old_connection = allocate_connection_generation().unwrap();
    let new_connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(52);
    let key = key("replacement-close-before-transfer");
    let (generation, lease) = register_new(key.clone(), old_connection, owner).unwrap();
    let anchor = quarantine_exact(&key, old_connection, owner, generation, lease)
        .unwrap()
        .expect("exact lease becomes a quarantine anchor");
    let reservation = reserve_reacquisition(
        &key,
        old_connection,
        new_connection,
        owner,
        generation,
        anchor,
    )
    .unwrap()
    .expect("fresh replacement becomes reserved");

    assert!(invalidate_connection_thread(&key, new_connection).unwrap());
    assert!(contains_quarantined(&key, old_connection, owner, generation, anchor).unwrap());
    assert!(
        transfer_quarantined(
            &key,
            old_connection,
            new_connection,
            owner,
            generation,
            anchor,
            reservation,
        )
        .unwrap()
        .is_none()
    );
    assert!(invalidate_connection_thread(&key, old_connection).unwrap());
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
}

#[test]
fn post_transfer_close_is_scoped_to_the_observing_connection() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old_connection = allocate_connection_generation().unwrap();
    let new_connection = allocate_connection_generation().unwrap();
    let owner = syndic_thread(53);
    let key = key("post-transfer-close");
    let (generation, lease) = register_new(key.clone(), old_connection, owner).unwrap();
    let anchor = quarantine_exact(&key, old_connection, owner, generation, lease)
        .unwrap()
        .expect("exact lease becomes a quarantine anchor");
    let reservation = reserve_reacquisition(
        &key,
        old_connection,
        new_connection,
        owner,
        generation,
        anchor,
    )
    .unwrap()
    .expect("fresh replacement becomes reserved");
    let (new_generation, new_lease) = transfer_quarantined(
        &key,
        old_connection,
        new_connection,
        owner,
        generation,
        anchor,
        reservation,
    )
    .unwrap()
    .expect("exact two-sided handoff transfers");

    assert!(!invalidate_connection_thread(&key, old_connection).unwrap());
    assert!(contains_exact(&key, new_connection, owner, new_generation, new_lease).unwrap());
    assert!(invalidate_connection_thread(&key, new_connection).unwrap());
    assert!(!contains_exact(&key, new_connection, owner, new_generation, new_lease).unwrap());
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
}

#[test]
fn connection_scoped_close_fences_before_reserved_native_transfer() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let new = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let old_router = Arc::new(event_router(old.generation_for_test().get()));
    let new_router = Arc::new(event_router(new.generation_for_test().get()));
    let owner = syndic_thread(54);
    let loaded_key = key("ordered-close-before-transfer");
    let (generation, lease) = old
        .register_new_for_test(loaded_key.clone(), owner)
        .unwrap()
        .expect("old connection registers exact loaded authority");
    let anchor = old
        .quarantine_exact_for_test(&loaded_key, owner, generation, lease)
        .unwrap()
        .expect("old loaded authority becomes a quarantine anchor");
    let old_command = old_router.authorize_command_for_test().unwrap();
    let new_command = new_router.authorize_command_for_test().unwrap();
    let reservation = ConnectionRegistryAuthority::reserve_reacquisition_for_test(
        &old,
        &old_router,
        &old_command,
        &new,
        &new_router,
        &new_command,
        &loaded_key,
        owner,
        generation,
        anchor,
    )
    .unwrap()
    .expect("fresh connection reserves exact native handoff");
    drop((old_command, new_command));

    let old_gate = old.lock_for_test();
    let closing_old = Arc::clone(&old);
    let closing_router = Arc::clone(&old_router);
    let closing_thread_id = loaded_key.cas_thread_id.clone();
    let closed = thread::spawn(move || {
        record_connection_thread_closed(&closing_old, &closing_router, &closing_thread_id)
    });

    let fence_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let lane_fenced = loop {
        if !old_router
            .permits_reacquisition_thread(&loaded_key.cas_thread_id)
            .unwrap()
        {
            break true;
        }
        if std::time::Instant::now() >= fence_deadline {
            break false;
        }
        thread::yield_now();
    };
    if !lane_fenced {
        drop(old_gate);
        let _ = closed.join();
        panic!("thread close did not record its router-lane fence before the bounded deadline");
    }
    assert!(
        contains_quarantined(
            &loaded_key,
            old.generation_for_test(),
            owner,
            generation,
            anchor,
        )
        .unwrap()
    );
    assert!(
        contains_reacquisition_reservation(
            &loaded_key,
            old.generation_for_test(),
            new.generation_for_test(),
            owner,
            generation,
            anchor,
            reservation,
        )
        .unwrap()
    );
    assert!(!closed.is_finished());

    let transferring_old = Arc::clone(&old);
    let transferring_old_router = Arc::clone(&old_router);
    let transferring_new = Arc::clone(&new);
    let transferring_new_router = Arc::clone(&new_router);
    let transferring_key = loaded_key.clone();
    let (transfer_ready, transfer_is_ready) = std::sync::mpsc::sync_channel(0);
    let transfer = thread::spawn(move || {
        let old_command = transferring_old_router
            .authorize_command_for_test()
            .unwrap();
        let new_command = transferring_new_router
            .authorize_command_for_test()
            .unwrap();
        transfer_ready.send(()).unwrap();
        ConnectionRegistryAuthority::transfer_quarantined_for_test(
            &transferring_old,
            &transferring_old_router,
            &old_command,
            &transferring_new,
            &transferring_new_router,
            &new_command,
            &transferring_key,
            owner,
            generation,
            anchor,
            reservation,
        )
    });
    transfer_is_ready
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("reserved transfer reaches the authority-gate race");
    assert!(!transfer.is_finished());
    drop(old_gate);

    let closed = closed.join().unwrap().unwrap();
    let transferred = transfer.join().unwrap().unwrap();
    assert!(!closed.connection_retired());
    assert!(transferred.is_none());
    assert!(
        !contains_quarantined(
            &loaded_key,
            old.generation_for_test(),
            owner,
            generation,
            anchor,
        )
        .unwrap()
    );
    assert!(
        !contains_reacquisition_reservation(
            &loaded_key,
            old.generation_for_test(),
            new.generation_for_test(),
            owner,
            generation,
            anchor,
            reservation,
        )
        .unwrap()
    );
    assert_eq!(
        acquire_existing(&loaded_key, old.generation_for_test(), owner).unwrap(),
        ExistingSubscription::Absent
    );
    assert_eq!(
        acquire_existing(&loaded_key, new.generation_for_test(), owner).unwrap(),
        ExistingSubscription::Absent
    );
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    old.retire().unwrap();
    new.retire().unwrap();
}

#[test]
fn connection_scoped_close_after_transfer_honors_observing_generation() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let new = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let old_router = Arc::new(event_router(old.generation_for_test().get()));
    let new_router = Arc::new(event_router(new.generation_for_test().get()));
    let owner = syndic_thread(55);
    let loaded_key = key("ordered-close-after-transfer");
    let (generation, lease) = old
        .register_new_for_test(loaded_key.clone(), owner)
        .unwrap()
        .expect("old connection registers exact loaded authority");
    let anchor = old
        .quarantine_exact_for_test(&loaded_key, owner, generation, lease)
        .unwrap()
        .expect("old loaded authority becomes a quarantine anchor");
    let old_command = old_router.authorize_command_for_test().unwrap();
    let new_command = new_router.authorize_command_for_test().unwrap();
    let reservation = ConnectionRegistryAuthority::reserve_reacquisition_for_test(
        &old,
        &old_router,
        &old_command,
        &new,
        &new_router,
        &new_command,
        &loaded_key,
        owner,
        generation,
        anchor,
    )
    .unwrap()
    .expect("fresh connection reserves exact native handoff");
    let (new_generation, new_lease) = ConnectionRegistryAuthority::transfer_quarantined_for_test(
        &old,
        &old_router,
        &old_command,
        &new,
        &new_router,
        &new_command,
        &loaded_key,
        owner,
        generation,
        anchor,
        reservation,
    )
    .unwrap()
    .expect("native handoff publishes one new loaded generation");
    drop((old_command, new_command));

    let obsolete =
        record_connection_thread_closed(&old, &old_router, &loaded_key.cas_thread_id).unwrap();
    assert!(!obsolete.registry_authority_revoked());
    assert!(
        contains_exact(
            &loaded_key,
            new.generation_for_test(),
            owner,
            new_generation,
            new_lease,
        )
        .unwrap()
    );

    let current =
        record_connection_thread_closed(&new, &new_router, &loaded_key.cas_thread_id).unwrap();
    assert!(current.registry_authority_revoked());
    assert!(
        !contains_exact(
            &loaded_key,
            new.generation_for_test(),
            owner,
            new_generation,
            new_lease,
        )
        .unwrap()
    );
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    old.retire().unwrap();
    new.retire().unwrap();
}

#[test]
fn connection_scoped_close_serializes_with_connection_retirement() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let router = Arc::new(event_router(authority.generation_for_test().get()));
    let owner = syndic_thread(60);
    let loaded_key = key("ordered-close-retirement-race");
    authority
        .register_new_for_test(loaded_key.clone(), owner)
        .unwrap()
        .expect("the exact connection owns one active loaded entry");

    let gate = authority.lock_for_test();
    let barrier = Arc::new(Barrier::new(3));
    let closing_authority = Arc::clone(&authority);
    let closing_router = Arc::clone(&router);
    let closing_key = loaded_key.clone();
    let closing_barrier = Arc::clone(&barrier);
    let retiring_authority = Arc::clone(&authority);
    let retiring_barrier = Arc::clone(&barrier);
    let close = thread::spawn(move || {
        closing_barrier.wait();
        record_connection_thread_closed(
            &closing_authority,
            &closing_router,
            &closing_key.cas_thread_id,
        )
    });
    let retirement = thread::spawn(move || {
        retiring_barrier.wait();
        retiring_authority.retire()
    });

    barrier.wait();
    drop(gate);
    let close = close.join().unwrap().unwrap();
    retirement.join().unwrap().unwrap();

    assert!(!close.connection_retired());
    assert!(authority.is_retired());
    assert!(
        !router
            .permits_reacquisition_thread(&loaded_key.cas_thread_id)
            .unwrap()
    );
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
}

#[test]
fn connection_scoped_close_revokes_loaded_authority_after_router_poison() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let router = Arc::new(event_router(authority.generation_for_test().get()));
    let owner = syndic_thread(61);
    let loaded_key = key("ordered-close-router-poison");
    authority
        .register_new_for_test(loaded_key.clone(), owner)
        .unwrap()
        .expect("the exact connection owns one active loaded entry");
    router.poison_state_for_test();

    assert!(matches!(
        record_connection_thread_closed(&authority, &router, &loaded_key.cas_thread_id),
        Err(ProjectionCoordinatorError::RegistryPoisoned {
            registry: super::ProjectionRegistryKind::LiveEventRouter,
        })
    ));
    assert_eq!(live_entry_count().unwrap(), 0);
    authority.retire().unwrap();
}

#[test]
fn connection_scoped_close_fences_before_poisoned_connection_authority_fails() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let router = Arc::new(event_router(authority.generation_for_test().get()));
    let owner = syndic_thread(62);
    let loaded_key = key("ordered-close-authority-poison");
    authority
        .register_new_for_test(loaded_key.clone(), owner)
        .unwrap()
        .expect("the exact connection owns one active loaded entry");
    authority.poison_for_recovery_test();

    assert!(matches!(
        record_connection_thread_closed(&authority, &router, &loaded_key.cas_thread_id),
        Err(ProjectionCoordinatorError::RegistryPoisoned {
            registry: super::ProjectionRegistryKind::ProjectionConnection,
        })
    ));
    assert!(authority.is_retired());
    assert!(
        !router
            .permits_reacquisition_thread(&loaded_key.cas_thread_id)
            .unwrap()
    );
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn old_connection_retirement_linearizes_with_reserved_transfer() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let new = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let old_router = Arc::new(event_router(85));
    let new_router = Arc::new(event_router(86));
    let owner = syndic_thread(56);
    let loaded_key = key("old-retirement-transfer-race");
    let (generation, lease) = old
        .register_new_for_test(loaded_key.clone(), owner)
        .unwrap()
        .expect("old authority is live");
    let anchor = old
        .quarantine_exact_for_test(&loaded_key, owner, generation, lease)
        .unwrap()
        .expect("old lease becomes quarantined");
    let old_command = old_router.authorize_command_for_test().unwrap();
    let new_command = new_router.authorize_command_for_test().unwrap();
    let reservation = ConnectionRegistryAuthority::reserve_reacquisition_for_test(
        &old,
        &old_router,
        &old_command,
        &new,
        &new_router,
        &new_command,
        &loaded_key,
        owner,
        generation,
        anchor,
    )
    .unwrap()
    .expect("fresh replacement becomes reserved");
    drop((old_command, new_command));

    let barrier = Arc::new(Barrier::new(3));
    let transfer_old = Arc::clone(&old);
    let transfer_new = Arc::clone(&new);
    let transfer_old_router = Arc::clone(&old_router);
    let transfer_new_router = Arc::clone(&new_router);
    let transfer_key = loaded_key.clone();
    let transfer_barrier = Arc::clone(&barrier);
    let retirement_old = Arc::clone(&old);
    let retirement_barrier = Arc::clone(&barrier);
    let gate = old.lock_for_test();

    let transfer = thread::spawn(move || {
        transfer_barrier.wait();
        let old_command = transfer_old_router.authorize_command_for_test().unwrap();
        let new_command = transfer_new_router.authorize_command_for_test().unwrap();
        ConnectionRegistryAuthority::transfer_quarantined_for_test(
            &transfer_old,
            &transfer_old_router,
            &old_command,
            &transfer_new,
            &transfer_new_router,
            &new_command,
            &transfer_key,
            owner,
            generation,
            anchor,
            reservation,
        )
    });
    let retirement = thread::spawn(move || {
        retirement_barrier.wait();
        retirement_old.retire()
    });

    barrier.wait();
    drop(gate);
    let transferred = transfer.join().unwrap().unwrap();
    retirement.join().unwrap().unwrap();

    assert!(old.is_retired());
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    match transferred {
        Some((new_generation, new_lease)) => {
            assert!(
                contains_exact(
                    &loaded_key,
                    new.generation_for_test(),
                    owner,
                    new_generation,
                    new_lease,
                )
                .unwrap()
            );
            assert_eq!(
                release_exact(
                    &loaded_key,
                    new.generation_for_test(),
                    owner,
                    new_generation,
                    new_lease,
                )
                .unwrap(),
                ReleaseDisposition::Last
            );
        }
        None => assert_eq!(live_entry_count().unwrap(), 0),
    }
    let _ = new.retire();
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn replacement_connection_retirement_linearizes_with_reserved_transfer() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    let old = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let new = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let old_router = Arc::new(event_router(87));
    let new_router = Arc::new(event_router(88));
    let owner = syndic_thread(57);
    let loaded_key = key("replacement-retirement-transfer-race");
    let (generation, lease) = old
        .register_new_for_test(loaded_key.clone(), owner)
        .unwrap()
        .expect("old authority is live");
    let anchor = old
        .quarantine_exact_for_test(&loaded_key, owner, generation, lease)
        .unwrap()
        .expect("old lease becomes quarantined");
    let old_command = old_router.authorize_command_for_test().unwrap();
    let new_command = new_router.authorize_command_for_test().unwrap();
    let reservation = ConnectionRegistryAuthority::reserve_reacquisition_for_test(
        &old,
        &old_router,
        &old_command,
        &new,
        &new_router,
        &new_command,
        &loaded_key,
        owner,
        generation,
        anchor,
    )
    .unwrap()
    .expect("fresh replacement becomes reserved");
    drop((old_command, new_command));

    let barrier = Arc::new(Barrier::new(3));
    let transfer_old = Arc::clone(&old);
    let transfer_new = Arc::clone(&new);
    let transfer_old_router = Arc::clone(&old_router);
    let transfer_new_router = Arc::clone(&new_router);
    let transfer_key = loaded_key.clone();
    let transfer_barrier = Arc::clone(&barrier);
    let retirement_new = Arc::clone(&new);
    let retirement_barrier = Arc::clone(&barrier);
    let gate = new.lock_for_test();

    let transfer = thread::spawn(move || {
        transfer_barrier.wait();
        let old_command = transfer_old_router.authorize_command_for_test().unwrap();
        let new_command = transfer_new_router.authorize_command_for_test().unwrap();
        ConnectionRegistryAuthority::transfer_quarantined_for_test(
            &transfer_old,
            &transfer_old_router,
            &old_command,
            &transfer_new,
            &transfer_new_router,
            &new_command,
            &transfer_key,
            owner,
            generation,
            anchor,
            reservation,
        )
    });
    let retirement = thread::spawn(move || {
        retirement_barrier.wait();
        retirement_new.retire()
    });

    barrier.wait();
    drop(gate);
    let transferred = transfer.join().unwrap().unwrap();
    retirement.join().unwrap().unwrap();

    assert!(new.is_retired());
    assert_eq!(live_reacquisition_reservation_count().unwrap(), 0);
    match transferred {
        Some((new_generation, new_lease)) => assert!(
            !contains_exact(
                &loaded_key,
                new.generation_for_test(),
                owner,
                new_generation,
                new_lease,
            )
            .unwrap()
        ),
        None => assert!(
            contains_quarantined(
                &loaded_key,
                old.generation_for_test(),
                owner,
                generation,
                anchor,
            )
            .unwrap()
        ),
    }
    old.retire().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn connection_retirement_cannot_leave_a_racing_new_registration() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let waiting_registration = Arc::clone(&authority);
    let waiting_retirement = Arc::clone(&authority);
    let barrier = Arc::new(Barrier::new(3));
    let registration_barrier = Arc::clone(&barrier);
    let retirement_barrier = Arc::clone(&barrier);
    let gate = authority.lock_for_test();

    let registration = thread::spawn(move || {
        registration_barrier.wait();
        waiting_registration.register_new_for_test(key("retire-new-race"), syndic_thread(47))
    });
    let retirement = thread::spawn(move || {
        retirement_barrier.wait();
        waiting_retirement.retire()
    });

    barrier.wait();
    drop(gate);
    registration.join().unwrap().unwrap();
    retirement.join().unwrap().unwrap();

    assert!(authority.is_retired());
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn session_release_and_new_registry_authority_have_closed_serialized_orders() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let registering_authority = Arc::clone(&authority);
    let releasing_authority = Arc::clone(&authority);
    let loaded_key = key("session-release-registration-race");
    let owner = syndic_thread(59);
    let barrier = Arc::new(Barrier::new(3));
    let registration_barrier = Arc::clone(&barrier);
    let release_barrier = Arc::clone(&barrier);
    let gate = authority.lock_for_test();

    let registration = thread::spawn(move || {
        registration_barrier.wait();
        registering_authority.register_new_for_test(loaded_key, owner)
    });
    let release = thread::spawn(move || {
        release_barrier.wait();
        releasing_authority.release_session_owner(|| true)
    });

    barrier.wait();
    drop(gate);
    let registration = registration.join().unwrap().unwrap();
    let detached = release.join().unwrap().unwrap();

    match registration {
        Some((generation, token)) => {
            assert!(!detached);
            assert!(!authority.is_retired());
            assert_eq!(
                release_exact(
                    &key("session-release-registration-race"),
                    authority.generation_for_test(),
                    owner,
                    generation,
                    token,
                )
                .unwrap(),
                ReleaseDisposition::Last
            );
            let _ = authority.retire();
        }
        None => {
            assert!(detached);
            assert!(authority.is_retired());
        }
    }
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn connection_retirement_cannot_leave_a_racing_sibling_acquisition() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let loaded_key = key("retire-acquire-race");
    authority
        .register_new_for_test(loaded_key.clone(), syndic_thread(48))
        .unwrap()
        .expect("active connection registers the initial lease");
    let waiting_acquisition = Arc::clone(&authority);
    let waiting_retirement = Arc::clone(&authority);
    let barrier = Arc::new(Barrier::new(3));
    let acquisition_barrier = Arc::clone(&barrier);
    let retirement_barrier = Arc::clone(&barrier);
    let gate = authority.lock_for_test();

    let acquisition = thread::spawn(move || {
        acquisition_barrier.wait();
        waiting_acquisition.acquire_existing_for_test(&loaded_key, syndic_thread(48))
    });
    let retirement = thread::spawn(move || {
        retirement_barrier.wait();
        waiting_retirement.retire()
    });

    barrier.wait();
    drop(gate);
    acquisition.join().unwrap().unwrap();
    retirement.join().unwrap().unwrap();

    assert!(authority.is_retired());
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn poisoned_connection_gate_retires_authority_without_leaving_an_entry() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new(runtime(41), process(41)).unwrap());
    let poisoning_authority = Arc::clone(&authority);

    assert!(
        thread::spawn(move || {
            let _gate = poisoning_authority.lock_for_test();
            panic!("poison the bounded connection-registry gate");
        })
        .join()
        .is_err()
    );

    assert!(matches!(
        authority.register_new_for_test(key("poisoned-gate"), syndic_thread(49)),
        Err(ProjectionCoordinatorError::RegistryPoisoned {
            registry: super::ProjectionRegistryKind::ProjectionConnection,
        })
    ));
    assert!(authority.is_retired());
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn cancellation_is_one_way_and_shared_by_clones() {
    let token = ProjectionCancellationToken::new();
    let observer = token.clone();
    let independent = ProjectionCancellationToken::new();
    assert!(!token.is_cancelled());
    assert!(!observer.is_cancelled());

    observer.cancel();
    assert!(token.is_cancelled());
    assert!(observer.is_cancelled());
    assert!(!independent.is_cancelled());

    token.cancel();
    assert!(observer.is_cancelled());
}

#[test]
fn projection_flight_is_process_wide_across_coordinator_instances() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let first = CasProjectionCoordinator::for_healthy_home(&store).unwrap();
    let second = CasProjectionCoordinator::for_healthy_home(&store).unwrap();
    let thread = syndic_thread(47);

    let first_flight = first.begin_projection(thread).unwrap();
    assert!(matches!(
        second.begin_projection(thread),
        Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id })
            if thread_id == thread
    ));

    drop(first_flight);
    let second_flight = second.begin_projection(thread).unwrap();
    drop(second_flight);
}

#[test]
fn scheduled_projection_arms_one_coalesced_release_wake() {
    let directory = tempfile::tempdir().unwrap();
    let store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let first = CasProjectionCoordinator::for_healthy_home(&store).unwrap();
    let second = CasProjectionCoordinator::for_healthy_home(&store).unwrap();
    let signal = AcceptedInputSchedulerSignal::new();
    let thread = syndic_thread(48);

    let first_flight = first.begin_projection(thread).unwrap();
    assert!(matches!(
        second.begin_scheduled_projection(thread, &signal),
        Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id })
            if thread_id == thread
    ));
    assert!(matches!(
        second.begin_scheduled_projection(thread, &signal),
        Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id })
            if thread_id == thread
    ));
    assert_eq!(signal.diagnostics().wake_count(), 0);

    drop(first_flight);
    assert_eq!(signal.diagnostics().wake_count(), 1);

    let scheduled = second.begin_scheduled_projection(thread, &signal).unwrap();
    drop(scheduled);
    assert_eq!(signal.diagnostics().wake_count(), 1);
}
