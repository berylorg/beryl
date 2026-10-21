#[allow(dead_code)]
#[path = "../support/managed_runtime.rs"]
pub(super) mod support;
#[allow(dead_code)]
#[path = "../runtime_session_preparation/submission.rs"]
pub(super) mod submission;

use beryl_app::{
    cas_projection::{
        OrdinaryTurnExecutionRequest, ProcessScheduledExecutionProvider, RuntimeInterestKind,
        RuntimeSessionPreparationConfig, RuntimeTokenDirectory, ScheduledExecutionSessions,
        ScheduledOrdinaryRequestPolicy,
    },
    lifecycle_attention::ProcessLifecycleAttentionPool,
};
use beryl_backend::{GeneratedOutputError, ThreadStartOptions, TurnStartOptions};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{AdmittedHostPath, ExecutionBinding, GeneratedSourceIncarnation, PathFlavor,
    RootId, RuntimeId, RuntimeLaunchForm, RuntimeMode, SyndicDraftId, SyndicThreadId};
use beryl_state::{AvailabilitySnapshot, CreateRuntimeWithHomeRoot, RootRegistration,
    RuntimeRegistration, UnixMillis};
use std::{fs, path::Path, sync::Arc};
use support::{Fixture, TIMEOUT, canonical_path, native, wait_until};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, GeneratedMediaSourceProof,
    SyndicTimestamp};

pub(super) fn thread_id() -> SyndicThreadId { SyndicThreadId::from_bytes([101; 16]) }

pub(super) fn submit(fixture: &Fixture, thread: SyndicThreadId) {
    submission::submit(fixture, thread);
}

fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}

pub(super) fn fixture() -> (Fixture, ScheduledExecutionSessions, Arc<ProcessLifecycleAttentionPool>) {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let fixture = Fixture::with_provider(Box::new(provider));
    let live = fixture.service().live_home_command().unwrap();
    let home = live.home();
    let executable = canonical_path(Path::new(env!("CARGO_BIN_EXE_managed-runtime-fixture")));
    let runtime = RuntimeRegistration::new(
        RuntimeId::from_bytes([99; 16]), host(&executable), RuntimeMode::Host,
        RuntimeLaunchForm::CodexCli, native(&executable), UnixMillis::new(1),
        AvailabilitySnapshot::unknown(),
    ).unwrap();
    let root_path = canonical_path(&fixture.root(1));
    let root = RootRegistration::new(RootId::from_bytes([1; 16]), native(&root_path),
        host(&root_path), UnixMillis::new(1), AvailabilitySnapshot::unknown());
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(fixture.state.runtime_roots().create_runtime_with_home_root(
        fixture.state.runtime_roots().revision(home).unwrap(),
        CreateRuntimeWithHomeRoot::new(runtime, root).unwrap(),
    )).unwrap();
    assert!(matches!(home.execute(command), CommandOutcome::Committed { later_failure: None, .. }));
    let binding = ExecutionBinding::new(RuntimeId::from_bytes([99; 16]), RootId::from_bytes([1; 16]), native(&root_path));
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(fixture.storage.create_thread(fixture.storage.revision(home).unwrap(),
        CreateThread::ordinary(thread_id(), SyndicDraftId::from_bytes([111; 16]), binding,
            SyndicTimestamp::from_unix_millis(1), DraftEditHistoryPolicyV1::new(64 * 1024 * 1024, 1).unwrap()),
    )).unwrap();
    assert!(matches!(home.execute(command), CommandOutcome::Committed { later_failure: None, .. }));
    drop(live);
    let attention = Arc::new(ProcessLifecycleAttentionPool::new());
    fixture.service().configure_runtime_session_preparation(&sessions, RuntimeSessionPreparationConfig {
        runtime_roots: fixture.state.runtime_roots(), assets: fixture.state.assets(),
        policy: ScheduledOrdinaryRequestPolicy::new(ThreadStartOptions::persistent(), Some(2_000_000),
            TIMEOUT, OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT)),
        token_directory: RuntimeTokenDirectory::from_admitted(host(&canonical_path(&fixture.tokens()))),
        wsl_supervisor_artifact: None,
    }, &attention).unwrap();
    (fixture, sessions, attention)
}

#[track_caller]
pub(super) fn source(fixture: &Fixture) -> GeneratedMediaSourceProof {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let live = fixture.service().live_home_command().unwrap();
        let home = live.home();
        macro_rules! read_source {
            ($operation:literal, $read:expr) => {
                match $read {
                    Ok(value) => value,
                    Err(syndic_storage::SyndicReadError::ConcurrentChange { operation }) => {
                        assert!(std::time::Instant::now() < deadline,
                            "generated source exceeded its polling bound during {}: concurrent change in {operation}", $operation);
                        drop(live);
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("generated source {} failed unexpectedly: {error:?}", $operation),
                }
            };
        }
        let sweep = read_source!("sweep", fixture.storage.begin_generated_media_sweep(home));
        let page = read_source!("pending page", fixture.storage.pending_generated_media_page(home, &sweep, 8, 65_536));
        let proof = if let Some(entry) = page.entries().first() {
            read_source!("proof", fixture.storage.generated_media_source(home, entry.resource_id()))
        } else { None };
        if let Some(proof) = proof.as_ref().filter(|proof| proof.saved_path().is_some()) {
            return proof.clone();
        }
        if std::time::Instant::now() >= deadline {
            let point = syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap();
            let turn = fixture.storage.thread(home, thread_id(), point).unwrap()
                .and_then(|thread| thread.committed_tail());
            let state = turn.map(|turn| fixture.storage.turn_state(home, turn, point));
            let evidence = |name: &str| fs::read_to_string(fixture.root(1).join(name))
                .map(|value| value.chars().take(2048).collect::<String>());
            panic!("generated source did not complete: high_water {}, entries {:?}, source {:?}, turn {:?}, state {:?}, gate {:?}, workers {:?}, scheduler {:?}, cut {:?}, started {:?}, projection {:?}, request {:?}",
                sweep.high_water(), page.entries(), proof.as_ref().map(|proof| proof.source_state()), turn, state,
                fixture.storage.input_gate(home, thread_id(), point),
                fixture.service().worker_pool_diagnostics(), fixture.service().accepted_input_scheduler_diagnostics(),
                fixture.service().persistent_failure_cut_snapshot(), evidence("execution-started-0.json"),
                evidence("execution-projection-0.json"), evidence("execution-request-0.json"));
        }
        drop(live);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn settled_eligibility(
    mut read: impl FnMut() -> Result<(), GeneratedOutputError>,
) -> Result<(), GeneratedOutputError> {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match read() {
            Err(GeneratedOutputError::Busy) => {
                assert!(std::time::Instant::now() < deadline, "original source owner remained busy");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            result => return result,
        }
    }
}

#[test]
fn exact_live_driver_refuses_wrong_historical_nonce_with_equal_generations_and_survives_target_retirement() {
    let (mut fixture, sessions, _attention) = fixture();
    fs::write(fixture.root(1).join("fixture-mode"), "execution-generated").unwrap();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    support::ready(&view);
    submission::submit(&fixture, thread_id());
    wait_until(|| fixture.root(1).join("execution-started-0.json").exists());
    let proof = source(&fixture);
    let incarnation = proof.source_incarnation().expect("real dispatch persists the original loaded owner");
    let generations = proof.execution_snapshot().loaded_generation();
    assert_eq!(settled_eligibility(|| fixture.service().generated_source_eligibility_for_test(&proof)), Ok(()));
    let mut altered = *incarnation.as_bytes();
    altered[31] ^= 1;
    assert_eq!(settled_eligibility(|| fixture.service().generated_source_incarnation_eligibility_for_test(
        &proof, Some(GeneratedSourceIncarnation::from_bytes(altered)))), Err(GeneratedOutputError::LostSourceAuthority));
    assert_eq!(settled_eligibility(|| fixture.service().generated_source_incarnation_eligibility_for_test(&proof, None)),
        Err(GeneratedOutputError::LostSourceAuthority));
    assert_eq!(proof.execution_snapshot().loaded_generation(), generations);
    fs::write(fixture.root(1).join("execution-release-0"), "complete").unwrap();
    wait_until(|| sessions.diagnostics().available == 1 && sessions.diagnostics().checked_out == 0
        && fixture.service().exact_stop_worker().selected_submission_availability(thread_id()) == Ok(()));
    assert!(!fixture.root(1).join("execution-unsubscribed-0").exists());
    let after = source(&fixture);
    assert_eq!(after.source_incarnation(), Some(incarnation));
    assert_eq!(after.execution_snapshot().loaded_generation(), generations);
    assert_eq!(settled_eligibility(|| fixture.service().generated_source_eligibility_for_test(&after)), Ok(()));
    drop(view);
    assert!(matches!(fixture.service.take().unwrap().close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed));
    assert_eq!(sessions.diagnostics().retained, 0);
}
