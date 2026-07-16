use std::{
    sync::{Arc, Barrier, Mutex},
    thread,
};

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{CasProcessGeneration, CasThreadId, RuntimeId, SyndicThreadId};

use super::{
    CasProjectionCoordinator, ProjectionCancellationToken, ProjectionCoordinatorError,
    connection::{
        ConnectionRegistryAuthority,
        registry::{
            ExistingSubscription, LoadedThreadKey, ReleaseDisposition, acquire_existing,
            allocate_connection_generation, contains_exact, live_entry_count, register_new,
            release_exact,
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
fn connection_retirement_cannot_leave_a_racing_new_registration() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new().unwrap());
    let waiting_registration = Arc::clone(&authority);
    let waiting_retirement = Arc::clone(&authority);
    let barrier = Arc::new(Barrier::new(3));
    let registration_barrier = Arc::clone(&barrier);
    let retirement_barrier = Arc::clone(&barrier);
    let gate = authority.lock_for_test();

    let registration = thread::spawn(move || {
        registration_barrier.wait();
        waiting_registration.register_new(key("retire-new-race"), syndic_thread(47))
    });
    let retirement = thread::spawn(move || {
        retirement_barrier.wait();
        waiting_retirement.retire();
    });

    barrier.wait();
    drop(gate);
    registration.join().unwrap().unwrap();
    retirement.join().unwrap();

    assert!(authority.is_retired());
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn connection_retirement_cannot_leave_a_racing_sibling_acquisition() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new().unwrap());
    let loaded_key = key("retire-acquire-race");
    authority
        .register_new(loaded_key.clone(), syndic_thread(48))
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
        waiting_acquisition.acquire_existing(&loaded_key, syndic_thread(48))
    });
    let retirement = thread::spawn(move || {
        retirement_barrier.wait();
        waiting_retirement.retire();
    });

    barrier.wait();
    drop(gate);
    acquisition.join().unwrap().unwrap();
    retirement.join().unwrap();

    assert!(authority.is_retired());
    assert_eq!(live_entry_count().unwrap(), 0);
}

#[test]
fn poisoned_connection_gate_retires_authority_without_leaving_an_entry() {
    let _serial = REGISTRY_TEST_LOCK.lock().unwrap();
    assert_eq!(live_entry_count().unwrap(), 0);
    let authority = Arc::new(ConnectionRegistryAuthority::new().unwrap());
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
        authority.register_new(key("poisoned-gate"), syndic_thread(49)),
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
