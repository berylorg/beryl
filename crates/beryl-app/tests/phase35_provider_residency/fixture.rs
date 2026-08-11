use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LiveEventPoll,
    LiveEventTarget,
    test_faults::{
        WebSocketIngressSnapshot, install_provider_fragment_stage_barrier,
        last_websocket_ingress_snapshot, provider_broker_snapshot,
    },
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions};
use beryl_home_store::{CommandOutcome, HomeHealthState, ReadError, test_faults::FaultController};
use beryl_model::{
    CasProcessGeneration, CasTurnId, SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ActivateBinding, CasTurnSource, LiveSourceEvent, PublishActiveCasTurn, SourceEventPayload,
    SourceEventSequence, SyndicReadError, SyndicTimestamp,
};

use super::{
    EXECUTION_ROOT,
    server::{AUTHORIZATION, CAS_THREAD_ID, CAS_TURN_ID, ObservationSpec, ProviderServer, TIMEOUT},
    syndic::{Fixture, SubmittedTurn, execution_binding, point_limit},
    verification::{assert_atomic_frontier, assert_item_absent, assert_item_digest},
};

const PUBLICATION_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) struct LiveHarness {
    fixture: Option<Fixture>,
    session: Option<AdmittedProjectionSession>,
    target: Option<LiveEventTarget>,
    server: Option<ProviderServer>,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: CasTurnSource,
}

impl LiveHarness {
    pub(super) fn new(seed: u8) -> Self {
        Self::from_fixture(Fixture::new(seed), seed)
    }

    pub(super) fn with_faults(seed: u8, faults: FaultController) -> Self {
        Self::from_fixture(Fixture::with_faults(seed, faults), seed)
    }

    fn from_fixture(mut fixture: Fixture, seed: u8) -> Self {
        let submitted = fixture.submit_text("phase35 bounded provider target");
        let server = ProviderServer::spawn();
        let connector =
            ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
        let process_generation = CasProcessGeneration::new(35_000 + u64::from(seed)).unwrap();
        let mut session = fixture
            .store
            .admit_lifecycle_test_candidate(
                &connector,
                execution_binding().runtime_id(),
                process_generation,
                Path::new(EXECUTION_ROOT),
                TIMEOUT,
            )
            .unwrap();
        let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
        let request = CasProjectionRequest::new(
            fixture.thread,
            fixture.selected_path(fixture.thread),
            execution_binding(),
            ThreadStartOptions::persistent(),
            Some(2_000_000),
            SyndicTimestamp::from_unix_millis(35_000),
            TIMEOUT,
        );
        let projection = coordinator
            .obtain_projection(
                &fixture.store,
                fixture.storage,
                &mut session,
                &request,
                &fixture.cancellation,
            )
            .unwrap();
        server.wait_for_projection();
        assert_eq!(projection.cas_thread_id().as_str(), CAS_THREAD_ID);
        let source = activate_projection(&fixture, submitted, &projection);
        let target = projection
            .into_active_live_event_target(source.turn_id().clone())
            .unwrap();
        Self {
            thread: fixture.thread,
            turn: submitted.turn,
            fixture: Some(fixture),
            session: Some(session),
            target: Some(target),
            server: Some(server),
            source,
        }
    }

    pub(super) fn store(&self) -> &beryl_home_store::HomeStore {
        &self.fixture.as_ref().unwrap().store
    }

    pub(super) fn storage(&self) -> syndic_storage::SyndicStorage {
        self.fixture.as_ref().unwrap().storage
    }

    pub(super) fn session(&self) -> &AdmittedProjectionSession {
        self.session.as_ref().unwrap()
    }

    pub(super) fn server(&self) -> &ProviderServer {
        self.server.as_ref().unwrap()
    }

    pub(super) fn send(
        &self,
        spec: ObservationSpec,
        expected_frontier: u64,
    ) -> super::server::ObservationReport {
        let settlement = self.next_provider_seal_ack();
        let report = self.server().send_observation(spec);
        assert_eq!(report.semantic_bytes, spec.semantic_bytes());
        assert!(report.wire_bytes > report.semantic_bytes);
        assert!(report.frame_count > 1);
        self.wait_for_provider_seal_ack(settlement);
        self.wait_for_frontier(expected_frontier);
        report
    }

    pub(super) fn wait_for_target_closed(&self) {
        match self.target.as_ref().unwrap().poll(TIMEOUT) {
            LiveEventPoll::Closed(_) => {}
            other => panic!("provider failure left the live target open: {other:?}"),
        }
    }

    pub(super) fn wait_for_page_leases(&self, expected: usize) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let diagnostics = self.session().provider_page_diagnostics();
            if diagnostics.leased == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "provider page leases stayed at {}, expected {expected}",
                diagnostics.leased
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn wait_for_broker_idle(&self) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let snapshot = provider_broker_snapshot(self.session());
            if snapshot.in_flight().current() == 0
                && snapshot.staged_fragments().current() == 0
                && snapshot.submitted() == snapshot.acked()
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "provider broker did not release its acknowledgement path: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn next_provider_seal_ack(&self) -> usize {
        provider_broker_snapshot(self.session())
            .provider_seal_acks()
            .checked_add(1)
            .expect("provider seal acknowledgement target overflowed")
    }

    pub(super) fn wait_for_provider_seal_ack(&self, expected: usize) {
        let deadline = Instant::now() + PUBLICATION_TIMEOUT;
        loop {
            let snapshot = provider_broker_snapshot(self.session());
            assert!(
                snapshot.provider_seal_acks() <= expected,
                "provider broker passed the expected seal acknowledgement: {snapshot:?}"
            );
            if snapshot.provider_seal_acks() == expected
                && snapshot.in_flight().current() == 0
                && snapshot.staged_fragments().current() == 0
                && snapshot.submitted() == snapshot.acked()
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "provider broker did not settle seal acknowledgement {expected}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn assert_unpublished(&self, sequence: u64) {
        assert_atomic_frontier(self.store(), self.storage(), self.thread, self.turn, 0);
        assert_item_absent(self.store(), self.storage(), &self.source, sequence);
    }

    pub(super) fn abandon_target(&mut self) {
        self.target.take();
    }

    pub(super) fn assert_frontier(&self, expected: u64) {
        assert_atomic_frontier(
            self.store(),
            self.storage(),
            self.thread,
            self.turn,
            expected,
        );
    }

    pub(super) fn wait_for_frontier(&self, expected: u64) {
        let expected = expected.checked_add(1).unwrap();
        let deadline = Instant::now() + PUBLICATION_TIMEOUT;
        loop {
            let actual = match self
                .storage()
                .turn_state(self.store(), self.turn, point_limit())
            {
                Ok(Some(state)) => state.source_event_count(),
                Ok(None) => panic!("active turn disappeared while waiting for source frontier"),
                Err(SyndicReadError::Read(ReadError::HealthGate(error)))
                    if error.state() == HomeHealthState::Verifying && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => {
                    panic!("source frontier read failed while waiting for {expected}: {error:?}")
                }
            };
            if actual == expected {
                return;
            }
            if Instant::now() >= deadline {
                let target = self
                    .target
                    .as_ref()
                    .map(|target| target.poll(Duration::ZERO));
                panic!(
                    "source frontier stayed at {actual}, expected {expected}; pages={:?}; broker={:?}; ingress={:?}; target={target:?}",
                    self.session().provider_page_diagnostics(),
                    provider_broker_snapshot(self.session()),
                    last_websocket_ingress_snapshot(self.session())
                );
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn assert_digest(&self, spec: ObservationSpec) {
        assert_item_digest(self.store(), self.storage(), &self.source, spec);
    }

    pub(super) fn close(mut self) {
        self.target.take();
        self.server().request_close();
        self.session().invalidate_connection();
        self.session.take();
        self.server.take().unwrap().join();
        let (directory, service) = self.fixture.take().unwrap().into_service();
        service.close().unwrap();
        drop(directory);
    }
}

pub(super) fn ingress_snapshot(
    harness: &LiveHarness,
    report: super::server::ObservationReport,
) -> WebSocketIngressSnapshot {
    let snapshot = last_websocket_ingress_snapshot(harness.session())
        .unwrap()
        .expect("a completed provider message records ingress diagnostics");
    assert_eq!(
        snapshot.message_bytes(),
        usize::try_from(report.wire_bytes).unwrap()
    );
    assert!((1..=8 * 1_024).contains(&snapshot.maximum_transport_chunk_bytes()));
    assert!((1..=8 * 1_024).contains(&snapshot.maximum_parser_buffer_bytes()));
    assert_eq!(snapshot.discarded_image_result_bytes(), 0);
    assert!(!snapshot.retained_item_result_present());
    snapshot
}

pub(super) fn assert_broker_idle_and_bounded(
    snapshot: beryl_app::cas_projection::test_faults::ProviderBrokerSnapshot,
) {
    assert_eq!(snapshot.in_flight().current(), 0);
    assert_eq!(snapshot.in_flight().high_water(), 1);
    assert_eq!(snapshot.submitted(), snapshot.acked());
    assert_eq!(snapshot.staged_fragments().current(), 0);
    assert_eq!(snapshot.staged_fragments().high_water(), 1);
    assert!(snapshot.staged_fragment_batches() > 0);
}

fn activate_projection(
    fixture: &Fixture,
    submitted: SubmittedTurn,
    projection: &beryl_app::cas_projection::LoadedCasProjection,
) -> CasTurnSource {
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = fixture.selected_path(fixture.thread);
    let snapshot = SyndicExecutionSnapshotId::from_bytes(*submitted.turn.as_bytes());
    let started_at = SyndicTimestamp::from_unix_millis(35_001);
    let outcome = fixture.store.execute_current(
            fixture
                .storage
                .current_activate_binding(ActivateBinding::new(
                    fixture.thread,
                    binding.binding().revision(),
                    gate.revision(),
                    selected,
                    snapshot,
                    submitted.turn,
                    projection.loaded_session_generation(),
                    started_at,
                )),
        );
    match outcome {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => panic!("expected committed binding activation, got {outcome:?}"),
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("expected committed binding activation, got {outcome:?}"),
    }
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let cas_turn = CasTurnId::new(CAS_TURN_ID).unwrap();
    let outcome = fixture.store.execute_current(fixture.storage.current_publish_active_cas_turn(
            PublishActiveCasTurn::new(
                fixture.thread,
                binding.binding().revision(),
                gate.revision(),
                snapshot,
                projection.cas_thread_id().clone(),
                cas_turn.clone(),
                started_at,
            ),
        ));
    match outcome {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => panic!("expected committed CAS publication, got {outcome:?}"),
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("expected committed CAS publication, got {outcome:?}"),
    }
    let source = CasTurnSource::new(projection.cas_thread_id().clone(), cas_turn);
    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let activation = LiveSourceEvent::new(
        fixture.thread,
        submitted.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(1).unwrap(),
        Some(source.clone()),
        SourceEventPayload::TurnActivated,
        started_at,
    )
    .unwrap();
    let outcome = fixture
        .store
        .execute_current(fixture.storage.current_admit_live_source_event(activation));
    match outcome {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => panic!("expected committed live event, got {outcome:?}"),
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("expected committed live event, got {outcome:?}"),
    }
    source
}

pub fn prove_transport_backpressure_and_cancellation() {
    const BACKPRESSURE_PATTERNS: u64 = 40_000;

    let harness = LiveHarness::new(102);
    let spec = ObservationSpec::new(1, BACKPRESSURE_PATTERNS);
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness.server().begin_backpressure(spec);
    barrier.wait_for_stage();

    let pages = harness.session().provider_page_diagnostics();
    assert_eq!(pages.leased, 1);
    assert_eq!(pages.high_water, 1);
    let blocked_broker = provider_broker_snapshot(harness.session());
    assert_eq!(blocked_broker.in_flight().current(), 1);
    assert_eq!(blocked_broker.in_flight().high_water(), 1);
    assert_eq!(blocked_broker.staged_fragments().current(), 1);
    assert_eq!(blocked_broker.staged_fragments().high_water(), 1);
    harness.assert_unpublished(spec.sequence);

    harness.server().probe_backpressure();
    harness.server().wait_for_no_pong();
    let settlement = harness.next_provider_seal_ack();
    barrier.release();
    let report = harness.server().finish_backpressure(spec.sequence);
    harness.wait_for_provider_seal_ack(settlement);
    harness.wait_for_frontier(1);
    let _ = ingress_snapshot(&harness, report);
    harness.wait_for_page_leases(0);
    assert_broker_idle_and_bounded(provider_broker_snapshot(harness.session()));
    assert_atomic_frontier(
        harness.store(),
        harness.storage(),
        harness.thread,
        harness.turn,
        1,
    );
    assert_item_digest(harness.store(), harness.storage(), &harness.source, spec);
    drop(barrier);
    harness.close();

    let harness = LiveHarness::new(103);
    let cancelled = ObservationSpec::new(1, BACKPRESSURE_PATTERNS);
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness.server().begin_backpressure(cancelled);
    barrier.wait_for_stage();
    harness.assert_unpublished(cancelled.sequence);
    let session = harness.session();
    thread::scope(|scope| {
        let shutdown = scope.spawn(move || session.invalidate_connection());
        barrier.wait_for_cancellation();
        barrier.release();
        shutdown.join().unwrap();
    });
    harness.wait_for_target_closed();
    harness.wait_for_page_leases(0);
    let released = provider_broker_snapshot(harness.session());
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.staged_fragments().current(), 0);
    harness.assert_unpublished(cancelled.sequence);
    drop(barrier);
    harness.close();
}
