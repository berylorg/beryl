use super::*;
use beryl_app::cas_projection::{RuntimeInterestKind, SubmissionUnavailable};
use syndic_storage::{BindingState, FirstAcceptanceKind, TurnLifecycle};

fn binding_revision(fixture: &Fixture) -> beryl_model::BindingRevision {
    fixture
        .storage
        .current_binding(
            &fixture.home_reference,
            thread_id(1),
            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .binding()
        .revision()
}

fn assert_resumed_input(
    fixture: &Fixture,
    first: &serde_json::Value,
    second: &serde_json::Value,
    text: &str,
    previous: beryl_model::BindingRevision,
) {
    assert!(fixture.root(1).join("execution-unsubscribed-0").exists());
    let resume: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root(1).join("execution-projection-1.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(resume["method"], "thread/resume");
    assert_eq!(resume["params"]["threadId"], first["thread"]);
    let start: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root(1).join("execution-request-1.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(start["method"], "turn/start");
    assert_eq!(start["params"]["threadId"], first["thread"]);
    assert_eq!(start["params"]["input"].as_array().unwrap().len(), 1);
    assert_eq!(start["params"]["input"][0]["text"], text);
    assert_eq!(second["thread"], first["thread"]);
    assert_ne!(second["turn"], first["turn"]);
    let binding = fixture
        .storage
        .current_binding(
            &fixture.home_reference,
            thread_id(1),
            syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(binding.binding().revision() > previous);
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("resumed ordinary turn must own its newly published active binding");
    };
    assert_eq!(
        active.usable().cas_thread_id().as_str(),
        first["thread"].as_str().unwrap()
    );
}

fn completed_idle(fixture: &Fixture, sessions: &ScheduledExecutionSessions) {
    execution_lifetime::turn(fixture, TurnLifecycle::Complete);
    wait_until(|| {
        sessions.diagnostics().available == 1
            && sessions.diagnostics().checked_out == 0
            && fixture
                .service()
                .exact_stop_worker()
                .selected_submission_availability(thread_id(1))
                == Ok(())
    });
}

#[test]
fn terminal_projection_supports_a_later_idle_submission_and_final_view_disposal() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-next").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let ready = support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let first = execution_lifetime::started(&fixture, &sessions, 0);
    let process = ProcessWitness::open(first["pid"].as_u64().unwrap() as u32);
    execution_lifetime::turn(&fixture, TurnLifecycle::Active);
    execution_lifetime::release(&fixture, 0);
    completed_idle(&fixture, &sessions);
    assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());

    let checkout = checkout(&fixture, 1);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert_eq!(
        fixture
            .service()
            .exact_stop_worker()
            .selected_submission_availability(thread_id(1)),
        Ok(())
    );
    drop(checkout);
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());
    let previous = binding_revision(&fixture);
    assert!(matches!(
        submission::submit_text(
            &fixture,
            thread_id(1),
            "after the original turn became idle",
            190,
            SyndicTimestamp::from_unix_millis(37_004),
        ),
        FirstAcceptanceKind::Idle { .. }
    ));
    let second = execution_lifetime::started(&fixture, &sessions, 1);
    assert_eq!(second["thread"], first["thread"]);
    assert_eq!(second["pid"], first["pid"]);
    assert_eq!(second["text"], "after the original turn became idle");
    execution_lifetime::turn(&fixture, TurnLifecycle::Active);
    assert_resumed_input(
        &fixture,
        &first,
        &second,
        "after the original turn became idle",
        previous,
    );
    execution_lifetime::release(&fixture, 1);
    completed_idle(&fixture, &sessions);
    assert_eq!(support::ready(&view), ready);
    drop(view);
    process.assert_exited();
    assert!(fixture.root(1).join("execution-unsubscribed-1").exists());
    assert!(!fixture.root(1).join("execution-started-2.json").exists());
    wait_until(|| {
        sessions.diagnostics().retained == 0
            && fixture.token_count() == 0
            && fixture.service().worker_pool_diagnostics().active() == 0
    });
    assert_eq!(
        fixture
            .service()
            .exact_stop_worker()
            .selected_submission_availability(thread_id(1)),
        Err(SubmissionUnavailable::BindingUnavailable)
    );
    close(&mut fixture, &sessions);
}

#[test]
fn a_view_winning_idle_projection_election_preserves_the_original_submission_lease() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-lifetime").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let ready = support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let first = execution_lifetime::started(&fixture, &sessions, 0);
    let process = ProcessWitness::open(first["pid"].as_u64().unwrap() as u32);
    execution_lifetime::turn(&fixture, TurnLifecycle::Active);
    execution_lifetime::release(&fixture, 0);
    completed_idle(&fixture, &sessions);
    let pause = sessions.install_idle_election_pause_for_test(thread_id(1));
    let before = fixture
        .service()
        .accepted_input_scheduler_diagnostics()
        .idle_pass_count();
    drop(view);
    pause.wait(TIMEOUT);
    let replacement = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(support::ready(&replacement), ready);
    pause.release();
    wait_until(|| {
        fixture
            .service()
            .accepted_input_scheduler_diagnostics()
            .idle_pass_count()
            > before
    });
    completed_idle(&fixture, &sessions);
    assert!(process.running());
    assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());
    drop(replacement);
    process.assert_exited();
    assert!(fixture.root(1).join("execution-unsubscribed-0").exists());
    wait_until(|| sessions.diagnostics().retained == 0 && fixture.token_count() == 0);
    close(&mut fixture, &sessions);
}

#[test]
fn pending_input_after_idle_observation_preserves_the_session_and_resumes_the_exact_source() {
    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "execution-next").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    support::ready(&view);
    submission::submit(&fixture, thread_id(1));
    let first = execution_lifetime::started(&fixture, &sessions, 0);
    let process = ProcessWitness::open(first["pid"].as_u64().unwrap() as u32);
    execution_lifetime::turn(&fixture, TurnLifecycle::Active);
    execution_lifetime::release(&fixture, 0);
    completed_idle(&fixture, &sessions);
    let previous = binding_revision(&fixture);
    let pause = sessions.install_idle_election_pause_for_test(thread_id(1));
    drop(view);
    pause.wait(TIMEOUT);
    assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());
    assert!(matches!(
        submission::submit_text(
            &fixture,
            thread_id(1),
            "retain the exact target for new durable work",
            210,
            SyndicTimestamp::from_unix_millis(37_004),
        ),
        FirstAcceptanceKind::Idle { .. }
    ));
    pause.release();
    let second = execution_lifetime::started(&fixture, &sessions, 1);
    assert_eq!(second["thread"], first["thread"]);
    assert_eq!(
        second["text"],
        "retain the exact target for new durable work"
    );
    execution_lifetime::turn(&fixture, TurnLifecycle::Active);
    assert_resumed_input(
        &fixture,
        &first,
        &second,
        "retain the exact target for new durable work",
        previous,
    );
    execution_lifetime::release(&fixture, 1);
    process.assert_exited();
    assert!(fixture.root(1).join("execution-unsubscribed-1").exists());
    assert!(!fixture.root(1).join("execution-started-2.json").exists());
    wait_until(|| sessions.diagnostics().retained == 0 && fixture.token_count() == 0);
    close(&mut fixture, &sessions);
}
