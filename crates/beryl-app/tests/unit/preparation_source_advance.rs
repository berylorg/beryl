use super::*;
use crate::cas_projection::*;
use beryl_model::AdmittedHostPath;
use beryl_state::{
    AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration, RuntimeRegistration,
    UnixMillis,
};
use std::num::NonZeroUsize;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

struct CaptureAdmission {
    provider: ProcessScheduledExecutionProvider,
    captured: Arc<Mutex<Option<ScheduledOrdinaryAdmission>>>,
}

impl ScheduledOrdinaryExecutionProvider for CaptureAdmission {
    fn attach(&mut self, context: ScheduledExecutionProviderContext) {
        self.provider.attach(context);
    }

    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        assert!(self.captured.lock().unwrap().replace(admission).is_none());
        Ok(ScheduledOrdinaryAdmissionResult::Unavailable(
            ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady,
        ))
    }

    fn shutdown(&mut self) {
        self.provider.shutdown();
    }
}

fn fixture() -> (Fixture, Arc<Mutex<Option<ScheduledOrdinaryAdmission>>>) {
    let captured = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::with_provider(false, 4, |provider| {
        Box::new(CaptureAdmission {
            provider,
            captured: Arc::clone(&captured),
        })
    });
    let state = {
        let live = fixture.service.live_home_command().unwrap();
        let state = BerylState::reacquire(live.home()).unwrap();
        let binding = SyndicStorage::reacquire(live.home())
            .unwrap()
            .thread_execution(live.home(), fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone();
        let executable =
            AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\source-advance\codex.exe")
                .unwrap();
        let runtime = RuntimeRegistration::new(
            binding.runtime_id(),
            executable.clone(),
            RuntimeMode::Host,
            beryl_model::RuntimeLaunchForm::CodexCli,
            RuntimeNativePath::from_admitted(
                RuntimeMode::Host,
                PathFlavor::Windows,
                executable.as_str(),
            )
            .unwrap(),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        )
        .unwrap();
        let root = RootRegistration::new(
            binding.root_id(),
            binding.root_path().clone(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, binding.root_path().as_str())
                .unwrap(),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        execute(
            live.home(),
            state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(live.home()).unwrap(),
                CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
            ),
        );
        state
    };
    let capacity = NonZeroUsize::new(1).unwrap();
    fixture
        .service
        .configure_runtime_interest(
            RuntimeInterestConfig::new(capacity, capacity, Duration::from_secs(1)).unwrap(),
            crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
                fixture.service.home_id(),
                capacity,
            ),
        )
        .unwrap();
    fixture
        .service
        .configure_runtime_session_preparation(
            &fixture.sessions,
            RuntimeSessionPreparationConfig {
                runtime_roots: state.runtime_roots(),
                assets: fixture.assets.clone(),
                policy: ScheduledOrdinaryRequestPolicy::new(
                    beryl_backend::ThreadStartOptions::persistent(),
                    None,
                    Duration::from_secs(1),
                    OrdinaryTurnExecutionRequest::new(
                        beryl_backend::TurnStartOptions::default(),
                        Duration::from_secs(1),
                    ),
                ),
                token_directory: RuntimeTokenDirectory::from_admitted(
                    AdmittedHostPath::from_admitted(
                        PathFlavor::Windows,
                        r"C:\source-advance\tokens",
                    )
                    .unwrap(),
                ),
                wsl_supervisor_artifact: None,
            },
            &Arc::new(crate::lifecycle_attention::ProcessLifecycleAttentionPool::new()),
        )
        .unwrap();
    {
        let live = fixture.service.live_home_command().unwrap();
        let (_, source) = submission_fixture::submit_atoms(
            live.home(),
            SyndicStorage::reacquire(live.home()).unwrap(),
            fixture.assets.clone(),
            fixture.thread,
            SyndicDraftId::from_bytes([73; 16]),
            SyndicItemId::from_bytes([74; 16]),
            &[submission_fixture::Atom::Text(
                "preserve pending preparation",
            )],
            75,
            SyndicTimestamp::from_unix_millis(3),
        );
        fixture.turn = source.submitted_turn_id();
    }
    fixture.service.notify_scheduled_ordinary_execution_ready();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while captured.lock().unwrap().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "original scheduler admission was not captured"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    (fixture, captured)
}

fn clear_wakes(sessions: &ScheduledExecutionSessions) -> AcceptedInputSchedulerSignal {
    let signal = AcceptedInputSchedulerSignal::new();
    sessions.lock().context.as_mut().unwrap().ready.signal = signal.clone();
    signal.assert_execution_lanes_for_test(false);
    signal
}

fn unrelated_write(context: &PreparationContext) {
    execute(
        &context.home,
        context.storage.create_thread(
            context.storage.revision(&context.home).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([83; 16]),
                SyndicDraftId::from_bytes([84; 16]),
                crate::support::exact_cas::execution_binding(),
                SyndicTimestamp::from_unix_millis(4),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
}

#[test]
fn witnessed_home_advance_reopens_both_original_execution_lanes_after_custody_release() {
    let (fixture, captured) = fixture();
    let signal = clear_wakes(&fixture.sessions);
    let context = fixture.sessions.lock().preparation.clone().unwrap();
    let admission = captured.lock().unwrap().take().unwrap();
    let before = context
        .storage
        .pending_dispatch_evidence(&context.home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(
        context
            .launch_spec_for(admission.thread_id(), admission.execution_binding())
            .is_ok()
    );
    let outcome = context.run_with_launch_spec(&fixture.sessions, &admission, None, || {
        context.launch_spec_for_after_read(
            admission.thread_id(),
            admission.execution_binding(),
            || {
                unrelated_write(&context);
            },
        )
    });
    assert_eq!(outcome, run::PreparationOutcome::HomeAdvanced);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 1);
    let _fence = fixture.gate.fence().unwrap();
    fixture.sessions.complete_preparation(admission, outcome);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
    let _released_flight = CasProjectionCoordinator::for_healthy_home(&context.home)
        .unwrap()
        .begin_projection(fixture.thread)
        .unwrap();
    signal.assert_execution_lanes_for_test(true);
    let after = context
        .storage
        .pending_dispatch_evidence(&context.home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(before.turn_id(), after.turn_id());
    assert_eq!(before.item_id(), after.item_id());
    assert_eq!(before.input(), after.input());
    assert_eq!(before.gate_revision(), after.gate_revision());
    assert_eq!(before.state_revision(), after.state_revision());
    signal.assert_execution_lanes_for_test(false);
}

#[test]
fn stable_target_refusal_and_retired_generation_do_not_reopen_execution_lanes() {
    for retire in [false, true] {
        let (fixture, captured) = fixture();
        let signal = clear_wakes(&fixture.sessions);
        let context = fixture.sessions.lock().preparation.clone().unwrap();
        let admission = captured.lock().unwrap().take().unwrap();
        let outcome = context.run_with_launch_spec(&fixture.sessions, &admission, None, || {
            if retire {
                context.launch_spec_for_after_read(
                    admission.thread_id(),
                    admission.execution_binding(),
                    || unrelated_write(&context),
                )
            } else {
                let binding = ExecutionBinding::new(
                    RuntimeId::from_bytes([85; 16]),
                    admission.execution_binding().root_id(),
                    admission.execution_binding().root_path().clone(),
                );
                context.launch_spec_for(admission.thread_id(), &binding)
            }
        });
        assert_eq!(
            outcome,
            if retire {
                run::PreparationOutcome::HomeAdvanced
            } else {
                run::PreparationOutcome::Settled
            },
        );
        if retire {
            fixture
                .faults
                .fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
            assert!(
                context
                    .storage
                    .thread(&context.home, fixture.thread, point_limit())
                    .is_err()
            );
            let _ = context.commands.observe_persistent_failure();
            assert!(!context.commands.is_open());
        }
        let _fence = (!retire).then(|| fixture.gate.fence().unwrap());
        fixture.sessions.complete_preparation(admission, outcome);
        signal.assert_execution_lanes_for_test(false);
        assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
    }
}
