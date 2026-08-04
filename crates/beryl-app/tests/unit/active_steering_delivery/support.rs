use std::{path::Path, thread, time::Instant};

use beryl_backend::{
    ManagedBackendClientConnector, NonIdempotentRequestOutcome, ThreadStartOptions,
    TurnStartOptions,
};
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasProcessGeneration, RuntimeId, SyndicAcceptedInputId, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId,
};
use beryl_state::BerylState;
use syndic_storage::{
    AcceptedInputAdmission, AcceptedInputLifecycle, AcceptedRouteEffectiveState, ActivateBinding,
    BindingState, CreateThread, IdleSubmission, InputGateRecord, SyndicReadySteeringInput,
    SyndicStorage,
};

use crate::{
    cas_projection::{
        AcceptedInputAdmissionExecutionError, AcceptedInputSchedulerDiagnostics,
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome, ActiveSteeringRetryState,
        AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LiveEventTarget,
        PendingTurnActivation, ProjectionCancellationToken, ProjectionConnectionService,
        ProjectionConnectionServiceCloseError, ProjectionConnectionServiceCloseOutcome,
        ProjectionServiceConfig, ProjectionWorkerPoolDiagnostics, ScheduledOrdinaryAdmission,
        ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
        ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
        input_replay::{InputReplayContext, InputReplayFactory, InputReplayRecord},
    },
    input_admission::{
        build_accepted_input_command, idle_submission_command, prepare_accepted_input_admission,
    },
};

#[cfg(feature = "test-faults")]
use crate::cas_projection::OrdinaryInputReplayDiagnostics;

use super::super::test_support::{DeliveryPause, install_delivery_pause};
use super::server::{AUTHORIZATION, CAS_THREAD_ID, CAS_TURN_ID, SteeringServer, TIMEOUT};

mod image {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/active_steering_delivery/support/image.rs"
    ));
}

mod storage {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/active_steering_delivery/support/storage.rs"
    ));
}

use self::{
    image::prepare_repeated_image,
    storage::{
        execute, execution_binding, point_limit, replace_current_text, route_entry,
        route_state_for, selected_path, timestamp,
    },
};

pub(super) const STEERING_TEXT: &str = "phase54 marker-free steering";
pub(super) const SECOND_STEERING_TEXT: &str = "phase57 second marker-free steering";
pub(super) const IMAGE_LEADING_TEXT: &str = "phase54 image ";
pub(super) const SUBMITTED_TEXT: &str = "phase54 active turn";
const EXECUTION_ROOT: &str = r"C:\work\beryl-phase54-steering";
const POINT_READ_BYTES: usize = 1_000_000;

struct UnavailableScheduledOrdinaryProvider;

impl ScheduledOrdinaryExecutionProvider for UnavailableScheduledOrdinaryProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

fn wait_for_initial_recovered_pending_settlement(service: &ProjectionConnectionService) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        if diagnostics.recovery_handed_off()
            && diagnostics.recovered_pending_pass_count() >= 2
            && diagnostics.recovered_pending_execution_unavailable() >= 1
            && diagnostics.workers_active() == 0
            && !diagnostics.recovered_pending_retained_source_cursor()
            && service.worker_pool_diagnostics().active() == 0
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "initial recovered-pending pass did not release its worker and projection flight: \
             {diagnostics:?}",
        );
        thread::yield_now();
    }
}

enum FixtureInput<'a> {
    Text(&'a str),
    RepeatedImage,
}

#[derive(Clone, Copy)]
enum AdmissionMode {
    Manual,
    Scheduled,
    ScheduledDescendantReconciliation,
    ScheduledPair,
    ScheduledUnresolvedReconciliation,
    ScheduledCancelled,
    ScheduledCancelledAndRenewed,
}

#[derive(Clone, Copy)]
pub(super) enum RetryRaceBranch {
    LifecycleArm,
    CommandAuthorization,
}

pub(super) struct DeliveryFixture {
    directory: tempfile::TempDir,
    seed: u8,
    service: ProjectionConnectionService,
    storage: SyndicStorage,
    session: AdmittedProjectionSession,
    target: Option<LiveEventTarget>,
    cancellation: ProjectionCancellationToken,
    thread_id: SyndicThreadId,
    accepted_input_id: SyndicAcceptedInputId,
    second_accepted_input_id: Option<SyndicAcceptedInputId>,
    #[cfg(feature = "test-faults")]
    faults: beryl_home_store::test_faults::FaultController,
}

fn admit_second_scheduled_input(
    service: &ProjectionConnectionService,
    storage: SyndicStorage,
    state: BerylState,
    thread_id: SyndicThreadId,
    active_turn_id: beryl_model::SyndicTurnId,
    seed: u8,
) -> SyndicAcceptedInputId {
    let live_home = service.live_home_command().unwrap();
    let home = live_home.home();
    let current = storage
        .current_draft(home, thread_id, point_limit())
        .unwrap()
        .expect("first admission creates a replacement draft");
    let updated_at = timestamp(
        current
            .draft()
            .updated_at()
            .unix_millis()
            .checked_add(1)
            .expect("fixture timestamps remain below the timestamp ceiling"),
    );
    replace_current_text(home, storage, thread_id, SECOND_STEERING_TEXT, updated_at);
    let current = storage
        .current_draft(home, thread_id, point_limit())
        .unwrap()
        .expect("second steering payload remains current");
    let gate = storage
        .input_gate(home, thread_id, point_limit())
        .unwrap()
        .expect("active thread retains its input gate");
    let active_turn_state = storage
        .turn_state(home, active_turn_id, point_limit())
        .unwrap()
        .expect("active ordinary turn remains available");
    let admission = AcceptedInputAdmission::new(
        thread_id,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([seed.wrapping_add(8); 16]),
        None,
        active_turn_state
            .updated_at()
            .max(current.draft().updated_at()),
    );
    let input_id = admission.accepted_input_id();
    let prepared =
        prepare_accepted_input_admission(home, storage, state.assets(), admission).unwrap();
    drop(live_home);
    service.execute_accepted_input_admission(prepared).unwrap();
    input_id
}

impl DeliveryFixture {
    pub(super) fn new(
        seed: u8,
        worker_capacity: u64,
        server: &SteeringServer,
        steering_text: &str,
    ) -> Self {
        Self::build(
            seed,
            worker_capacity,
            server,
            FixtureInput::Text(steering_text),
            AdmissionMode::Manual,
        )
    }

    pub(super) fn new_scheduled(
        seed: u8,
        worker_capacity: u64,
        server: &SteeringServer,
        steering_text: &str,
    ) -> Self {
        Self::build(
            seed,
            worker_capacity,
            server,
            FixtureInput::Text(steering_text),
            AdmissionMode::Scheduled,
        )
    }

    pub(super) fn new_scheduled_descendant_reconciliation(
        seed: u8,
        server: &SteeringServer,
        steering_text: &str,
    ) -> Self {
        Self::build(
            seed,
            4,
            server,
            FixtureInput::Text(steering_text),
            AdmissionMode::ScheduledDescendantReconciliation,
        )
    }

    pub(super) fn new_scheduled_pair(seed: u8, server: &SteeringServer) -> Self {
        Self::build(
            seed,
            5,
            server,
            FixtureInput::Text(STEERING_TEXT),
            AdmissionMode::ScheduledPair,
        )
    }

    pub(super) fn new_scheduled_unresolved_reconciliation(
        seed: u8,
        server: &SteeringServer,
        steering_text: &str,
    ) -> Self {
        Self::build(
            seed,
            4,
            server,
            FixtureInput::Text(steering_text),
            AdmissionMode::ScheduledUnresolvedReconciliation,
        )
    }

    pub(super) fn new_scheduled_cancelled(
        seed: u8,
        worker_capacity: u64,
        server: &SteeringServer,
        steering_text: &str,
    ) -> Self {
        Self::build(
            seed,
            worker_capacity,
            server,
            FixtureInput::Text(steering_text),
            AdmissionMode::ScheduledCancelled,
        )
    }

    pub(super) fn new_scheduled_cancelled_and_renewed(
        seed: u8,
        worker_capacity: u64,
        server: &SteeringServer,
        steering_text: &str,
    ) -> Self {
        Self::build(
            seed,
            worker_capacity,
            server,
            FixtureInput::Text(steering_text),
            AdmissionMode::ScheduledCancelledAndRenewed,
        )
    }

    pub(super) fn new_repeated_image(
        seed: u8,
        worker_capacity: u64,
        server: &SteeringServer,
    ) -> Self {
        Self::build(
            seed,
            worker_capacity,
            server,
            FixtureInput::RepeatedImage,
            AdmissionMode::Manual,
        )
    }

    fn build(
        seed: u8,
        worker_capacity: u64,
        server: &SteeringServer,
        input: FixtureInput<'_>,
        admission_mode: AdmissionMode,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let options = HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT);
        #[cfg(feature = "test-faults")]
        let faults = beryl_home_store::test_faults::FaultController::new();
        #[cfg(feature = "test-faults")]
        let mut home = HomeStore::open_with_faults(options, faults.clone()).unwrap();
        #[cfg(not(feature = "test-faults"))]
        let mut home = HomeStore::open(options).unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let state = BerylState::register(&mut home).unwrap();
        let thread_id = SyndicThreadId::from_bytes([seed; 16]);
        let runtime_id = RuntimeId::from_bytes([seed.wrapping_add(4); 16]);
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread_id,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(runtime_id, seed),
                    timestamp(1),
                ),
            ),
        );
        replace_current_text(&home, storage, thread_id, SUBMITTED_TEXT, timestamp(2));
        let current = storage
            .current_draft(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(&home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let submitted_item_id = SyndicItemId::from_bytes([seed.wrapping_add(3); 16]);
        let submission = IdleSubmission::new(
            thread_id,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
            submitted_item_id,
            None,
            timestamp(3),
        );
        let submitted_turn_id = submission.submitted_turn_id();
        home.execute(idle_submission_command(&home, storage, state.assets(), submission).unwrap())
            .unwrap();

        let config = ProjectionServiceConfig::try_new(16, worker_capacity).unwrap();
        let service = ProjectionConnectionService::new(
            home,
            storage,
            config,
            Box::new(UnavailableScheduledOrdinaryProvider),
        )
        .unwrap();
        wait_for_initial_recovered_pending_settlement(&service);
        let connector =
            ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
        let mut session = service
            .admit_lifecycle_test_candidate(
                &connector,
                runtime_id,
                CasProcessGeneration::new(540_000 + u64::from(seed)).unwrap(),
                Path::new(EXECUTION_ROOT),
                TIMEOUT,
            )
            .unwrap();
        let projection = {
            let live_home = service.live_home_command().unwrap();
            let home = live_home.home();
            let coordinator = CasProjectionCoordinator::for_healthy_home(home).unwrap();
            let request = CasProjectionRequest::new(
                thread_id,
                selected_path(home, storage, thread_id),
                execution_binding(runtime_id, seed),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                timestamp(4),
                TIMEOUT,
            );
            coordinator
                .obtain_projection(
                    home,
                    storage,
                    &mut session,
                    &request,
                    &ProjectionCancellationToken::new(),
                )
                .unwrap()
        };
        server.wait_for_projection();
        assert_eq!(projection.cas_thread_id().as_str(), CAS_THREAD_ID);

        let live_home = service.live_home_command().unwrap();
        let home = live_home.home();
        let submitted_item = storage
            .canonical_item(home, submitted_item_id, point_limit())
            .unwrap()
            .expect("submitted ordinary item must remain canonical");
        let submitted_content = submitted_item
            .presentation_content()
            .expect("submitted ordinary item must retain its sealed content");
        let pending_turn_state = storage
            .turn_state(home, submitted_turn_id, point_limit())
            .unwrap()
            .expect("submitted ordinary turn must retain its pending state");
        let cancellation = ProjectionCancellationToken::new();
        let replay = InputReplayFactory::prepare(
            home,
            storage,
            state.assets(),
            InputReplayContext::from_projection(&projection),
            InputReplayRecord::submitted(thread_id, submitted_item_id),
            submitted_content,
            None,
            None,
            &cancellation,
            #[cfg(feature = "test-faults")]
            OrdinaryInputReplayDiagnostics::new(),
        )
        .unwrap();
        let binding = storage
            .current_binding(home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let snapshot_id = SyndicExecutionSnapshotId::from_bytes([seed.wrapping_add(5); 16]);
        execute(
            home,
            storage.activate_binding(
                storage.revision(home).unwrap(),
                ActivateBinding::new(
                    thread_id,
                    binding.binding().revision(),
                    gate.revision(),
                    selected_path(home, storage, thread_id),
                    snapshot_id,
                    submitted_turn_id,
                    projection.loaded_session_generation(),
                    timestamp(5),
                ),
            ),
        );
        let binding = storage
            .current_binding(home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let activation = PendingTurnActivation::new(
            thread_id,
            submitted_turn_id,
            binding.binding().revision(),
            gate.revision(),
            pending_turn_state.revision(),
            snapshot_id,
            timestamp(5),
        );
        let target = projection
            .into_pending_live_event_target(activation)
            .unwrap();
        let mut replay = replay.fresh_source();
        let start = target
            .start_streamed_turn(
                TurnStartOptions::default(),
                TIMEOUT,
                replay.service(home, storage, &cancellation),
            )
            .unwrap();
        assert!(
            start.response_activation_failure().is_none(),
            "exact turn/start response must activate the pending target: {:?}",
            start.response_activation_failure(),
        );
        match start.outcome() {
            NonIdempotentRequestOutcome::ExactResponse { response } => {
                assert_eq!(response.turn_id().as_str(), CAS_TURN_ID);
            }
            outcome => panic!("ordinary fixture turn/start was not exact: {outcome:?}"),
        }
        drop(live_home);

        let live_home = service.live_home_command().unwrap();
        let home = live_home.home();
        let asset_reference_set = match input {
            FixtureInput::Text(steering_text) => {
                replace_current_text(home, storage, thread_id, steering_text, timestamp(7));
                None
            }
            FixtureInput::RepeatedImage => Some(prepare_repeated_image(
                home, storage, &state, thread_id, seed,
            )),
        };
        let current = storage
            .current_draft(home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let active_turn_state = storage
            .turn_state(home, submitted_turn_id, point_limit())
            .unwrap()
            .expect("active ordinary turn must retain its durable state");
        let mut scheduler_blockers =
            matches!(admission_mode, AdmissionMode::ScheduledPair).then(|| {
                let available = service.worker_pool_diagnostics().available();
                (0..available)
                    .map(|_| service.acquire_steering_worker_for_test().unwrap())
                    .collect::<Vec<_>>()
            });
        let admission = AcceptedInputAdmission::new(
            thread_id,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([seed.wrapping_add(6); 16]),
            asset_reference_set,
            active_turn_state
                .updated_at()
                .max(current.draft().updated_at()),
        );
        let accepted_input_id = admission.accepted_input_id();
        drop(live_home);
        let mut delivery_pause = matches!(
            admission_mode,
            AdmissionMode::ScheduledDescendantReconciliation
                | AdmissionMode::ScheduledPair
                | AdmissionMode::ScheduledCancelled
                | AdmissionMode::ScheduledCancelledAndRenewed
        )
        .then(|| install_delivery_pause(accepted_input_id, DeliveryPause::BeforeLifecycleArm));
        match admission_mode {
            AdmissionMode::Manual => {
                let live_home = service.live_home_command().unwrap();
                let home = live_home.home();
                home.execute(
                    build_accepted_input_command(home, storage, state.assets(), admission).unwrap(),
                )
                .unwrap();
            }
            AdmissionMode::ScheduledDescendantReconciliation => {
                let prepared = {
                    let live_home = service.live_home_command().unwrap();
                    prepare_accepted_input_admission(
                        live_home.home(),
                        storage,
                        state.assets(),
                        admission,
                    )
                    .unwrap()
                };
                let reconciliation = service
                    .pause_admission_reconciliation_after_dispatch_for_test(accepted_input_id);
                let delivery = delivery_pause
                    .take()
                    .expect("descendant race installs a delivery pause");
                let result = thread::scope(|scope| {
                    let admission =
                        scope.spawn(|| service.execute_accepted_input_admission(prepared));
                    reconciliation.wait_until_paused(TIMEOUT);
                    service.signal_accepted_ready_for_test();
                    delivery.wait_until_paused(TIMEOUT);
                    let route = {
                        let live_home = service.live_home_command().unwrap();
                        route_entry(live_home.home(), storage, thread_id, accepted_input_id)
                    };
                    assert_eq!(
                        route,
                        (
                            AcceptedRouteEffectiveState::Delivering,
                            AcceptedInputLifecycle::Delivering,
                        ),
                        "the legal delivery descendant commits before reconciliation",
                    );
                    reconciliation.release();
                    admission.join().unwrap()
                });
                assert!(
                    result.is_ok(),
                    "the durable receipt must reconcile across a legal descendant: {result:?}",
                );
                delivery.release();
            }
            AdmissionMode::Scheduled
            | AdmissionMode::ScheduledPair
            | AdmissionMode::ScheduledUnresolvedReconciliation
            | AdmissionMode::ScheduledCancelled
            | AdmissionMode::ScheduledCancelledAndRenewed => {
                let prepared = {
                    let live_home = service.live_home_command().unwrap();
                    prepare_accepted_input_admission(
                        live_home.home(),
                        storage,
                        state.assets(),
                        admission,
                    )
                    .unwrap()
                };
                if matches!(
                    admission_mode,
                    AdmissionMode::ScheduledUnresolvedReconciliation
                ) {
                    service.fail_admission_reconciliation_for_test(2);
                }
                let result = service.execute_accepted_input_admission(prepared);
                if matches!(
                    admission_mode,
                    AdmissionMode::ScheduledUnresolvedReconciliation
                ) {
                    assert!(matches!(
                        result,
                        Err(AcceptedInputAdmissionExecutionError::Reconciliation(_,))
                    ));
                    assert!(!service.is_accepting_for_test());
                } else {
                    result.unwrap();
                }
            }
        }
        let second_accepted_input_id = if matches!(admission_mode, AdmissionMode::ScheduledPair) {
            let input_id = admit_second_scheduled_input(
                &service,
                storage,
                state,
                thread_id,
                submitted_turn_id,
                seed,
            );
            drop(scheduler_blockers.take());
            let pause = delivery_pause
                .take()
                .expect("scheduled pair installs a first-delivery pause");
            pause.wait_until_paused(TIMEOUT);
            assert!(
                service
                    .accepted_input_scheduler_diagnostics()
                    .attempt_waits()
                    >= 1,
                "same-source scan must observe the occupied target attempt",
            );
            pause.release();
            Some(input_id)
        } else {
            None
        };
        if let Some(pause) = delivery_pause {
            pause.wait_until_paused(TIMEOUT);
            service.cancel_active_steering_lifecycle_for_test();
            if matches!(admission_mode, AdmissionMode::ScheduledCancelledAndRenewed) {
                service.renew_active_steering_lifecycle_for_test();
            }
            pause.release();
            if matches!(admission_mode, AdmissionMode::ScheduledCancelled) {
                let deadline = Instant::now() + TIMEOUT;
                loop {
                    let diagnostics = service.accepted_input_scheduler_diagnostics();
                    if diagnostics.retry_state() == ActiveSteeringRetryState::Parked
                        && diagnostics.workers_active() == 0
                        && diagnostics.workers_joined() == 1
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "automatic cancellation did not park its exact retry: {diagnostics:?}",
                    );
                    thread::yield_now();
                }
            }
        }
        drop(scheduler_blockers);
        if matches!(admission_mode, AdmissionMode::Manual) {
            let live_home = service.live_home_command().unwrap();
            let ready = storage
                .ready_steering_input(live_home.home(), accepted_input_id, point_limit())
                .unwrap()
                .expect("accepted active-turn input must be ready for steering");
            assert_eq!(ready.input().id(), accepted_input_id);
            assert_eq!(
                ready.target().pending().cas_thread_id().as_str(),
                CAS_THREAD_ID
            );
            assert_eq!(ready.target().cas_turn_id().as_str(), CAS_TURN_ID);
        }

        Self {
            directory,
            seed,
            service,
            storage,
            session,
            target: Some(target),
            cancellation,
            thread_id,
            accepted_input_id,
            second_accepted_input_id,
            #[cfg(feature = "test-faults")]
            faults,
        }
    }

    pub(super) fn deliver(
        &self,
    ) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
        self.service.deliver_active_steering_input(
            self.target
                .as_ref()
                .expect("delivery fixture retains its live target"),
            self.accepted_input_id,
            &self.cancellation,
            TIMEOUT,
        )
    }

    pub(super) fn deliver_with_sibling_at(
        &mut self,
        stage: DeliveryPause,
    ) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
        let pause = install_delivery_pause(self.accepted_input_id, stage);
        let active_turn_id = self
            .input_gate()
            .state()
            .blocking_turn_id()
            .expect("active steering fixture retains its blocking turn");
        let target = self
            .target
            .take()
            .expect("sibling-race delivery owns the live target");
        let service = &self.service;
        let storage = self.storage;
        let state = {
            let live_home = service.live_home_command().unwrap();
            BerylState::reacquire(live_home.home()).unwrap()
        };
        let cancellation = &self.cancellation;
        let input_id = self.accepted_input_id;
        let thread_id = self.thread_id;
        let seed = self.seed;

        let (target, second_input_id, outcome) = thread::scope(|scope| {
            let delivery = scope.spawn(move || {
                let outcome =
                    service.deliver_active_steering_input(&target, input_id, cancellation, TIMEOUT);
                (target, outcome)
            });
            pause.wait_until_paused(TIMEOUT);
            let second_input_id = admit_second_scheduled_input(
                service,
                storage,
                state,
                thread_id,
                active_turn_id,
                seed,
            );
            pause.release();
            let (target, outcome) = delivery.join().unwrap();
            (target, second_input_id, outcome)
        });
        self.target = Some(target);
        self.second_accepted_input_id = Some(second_input_id);
        outcome
    }

    pub(super) fn deliver_with_workers_occupied(
        &self,
    ) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
        let available = self.service.worker_pool_diagnostics().available();
        let workers = (0..available)
            .map(|_| self.service.acquire_steering_worker_for_test().unwrap())
            .collect::<Vec<_>>();
        let outcome = self.deliver();
        drop(workers);
        outcome
    }

    pub(super) fn deliver_with_connection_attempt_occupied(
        &self,
    ) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
        let ready = {
            let live_home = self.service.live_home_command().unwrap();
            self.storage
                .ready_steering_input(live_home.home(), self.accepted_input_id, point_limit())
                .unwrap()
                .expect("fixture accepted input remains ready")
        };
        let attempt = self
            .target
            .as_ref()
            .expect("delivery fixture retains its live target")
            .acquire_active_steering_attempt(&ready)
            .unwrap();
        let outcome = self.deliver();
        attempt.finish().unwrap();
        outcome
    }

    pub(super) fn deliver_retry_during_ordinary_loss(
        &mut self,
        branch: RetryRaceBranch,
    ) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
        let before_stage = match branch {
            RetryRaceBranch::LifecycleArm => DeliveryPause::BeforeLifecycleArm,
            RetryRaceBranch::CommandAuthorization => DeliveryPause::BeforeCommandAuthorization,
        };
        let before = install_delivery_pause(self.accepted_input_id, before_stage);
        let after =
            install_delivery_pause(self.accepted_input_id, DeliveryPause::AfterRetryDisposition);
        let target = self
            .target
            .take()
            .expect("retry-race delivery owns the live target");
        let service = &self.service;
        let storage = self.storage;
        let thread_id = self.thread_id;
        let cancellation = &self.cancellation;
        let input_id = self.accepted_input_id;
        let race = target.active_steering_race_probe_for_test();

        let (target, outcome) = thread::scope(|scope| {
            let delivery = scope.spawn(move || {
                let outcome =
                    service.deliver_active_steering_input(&target, input_id, cancellation, TIMEOUT);
                (target, outcome)
            });
            before.wait_until_paused(TIMEOUT);
            if matches!(branch, RetryRaceBranch::LifecycleArm) {
                race.close_checked_steering_lifecycles();
            }
            let loss_race = &race;
            let loss = scope.spawn(move || loss_race.converge_target_loss());
            let deadline = Instant::now() + TIMEOUT;
            while !race.target_loss_requested() {
                assert!(
                    Instant::now() < deadline,
                    "ordinary target-loss waiter did not reach the router",
                );
                thread::yield_now();
            }
            before.release();

            after.wait_until_paused(TIMEOUT);
            {
                let live_home = service.live_home_command().unwrap();
                let home = live_home.home();
                assert_eq!(
                    route_state_for(home, storage, thread_id, input_id),
                    AcceptedRouteEffectiveState::Ready,
                    "exact Retry must commit before ordinary target-loss publication",
                );
                assert_eq!(
                    route_entry(home, storage, thread_id, input_id).1,
                    AcceptedInputLifecycle::Retryable,
                    "exact Retry must preserve the accepted input before target loss",
                );
            }
            after.release();

            let (target, outcome) = delivery.join().unwrap();
            assert!(
                loss.join().unwrap().unwrap(),
                "ordinary loss waiter must publish the incomplete target loss",
            );
            (target, outcome)
        });
        self.target = Some(target);
        outcome
    }

    pub(super) fn deliver_after_delayed_lifecycle(
        &mut self,
        server: &SteeringServer,
    ) -> (
        AcceptedRouteEffectiveState,
        Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError>,
    ) {
        let target = self
            .target
            .take()
            .expect("delayed delivery owns the sole live target");
        let service = &self.service;
        let storage = self.storage;
        let cancellation = &self.cancellation;
        let thread_id = self.thread_id;
        let input_id = self.accepted_input_id;
        let (target, state_before_lifecycle, outcome) = thread::scope(|scope| {
            let delivery = scope.spawn(move || {
                let outcome =
                    service.deliver_active_steering_input(&target, input_id, cancellation, TIMEOUT);
                (target, outcome)
            });
            server.wait_for_exact_response();
            let state_before_lifecycle = {
                let live_home = service.live_home_command().unwrap();
                route_state_for(live_home.home(), storage, thread_id, input_id)
            };
            server.release_lifecycle();
            let (target, outcome) = delivery.join().unwrap();
            (target, state_before_lifecycle, outcome)
        });
        self.target = Some(target);
        (state_before_lifecycle, outcome)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn fail_next_write_before_dispatch(&self) {
        self.session
            .fail_next_write_before_dispatch_for_test()
            .unwrap();
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn fail_next_claim_after_persist(&self) {
        self.faults
            .fail_next(beryl_home_store::test_faults::FaultPoint::AfterPersist);
    }

    pub(super) fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub(super) fn route_state(&self) -> AcceptedRouteEffectiveState {
        let live_home = self.service.live_home_command().unwrap();
        route_state_for(
            live_home.home(),
            self.storage,
            self.thread_id,
            self.accepted_input_id,
        )
    }

    pub(super) fn route_state_after_service_close(&self) -> AcceptedRouteEffectiveState {
        route_state_for(
            self.service.retained_home_for_test(),
            self.storage,
            self.thread_id,
            self.accepted_input_id,
        )
    }

    pub(super) fn second_route_state(&self) -> AcceptedRouteEffectiveState {
        let live_home = self.service.live_home_command().unwrap();
        route_state_for(
            live_home.home(),
            self.storage,
            self.thread_id,
            self.second_accepted_input_id
                .expect("scheduled-pair fixture retains its second input"),
        )
    }

    pub(super) fn binding_state(&self) -> BindingState {
        let live_home = self.service.live_home_command().unwrap();
        self.storage
            .current_binding(live_home.home(), self.thread_id, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state()
            .clone()
    }

    pub(super) fn route_lifecycle(&self) -> AcceptedInputLifecycle {
        let live_home = self.service.live_home_command().unwrap();
        route_entry(
            live_home.home(),
            self.storage,
            self.thread_id,
            self.accepted_input_id,
        )
        .1
    }

    pub(super) fn route_lifecycle_after_service_close(&self) -> AcceptedInputLifecycle {
        route_entry(
            self.service.retained_home_for_test(),
            self.storage,
            self.thread_id,
            self.accepted_input_id,
        )
        .1
    }

    pub(super) fn ready_input(&self) -> SyndicReadySteeringInput {
        let live_home = self.service.live_home_command().unwrap();
        self.storage
            .ready_steering_input(live_home.home(), self.accepted_input_id, point_limit())
            .unwrap()
            .expect("fixture accepted input remains ready")
    }

    pub(super) fn input_gate(&self) -> InputGateRecord {
        let live_home = self.service.live_home_command().unwrap();
        self.storage
            .input_gate(live_home.home(), self.thread_id, point_limit())
            .unwrap()
            .expect("fixture thread retains its input gate")
    }

    pub(super) fn worker_diagnostics(&self) -> ProjectionWorkerPoolDiagnostics {
        self.service.worker_pool_diagnostics()
    }

    pub(super) fn scheduler_diagnostics(&self) -> AcceptedInputSchedulerDiagnostics {
        self.service.accepted_input_scheduler_diagnostics()
    }

    pub(super) fn service_is_accepting(&self) -> bool {
        self.service.is_accepting_for_test()
    }

    pub(super) fn signal_ordinary_ready(&self) {
        self.service.signal_accepted_ready_for_test();
    }

    pub(super) fn renew_scheduler_cancellation(&self) {
        self.service.renew_active_steering_lifecycle_for_test();
    }

    pub(super) fn close(self, server: SteeringServer) {
        drop(self.finish_close(server).unwrap());
    }

    pub(super) fn close_after_scheduler_failure(self, server: SteeringServer) {
        assert!(matches!(
            self.finish_close(server),
            Err(ProjectionConnectionServiceCloseError::SchedulerShutdown)
        ));
    }

    fn finish_close(
        self,
        server: SteeringServer,
    ) -> Result<ProjectionConnectionServiceCloseOutcome, ProjectionConnectionServiceCloseError>
    {
        let Self {
            directory,
            seed: _,
            service,
            storage: _,
            session,
            target,
            cancellation: _,
            thread_id: _,
            accepted_input_id: _,
            second_accepted_input_id: _,
            #[cfg(feature = "test-faults")]
                faults: _,
        } = self;
        drop(target);
        session.invalidate_connection();
        drop(session);
        server.join();
        let outcome = service.close();
        drop(directory);
        outcome
    }
}
