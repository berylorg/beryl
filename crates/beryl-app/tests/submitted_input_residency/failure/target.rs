use beryl_app::cas_projection::{
    LiveEventConnectionState, LiveEventTargetCloseReason, OrdinaryTurnCaptureLoss,
    OrdinaryTurnExecutionOutcome,
};
use beryl_backend::TurnStartOptions;

use crate::{
    content::{LogicalInput, seed_submitted_input},
    fixture::{CompletedExecution, PreparedExecution, close_execution},
    server::{RawCasServer, ServerScenario, TIMEOUT},
    syndic::Fixture,
    wire::RequestOutcome,
};

use super::common;

pub fn exact_target_abandonment() {
    let mut fixture = Fixture::new(147);
    let thread = fixture.thread;
    let seeded = seed_submitted_input(&mut fixture, thread, LogicalInput::marker_free(4_096), None);
    let server =
        RawCasServer::spawn_scenario(37, seeded.wire, ServerScenario::HoldOpenAfterResponse);
    let prepared = PreparedExecution::new(&fixture, thread, &server);
    let abandonment = prepared.install_target_abandonment(thread);
    let request = beryl_app::cas_projection::OrdinaryTurnExecutionRequest::new(
        TurnStartOptions::default(),
        TIMEOUT,
    );
    let CompletedExecution { result, session } = prepared.execute(&fixture, &request, |session| {
        assert!(abandonment.wait_until_abandoned(TIMEOUT));
        let registered = session.live_event_snapshot().unwrap();
        assert_eq!(registered.state(), LiveEventConnectionState::Active);
        assert_eq!(registered.target_count(), 1);
        assert_eq!(registered.retired_thread_lane_count(), 0);
        let RequestOutcome::Complete(_) = server.wait_for_request() else {
            panic!("target-abandonment request aborted with a healthy transport")
        };
        server.release_lifecycle();
        server.wait_for_response();
    });
    drop(abandonment);

    let OrdinaryTurnExecutionOutcome::Incomplete {
        reason: OrdinaryTurnCaptureLoss::TargetClosed(LiveEventTargetCloseReason::WorkerStopped),
    } = result.unwrap()
    else {
        panic!("exact target abandonment lost its target-local taxonomy")
    };
    let converged = session.live_event_snapshot().unwrap();
    assert_eq!(converged.state(), LiveEventConnectionState::Active);
    assert_eq!(converged.target_count(), 0);
    assert_eq!(converged.retired_thread_lane_count(), 1);
    common::assert_durable_stream_loss(&fixture, thread, seeded.submitted.turn, 4);
    common::assert_released(&session);
    close_execution(session, server);
    common::finish_fixture(fixture);
}
