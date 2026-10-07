use super::*;
use beryl_app::{discussion_settlement::*, lifecycle_attention::ProcessLifecycleAttentionPool};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{
    AdmittedHostPath, ExecutionBinding, JobId, PathFlavor, RuntimeMode, RuntimeNativePath,
};
use beryl_state::{
    AvailabilitySnapshot, BranchHandoffJobLifecycle, CreateRuntimeWithHomeRoot, HandoffFailureKind,
    RootRegistration, RuntimeRegistration, UnixMillis,
};
use std::{
    fs,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

struct PreparedFixture {
    fixture: syndic::Fixture,
    sessions: ScheduledExecutionSessions,
    operations: DiscussionSettlementOperations,
    root: tempfile::TempDir,
    binding: ExecutionBinding,
    thread: beryl_model::SyndicThreadId,
    job: JobId,
    before: PendingDispatchEvidence,
}

#[track_caller]
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "generated preparation condition timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}
fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
}
fn canonical(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap()
        .to_str()
        .unwrap()
        .trim_start_matches(r"\\?\")
        .to_owned()
}

fn prepared(mode: &str, capacity: u64, missing_root: bool) -> PreparedFixture {
    let mut configured = None;
    let mut registry = None;
    let mut operations = None;
    let mut fixture = syndic::Fixture::new_with_execution_authority_and_runtime_config(
        138,
        capacity,
        (
            RuntimeInterestConfig::new(
                NonZeroUsize::new(1).unwrap(),
                NonZeroUsize::new(4).unwrap(),
                Duration::from_secs(10),
            )
            .unwrap(),
            NonZeroUsize::new(1).unwrap(),
        ),
        |home, state, storage, process| {
            let custody =
                DiscussionSettlementOperations::new(process.clone(), NonZeroUsize::new(1).unwrap());
            let handoff = DiscussionSettlementService::new(
                custody.clone(),
                home.service_reference(),
                state.clone(),
                storage.clone(),
            );
            operations = Some(custody);
            configured = Some(handoff.clone());
            let (provider, sessions) = ProcessScheduledExecutionProvider::new();
            registry = Some(sessions);
            Box::new(provider.with_discussion_settlement(handoff))
        },
    );
    let (_, input, receipt) =
        generated_handoff::seed_with_service(&fixture, "Prepared resolution", configured);
    let thread = input.thread_id();
    let job = JobId::from_bytes(*input.id().as_bytes());
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("tokens")).unwrap();
    fs::write(root.path().join("fixture-mode"), mode).unwrap();
    let path = canonical(root.path());
    let execution = fixture
        .storage
        .thread_execution(
            &fixture.home(),
            thread,
            SyndicPointReadLimit::new(400_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    let binding = ExecutionBinding::new(
        execution.execution().runtime_id(),
        execution.execution().root_id(),
        native(&path),
    );
    let point = SyndicPointReadLimit::new(400_000).unwrap();
    assert!(matches!(
        fixture
            .storage
            .current_binding(&fixture.home(), thread, point)
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Unbound { .. }
    ));
    support::commit(
        &fixture.home(),
        fixture.storage.clone(),
        support::batch(
            [syndic_storage::test_faults::FixtureRecord::ThreadExecution(
                ThreadExecutionRecord::new(thread, binding.clone()),
            )],
        ),
    );
    let turn = fixture
        .storage
        .turn(&fixture.home(), receipt.parent_turn_id, point)
        .unwrap()
        .unwrap();
    let parent = fixture
        .storage
        .turn(&fixture.home(), turn.parent().turn().unwrap(), point)
        .unwrap()
        .unwrap();
    let selected = fixture.selected_path(thread);
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let current = fixture
        .storage
        .current_binding(&fixture.home(), thread, point)
        .unwrap()
        .unwrap();
    support::discussion_input::committed(
        &fixture.home(),
        fixture.storage.publish_valid_binding(
            fixture.storage.revision(&fixture.home()).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                binding.clone(),
                CasThreadId::new("prepared-handoff-thread").unwrap(),
                represented,
                CasNativeTurnCount::new(parent.depth().get()),
                beryl_app::conversation_tools::ConversationToolRegistry::canonical().profile(),
                CasLineageProof::native(NativeCasLineage::Resume, represented).unwrap(),
            ),
        ),
    );
    let executable = canonical(Path::new(env!("CARGO_BIN_EXE_managed-runtime-fixture")));
    let runtime = RuntimeRegistration::new(
        binding.runtime_id(),
        host(&executable),
        RuntimeMode::Host,
        native(&executable),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    )
    .unwrap();
    let registered_root = if missing_root {
        beryl_model::RootId::from_bytes([239; 16])
    } else {
        binding.root_id()
    };
    let registration = RootRegistration::new(
        registered_root,
        native(&path),
        host(&path),
        UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    );
    {
        let home = fixture.home();
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(fixture.state.runtime_roots().create_runtime_with_home_root(
                fixture.state.runtime_roots().revision(&home).unwrap(),
                CreateRuntimeWithHomeRoot::new(runtime, registration).unwrap(),
            ))
            .unwrap();
        assert!(matches!(
            home.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
    let before = fixture
        .storage
        .pending_dispatch_evidence(
            &fixture.home(),
            thread,
            SyndicPointReadLimit::new(400_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    let sessions = registry.unwrap();
    let tokens = canonical(&root.path().join("tokens"));
    fixture
        .store
        .configure_runtime_session_preparation(
            &sessions,
            RuntimeSessionPreparationConfig {
                runtime_roots: fixture.state.runtime_roots(),
                assets: fixture.state.assets(),
                policy: ScheduledOrdinaryRequestPolicy::new(
                    ThreadStartOptions::persistent(),
                    Some(2_000_000),
                    Duration::from_secs(10),
                    OrdinaryTurnExecutionRequest::new(
                        TurnStartOptions::default(),
                        Duration::from_secs(10),
                    ),
                ),
                token_directory: RuntimeTokenDirectory::from_admitted(host(&tokens)),
                wsl_supervisor_artifact: None,
            },
            &Arc::new(ProcessLifecycleAttentionPool::new()),
        )
        .unwrap();
    PreparedFixture {
        fixture,
        sessions,
        operations: operations.unwrap(),
        root,
        binding,
        thread,
        job,
        before,
    }
}

impl PreparedFixture {
    fn job(&self) -> beryl_state::BranchHandoffJobRecord {
        self.fixture
            .state
            .durable_jobs()
            .job(&self.fixture.home(), self.job)
            .unwrap()
            .unwrap()
    }
    fn assert_unchanged_dispatch(&self) {
        let now = self
            .fixture
            .storage
            .pending_dispatch_evidence(
                &self.fixture.home(),
                self.thread,
                SyndicPointReadLimit::new(400_000).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(now.turn_id(), self.before.turn_id());
        assert_eq!(now.state_revision(), self.before.state_revision());
        assert_eq!(now.dispatch_provenance(), self.before.dispatch_provenance());
        assert_eq!(now.input(), self.before.input());
    }
    fn close(self) {
        fs::write(self.root.path().join("release-config"), "ready").unwrap();
        let (directory, service) = self.fixture.into_service();
        assert!(matches!(
            service.close().unwrap(),
            ProjectionConnectionServiceCloseOutcome::Closed
        ));
        assert_eq!(self.sessions.diagnostics().retained, 0);
        assert_eq!(self.operations.pending_nondispatch_count(), 0);
        drop(directory);
    }
}

#[test]
fn managed_rejection_pauses_generated_parent_and_runtime_retry_does_not_resume_handoff() {
    let f = prepared("reject-config", 8, false);
    wait_until(|| f.job().lifecycle() == BranchHandoffJobLifecycle::RetryableFailed);
    f.assert_unchanged_dispatch();
    wait_until(|| {
        f.sessions
            .runtime_failure(f.binding.runtime_id())
            .is_some_and(|failure| failure.retry_ready())
    });
    let failure = f.sessions.runtime_failure(f.binding.runtime_id()).unwrap();
    fs::write(f.root.path().join("fixture-mode"), "paused-handoff").unwrap();
    f.sessions
        .retry_runtime_session(failure, f.thread, f.binding.clone())
        .unwrap();
    wait_until(|| f.root.path().join("handoff-projection-observed").exists());
    let deadline = Instant::now() + Duration::from_secs(20);
    while f.sessions.diagnostics().available != 1 {
        assert!(
            Instant::now() < deadline,
            "runtime recovery stalled: sessions={:?}, failure={:?}, scheduler={:?}, files={:?}",
            f.sessions.diagnostics(),
            f.sessions.runtime_failure(f.binding.runtime_id()),
            f.fixture.store.accepted_input_scheduler_diagnostics(),
            fs::read_dir(f.root.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !f.fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .fatal()
    );
    assert!(!f.root.path().join("unexpected-handoff-dispatch").exists());
    assert_eq!(
        f.job().lifecycle(),
        BranchHandoffJobLifecycle::RetryableFailed
    );
    f.assert_unchanged_dispatch();
    f.close();
}

#[test]
fn starting_runtime_and_capacity_pressure_keep_generated_handoff_pending() {
    for capacity in [5, 8] {
        let f = prepared("pause-config", capacity, false);
        if capacity == 8 {
            wait_until(|| f.root.path().join("runtime-evidence.json").exists());
        } else {
            wait_until(|| {
                f.fixture
                    .store
                    .accepted_input_scheduler_diagnostics()
                    .recovered_pending_pass_count()
                    > 0
            });
            std::thread::sleep(Duration::from_millis(150));
            assert!(!f.root.path().join("runtime-evidence.json").exists());
        }
        assert_eq!(
            f.job().lifecycle(),
            BranchHandoffJobLifecycle::StartingParent
        );
        f.assert_unchanged_dispatch();
        assert_eq!(f.operations.pending_nondispatch_count(), 0);
        f.close();
    }
}

#[test]
fn missing_exact_root_pauses_generated_parent_without_launching_cas() {
    let f = prepared("", 8, true);
    wait_until(|| f.job().lifecycle() == BranchHandoffJobLifecycle::RetryableFailed);
    assert!(!f.root.path().join("runtime-evidence.json").exists());
    f.assert_unchanged_dispatch();
    assert_eq!(
        f.job().state().failure_evidence().unwrap().kind(),
        HandoffFailureKind::RootUnavailable
    );
    f.close();
}
