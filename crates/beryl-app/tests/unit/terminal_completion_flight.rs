use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

#[test]
fn observers_preserve_exact_identity_without_retaining_flight_exclusivity_or_history() {
    let fixture = Fixture::idle();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let flight = coordinator.begin_projection(fixture.thread).unwrap();
    let old_revision = FlightRegistry::work_revision().unwrap();
    let publisher = flight.bind_terminal_completion(fixture.turn).unwrap();
    assert!(
        FlightRegistry::terminal_completion(
            coordinator.home_id(),
            coordinator.home_generation(),
            fixture.thread,
            old_revision
        )
        .is_err()
    );
    let observer = FlightRegistry::terminal_completion(
        coordinator.home_id(),
        coordinator.home_generation(),
        fixture.thread,
        FlightRegistry::work_revision().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert!(observer.matches(
        coordinator.home_id(),
        coordinator.home_generation(),
        fixture.thread,
        fixture.turn
    ));
    assert!(!observer.matches(
        BerylHomeId::from_bytes([198; 16]),
        coordinator.home_generation(),
        fixture.thread,
        fixture.turn,
    ));
    assert!(!observer.matches(
        coordinator.home_id(),
        coordinator.home_generation(),
        fixture.thread,
        SyndicTurnId::from_bytes([121; 16])
    ));
    assert!(!observer.matches(
        coordinator.home_id(),
        coordinator.home_generation(),
        SyndicThreadId::from_bytes([122; 16]),
        fixture.turn
    ));
    assert_eq!(observer.completion(), None);
    let weak = Arc::downgrade(&observer.0);
    drop(publisher);
    drop(flight);
    let replacement = coordinator.begin_projection(fixture.thread).unwrap();
    let successor = SyndicTurnId::from_bytes([123; 16]);
    let replacement_publisher = replacement.bind_terminal_completion(successor).unwrap();
    let replacement_observer = FlightRegistry::terminal_completion(
        coordinator.home_id(),
        coordinator.home_generation(),
        fixture.thread,
        FlightRegistry::work_revision().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_ne!(observer, replacement_observer);
    assert_eq!(observer.turn_id(), fixture.turn);
    assert_eq!(replacement_observer.turn_id(), successor);
    drop(observer);
    assert!(weak.upgrade().is_none());
    drop(replacement_publisher);
    drop(replacement);
    assert!(
        FlightRegistry::terminal_completion(
            coordinator.home_id(),
            coordinator.home_generation(),
            fixture.thread,
            FlightRegistry::work_revision().unwrap()
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn old_flight_observer_rejects_recovered_generation_of_the_same_home() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};
    use beryl_state::{
        ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
    };
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&home).unwrap();
    let thread = SyndicThreadId::from_bytes([193; 16]);
    let turn = SyndicTurnId::from_bytes([194; 16]);
    let flight = coordinator.begin_projection(thread).unwrap();
    let publisher = flight.bind_terminal_completion(turn).unwrap();
    let observer = FlightRegistry::terminal_completion(
        coordinator.home_id(),
        coordinator.home_generation(),
        thread,
        FlightRegistry::work_revision().unwrap(),
    )
    .unwrap()
    .unwrap();
    drop(publisher);
    drop(flight);
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions("generation recovery evidence").unwrap(),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(state.settings().apply(
            state.settings().revision(&home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    faults.panic_next(FaultPoint::BeforeCommit);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| home.execute(command))).is_err()
    );
    drop(state);
    let candidate = home.recover_same_home().unwrap();
    assert_eq!(candidate.home_id(), coordinator.home_id());
    assert_ne!(candidate.generation(), coordinator.home_generation());
    assert!(!observer.matches(candidate.home_id(), candidate.generation(), thread, turn));
    assert!(observer.matches(
        coordinator.home_id(),
        coordinator.home_generation(),
        thread,
        turn
    ));
    candidate.abort().close().unwrap();
}
