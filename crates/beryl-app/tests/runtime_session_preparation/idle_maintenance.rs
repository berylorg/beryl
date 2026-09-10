use super::support::ready;
use super::*;
use beryl_app::cas_projection::{
    OrdinaryDynamicToolAuthority, OrdinaryDynamicToolHandlers, RuntimeInterestKind,
    ScheduledSessionRegistration,
};

struct NoToolInvocation;

impl OrdinaryDynamicToolAuthority for NoToolInvocation {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        panic!("idle maintenance does not invoke tools")
    }
}

fn idle_passes(fixture: &Fixture) -> u64 {
    fixture
        .service()
        .accepted_input_scheduler_diagnostics()
        .idle_pass_count()
}

fn register(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    root: u8,
) -> ScheduledSessionRegistration {
    let interest = fixture
        .acquire(root, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let session = fixture
        .service()
        .admit_runtime_session(interest, TIMEOUT)
        .unwrap();
    register_admitted(fixture, sessions, root, session)
}

fn register_admitted(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    root: u8,
    session: beryl_app::cas_projection::AdmittedProjectionSession,
) -> ScheduledSessionRegistration {
    sessions
        .register(
            thread_id(root),
            binding(fixture, root),
            session,
            ScheduledOrdinaryRequestPolicy::new(
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                TIMEOUT,
                OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
            ),
            fixture.state.assets(),
            Box::new(NoToolInvocation),
        )
        .unwrap()
}

#[test]
fn pending_work_preserves_the_session_after_its_loaded_projection_releases() {
    use beryl_app::cas_projection::{
        CasProjectionCoordinator, CasProjectionRequest, ProjectionCancellationToken,
    };
    use syndic_storage::{SelectedPathProof, SyndicPointReadLimit};

    let (mut fixture, sessions, _attention) = fixture(8);
    fs::write(fixture.root(1).join("fixture-mode"), "projection-lifetime").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = process(&fixture);
    let interest = fixture
        .acquire(1, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let mut session = fixture
        .service()
        .admit_runtime_session(interest, TIMEOUT)
        .unwrap();
    submission::submit(&fixture, thread_id(1));
    let loaded = {
        let live = fixture.service().live_home_command().unwrap();
        let home = live.home();
        let current = fixture
            .storage
            .thread(
                home,
                thread_id(1),
                SyndicPointReadLimit::new(1_000_000).unwrap(),
            )
            .unwrap()
            .unwrap();
        let request = CasProjectionRequest::new(
            thread_id(1),
            SelectedPathProof::new(
                current.committed_tail(),
                current.revision(),
                current.selected_path_digest(),
            ),
            binding(&fixture, 1),
            ThreadStartOptions::persistent(),
            Some(2_000_000),
            SyndicTimestamp::from_unix_millis(2),
            TIMEOUT,
        );
        CasProjectionCoordinator::for_healthy_home(home)
            .unwrap()
            .obtain_projection(
                home,
                &fixture.storage,
                &mut session,
                &request,
                &ProjectionCancellationToken::new(),
            )
            .unwrap()
    };
    let flight = fixture
        .service()
        .hold_scheduled_flight_for_test(thread_id(1))
        .unwrap();
    register_admitted(&fixture, &sessions, 1, session);
    let original = session_serial(&sessions, 1);
    let before = idle_passes(&fixture);
    drop(view);
    wait_until(|| idle_passes(&fixture) > before);
    assert!(loaded.is_live().unwrap());
    assert!(process.running());
    assert_eq!(session_serial(&sessions, 1), original);
    let before = idle_passes(&fixture);
    loaded.release().unwrap();
    wait_until(|| idle_passes(&fixture) > before);
    assert!(process.running());
    assert_eq!(session_serial(&sessions, 1), original);
    let work = fixture
        .service()
        .required_session_work_for_test(&sessions, &ProjectionCancellationToken::new())
        .unwrap();
    assert!(work[0].2.pending);
    let service = fixture.service.take().unwrap();
    let closed = thread::spawn(move || service.close());
    wait_until(|| sessions.diagnostics().closed);
    drop(flight);
    assert!(matches!(
        closed.join().unwrap().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    process.assert_exited();
    assert_eq!(sessions.diagnostics().retained, 0);
}

fn process(fixture: &Fixture) -> ProcessWitness {
    ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32)
}

fn session_serial(sessions: &ScheduledExecutionSessions, root: u8) -> u64 {
    use beryl_app::cas_projection::{ScheduledSessionWorkError, ScheduledSessionWorkPageLimits};
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let revision = sessions.work_revision().unwrap();
        match sessions.work_page(
            &revision,
            None,
            ScheduledSessionWorkPageLimits::new(16, 65_536).unwrap(),
        ) {
            Ok(page) => {
                return page
                    .records()
                    .iter()
                    .find(|record| record.thread_id() == thread_id(root))
                    .unwrap()
                    .session()
                    .unwrap()
                    .registration_serial();
            }
            Err(ScheduledSessionWorkError::StaleRevision) => {}
            Err(error) => panic!("session observation failed: {error}"),
        }
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn final_view_release_retires_the_idle_session_without_another_request_or_observation() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = process(&fixture);
    let before = idle_passes(&fixture);
    register(&fixture, &sessions, 1);
    wait_until(|| idle_passes(&fixture) > before);
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(process.running());
    drop(view);
    process.assert_exited();
    assert_eq!(sessions.diagnostics().retained, 0);
    close(&mut fixture, &sessions);
}

#[test]
fn another_root_view_preserves_the_process_without_pinning_an_idle_session() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = process(&fixture);
    register(&fixture, &sessions, 2);
    wait_until(|| sessions.diagnostics().retained == 0);
    assert!(process.running());
    assert_eq!(fixture.token_count(), 1);
    drop(view);
    process.assert_exited();
    close(&mut fixture, &sessions);
}

#[test]
fn checkout_and_matching_reattachment_preserve_the_session_until_the_final_release() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = process(&fixture);
    register(&fixture, &sessions, 1);
    let lease = checkout(&fixture, 1);
    let before = idle_passes(&fixture);
    drop(view);
    wait_until(|| idle_passes(&fixture) > before);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert!(process.running());
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(ready(&view), initial);
    let before = idle_passes(&fixture);
    drop(lease);
    wait_until(|| idle_passes(&fixture) > before);
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(process.running());
    drop(view);
    process.assert_exited();
    close(&mut fixture, &sessions);
}

#[test]
fn returning_the_last_checkout_retires_without_a_view_or_later_scheduler_input() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = process(&fixture);
    register(&fixture, &sessions, 1);
    let lease = checkout(&fixture, 1);
    let before = idle_passes(&fixture);
    drop(view);
    wait_until(|| idle_passes(&fixture) > before);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    drop(lease);
    process.assert_exited();
    close(&mut fixture, &sessions);
}

#[test]
fn view_acquisition_after_work_observation_prevents_idle_election() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let process = process(&fixture);
    register(&fixture, &sessions, 1);
    let pause = sessions.install_idle_election_pause_for_test(thread_id(1));
    let before = idle_passes(&fixture);
    drop(view);
    pause.wait(TIMEOUT);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    assert_eq!(ready(&view), initial);
    pause.release();
    wait_until(|| idle_passes(&fixture) > before);
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(process.running());
    drop(view);
    process.assert_exited();
    close(&mut fixture, &sessions);
}

#[test]
fn required_work_admitted_after_idle_observation_preserves_its_registered_session() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = process(&fixture);
    register(&fixture, &sessions, 1);
    let original = session_serial(&sessions, 1);
    let pause = sessions.install_idle_election_pause_for_test(thread_id(1));
    let before = idle_passes(&fixture);
    drop(view);
    pause.wait(TIMEOUT);
    submission::submit(&fixture, thread_id(1));
    let work = fixture
        .service()
        .required_session_work_for_test(
            &sessions,
            &beryl_app::cas_projection::ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(work.len(), 1);
    assert!(work[0].2.pending);
    pause.release();
    wait_until(|| idle_passes(&fixture) > before);
    let diagnostics = sessions.diagnostics();
    assert_eq!(
        diagnostics.available + diagnostics.checked_out,
        1,
        "{diagnostics:?}"
    );
    assert!(process.running());
    assert_eq!(session_serial(&sessions, 1), original);
    close(&mut fixture, &sessions);
}

#[test]
fn required_work_admitted_after_idle_retirement_prepares_a_fresh_session() {
    let (mut fixture, sessions, _attention) = fixture(8);
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let retired_process = process(&fixture);
    register(&fixture, &sessions, 1);
    let original = session_serial(&sessions, 1);
    drop(view);
    retired_process.assert_exited();
    assert_eq!(sessions.diagnostics().retained, 0);

    fs::write(fixture.root(1).join("fixture-mode"), "pause-projection").unwrap();
    submission::submit(&fixture, thread_id(1));
    begin(&fixture, 1);
    wait_until(|| {
        fixture
            .root(1)
            .join("runtime-projection-evidence.json")
            .exists()
    });
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert_ne!(session_serial(&sessions, 1), original);
    let current_process = process(&fixture);
    assert!(current_process.running());
    let service = fixture.service.take().unwrap();
    let closed = thread::spawn(move || service.close());
    wait_until(|| sessions.diagnostics().closed);
    fs::write(fixture.root(1).join("release-projection"), "complete").unwrap();
    assert!(matches!(
        closed.join().unwrap().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(sessions.diagnostics().retained, 0);
    current_process.assert_exited();
    assert_eq!(fixture.token_count(), 0);
}
