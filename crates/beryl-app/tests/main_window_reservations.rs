use std::{
    sync::{Arc, Barrier},
    thread,
};

use beryl_app::window_acquisition::{
    RuntimeBackedWindowMainWindowReservationError, RuntimeBackedWindowProcessRegistry,
};
use beryl_model::WindowId;

const MAIN_WINDOW_CAPACITY: usize = 256;

fn window_id(value: u16) -> WindowId {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&value.to_le_bytes());
    WindowId::from_bytes(bytes)
}

#[test]
fn reserves_exactly_256_main_window_slots() {
    let process = RuntimeBackedWindowProcessRegistry::new();
    let mut reservations = (0..MAIN_WINDOW_CAPACITY)
        .map(|index| {
            process
                .reserve_main_window(window_id(index as u16))
                .expect("slot within capacity")
        })
        .collect::<Vec<_>>();

    assert_eq!(process.main_window_occupancy(), MAIN_WINDOW_CAPACITY);
    assert!(matches!(
        process.reserve_main_window(window_id(MAIN_WINDOW_CAPACITY as u16)),
        Err(RuntimeBackedWindowMainWindowReservationError::Capacity)
    ));

    let released = reservations
        .pop()
        .expect("the final slot reservation")
        .window_id();
    assert_eq!(released, window_id((MAIN_WINDOW_CAPACITY - 1) as u16));
    assert!(matches!(
        process.reserve_main_window(window_id(0)),
        Err(RuntimeBackedWindowMainWindowReservationError::DuplicateWindowIdentity)
    ));
    let recovered = process
        .reserve_main_window(released)
        .expect("exact released slot is reusable");
    assert!(matches!(
        process.reserve_main_window(window_id(MAIN_WINDOW_CAPACITY as u16)),
        Err(RuntimeBackedWindowMainWindowReservationError::Capacity)
    ));

    drop(recovered);
    drop(reservations);
    assert_eq!(process.main_window_occupancy(), 0);
}

#[test]
fn rejects_duplicates_across_registry_clones_and_reuses_exact_release() {
    let process = RuntimeBackedWindowProcessRegistry::new();
    let sibling = process.clone();
    let id = window_id(1);
    let reservation = process.reserve_main_window(id).expect("first admission");

    assert!(matches!(
        sibling.reserve_main_window(id),
        Err(RuntimeBackedWindowMainWindowReservationError::DuplicateWindowIdentity)
    ));
    assert_eq!(sibling.main_window_occupancy(), 1);

    drop(reservation);
    let recovered = sibling
        .reserve_main_window(id)
        .expect("exact slot is reusable after release");
    assert_eq!(recovered.window_id(), id);
}

#[test]
fn independent_registries_admit_the_same_window_identity() {
    let first = RuntimeBackedWindowProcessRegistry::new();
    let second = RuntimeBackedWindowProcessRegistry::new();
    let id = window_id(1);

    let _first = first.reserve_main_window(id).expect("first registry");
    let _second = second.reserve_main_window(id).expect("second registry");

    assert_eq!(first.main_window_occupancy(), 1);
    assert_eq!(second.main_window_occupancy(), 1);
}

#[test]
fn concurrent_admission_never_duplicates_or_exceeds_capacity() {
    let process = RuntimeBackedWindowProcessRegistry::new();
    let start = Arc::new(Barrier::new(4));
    let workers = (0..4)
        .map(|worker| {
            let process = process.clone();
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                (0..80).fold((Vec::new(), Vec::new()), |mut result, offset| {
                    match process.reserve_main_window(window_id((worker * 80 + offset) as u16)) {
                        Ok(reservation) => result.0.push(reservation),
                        Err(error) => result.1.push(error),
                    }
                    result
                })
            })
        })
        .collect::<Vec<_>>();
    let (reservations, errors) =
        workers
            .into_iter()
            .fold((Vec::new(), Vec::new()), |mut result, worker| {
                let (worker_reservations, worker_errors) = worker.join().expect("capacity worker");
                result.0.extend(worker_reservations);
                result.1.extend(worker_errors);
                result
            });

    assert_eq!(reservations.len(), MAIN_WINDOW_CAPACITY);
    assert_eq!(errors.len(), 64);
    assert!(
        errors
            .iter()
            .all(|error| { *error == RuntimeBackedWindowMainWindowReservationError::Capacity })
    );
    assert_eq!(process.main_window_occupancy(), MAIN_WINDOW_CAPACITY);

    drop(reservations);
    assert_eq!(process.main_window_occupancy(), 0);
}

#[test]
fn retained_reservation_outlives_registry_handles_and_repeated_cycles_release() {
    let process = RuntimeBackedWindowProcessRegistry::new();
    let observer = process.clone();
    let retained = process
        .reserve_main_window(window_id(1))
        .expect("initial reservation");
    drop(process);
    assert_eq!(observer.main_window_occupancy(), 1);
    drop(observer);
    assert_eq!(retained.window_id(), window_id(1));
    drop(retained);

    let process = RuntimeBackedWindowProcessRegistry::new();
    for _ in 0..16 {
        let reservation = process
            .reserve_main_window(window_id(1))
            .expect("repeated reservation");
        assert_eq!(process.main_window_occupancy(), 1);
        drop(reservation);
        assert_eq!(process.main_window_occupancy(), 0);
    }
}
