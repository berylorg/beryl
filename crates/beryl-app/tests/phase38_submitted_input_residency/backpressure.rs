use beryl_app::cas_projection::{
    OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    test_faults::{install_checked_user_publication_barrier, provider_broker_snapshot},
};
use beryl_backend::{TurnStartOptions, UserMessageEchoLifecycle};

use crate::{
    content::{LogicalInput, seed_submitted_input},
    fixture::{CompletedExecution, PreparedExecution, close_execution},
    server::{RawCasServer, ServerScenario, TIMEOUT},
    syndic::Fixture,
    verification::{assert_connection_released, assert_durable_success, assert_three_pass_work},
    wire::RequestOutcome,
};

pub fn run() {
    let mut fixture = Fixture::new(139);
    let thread = fixture.create_ordinary(160);
    let shape = LogicalInput::marker_free(4_096);
    let seeded = seed_submitted_input(&mut fixture, thread, shape, None);
    let server =
        RawCasServer::spawn_scenario(20, seeded.wire, ServerScenario::ObserveTailAfterTerminal);
    let prepared = PreparedExecution::new(&fixture, thread, &server);
    let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let diagnostics = request.input_replay_diagnostics();
    let source_barrier = diagnostics.install_source_page_handoff_barrier(2);
    let mut request_observation = None;

    let CompletedExecution { result, session } = prepared.execute(&fixture, &request, |session| {
        assert!(source_barrier.wait_until_paused(TIMEOUT));
        let blocked_input = diagnostics.snapshot();
        assert!(blocked_input.source_request_count() >= 2);
        assert!(blocked_input.text_page_requests() >= 2);
        server.assert_request_pending();
        assert_eq!(diagnostics.snapshot(), blocked_input);
        source_barrier.release();

        let RequestOutcome::Complete(observation) = server.wait_for_request() else {
            panic!("backpressure request aborted after its source handoff was released")
        };
        request_observation = Some(observation);

        let checked_barrier =
            install_checked_user_publication_barrier(session, UserMessageEchoLifecycle::Completed);
        server.release_lifecycle();
        assert!(checked_barrier.wait_until_paused(TIMEOUT));
        let blocked_broker = provider_broker_snapshot(session);
        assert_eq!(blocked_broker.in_flight().current(), 1);
        assert_eq!(blocked_broker.in_flight().high_water(), 1);
        assert_eq!(blocked_broker.submitted(), 2);
        assert_eq!(blocked_broker.acked(), 1);
        assert_eq!(
            blocked_broker
                .checked_user_publications()
                .activity()
                .current(),
            1
        );
        assert_eq!(blocked_broker.checked_user_publications().publications(), 2);
        let blocked_input = diagnostics.snapshot();

        server.wait_for_tail();
        assert_eq!(provider_broker_snapshot(session), blocked_broker);
        assert_eq!(diagnostics.snapshot(), blocked_input);
        checked_barrier.release();
    });

    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = result.unwrap() else {
        panic!("released backpressure execution did not reach terminal")
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    assert!(request_observation.unwrap().frame_count() > 1);
    let input = diagnostics.snapshot();
    assert_three_pass_work(
        input,
        usize::try_from(seeded.descriptor_count).unwrap(),
        seeded.authored_logical_text_bytes,
    );
    assert_connection_released(&session);
    assert_durable_success(&fixture, thread, seeded.submitted.turn, status);

    drop(projection);
    close_execution(session, server);
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}
