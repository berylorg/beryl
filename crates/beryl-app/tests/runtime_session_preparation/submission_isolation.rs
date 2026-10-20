use super::*;
use beryl_app::cas_projection::{
    RuntimeFailure, SelectedRuntimeFailureObservation, SubmissionUnavailable,
};
use syndic_storage::SyndicPointReadLimit;

fn commit(home: &beryl_home_store::HomeStore, mutation: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(mutation).unwrap();
    let outcome = home.execute(command);
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

#[test]
fn unrelated_managed_runtime_failure_preserves_selected_submission_authority() {
    let (mut fixture, sessions, _attention) =
        fixture_with_runtime_capacity(6, std::num::NonZeroUsize::new(2).unwrap());
    eprintln!(
        "owned submission-isolation fixture: {}",
        fixture.root(1).parent().unwrap().display()
    );
    let (window, claim) = runtime_notice::select(&fixture);
    let other_runtime = RuntimeId::from_bytes([88; 16]);
    let other_root = RootId::from_bytes([88; 16]);
    let other_thread = SyndicThreadId::from_bytes([88; 16]);
    let executable_path = fixture.root(2).join("submission-runtime-fixture.exe");
    fs::copy(
        env!("CARGO_BIN_EXE_managed-runtime-fixture"),
        &executable_path,
    )
    .unwrap();
    let executable = canonical_path(&executable_path);
    let root_path = canonical_path(&fixture.root(2));
    let other = ExecutionBinding::new(other_runtime, other_root, native(&root_path));
    {
        let live = fixture.service().live_home_command().unwrap();
        let home = live.home();
        commit(
            home,
            fixture.state.runtime_roots().create_runtime_with_home_root(
                fixture.state.runtime_roots().revision(home).unwrap(),
                CreateRuntimeWithHomeRoot::new(
                    RuntimeRegistration::new(
                        other_runtime,
                        host(&executable),
                        RuntimeMode::Host,
                        beryl_model::RuntimeLaunchForm::CodexCli,
                        native(&executable),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::unknown(),
                    )
                    .unwrap(),
                    RootRegistration::new(
                        other_root,
                        native(&root_path),
                        host(&root_path),
                        UnixMillis::new(1),
                        AvailabilitySnapshot::unknown(),
                    ),
                )
                .unwrap(),
            ),
        );
        commit(
            home,
            fixture.storage.create_thread(
                fixture.storage.revision(home).unwrap(),
                CreateThread::ordinary(
                    other_thread,
                    SyndicDraftId::from_bytes([89; 16]),
                    other.clone(),
                    SyndicTimestamp::from_unix_millis(1),
                    DraftEditHistoryPolicyV1::new(64 * 1024 * 1024, 1).unwrap(),
                ),
            ),
        );
    }
    let selected = thread_id(1);
    let worker = fixture.service().exact_stop_worker();
    wait_until(|| {
        worker.selected_submission_availability(selected) == Ok(())
            && worker.selected_submission_availability(other_thread) == Ok(())
    });
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let before = {
        let home = &fixture.home_reference;
        (
            fixture
                .storage
                .thread(home, selected, limit)
                .unwrap()
                .unwrap(),
            fixture
                .storage
                .input_gate(home, selected, limit)
                .unwrap()
                .unwrap(),
            fixture
                .storage
                .current_binding(home, selected, limit)
                .unwrap(),
        )
    };
    fs::write(fixture.root(2).join("fixture-mode"), "reject-config").unwrap();
    assert!(matches!(
        fixture
            .service()
            .checkout_scheduled_session_for_test(other_thread, other.clone())
            .unwrap(),
        ScheduledOrdinaryAdmissionResult::Unavailable(_)
    ));
    wait_until(|| {
        sessions
            .runtime_failure(other_runtime)
            .is_some_and(|failure| failure.retry_ready())
    });
    let failure = sessions.runtime_failure(other_runtime).unwrap();
    assert_eq!(failure.failure(), RuntimeFailure::Admission);
    assert!(
        sessions
            .runtime_failure(binding(&fixture, 1).runtime_id())
            .is_none()
    );
    assert_eq!(
        worker.selected_submission_availability(other_thread),
        Err(SubmissionUnavailable::RuntimeUnavailable)
    );
    wait_until(|| worker.selected_submission_availability(selected) == Ok(()));
    assert_eq!(
        worker
            .runtime_failure_reader()
            .observe(&fixture.state.session(), window, claim),
        SelectedRuntimeFailureObservation::Unknown
    );
    {
        let home = &fixture.home_reference;
        assert_eq!(
            fixture
                .storage
                .thread(home, selected, limit)
                .unwrap()
                .unwrap(),
            before.0
        );
        assert_eq!(
            fixture
                .storage
                .input_gate(home, selected, limit)
                .unwrap()
                .unwrap(),
            before.1
        );
        assert_eq!(
            fixture
                .storage
                .current_binding(home, selected, limit)
                .unwrap(),
            before.2
        );
        assert_eq!(
            fixture
                .state
                .session()
                .minimal_bootstrap(home)
                .unwrap()
                .unwrap()
                .windows()[0]
                .selected_thread(),
            Some(claim)
        );
    }
    close(&mut fixture, &sessions);
    assert_eq!(
        worker.selected_submission_availability(selected),
        Err(SubmissionUnavailable::AuthorityUnavailable)
    );
}
