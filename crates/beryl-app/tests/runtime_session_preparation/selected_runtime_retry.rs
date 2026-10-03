use super::*;
use beryl_app::cas_projection::{
    ProjectionCancellationToken, RuntimeInterestKind, SelectedRuntimeRetryError,
};

fn failed_idle_runtime(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
) -> beryl_app::cas_projection::RuntimeFailureSnapshot {
    fs::write(fixture.root(1).join("fixture-mode"), "reject-config").unwrap();
    let interest = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    wait_until(|| {
        sessions
            .runtime_failure(binding(fixture, 1).runtime_id())
            .is_some_and(|failure| failure.retry_ready())
    });
    let failure = sessions
        .runtime_failure(binding(fixture, 1).runtime_id())
        .unwrap();
    drop(interest);
    fs::write(fixture.root(1).join("fixture-mode"), "projection-lifetime").unwrap();
    failure
}

#[test]
fn selected_retry_retains_a_usable_projection_without_input_and_fences_later_mutation() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let (window, claim) = runtime_notice::select(&fixture);
    let session = fixture.state.session();
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let failure = failed_idle_runtime(&fixture, &sessions);
    let execution = binding(&fixture, 1);
    let before = fixture
        .storage
        .input_gate(
            &fixture.home_reference,
            thread_id(1),
            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap();
    assert!(worker.eligible(&session, window, claim, &execution, failure));
    let proof = recovery::recover_with_idle_election_paused(
        &fixture, &sessions, window, claim, failure, &execution,
    );
    let fence = fixture.process_admission.test_fence().unwrap();
    assert!(!worker.usability_current(&session, &proof));
    fence.try_reopen(true).unwrap();
    let after = fixture
        .storage
        .input_gate(
            &fixture.home_reference,
            thread_id(1),
            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(sessions.diagnostics().available, 1);
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let bootstrap = session.minimal_bootstrap(home).unwrap().unwrap();
    let replacement = binding(&fixture, 2);
    notice_shell::commit(
        home,
        session.replace_claim(
            session.revision(home).unwrap(),
            beryl_state::ReplaceWindowClaim::new(
                bootstrap.header().revision(),
                window,
                bootstrap.windows()[0].revision(),
                Some(claim),
                beryl_state::RememberedTarget::new(replacement.runtime_id(), replacement.root_id()),
                thread_id(2),
            ),
        ),
    );
    drop(live);
    assert!(!worker.usability_current(&session, &proof));
    assert!(!worker.eligible(&session, window, claim, &execution, failure));
    close(&mut fixture, &sessions);
    assert!(!worker.usability_current(&session, &proof));
}

#[test]
fn cancelled_or_foreign_selected_retry_admits_no_projection_or_input() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let (window, claim) = runtime_notice::select(&fixture);
    let session = fixture.state.session();
    let worker = fixture.service().selected_runtime_retry_worker(&sessions);
    let failure = failed_idle_runtime(&fixture, &sessions);
    let cancel = ProjectionCancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        worker.recover(
            &session,
            window,
            claim,
            &binding(&fixture, 1),
            failure,
            &cancel
        ),
        Err(SelectedRuntimeRetryError::Cancelled)
    ));
    assert!(!worker.eligible(
        &session,
        beryl_model::WindowId::from_bytes([201; 16]),
        claim,
        &binding(&fixture, 1),
        failure
    ));
    assert!(!worker.eligible(&session, window, claim, &binding(&fixture, 2), failure));
    assert_eq!(sessions.diagnostics().retained, 0);
    let fence = fixture.process_admission.test_fence().unwrap();
    assert!(!worker.eligible(&session, window, claim, &binding(&fixture, 1), failure));
    assert!(matches!(
        worker.recover(
            &session,
            window,
            claim,
            &binding(&fixture, 1),
            failure,
            &ProjectionCancellationToken::new()
        ),
        Err(SelectedRuntimeRetryError::Unavailable)
    ));
    fence.try_reopen(true).unwrap();
    close(&mut fixture, &sessions);
}
