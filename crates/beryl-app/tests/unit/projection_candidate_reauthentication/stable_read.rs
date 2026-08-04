#[test]
fn phase83_two_pass_read_detects_a_concurrent_durable_revision() {
    let mut fixture = Phase83Fixture::new(216, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let home = Arc::clone(&fixture.home);
    let storage = fixture.storage;
    let thread_id = fixture.syndic_thread_id;
    let registry_before = fixture.registry_before.clone();
    let pause = fixture
        .ledger()
        .pause_candidate_before_stable_read_confirmation_for_test(candidate);
    let ledger = fixture.ledger_mut();

    let outcome = std::thread::scope(|scope| {
        let reauthentication =
            scope.spawn(move || ledger.reauthenticate_candidate(candidate));
        pause.wait_until_paused(Duration::from_secs(5));
        let summary = storage
            .history_summary(&home, thread_id, phase83_point_limit())
            .unwrap()
            .unwrap();
        let changed = HistorySummaryRecord::new(
            summary.thread_id(),
            summary.revision().checked_next().unwrap(),
            summary.thread_revision(),
            summary.committed_tail(),
            summary.selected_path_digest(),
            summary.complete(),
            summary.last_activity_at(),
        );
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::HistorySummary(changed))
            .unwrap();
        phase83_execute(
            &home,
            storage.fixture_contribution(storage.revision(&home).unwrap(), batch),
        );
        pause.release();
        reauthentication.join().unwrap().unwrap()
    });

    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::PendingOrdinaryConcurrentChange)
    );
    assert_eq!(fixture.ledger().metadata().terminal_reason(), None);
    phase83_assert_counts(fixture.ledger(), 0, 1, 0, 0);
    assert_eq!(fixture.registry_now(), registry_before);
    fixture.ledger_mut().dispose_candidate(candidate).unwrap();
    drop(fixture.take_ledger().seal().unwrap());
    fixture.close();
}

#[test]
fn phase83_recovered_home_generation_change_after_stable_read_is_terminal() {
    let mut fixture = Phase83Fixture::new(217, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let home = Arc::clone(&fixture.home);
    let faults = fixture.faults.clone();
    let adopted_generation = fixture.recovered_generation;
    let pause = fixture
        .ledger()
        .pause_candidate_after_stable_read_for_test(candidate);
    let ledger = fixture.ledger_mut();

    let outcome = std::thread::scope(|scope| {
        let reauthentication =
            scope.spawn(move || ledger.reauthenticate_candidate(candidate));
        pause.wait_until_paused(Duration::from_secs(5));
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(home.home_revision().is_err());
        faults.fail_next(FaultPoint::BeforeVerification);
        assert!(home.verify_health().is_err());
        assert_eq!(home.health().state(), HomeHealthState::Failed);
        let recovered_generation = home.recover_same_home().unwrap().generation();
        assert_ne!(recovered_generation, adopted_generation);
        pause.release();
        reauthentication.join().unwrap().unwrap()
    });

    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::FinalRecoveredHomeMismatch
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::FinalRecoveredHomeMismatch,
    );
    let terminal = phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::FinalRecoveredHomeMismatch,
    );
    drop(terminal);
    fixture.close();
}

#[test]
fn phase83_reauthentication_source_excludes_external_work_and_preparation_carryover() {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let transaction = std::fs::read_to_string(
        crate_root.join("src/cas_projection/service/adoption/reauthentication/transaction.rs"),
    )
    .unwrap();
    let seal = std::fs::read_to_string(
        crate_root.join("src/cas_projection/service/adoption/reauthentication/seal.rs"),
    )
    .unwrap();
    let model = std::fs::read_to_string(
        crate_root.join("src/cas_projection/service/adoption/reauthentication/model.rs"),
    )
    .unwrap();
    for (name, source) in [("transaction", &transaction), ("seal", &seal)] {
        for forbidden in [
            "ManagedBackend",
            "ConnectionRequestSession",
            "call_ordered",
            "HomeCommand",
            ".execute(",
            "idle_submission_command",
            "publish_valid_binding",
            "activate_binding",
            "register_new",
            "acquire_existing",
        ] {
            assert!(
                !source.contains(forbidden),
                "Phase 83 {name} crossed the no-external-work boundary with {forbidden}"
            );
        }
    }
    for forbidden in [
        "PendingOrdinaryExecution",
        "PreparedAcceptedInputAdmission",
        "PreparedOrdinary",
        "PendingTurnActivation",
    ] {
        assert!(
            !model.contains(forbidden),
            "Phase 83 accepted inventory carried preparation evidence: {forbidden}"
        );
    }
}
