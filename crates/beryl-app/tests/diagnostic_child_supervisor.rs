#[path = "support/tempdir.rs"]
mod tempdir_support;

pub use beryl_app::{BerylHomeDir, BerylHomeDirError};

#[path = "../src/acceptance_digest.rs"]
mod acceptance_digest;

#[path = "../src/diagnostic_child_protocol.rs"]
mod diagnostic_child_protocol;

#[path = "../src/diagnostic_child_control.rs"]
mod diagnostic_child_control;

// This target path-includes `acceptance_session.rs` to exercise its isolated
// process helpers. Its canonical dynamic-operation implementation depends on
// GUI control request types that this target never executes. Keep the adapter
// type-only so it cannot become a second validation authority.
mod gui_control_dynamic_tools {
    use std::fmt;

    use serde_json::Value;

    pub(crate) const CLOSE_POPUPS_TOOL: &str = "close_popups";
    pub(crate) const DEFAULT_UI_VISIBLE_ROW_LIMIT: usize = 32;
    pub(crate) const MAX_SCROLL_REPEAT: usize = 8;
    pub(crate) const MAX_UI_VISIBLE_ROW_LIMIT: usize = 64;
    pub(crate) const READ_UI_STATE_TOOL: &str = "read_ui_state";
    pub(crate) const SCROLL_TRANSCRIPT_TOOL: &str = "scroll_transcript";
    pub(crate) const SWITCH_THREAD_TOOL: &str = "switch_thread";
    pub(crate) const SWITCH_WORKSPACE_TOOL: &str = "switch_workspace";

    pub(crate) enum GuiControlToolRequest {
        ReadUiState { visible_row_limit: usize },
        SwitchWorkspace(SwitchWorkspaceArguments),
        SwitchThread(SwitchThreadArguments),
        ScrollTranscript(ScrollTranscriptArguments),
        ClosePopups,
    }

    pub(crate) struct SwitchWorkspaceArguments {
        pub(crate) workspace_id: String,
    }

    pub(crate) struct SwitchThreadArguments {
        pub(crate) thread_id: String,
    }

    pub(crate) struct ScrollTranscriptArguments {
        pub(crate) command: String,
        pub(crate) repeat: usize,
    }

    #[derive(Debug)]
    pub(crate) struct GuiControlToolError;

    impl GuiControlToolError {
        pub(crate) fn kind(&self) -> &'static str {
            "test_adapter"
        }
    }

    impl fmt::Display for GuiControlToolError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("GUI control parsing is unavailable in this supervisor-only target")
        }
    }

    pub(crate) fn parse_gui_control_tool_request(
        _tool: &str,
        _arguments: &Value,
    ) -> Result<GuiControlToolRequest, GuiControlToolError> {
        Err(GuiControlToolError)
    }
}

#[path = "../src/diagnostic_child_supervisor.rs"]
mod diagnostic_child_supervisor;

#[path = "../src/diagnostic_child_dynamic_tools.rs"]
mod diagnostic_child_dynamic_tools;

#[path = "../src/acceptance_session.rs"]
mod acceptance_session;

use acceptance_session::executable_identity_for_test;
#[cfg(windows)]
use acceptance_session::{
    AcceptanceCleanupFinalState, AcceptanceLaunchMode, AcceptanceLimits,
    AcceptancePublicationState, AcceptanceRequest, AcceptanceSession, AcceptanceSessionConfig,
};

use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use diagnostic_child_protocol::{
    DIAGNOSTIC_CHILD_PROTOCOL_NAME, DIAGNOSTIC_CHILD_PROTOCOL_VERSION, DiagnosticChildCommand,
};
use diagnostic_child_supervisor::{
    AcceptanceStartupFailureStage, AcceptanceTestObservation, AcceptanceTestPlan,
    DIAGNOSTIC_CHILD_STOP_BUDGET, DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT,
    DiagnosticAcceptanceCleanupRetry, DiagnosticAcceptanceProcessOwner, DiagnosticChildLaunch,
    DiagnosticChildStartOutcome, DiagnosticChildStopOutcome, DiagnosticChildSupervisor,
    DiagnosticChildSupervisorError, SpawnedDiagnosticChildGuard,
    install_child_wait_poll_observer_for_test, same_home_path,
};

#[test]
fn acceptance_sha256_matches_fixed_standard_vectors() {
    let vectors = [
        (
            "",
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
        ),
        (
            "abc",
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
        ),
        (
            "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248D6A61D20638B8E5C026930C3E6039A33CE45964FF2167F6ECEDD419DB06C1",
        ),
    ];
    for (input, expected) in vectors {
        assert_eq!(
            acceptance_digest::Sha256::digest_hex(input.as_bytes()),
            expected
        );
    }
}

#[test]
fn acceptance_sha256_matches_fixed_block_boundary_vectors() {
    let vectors = [
        (
            55,
            "9F4390F8D30C2DD92EC9F095B65E2B9AE9B0A925A5258E241C9F1E910F734318",
        ),
        (
            56,
            "B35439A4AC6F0948B6D6F9E3C6AF0F5F590CE20F1BDE7090EF7970686EC6738A",
        ),
        (
            63,
            "7D3E74A05D7DB15BCE4AD9EC0658EA98E3F06EEECF16B4C6FFF2DA457DDC2F34",
        ),
        (
            64,
            "FFE054FE7AE0CB6DC65C3AF9B61D5209F439851DB43D0BA5997337DF154668EB",
        ),
        (
            65,
            "635361C48BB9EAB14198E76EA8AB7F1A41685D6AD62AA9146D301D4F17EB0AE0",
        ),
    ];
    for (length, expected) in vectors {
        assert_eq!(
            acceptance_digest::Sha256::digest_hex(&vec![b'a'; length]),
            expected
        );
    }
}

#[test]
fn acceptance_sha256_incremental_chunks_match_fixed_vector() {
    let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    let mut sha256 = acceptance_digest::Sha256::new();
    for chunk in input.chunks(7) {
        sha256.update(chunk);
    }
    assert_eq!(
        sha256.finalize_hex(),
        "248D6A61D20638B8E5C026930C3E6039A33CE45964FF2167F6ECEDD419DB06C1"
    );
}

#[test]
fn acceptance_executable_identity_counts_the_exact_hashed_stream() {
    let root = tempdir_support::temp_dir("beryl-acceptance-executable-identity-");
    let executable = root.join("fixture.bin");
    fs::write(&executable, b"deterministic executable fixture\n").unwrap();

    let (bytes, sha256) = executable_identity_for_test(&executable).unwrap();

    assert_eq!(bytes, 33);
    assert_eq!(
        sha256,
        "0767D3B49A0BB445E3ECC74A6BDB1D88515782F54E3418CB131EA5B484D48955"
    );
    root.close().unwrap();
}

#[test]
fn start_rejects_supervisor_home_as_child_home() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();

    let launch = DiagnosticChildLaunch::new(root.path(), PathBuf::from("not-needed"));
    let error = supervisor.start(&home, launch).unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::HomeCollidesWithSupervisor { .. }
    ));
}

#[test]
fn start_rejects_invalid_executable_paths_before_spawn() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let directory = tempdir_support::temp_dir("beryl-diagnostic-executable-dir-");
    let over_limit = root.path().join("x".repeat(1100));
    let cases = [
        (
            PathBuf::new(),
            "empty executable path should be rejected as invalid",
        ),
        (
            PathBuf::from("relative-beryl.exe"),
            "relative executable path should be rejected as invalid",
        ),
        (
            over_limit,
            "over-limit executable path should be rejected as invalid",
        ),
        (
            directory.path().to_path_buf(),
            "directory executable path should be rejected as invalid",
        ),
    ];

    for (path, message) in cases {
        let mut supervisor = DiagnosticChildSupervisor::default();
        let launch = DiagnosticChildLaunch::new(child.path(), path);
        let error = supervisor.start(&home, launch).unwrap_err();
        assert!(
            matches!(
                error,
                DiagnosticChildSupervisorError::InvalidExecutablePath { .. }
            ),
            "{message}: {error}"
        );
        assert!(!supervisor.has_child_for_test());
    }

    let mut supervisor = DiagnosticChildSupervisor::default();
    let missing = root.path().join("missing-beryl.exe");
    let launch = DiagnosticChildLaunch::new(child.path(), missing);
    let error = supervisor.start(&home, launch).unwrap_err();
    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::ExecutablePathAccess { .. }
    ));
    assert!(!supervisor.has_child_for_test());

    directory.close().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn launch_preserves_legacy_arguments_and_forwards_optional_host_workspace() {
    for with_workspace in [false, true] {
        let root = tempdir_support::temp_dir("beryl-diagnostic-launch-args-");
        let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
        let workspace = root.join("execution workspace");
        fs::create_dir(&workspace).unwrap();
        let args_path = root.join(if with_workspace {
            "args-with-workspace.txt"
        } else {
            "args-legacy.txt"
        });
        let executable = fake_argument_child_executable(root.path(), &args_path);
        let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
        let mut launch = DiagnosticChildLaunch::new(child.path(), executable);
        if with_workspace {
            launch = launch.with_host_workspace(&workspace);
        }
        let mut supervisor = DiagnosticChildSupervisor::default();
        supervisor.start(&home, launch).unwrap();
        supervisor.stop().unwrap();

        let args = fs::read_to_string(&args_path).unwrap();
        assert!(args.contains("--diagnostic-target-stdio"));
        assert!(args.contains("--beryl-home-dir"));
        assert!(!args.contains("--diagnostic-acceptance-startup-gate"));
        assert_eq!(
            args.matches("--host-path").count(),
            usize::from(with_workspace)
        );
        if with_workspace {
            assert!(args.contains(workspace.to_string_lossy().as_ref()));
        }
        child.close().unwrap();
        root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn acceptance_gate_is_held_until_job_assignment_completes() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-gate-order-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-gate-home-");
    let marker = root.join("gate-released.txt");
    let executable = fake_gated_child_executable(root.path(), &marker, false);
    let launch = DiagnosticChildLaunch::new(child.path(), executable);
    let (assigned_sender, assigned_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let test_plan = AcceptanceTestPlan::new().with_job_assignment(
        assigned_sender,
        Some(release_receiver),
        false,
    );

    let started = thread::spawn(move || {
        let mut supervisor = DiagnosticChildSupervisor::default();
        let result = supervisor.start_for_acceptance_with_test_plan(
            launch,
            Duration::from_secs(2),
            Duration::from_millis(10),
            Duration::from_secs(1),
            test_plan,
        );
        (supervisor, result)
    });
    let _pid = assigned_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    assert!(
        !marker.exists(),
        "target crossed its gate before Job assignment released"
    );
    release_sender.send(()).unwrap();
    let (mut supervisor, result) = started.join().unwrap();
    assert!(matches!(
        result,
        Ok(DiagnosticChildStartOutcome::Started(_))
    ));
    assert!(marker.exists());
    supervisor.stop().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn acceptance_test_plans_isolate_simultaneous_launch_lanes() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-plan-lanes-");
    let child_a = tempdir_support::temp_dir("beryl-diagnostic-plan-lane-a-home-");
    let child_b = tempdir_support::temp_dir("beryl-diagnostic-plan-lane-b-home-");
    let child_c = tempdir_support::temp_dir("beryl-diagnostic-plan-lane-c-home-");
    let lane_a = root.join("lane-a");
    let lane_b = root.join("lane-b");
    let lane_c = root.join("lane-c");
    fs::create_dir_all(&lane_a).unwrap();
    fs::create_dir_all(&lane_b).unwrap();
    fs::create_dir_all(&lane_c).unwrap();

    let (spawned_a, reached_a) = mpsc::sync_channel(1);
    let (release_a, wait_a) = mpsc::sync_channel(0);
    let (observed_a, observations_a) = mpsc::sync_channel(8);
    let plan_a = AcceptanceTestPlan::new()
        .with_spawn_barrier(spawned_a, wait_a)
        .with_startup_failure(AcceptanceStartupFailureStage::JobConfigure)
        .with_cleanup_failures(1)
        .with_observer(observed_a);
    let launch_a = DiagnosticChildLaunch::new(
        child_a.path(),
        fake_gated_child_executable(&lane_a, &lane_a.join("gate.txt"), false),
    );

    let (spawned_b, reached_b) = mpsc::sync_channel(1);
    let (release_b, wait_b) = mpsc::sync_channel(0);
    let (observed_b, observations_b) = mpsc::sync_channel(8);
    let plan_b = AcceptanceTestPlan::new()
        .with_spawn_barrier(spawned_b, wait_b)
        .with_startup_failure(AcceptanceStartupFailureStage::GateWrite)
        .with_cleanup_failures(2)
        .with_observer(observed_b);
    let launch_b = DiagnosticChildLaunch::new(
        child_b.path(),
        fake_gated_child_executable(&lane_b, &lane_b.join("gate.txt"), false),
    );

    let (spawned_c, reached_c) = mpsc::sync_channel(1);
    let (release_c, wait_c) = mpsc::sync_channel(0);
    let (observed_c, observations_c) = mpsc::sync_channel(8);
    let plan_c = AcceptanceTestPlan::new()
        .with_spawn_barrier(spawned_c, wait_c)
        .with_startup_failure(AcceptanceStartupFailureStage::Handshake)
        .with_cleanup_failures(3)
        .with_observer(observed_c);
    let launch_c = DiagnosticChildLaunch::new(
        child_c.path(),
        fake_gated_child_executable(&lane_c, &lane_c.join("gate.txt"), false),
    );

    let run_lane = |launch, plan| {
        thread::spawn(move || {
            DiagnosticChildSupervisor::default().start_for_acceptance_with_test_plan(
                launch,
                Duration::from_secs(2),
                Duration::from_millis(1),
                Duration::from_millis(1),
                plan,
            )
        })
    };
    let lane_a = run_lane(launch_a, plan_a);
    let lane_b = run_lane(launch_b, plan_b);
    let lane_c = run_lane(launch_c, plan_c);

    let pid_a = reached_a.recv_timeout(Duration::from_secs(1)).unwrap();
    let pid_b = reached_b.recv_timeout(Duration::from_secs(1)).unwrap();
    let pid_c = reached_c.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_ne!(pid_a, pid_b);
    assert_ne!(pid_b, pid_c);
    assert_ne!(pid_a, pid_c);
    let exact_a = ExactWindowsProcess::open_while_known_live(pid_a);
    let exact_b = ExactWindowsProcess::open_while_known_live(pid_b);
    let exact_c = ExactWindowsProcess::open_while_known_live(pid_c);

    release_c.send(()).unwrap();
    release_a.send(()).unwrap();
    release_b.send(()).unwrap();
    let failure_a = lane_a.join().unwrap().unwrap_err();
    let failure_b = lane_b.join().unwrap().unwrap_err();
    let failure_c = lane_c.join().unwrap().unwrap_err();
    let (cause_a, initial_a, owner_a) = failure_a.into_parts();
    let (cause_b, initial_b, owner_b) = failure_b.into_parts();
    let (cause_c, initial_c, owner_c) = failure_c.into_parts();
    assert!(matches!(
        cause_a,
        DiagnosticChildSupervisorError::ConfigureProcessJob { .. }
    ));
    assert!(matches!(
        cause_b,
        DiagnosticChildSupervisorError::WriteRequest { .. }
    ));
    assert!(matches!(
        cause_c,
        DiagnosticChildSupervisorError::StartupProtocolIncompatible { .. }
    ));
    assert!(initial_a.is_some() && initial_b.is_some() && initial_c.is_some());
    assert_eq!(
        observations_a.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::StartupFailureConsumed {
            pid: pid_a,
            stage: AcceptanceStartupFailureStage::JobConfigure,
        }
    );
    assert_eq!(
        observations_b.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::JobConfigured { pid: pid_b }
    );
    assert_eq!(
        observations_b.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::StartupFailureConsumed {
            pid: pid_b,
            stage: AcceptanceStartupFailureStage::GateWrite,
        }
    );
    assert_eq!(
        observations_c.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::JobConfigured { pid: pid_c }
    );
    assert_eq!(
        observations_c.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::GateWriteCompleted { pid: pid_c }
    );
    assert_eq!(
        observations_c.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::StartupFailureConsumed {
            pid: pid_c,
            stage: AcceptanceStartupFailureStage::Handshake,
        }
    );

    reclaim_lane_owner(owner_a.unwrap(), pid_a, 1);
    reclaim_lane_owner(owner_b.unwrap(), pid_b, 2);
    reclaim_lane_owner(owner_c.unwrap(), pid_c, 3);
    assert_lane_cleanup_observations(&observations_a, pid_a, 1);
    assert_lane_cleanup_observations(&observations_b, pid_b, 2);
    assert_lane_cleanup_observations(&observations_c, pid_c, 3);
    exact_a.assert_exited();
    exact_b.assert_exited();
    exact_c.assert_exited();
    child_a.close().unwrap();
    child_b.close().unwrap();
    child_c.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn acceptance_ready_frame_must_precede_handshake_request() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-ready-order-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-ready-order-home-");
    let premature_handshake = root.join("premature-handshake.txt");
    let executable =
        fake_ready_before_handshake_child_executable(root.path(), &premature_handshake);
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor.mark_non_gate_writes_for_test(premature_handshake);

    let result = supervisor.start_for_acceptance(
        DiagnosticChildLaunch::new(child.path(), executable),
        Duration::from_secs(2),
        Duration::from_millis(10),
        Duration::from_secs(1),
    );

    if let Err(error) = result {
        panic!(
            "acceptance startup must wait for the ready frame before sending the handshake: {}",
            error.cause_for_test()
        );
    }
    supervisor.stop().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn post_assignment_setup_failure_cleans_direct_child_before_gate_release() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-gate-failure-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-gate-home-");
    let marker = root.join("gate-released.txt");
    let executable = fake_gated_child_executable(root.path(), &marker, true);
    let launch = DiagnosticChildLaunch::new(child.path(), executable);
    let (assigned_sender, assigned_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let test_plan = AcceptanceTestPlan::new().with_job_assignment(
        assigned_sender,
        Some(release_receiver),
        true,
    );

    let started = thread::spawn(move || {
        DiagnosticChildSupervisor::default().start_for_acceptance_with_test_plan(
            launch,
            Duration::from_secs(1),
            Duration::ZERO,
            Duration::from_secs(1),
            test_plan,
        )
    });
    let pid = assigned_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    let direct_child = ExactWindowsProcess::open_while_known_live(pid);
    release_sender.send(()).unwrap();
    let error = started.join().unwrap().unwrap_err();
    assert!(matches!(
        error.cause_for_test(),
        DiagnosticChildSupervisorError::AssignProcessToJob { .. }
    ));
    assert!(!marker.exists(), "failed setup released the target gate");
    direct_child.assert_exited();
    child.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn acceptance_startup_failure_transfers_exact_owner_for_bounded_retries() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-owner-transfer-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-owner-home-");
    let marker = root.join("gate-released.txt");
    let executable = fake_gated_child_executable(root.path(), &marker, false);
    let launch = DiagnosticChildLaunch::new(child.path(), executable);
    let (assigned_sender, assigned_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let test_plan = AcceptanceTestPlan::new()
        .with_job_assignment(assigned_sender, Some(release_receiver), true)
        .with_cleanup_failures(2);

    let started_at = Instant::now();
    let started = thread::spawn(move || {
        DiagnosticChildSupervisor::default().start_for_acceptance_with_test_plan(
            launch,
            Duration::from_secs(1),
            Duration::from_millis(1),
            Duration::from_millis(1),
            test_plan,
        )
    });
    let pid = assigned_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    let direct_child = ExactWindowsProcess::open_while_known_live(pid);
    release_sender.send(()).unwrap();
    let failure = started.join().unwrap().unwrap_err();
    assert!(started_at.elapsed() < Duration::from_secs(2));
    let (cause, initial_cleanup_error, mut owner) = failure.into_parts();
    assert!(matches!(
        cause,
        DiagnosticChildSupervisorError::AssignProcessToJob { .. }
    ));
    assert!(initial_cleanup_error.is_some());
    let mut owner = owner.take().expect("expired cleanup transfers owner");
    assert_eq!(owner.identity().pid, pid);

    assert!(matches!(
        owner.retry_cleanup(Duration::ZERO, Duration::from_millis(3)),
        DiagnosticAcceptanceCleanupRetry::StillRetained {
            identity,
            ..
        } if identity.pid == pid
    ));
    direct_child.assert_still_active();
    assert!(matches!(
        owner.retry_cleanup(Duration::ZERO, Duration::from_secs(2)),
        DiagnosticAcceptanceCleanupRetry::Reclaimed(identity) if identity.pid == pid
    ));
    assert!(matches!(
        owner.retry_cleanup(Duration::ZERO, Duration::from_millis(1)),
        DiagnosticAcceptanceCleanupRetry::AlreadyReclaimed
    ));
    direct_child.assert_exited();
    assert!(
        !marker.exists(),
        "failed setup never released the target gate"
    );
    child.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn every_injected_startup_setup_failure_retains_or_terminally_cleans_resources() {
    for stage in [
        AcceptanceStartupFailureStage::JobCreate,
        AcceptanceStartupFailureStage::JobConfigure,
        AcceptanceStartupFailureStage::JobAssign,
        AcceptanceStartupFailureStage::WriterSpawn,
        AcceptanceStartupFailureStage::StdoutReaderSpawn,
        AcceptanceStartupFailureStage::StderrReaderSpawn,
        AcceptanceStartupFailureStage::GateWrite,
        AcceptanceStartupFailureStage::GateReady,
        AcceptanceStartupFailureStage::Handshake,
    ] {
        let root = tempdir_support::temp_dir("beryl-diagnostic-stage-owner-");
        let child = tempdir_support::temp_dir("beryl-diagnostic-stage-home-");
        let marker = root.join("gate-released.txt");
        let executable = fake_gated_child_executable(root.path(), &marker, false);
        let launch = DiagnosticChildLaunch::new(child.path(), executable);
        let (spawned_sender, spawned_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::sync_channel(0);
        let (observation_sender, observation_receiver) = mpsc::sync_channel(8);
        let test_plan = AcceptanceTestPlan::new()
            .with_spawn_barrier(spawned_sender, release_receiver)
            .with_startup_failure(stage);
        let test_plan = if stage == AcceptanceStartupFailureStage::GateWrite {
            test_plan.with_startup_failure(AcceptanceStartupFailureStage::GateReady)
        } else {
            test_plan
        };
        let test_plan = test_plan
            .with_cleanup_failures(1)
            .with_observer(observation_sender);
        let started = thread::spawn(move || {
            DiagnosticChildSupervisor::default().start_for_acceptance_with_test_plan(
                launch,
                Duration::from_secs(1),
                Duration::from_millis(1),
                Duration::from_millis(1),
                test_plan,
            )
        });
        let pid = spawned_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        let direct_child = ExactWindowsProcess::open_while_known_live(pid);
        release_sender.send(()).unwrap();
        let failure = started.join().unwrap().unwrap_err();
        let (cause, initial_cleanup_error, owner) = failure.into_parts();
        assert!(initial_cleanup_error.is_some(), "stage {stage:?}");
        if stage == AcceptanceStartupFailureStage::JobAssign {
            assert_eq!(
                observation_receiver
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap(),
                AcceptanceTestObservation::JobConfigured { pid },
                "injected Job assignment failure must occur after Job configuration"
            );
        }
        match stage {
            AcceptanceStartupFailureStage::JobCreate => assert!(matches!(
                cause,
                DiagnosticChildSupervisorError::CreateProcessJob { .. }
            )),
            AcceptanceStartupFailureStage::JobConfigure => assert!(matches!(
                cause,
                DiagnosticChildSupervisorError::ConfigureProcessJob { .. }
            )),
            AcceptanceStartupFailureStage::JobAssign => assert!(matches!(
                cause,
                DiagnosticChildSupervisorError::AssignProcessToJob { .. }
            )),
            AcceptanceStartupFailureStage::WriterSpawn => assert!(matches!(
                cause,
                DiagnosticChildSupervisorError::SpawnWriter { .. }
            )),
            AcceptanceStartupFailureStage::StdoutReaderSpawn => assert!(matches!(
                cause,
                DiagnosticChildSupervisorError::SpawnStdoutReader { .. }
            )),
            AcceptanceStartupFailureStage::StderrReaderSpawn => assert!(matches!(
                cause,
                DiagnosticChildSupervisorError::SpawnStderrReader { .. }
            )),
            AcceptanceStartupFailureStage::GateWrite => assert!(matches!(
                cause,
                DiagnosticChildSupervisorError::WriteRequest { .. }
            )),
            AcceptanceStartupFailureStage::GateReady | AcceptanceStartupFailureStage::Handshake => {
                assert!(matches!(
                    cause,
                    DiagnosticChildSupervisorError::StartupProtocolIncompatible { .. }
                ))
            }
        }
        let mut owner = owner.expect("forced cleanup expiry transfers staged resources");
        assert_eq!(owner.identity().pid, pid);
        if matches!(stage, AcceptanceStartupFailureStage::WriterSpawn) {
            assert!(
                owner.owns_raw_stdin_for_test(),
                "writer spawn failure must transfer the exact raw stdin pipe"
            );
        }
        if matches!(stage, AcceptanceStartupFailureStage::StdoutReaderSpawn) {
            assert!(
                owner.owns_raw_stdout_for_test(),
                "stdout reader spawn failure must transfer the exact raw stdout pipe"
            );
        }
        if matches!(stage, AcceptanceStartupFailureStage::StderrReaderSpawn) {
            assert!(
                owner.owns_raw_stderr_for_test(),
                "stderr reader spawn failure must transfer the exact raw stderr pipe"
            );
        }
        if !matches!(stage, AcceptanceStartupFailureStage::JobCreate) {
            assert!(
                owner.owns_job_for_test(),
                "stage {stage:?} must retain its exact created Job through cleanup retry"
            );
        }
        assert!(matches!(
            owner.retry_cleanup(Duration::ZERO, Duration::from_secs(2)),
            DiagnosticAcceptanceCleanupRetry::Reclaimed(identity) if identity.pid == pid
        ));
        let observations = observation_receiver.try_iter().collect::<Vec<_>>();
        if stage == AcceptanceStartupFailureStage::GateWrite {
            assert!(
                observations.contains(&AcceptanceTestObservation::StartupFailureConsumed {
                    pid,
                    stage: AcceptanceStartupFailureStage::GateWrite,
                })
            );
            assert!(!observations.iter().any(|observation| matches!(
                observation,
                AcceptanceTestObservation::GateWriteCompleted { .. }
            )));
            assert!(!observations.iter().any(|observation| matches!(
                observation,
                AcceptanceTestObservation::StartupFailureConsumed {
                    stage: AcceptanceStartupFailureStage::GateReady,
                    ..
                }
            )));
        }
        if stage == AcceptanceStartupFailureStage::GateReady {
            let gate_write = observations
                .iter()
                .position(|observation| {
                    *observation == AcceptanceTestObservation::GateWriteCompleted { pid }
                })
                .expect("gate write completion must be observed before ready-frame injection");
            assert_eq!(
                observations.get(gate_write + 1),
                Some(&AcceptanceTestObservation::StartupFailureConsumed {
                    pid,
                    stage: AcceptanceStartupFailureStage::GateReady,
                }),
                "GateReady must be consumed on entry to ready-frame handling"
            );
        }
        direct_child.assert_exited();
        if !matches!(
            stage,
            AcceptanceStartupFailureStage::GateReady | AcceptanceStartupFailureStage::Handshake
        ) {
            assert!(
                !marker.exists(),
                "stage {stage:?} released the startup gate"
            );
        }
        child.close().unwrap();
        root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn bounded_join_timeout_retains_exact_job_for_cleanup_retry() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-job-retry-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-job-retry-home-");
    let marker = root.join("gate-released.txt");
    let executable = fake_gated_child_executable(root.path(), &marker, false);
    let launch = DiagnosticChildLaunch::new(child.path(), executable);
    let (spawned_sender, spawned_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let test_plan = AcceptanceTestPlan::new()
        .with_spawn_barrier(spawned_sender, release_receiver)
        .with_startup_failure(AcceptanceStartupFailureStage::Handshake)
        .with_cleanup_failures(1);
    let started = thread::spawn(move || {
        DiagnosticChildSupervisor::default().start_for_acceptance_with_test_plan(
            launch,
            Duration::from_secs(1),
            Duration::from_millis(1),
            Duration::from_millis(1),
            test_plan,
        )
    });
    let pid = spawned_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    release_sender.send(()).unwrap();
    let failure = started.join().unwrap().unwrap_err();
    let (_, _, owner) = failure.into_parts();
    let mut owner = owner.expect("forced initial cleanup failure transfers owner");
    assert!(owner.owns_job_for_test());
    owner.force_writer_join_timeout_once_for_test();

    assert!(matches!(
        owner.retry_cleanup(Duration::ZERO, Duration::from_secs(1)),
        DiagnosticAcceptanceCleanupRetry::StillRetained { identity, .. }
            if identity.pid == pid
    ));
    assert!(
        owner.owns_job_for_test(),
        "join timeout must retain the exact Job for the next bounded retry"
    );
    assert!(matches!(
        owner.retry_cleanup(Duration::ZERO, Duration::from_secs(1)),
        DiagnosticAcceptanceCleanupRetry::Reclaimed(identity) if identity.pid == pid
    ));
    child.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn dropping_unconsumed_startup_owner_closes_job_without_timed_retry() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-owner-drop-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-owner-drop-home-");
    let descendant_pid_path = root.join("descendant.pid");
    let executable =
        fake_gated_incompatible_descendant_executable(root.path(), &descendant_pid_path);
    let launch = DiagnosticChildLaunch::new(child.path(), executable);
    let (assigned_sender, assigned_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let (observation_sender, observation_receiver) = mpsc::sync_channel(4);
    let test_plan = AcceptanceTestPlan::new()
        .with_job_assignment(assigned_sender, Some(release_receiver), false)
        .with_cleanup_failures(2)
        .with_observer(observation_sender);

    let started = thread::spawn(move || {
        DiagnosticChildSupervisor::default().start_for_acceptance_with_test_plan(
            launch,
            Duration::from_secs(2),
            Duration::from_millis(1),
            Duration::from_millis(1),
            test_plan,
        )
    });
    let pid = assigned_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    release_sender.send(()).unwrap();
    let failure = started.join().unwrap().unwrap_err();
    let (_, initial_cleanup_error, owner) = failure.into_parts();
    assert!(initial_cleanup_error.is_some());
    let owner = owner.expect("forced initial expiry retains exact owner");
    let descendant_pid = fs::read_to_string(&descendant_pid_path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let descendant = ExactWindowsProcess::open_while_known_live(descendant_pid);
    assert_ne!(descendant.creation_identity(), 0);
    descendant.assert_still_active();

    let dropped_at = Instant::now();
    drop(owner);
    assert!(dropped_at.elapsed() < Duration::from_millis(100));
    assert_eq!(
        observation_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap(),
        AcceptanceTestObservation::JobConfigured { pid }
    );
    assert_eq!(
        observation_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap(),
        AcceptanceTestObservation::GateWriteCompleted { pid }
    );
    assert_eq!(
        observation_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap(),
        AcceptanceTestObservation::CleanupAttempt {
            pid,
            ordinal: 1,
            forced_failure: true,
            remaining_forced_failures: 1,
        }
    );
    assert_eq!(
        observation_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap(),
        AcceptanceTestObservation::FailSafeRelease { pid },
        "owner Drop must release fail-safe without a timed cleanup retry"
    );
    assert!(observation_receiver.try_recv().is_err());
    descendant.wait_until_exited(Duration::from_secs(2));
    descendant.assert_exited();
    child.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn acceptance_finish_recovers_before_publishing_and_records_both_attempts() {
    let root = tempdir_support::temp_dir("beryl-acceptance-finish-recovery-");
    let (session, evidence_path) = terminal_acceptance_session(
        root.path(),
        Duration::from_secs(1),
        AcceptanceTestPlan::new().with_cleanup_failures(1),
    );
    let pid = session.diagnostic_child_pid_for_test();
    let exact_child = ExactWindowsProcess::open_while_known_live(pid);

    let outcome = session.finish();

    assert!(matches!(
        outcome.cleanup(),
        AcceptanceCleanupFinalState::VerifiedReclaimed
    ));
    assert!(matches!(
        outcome.publication(),
        AcceptancePublicationState::Published
    ));
    assert_eq!(outcome.evidence().cleanup.attempts.len(), 2);
    assert_eq!(outcome.evidence().cleanup.attempts[0].ordinal, 1);
    assert_eq!(
        outcome.evidence().cleanup.attempts[0].residue,
        "indeterminate"
    );
    let cleanup_error = outcome.evidence().cleanup.attempts[0]
        .error
        .as_ref()
        .unwrap();
    assert_eq!(
        cleanup_error.prefix_bytes,
        cleanup_error.bounded_prefix.len()
    );
    assert_eq!(cleanup_error.total_bytes, 67);
    assert_eq!(cleanup_error.prefix_bytes, 64);
    assert_eq!(
        cleanup_error.sha256,
        "AA8EDE5ECC11467A89DC71C357E8206CA2A55922743767BB1D1B3EE0C00D2BC2"
    );
    assert!(cleanup_error.truncated);
    assert_eq!(outcome.evidence().cleanup.attempts[1].ordinal, 2);
    assert_eq!(
        outcome.evidence().cleanup.attempts[1].residue,
        "verified_reclaimed"
    );
    assert!(evidence_path.is_file());
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(&evidence_path).unwrap()).unwrap();
    assert_eq!(persisted["schemaVersion"].as_u64(), Some(5));
    assert_eq!(
        persisted["cleanup"]["finalState"].as_str(),
        Some("verified_reclaimed")
    );
    let persisted_attempts = persisted["cleanup"]["attempts"].as_array().unwrap();
    assert_eq!(persisted_attempts.len(), 2);
    assert_eq!(persisted_attempts[0]["ordinal"].as_u64(), Some(1));
    assert_eq!(
        persisted_attempts[0]["residue"].as_str(),
        Some("indeterminate")
    );
    assert_eq!(
        persisted_attempts[0]["error"]["totalBytes"].as_u64(),
        Some(67)
    );
    assert_eq!(persisted_attempts[1]["ordinal"].as_u64(), Some(2));
    assert_eq!(
        persisted_attempts[1]["residue"].as_str(),
        Some("verified_reclaimed")
    );
    assert!(persisted_attempts[1]["error"].is_null());
    assert_eq!(
        persisted["publication"]["outcome"].as_str(),
        Some("published")
    );
    assert!(persisted["publication"]["error"].is_null());
    exact_child.wait_until_exited(Duration::from_secs(2));
    exact_child.assert_exited();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn acceptance_budget_is_shared_by_request_cleanup_error_and_stderr() {
    let root = tempdir_support::temp_dir("beryl-acceptance-shared-budget-");
    let mut session = terminal_budget_acceptance_session(
        root.path(),
        AcceptanceTestPlan::new().with_cleanup_failures(1),
    );
    let pid = session.diagnostic_child_pid_for_test();
    let exact_child = ExactWindowsProcess::open_while_known_live(pid);

    let request_error = session
        .request(AcceptanceRequest::new("read_process", serde_json::json!({})).unwrap())
        .unwrap_err();
    assert!(request_error.to_string().contains("after 100ms"));
    let outcome = session.finish();
    let evidence = outcome.evidence();
    assert!(matches!(
        outcome.cleanup(),
        AcceptanceCleanupFinalState::VerifiedReclaimed
    ));
    assert_eq!(evidence.requests.len(), 1);
    let request = evidence.requests[0].error.as_ref().unwrap();
    assert_eq!(request.total_bytes, 68);
    assert_eq!(
        request.sha256,
        "7673686531D6A28B250E61E06EDB9FBDAD64B360F8733EFE9DDAD38D2D92D9DE"
    );
    assert_eq!(request.prefix_bytes, 68);
    assert!(!request.truncated);

    assert_eq!(evidence.cleanup.attempts.len(), 2);
    let cleanup = evidence.cleanup.attempts[0].error.as_ref().unwrap();
    assert_eq!(cleanup.total_bytes, 67);
    assert_eq!(
        cleanup.sha256,
        "AA8EDE5ECC11467A89DC71C357E8206CA2A55922743767BB1D1B3EE0C00D2BC2"
    );
    assert_eq!(cleanup.prefix_bytes, 67);
    assert!(!cleanup.truncated);
    assert!(evidence.cleanup.attempts[1].error.is_none());

    assert_eq!(evidence.stderr.total_bytes, 14);
    assert_eq!(
        evidence.stderr.sha256,
        "510CE5870B1E023349550DBFB10C1DDB3DD060C3CD0A5F3BBD85A4503671D5EB"
    );
    assert_eq!(evidence.stderr.bounded_prefix, "f");
    assert_eq!(evidence.stderr.prefix_bytes, 1);
    assert!(evidence.stderr.truncated);
    assert!(evidence.stderr.capture_complete);
    assert_eq!(
        request.prefix_bytes + cleanup.prefix_bytes + evidence.stderr.prefix_bytes,
        136
    );

    exact_child.wait_until_exited(Duration::from_secs(2));
    exact_child.assert_exited();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn acceptance_finish_persists_indeterminate_cleanup_and_retains_exact_owner() {
    let root = tempdir_support::temp_dir("beryl-acceptance-finish-retained-");
    let (session, evidence_path) = terminal_acceptance_session(
        root.path(),
        Duration::from_millis(50),
        AcceptanceTestPlan::new().with_cleanup_failures(2),
    );
    let pid = session.diagnostic_child_pid_for_test();
    let exact_child = ExactWindowsProcess::open_while_known_live(pid);
    let mut outcome = session.finish();

    assert!(matches!(
        outcome.cleanup(),
        AcceptanceCleanupFinalState::Indeterminate { identity } if identity.pid() == pid
    ));
    assert!(matches!(
        outcome.publication(),
        AcceptancePublicationState::Published
    ));
    assert_eq!(outcome.evidence().cleanup.final_state, "indeterminate");
    assert_eq!(outcome.evidence().cleanup.attempts.len(), 2);
    let first_error = outcome.evidence().cleanup.attempts[0]
        .error
        .as_ref()
        .unwrap();
    let second_error = outcome.evidence().cleanup.attempts[1]
        .error
        .as_ref()
        .unwrap();
    assert_eq!(first_error.prefix_bytes, first_error.total_bytes.min(64));
    assert_eq!(
        second_error.prefix_bytes,
        second_error
            .total_bytes
            .min(64_usize.saturating_sub(first_error.prefix_bytes))
    );
    assert_eq!(
        first_error.truncated,
        first_error.prefix_bytes < first_error.total_bytes
    );
    assert_eq!(
        second_error.truncated,
        second_error.prefix_bytes < second_error.total_bytes
    );
    assert_eq!(first_error.sha256.len(), 64);
    assert_eq!(second_error.sha256.len(), 64);
    assert_eq!(outcome.evidence().stderr.prefix_bytes, 0);
    assert_eq!(
        outcome
            .evidence()
            .cleanup
            .retained_process
            .as_ref()
            .unwrap()
            .pid,
        pid
    );
    assert!(evidence_path.is_file());
    exact_child.assert_still_active();
    assert_eq!(
        outcome
            .release_owner_fail_safe_nonblocking()
            .expect("terminal outcome retained exact owner")
            .pid(),
        pid
    );
    exact_child.wait_until_exited(Duration::from_secs(2));
    exact_child.assert_exited();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn acceptance_finish_reports_combined_cleanup_and_publication_failure_without_clobber() {
    let root = tempdir_support::temp_dir("beryl-acceptance-finish-combined-");
    let (session, evidence_path) = terminal_acceptance_session(
        root.path(),
        Duration::from_millis(50),
        AcceptanceTestPlan::new().with_cleanup_failures(2),
    );
    let pid = session.diagnostic_child_pid_for_test();
    let exact_child = ExactWindowsProcess::open_while_known_live(pid);
    fs::write(&evidence_path, b"operator-owned").unwrap();
    let mut outcome = session.finish();

    assert!(matches!(
        outcome.cleanup(),
        AcceptanceCleanupFinalState::Indeterminate { identity } if identity.pid() == pid
    ));
    assert!(matches!(
        outcome.publication(),
        AcceptancePublicationState::Failed { .. }
    ));
    assert_eq!(outcome.evidence().cleanup.final_state, "indeterminate");
    assert_eq!(outcome.evidence().publication.outcome, "failed");
    let publication_error = outcome.evidence().publication.error.as_ref().unwrap();
    assert_eq!(
        publication_error.total_bytes,
        publication_error.bounded_prefix.len()
    );
    assert_eq!(
        publication_error.prefix_bytes,
        publication_error.bounded_prefix.len()
    );
    assert!(publication_error.prefix_bytes <= 4 * 1024);
    assert_eq!(publication_error.sha256.len(), 64);
    assert!(!publication_error.truncated);
    let AcceptancePublicationState::Failed { error } = outcome.publication() else {
        unreachable!("publication outcome was already asserted failed");
    };
    assert_eq!(error, &publication_error.bounded_prefix);
    assert_eq!(fs::read(&evidence_path).unwrap(), b"operator-owned");
    assert_eq!(
        outcome
            .release_owner_fail_safe_nonblocking()
            .expect("combined failure retained exact owner")
            .pid(),
        pid
    );
    exact_child.wait_until_exited(Duration::from_secs(2));
    exact_child.assert_exited();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn terminal_and_unfinished_drops_never_start_hidden_timed_cleanup_retries() {
    let root = tempdir_support::temp_dir("beryl-acceptance-finish-drop-");
    let (finished_observer, finished_observations) = mpsc::sync_channel(8);
    let finished_plan = AcceptanceTestPlan::new()
        .with_cleanup_failures(3)
        .with_observer(finished_observer);
    let (session, evidence_path) =
        terminal_acceptance_session(root.path(), Duration::from_millis(50), finished_plan);
    let pid = session.diagnostic_child_pid_for_test();
    let exact_child = ExactWindowsProcess::open_while_known_live(pid);
    let outcome = session.finish();
    assert!(
        evidence_path.is_file(),
        "finish must publish terminal evidence"
    );
    drop(outcome);
    assert_cleanup_observations(&finished_observations, pid, &[(1, 2), (2, 1)], true);
    exact_child.wait_until_exited(Duration::from_secs(2));
    exact_child.assert_exited();

    let (unfinished_observer, unfinished_observations) = mpsc::sync_channel(8);
    let unfinished_plan = AcceptanceTestPlan::new()
        .with_cleanup_failures(2)
        .with_observer(unfinished_observer);
    let (session, _) =
        terminal_acceptance_session(root.path(), Duration::from_millis(50), unfinished_plan);
    let pid = session.diagnostic_child_pid_for_test();
    let exact_child = ExactWindowsProcess::open_while_known_live(pid);
    drop(session);
    assert_cleanup_observations(&unfinished_observations, pid, &[(1, 1)], true);
    exact_child.wait_until_exited(Duration::from_secs(2));
    exact_child.assert_exited();
    root.close().unwrap();
}

#[test]
fn launch_rejects_invalid_host_workspace_before_spawn() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-workspace-validation-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::HandshakeOk);

    for workspace in [PathBuf::from("relative"), root.join("missing")] {
        let launch =
            DiagnosticChildLaunch::new(child.path(), &executable).with_host_workspace(workspace);
        let error = DiagnosticChildSupervisor::default()
            .start(&home, launch)
            .unwrap_err();
        assert!(matches!(
            error,
            DiagnosticChildSupervisorError::InvalidHostWorkspacePath { .. }
                | DiagnosticChildSupervisorError::HostWorkspacePathAccess { .. }
        ));
    }
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn start_verifies_startup_protocol_before_reporting_started() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::HandshakeOk);
    let mut supervisor = DiagnosticChildSupervisor::default();
    let launch = DiagnosticChildLaunch::new(child.path(), executable.clone());

    let outcome = supervisor.start(&home, launch).unwrap();

    let DiagnosticChildStartOutcome::Started(identity) = outcome else {
        panic!("expected started diagnostic child");
    };
    assert_eq!(
        identity.executable_path,
        fs::canonicalize(&executable).unwrap()
    );
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn version_one_error_response_preserves_supervisor_child_ownership() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::V1ErrorAfterHandshake);
    let mut supervisor = DiagnosticChildSupervisor::default();
    let launch = DiagnosticChildLaunch::new(child.path(), executable);
    supervisor.start(&home, launch).unwrap();

    let error = supervisor
        .request(
            DiagnosticChildCommand::ReadProcess,
            serde_json::json!({}),
            Duration::from_secs(1),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::ChildError { ref kind, .. } if kind == "shell_timeout"
    ));
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn malformed_protocol_retains_exact_child_until_explicit_cleanup() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-malformed-ownership-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-malformed-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::MalformedAfterHandshake);
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .start(&home, DiagnosticChildLaunch::new(child.path(), executable))
        .unwrap();

    let error = supervisor
        .request(
            DiagnosticChildCommand::ReadProcess,
            serde_json::json!({}),
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert!(matches!(error, DiagnosticChildSupervisorError::Protocol(_)));
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
    assert_eq!(supervisor.last_stop_method(), "direct_kill");
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn protocol_eof_retains_exited_child_until_explicit_reap() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-eof-ownership-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-eof-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::EofAfterHandshake);
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .start(&home, DiagnosticChildLaunch::new(child.path(), executable))
        .unwrap();

    let error = supervisor
        .request(
            DiagnosticChildCommand::ReadProcess,
            serde_json::json!({}),
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert!(matches!(error, DiagnosticChildSupervisorError::ProtocolEof));
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
    assert_eq!(supervisor.last_stop_method(), "graceful_eof");
    child.close().unwrap();
    root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn status_explicitly_reaps_an_observed_exit_and_preserves_method() {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{Foundation::HANDLE, System::Threading::WaitForSingleObject};

    let root = tempdir_support::temp_dir("beryl-diagnostic-observed-exit-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let child = spawn_exit_child();
    let process_handle = HANDLE(child.as_raw_handle());
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .adopt_child_for_test(child, home, PathBuf::from("test-child"))
        .unwrap();
    assert_eq!(unsafe { WaitForSingleObject(process_handle, 2_000) }.0, 0);

    assert!(matches!(
        supervisor.status().unwrap(),
        diagnostic_child_supervisor::DiagnosticChildStatus::NotRunning
    ));
    assert!(!supervisor.has_child_for_test());
    assert_eq!(supervisor.last_stop_method(), "observed_exit");
    root.close().unwrap();
}

#[test]
fn acceptance_request_retains_observed_exit_for_bounded_finish_cleanup() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-acceptance-observed-exit-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .adopt_acceptance_child_for_test(spawn_exit_child(), home, PathBuf::from("test-child"))
        .unwrap();
    assert!(
        supervisor
            .wait_for_child_exit_for_test(Duration::from_secs(2))
            .unwrap(),
        "test child must be a confirmed observed exit before the acceptance request"
    );

    let started = Instant::now();
    let (request_id, result) = supervisor.request_with_id_retaining_observed_exit(
        DiagnosticChildCommand::ReadProcess,
        serde_json::json!({}),
        Duration::from_millis(50),
    );

    assert_eq!(request_id.as_deref(), Some("1"));
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(
        supervisor.has_child_for_test(),
        "acceptance request failure must retain the exact observed-exit owner"
    );
    supervisor
        .stop_with_timeouts(Duration::from_millis(10), Duration::from_secs(1))
        .unwrap();
    assert_eq!(supervisor.last_stop_method(), "observed_exit");
    root.close().unwrap();
}

#[test]
fn ordinary_request_still_reaps_an_observed_exit_before_dispatch() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-ordinary-observed-exit-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .adopt_child_for_test(spawn_exit_child(), home, PathBuf::from("test-child"))
        .unwrap();
    assert!(
        supervisor
            .wait_for_child_exit_for_test(Duration::from_secs(2))
            .unwrap()
    );

    let (request_id, result) = supervisor.request_with_id(
        DiagnosticChildCommand::ReadProcess,
        serde_json::json!({}),
        Duration::from_millis(50),
    );

    assert_eq!(request_id.as_deref(), Some("1"));
    assert!(matches!(
        result,
        Err(DiagnosticChildSupervisorError::ProtocolEof)
    ));
    assert!(!supervisor.has_child_for_test());
    assert_eq!(supervisor.last_stop_method(), "observed_exit");
    root.close().unwrap();
}

#[test]
fn public_acceptance_start_cause_preserves_supervisor_failure_kinds() {
    use acceptance_session::{
        AcceptanceDiagnosticStartupCauseKind, AcceptanceSessionStartCause,
        diagnostic_start_cause_for_test,
    };

    let cases = [
        (
            DiagnosticChildSupervisorError::SpawnWriter {
                source: std::io::Error::other("writer"),
            },
            AcceptanceDiagnosticStartupCauseKind::WriterSpawn,
        ),
        (
            DiagnosticChildSupervisorError::WriteRequest {
                source: std::io::Error::other("write"),
            },
            AcceptanceDiagnosticStartupCauseKind::RequestWrite,
        ),
        (
            DiagnosticChildSupervisorError::SpawnStdoutReader {
                source: std::io::Error::other("reader"),
            },
            AcceptanceDiagnosticStartupCauseKind::ReaderSpawn,
        ),
        (
            DiagnosticChildSupervisorError::StartupProtocolIncompatible {
                message: "handshake".to_string(),
            },
            AcceptanceDiagnosticStartupCauseKind::StartupProtocol,
        ),
    ];

    for (error, expected_kind) in cases {
        let AcceptanceSessionStartCause::Diagnostic(cause) = diagnostic_start_cause_for_test(error)
        else {
            panic!("supervisor failure must remain a typed diagnostic startup cause");
        };
        assert_eq!(cause.kind(), expected_kind);
        assert!(!cause.message().is_empty());
    }
}

#[test]
fn blocked_full_pipe_write_times_out_and_cleanup_joins_owned_writer() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-blocked-write-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-blocked-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable =
        fake_child_executable(root.path(), FakeChildBehavior::StopReadingAfterHandshake);
    let mut supervisor = DiagnosticChildSupervisor::default();
    let identity = match supervisor
        .start(&home, DiagnosticChildLaunch::new(child.path(), executable))
        .unwrap()
    {
        DiagnosticChildStartOutcome::Started(identity) => identity,
        DiagnosticChildStartOutcome::AlreadyRunning(_) => unreachable!(),
    };
    #[cfg(windows)]
    let exact_child = ExactWindowsProcess::open_while_known_live(identity.pid);
    let payload = "x".repeat(200_000);
    let started = Instant::now();
    let error = supervisor
        .request(
            DiagnosticChildCommand::ReadProcess,
            serde_json::json!({"payload": payload}),
            Duration::from_millis(50),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::RequestTimeout { .. }
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(supervisor.has_child_for_test());
    assert_eq!(
        supervisor.stdin_writer_is_finished_for_test(),
        Some(false),
        "the request timeout must leave the owned writer blocked on the full child pipe"
    );
    supervisor
        .stop_with_timeouts(Duration::ZERO, Duration::from_secs(1))
        .unwrap();
    #[cfg(windows)]
    {
        exact_child.assert_exited();
        assert_eq!(
            supervisor.last_cleanup_writer_joined_before_job_release_for_test(),
            Some(true),
            "cleanup must retain the exact Job through its owned stdin writer join"
        );
    }
    assert_eq!(supervisor.last_stop_method(), "direct_kill");
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn request_uses_one_cumulative_write_and_response_deadline() {
    const REQUEST_TIMEOUT: Duration = Duration::from_millis(400);
    const WRITE_DELAY: Duration = Duration::from_millis(250);
    const RESPONSE_DELAY: Duration = Duration::from_millis(250);
    assert!(WRITE_DELAY < REQUEST_TIMEOUT);
    assert!(RESPONSE_DELAY < REQUEST_TIMEOUT);
    assert!(WRITE_DELAY + RESPONSE_DELAY > REQUEST_TIMEOUT);

    let root = tempdir_support::temp_dir("beryl-diagnostic-cumulative-deadline-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-cumulative-deadline-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(
        root.path(),
        FakeChildBehavior::DelayedResponseAfterHandshake,
    );
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .start(&home, DiagnosticChildLaunch::new(child.path(), executable))
        .unwrap();
    supervisor.delay_next_write_for_test(WRITE_DELAY);

    let result = supervisor.request(
        DiagnosticChildCommand::ReadProcess,
        serde_json::json!({}),
        REQUEST_TIMEOUT,
    );

    assert!(matches!(
        result,
        Err(DiagnosticChildSupervisorError::RequestTimeout { timeout })
            if timeout == REQUEST_TIMEOUT
    ));
    supervisor.stop().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn absolute_request_deadline_is_not_reanchored_after_dispatch_delay() {
    const DEADLINE_BUDGET: Duration = Duration::from_millis(400);
    const DISPATCH_DELAY: Duration = Duration::from_millis(200);
    const WRITE_DELAY: Duration = Duration::from_millis(250);

    let root = tempdir_support::temp_dir("beryl-diagnostic-absolute-deadline-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-absolute-deadline-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(
        root.path(),
        FakeChildBehavior::DelayedResponseAfterHandshake,
    );
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .start(&home, DiagnosticChildLaunch::new(child.path(), executable))
        .unwrap();
    supervisor.delay_next_write_for_test(WRITE_DELAY);

    let started = Instant::now();
    let deadline = started + DEADLINE_BUDGET;
    std::thread::sleep(DISPATCH_DELAY);
    let result = supervisor.request_until(
        DiagnosticChildCommand::ReadProcess,
        serde_json::json!({}),
        deadline,
    );

    assert!(matches!(
        result,
        Err(DiagnosticChildSupervisorError::RequestTimeout { timeout })
            if timeout <= DEADLINE_BUDGET - DISPATCH_DELAY
    ));
    assert!(
        started.elapsed() < Duration::from_millis(650),
        "request must retain the pre-dispatch absolute deadline instead of receiving a fresh budget"
    );
    supervisor.stop().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn short_cleanup_waits_do_not_sleep_past_phase_budget() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-short-cleanup-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .adopt_child_for_test(spawn_sleep_child(), home, PathBuf::from("test-child"))
        .unwrap();
    let (poll_sender, poll_receiver) = mpsc::sync_channel(1);
    install_child_wait_poll_observer_for_test(poll_sender);
    supervisor
        .stop_with_timeouts(Duration::from_millis(3), Duration::from_secs(1))
        .unwrap();
    let requested_sleep = poll_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("cleanup wait must publish its first requested poll sleep");
    assert!(requested_sleep <= Duration::from_millis(3));
    assert!(requested_sleep < Duration::from_millis(25));
    root.close().unwrap();
}

#[test]
fn startup_protocol_failures_are_cleaned_up_without_retaining_child() {
    let cases = [
        (
            FakeChildBehavior::Eof,
            "EOF should be reported as startup protocol EOF",
        ),
        (
            FakeChildBehavior::Malformed,
            "malformed response should be reported as startup protocol malformed",
        ),
        (
            FakeChildBehavior::RemoteError,
            "remote error should be reported as startup protocol rejection",
        ),
        (
            FakeChildBehavior::Incompatible,
            "incompatible handshake should be reported as startup incompatibility",
        ),
    ];

    for (behavior, message) in cases {
        let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
        let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
        let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
        let executable = fake_child_executable(root.path(), behavior);
        let mut supervisor = DiagnosticChildSupervisor::default();
        let launch = DiagnosticChildLaunch::new(child.path(), executable);

        let error = supervisor.start(&home, launch).unwrap_err();

        match behavior {
            FakeChildBehavior::Eof => assert!(
                matches!(error, DiagnosticChildSupervisorError::StartupProtocolEof),
                "{message}: {error}"
            ),
            FakeChildBehavior::Malformed => assert!(
                matches!(
                    error,
                    DiagnosticChildSupervisorError::StartupProtocolMalformed { .. }
                ),
                "{message}: {error}"
            ),
            FakeChildBehavior::RemoteError => assert!(
                matches!(
                    error,
                    DiagnosticChildSupervisorError::StartupProtocolRejected { .. }
                ),
                "{message}: {error}"
            ),
            FakeChildBehavior::Incompatible => assert!(
                matches!(
                    error,
                    DiagnosticChildSupervisorError::StartupProtocolIncompatible { .. }
                ),
                "{message}: {error}"
            ),
            FakeChildBehavior::HandshakeOk
            | FakeChildBehavior::Timeout
            | FakeChildBehavior::V1ErrorAfterHandshake
            | FakeChildBehavior::MalformedAfterHandshake
            | FakeChildBehavior::EofAfterHandshake
            | FakeChildBehavior::StopReadingAfterHandshake
            | FakeChildBehavior::DelayedResponseAfterHandshake => unreachable!(),
        }
        assert!(!supervisor.has_child_for_test());
        child.close().unwrap();
        root.close().unwrap();
    }
}

#[test]
fn startup_protocol_timeout_is_cleaned_up_without_retaining_child() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable = fake_child_executable(root.path(), FakeChildBehavior::Timeout);
    let mut supervisor = DiagnosticChildSupervisor::default();
    let launch = DiagnosticChildLaunch::new(child.path(), executable);

    let error = supervisor
        .start_for_test(&home, launch, Duration::from_millis(100))
        .unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::StartupProtocolTimeout { .. }
    ));
    assert!(!supervisor.has_child_for_test());
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn startup_cleanup_failure_retains_child_for_stop_retry() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-supervisor-home-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();

    let error = supervisor
        .retain_startup_failure_child_for_test(
            spawn_sleep_child(),
            home,
            PathBuf::from("test-child"),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::RequestTimeout { .. }
    ));
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
    root.close().unwrap();
}

#[test]
fn stop_without_running_child_is_idempotent() {
    let mut supervisor = DiagnosticChildSupervisor::default();

    let first = supervisor.stop().unwrap();
    let second = supervisor.stop().unwrap();

    assert_eq!(first, DiagnosticChildStopOutcome::NotRunning);
    assert_eq!(second, DiagnosticChildStopOutcome::NotRunning);
}

#[test]
fn same_home_path_uses_existing_directory_canonicalization() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-home-canonical-");
    let nested = root.path().join("child");
    std::fs::create_dir_all(&nested).unwrap();
    let equivalent = root.path().join(".").join("child");

    assert!(same_home_path(&nested, &equivalent));
}

#[test]
fn stop_response_timeout_exceeds_shutdown_budget() {
    assert!(DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT > DIAGNOSTIC_CHILD_STOP_BUDGET);
}

#[test]
fn spawned_child_guard_cleans_unclaimed_process() {
    let child = spawn_sleep_child();
    let mut guard = SpawnedDiagnosticChildGuard::new(child);

    assert!(guard.cleanup_for_test(Duration::from_secs(2)).unwrap());
}

#[test]
fn failed_stop_keeps_child_owned_for_retry() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-stop-ownership-");
    let home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    supervisor
        .adopt_child_for_test(spawn_sleep_child(), home, PathBuf::from("test-child"))
        .unwrap();

    let error = supervisor.force_stop_error_for_test().unwrap_err();

    assert!(matches!(
        error,
        DiagnosticChildSupervisorError::RequestTimeout { .. }
    ));
    assert!(supervisor.has_child_for_test());
    supervisor.stop().unwrap();
}

#[derive(Clone, Copy)]
enum FakeChildBehavior {
    HandshakeOk,
    V1ErrorAfterHandshake,
    Eof,
    Malformed,
    RemoteError,
    Incompatible,
    Timeout,
    MalformedAfterHandshake,
    EofAfterHandshake,
    StopReadingAfterHandshake,
    DelayedResponseAfterHandshake,
}

fn fake_child_executable(root: &std::path::Path, behavior: FakeChildBehavior) -> PathBuf {
    let path = root.join(fake_child_file_name(behavior));
    fs::write(&path, fake_child_script(behavior)).unwrap();
    make_executable_for_test(&path);
    path
}

#[cfg(windows)]
fn assert_cleanup_observations(
    observations: &mpsc::Receiver<AcceptanceTestObservation>,
    pid: u32,
    expected_attempts: &[(usize, usize)],
    expect_fail_safe_release: bool,
) {
    assert_eq!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::JobConfigured { pid }
    );
    assert_eq!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::GateWriteCompleted { pid }
    );
    for &(ordinal, remaining_forced_failures) in expected_attempts {
        assert_eq!(
            observations.recv_timeout(Duration::from_secs(1)).unwrap(),
            AcceptanceTestObservation::CleanupAttempt {
                pid,
                ordinal,
                forced_failure: true,
                remaining_forced_failures,
            }
        );
    }
    if expect_fail_safe_release {
        assert_eq!(
            observations.recv_timeout(Duration::from_secs(1)).unwrap(),
            AcceptanceTestObservation::FailSafeRelease { pid }
        );
    }
    assert!(observations.try_recv().is_err());
}

#[cfg(windows)]
fn reclaim_lane_owner(
    mut owner: DiagnosticAcceptanceProcessOwner,
    pid: u32,
    forced_cleanup_failures: usize,
) {
    for retry in 1..=forced_cleanup_failures {
        let result = owner.retry_cleanup(Duration::ZERO, Duration::from_secs(2));
        if retry == forced_cleanup_failures {
            assert!(matches!(
                result,
                DiagnosticAcceptanceCleanupRetry::Reclaimed(identity) if identity.pid == pid
            ));
        } else {
            assert!(matches!(
                result,
                DiagnosticAcceptanceCleanupRetry::StillRetained { identity, .. }
                    if identity.pid == pid
            ));
        }
    }
}

#[cfg(windows)]
fn assert_lane_cleanup_observations(
    observations: &mpsc::Receiver<AcceptanceTestObservation>,
    pid: u32,
    forced_cleanup_failures: usize,
) {
    for ordinal in 1..=forced_cleanup_failures {
        assert_eq!(
            observations.recv_timeout(Duration::from_secs(1)).unwrap(),
            AcceptanceTestObservation::CleanupAttempt {
                pid,
                ordinal,
                forced_failure: true,
                remaining_forced_failures: forced_cleanup_failures - ordinal,
            }
        );
    }
    assert_eq!(
        observations.recv_timeout(Duration::from_secs(1)).unwrap(),
        AcceptanceTestObservation::CleanupAttempt {
            pid,
            ordinal: forced_cleanup_failures + 1,
            forced_failure: false,
            remaining_forced_failures: 0,
        }
    );
    assert!(observations.try_recv().is_err());
}

#[cfg(windows)]
fn terminal_acceptance_session(
    root: &std::path::Path,
    recovery_cleanup_timeout: Duration,
    test_plan: AcceptanceTestPlan,
) -> (AcceptanceSession, PathBuf) {
    static SEQUENCE: AtomicUsize = AtomicUsize::new(1);

    let fixture = root.join(format!(
        "terminal-fixture-{}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let workspace = fixture.join("workspace");
    let evidence_dir = fixture.join("evidence");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir(&evidence_dir).unwrap();
    let evidence_path = evidence_dir.join("run.json");
    let executable =
        fake_gated_child_executable(&fixture, &fixture.join("gate-released.txt"), false);
    let limits = AcceptanceLimits::new(
        Duration::from_secs(2),
        Duration::from_millis(100),
        Duration::from_secs(10),
        1,
        64,
        Duration::from_millis(30),
    )
    .unwrap();
    let config = AcceptanceSessionConfig::new(
        executable,
        fixture.join("isolated-home"),
        AcceptanceLaunchMode::FreshWorkspace,
        Some(workspace),
        &evidence_path,
        "terminal-cleanup-test",
        limits,
        recovery_cleanup_timeout,
    )
    .unwrap();
    (
        AcceptanceSession::start_with_test_plan(config, test_plan).unwrap(),
        evidence_path,
    )
}

#[cfg(windows)]
fn terminal_budget_acceptance_session(
    root: &std::path::Path,
    test_plan: AcceptanceTestPlan,
) -> AcceptanceSession {
    let fixture = root.join("terminal-budget-fixture");
    let workspace = fixture.join("workspace");
    let evidence_dir = fixture.join("evidence");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir(&evidence_dir).unwrap();
    let executable = fake_terminal_budget_child_executable(&fixture);
    let limits = AcceptanceLimits::new(
        Duration::from_secs(2),
        Duration::from_millis(100),
        Duration::from_secs(10),
        1,
        136,
        Duration::from_millis(30),
    )
    .unwrap();
    let config = AcceptanceSessionConfig::new(
        executable,
        fixture.join("isolated-home"),
        AcceptanceLaunchMode::FreshWorkspace,
        Some(workspace),
        evidence_dir.join("run.json"),
        "shared-budget-test",
        limits,
        Duration::from_secs(1),
    )
    .unwrap();
    AcceptanceSession::start_with_test_plan(config, test_plan).unwrap()
}

#[cfg(windows)]
fn fake_terminal_budget_child_executable(root: &std::path::Path) -> PathBuf {
    let path = root.join("fake terminal budget child.cmd");
    let response = format!(
        "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{DIAGNOSTIC_CHILD_PROTOCOL_NAME}\",\"protocolVersion\":{DIAGNOSTIC_CHILD_PROTOCOL_VERSION}}}}}"
    );
    let script = format!(
        "@echo off\r\nset /p gate=\r\npowershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\nset /p line=\r\npowershell.exe -NoProfile -Command \"$stderr=[Console]::OpenStandardError(); $bytes=[Text.Encoding]::UTF8.GetBytes('fixture-stderr'); $stderr.Write($bytes,0,$bytes.Length)\"\r\necho {response}\r\nset /p line=\r\nset /p line=\r\n"
    );
    fs::write(&path, script).unwrap();
    path
}

fn fake_argument_child_executable(root: &std::path::Path, args_path: &std::path::Path) -> PathBuf {
    let path = root.join(if cfg!(windows) {
        "fake child arguments.cmd"
    } else {
        "fake-child-arguments.sh"
    });
    let response = format!(
        "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{DIAGNOSTIC_CHILD_PROTOCOL_NAME}\",\"protocolVersion\":{DIAGNOSTIC_CHILD_PROTOCOL_VERSION}}}}}"
    );
    #[cfg(windows)]
    let script = format!(
        "@echo off\r\necho %*>\"{}\"\r\nset /p line=\r\necho {response}\r\nset /p line=\r\n",
        args_path.display()
    );
    #[cfg(not(windows))]
    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$*\" > '{}'\nIFS= read -r line\nprintf '%s\\n' '{response}'\nIFS= read -r line\n",
        args_path.display()
    );
    fs::write(&path, script).unwrap();
    make_executable_for_test(&path);
    path
}

#[cfg(windows)]
fn fake_gated_child_executable(
    root: &std::path::Path,
    marker: &std::path::Path,
    descendant_after_gate: bool,
) -> PathBuf {
    let path = root.join(if descendant_after_gate {
        "fake gated descendant.cmd"
    } else {
        "fake gated child.cmd"
    });
    let response = format!(
        "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{DIAGNOSTIC_CHILD_PROTOCOL_NAME}\",\"protocolVersion\":{DIAGNOSTIC_CHILD_PROTOCOL_VERSION}}}}}"
    );
    let descendant = if descendant_after_gate {
        "start \"\" /b ping -n 60 127.0.0.1 >nul\r\n"
    } else {
        ""
    };
    let script = format!(
        "@echo off\r\nset /p gate=\r\necho released>\"{}\"\r\npowershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\n{descendant}set /p line=\r\necho {response}\r\nset /p line=\r\n",
        marker.display()
    );
    fs::write(&path, script).unwrap();
    path
}

#[cfg(windows)]
fn fake_ready_before_handshake_child_executable(
    root: &std::path::Path,
    premature_handshake: &std::path::Path,
) -> PathBuf {
    let path = root.join("fake ready before handshake.cmd");
    let response = format!(
        "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{DIAGNOSTIC_CHILD_PROTOCOL_NAME}\",\"protocolVersion\":{DIAGNOSTIC_CHILD_PROTOCOL_VERSION}}}}}"
    );
    let script = format!(
        "@echo off\r\nset /p gate=\r\npowershell.exe -NoProfile -Command \"Start-Sleep -Milliseconds 200\"\r\nif exist \"{}\" exit /b 91\r\npowershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\nset /p line=\r\necho {response}\r\nping -n 60 127.0.0.1 >nul\r\n",
        premature_handshake.display()
    );
    fs::write(&path, script).unwrap();
    path
}

#[cfg(windows)]
fn fake_gated_incompatible_descendant_executable(
    root: &std::path::Path,
    descendant_pid_path: &std::path::Path,
) -> PathBuf {
    let path = root.join("fake gated incompatible descendant.cmd");
    let response = format!(
        "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{DIAGNOSTIC_CHILD_PROTOCOL_NAME}\",\"protocolVersion\":99}}}}"
    );
    let script = format!(
        "@echo off\r\nset /p gate=\r\npowershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\npowershell.exe -NoProfile -Command \"$p=Start-Process ping -ArgumentList '-n','60','127.0.0.1' -PassThru -WindowStyle Hidden; [IO.File]::WriteAllText('{}', [string]$p.Id)\"\r\nset /p line=\r\necho {response}\r\nping -n 60 127.0.0.1 >nul\r\n",
        descendant_pid_path.display()
    );
    fs::write(&path, script).unwrap();
    path
}

#[cfg(target_os = "windows")]
fn fake_child_file_name(behavior: FakeChildBehavior) -> &'static str {
    match behavior {
        FakeChildBehavior::HandshakeOk => "fake child ok.cmd",
        FakeChildBehavior::V1ErrorAfterHandshake => "fake child v1 error.cmd",
        FakeChildBehavior::Eof => "fake child eof.cmd",
        FakeChildBehavior::Malformed => "fake child malformed.cmd",
        FakeChildBehavior::RemoteError => "fake child error.cmd",
        FakeChildBehavior::Incompatible => "fake child incompatible.cmd",
        FakeChildBehavior::Timeout => "fake child timeout.cmd",
        FakeChildBehavior::MalformedAfterHandshake => "fake child later malformed.cmd",
        FakeChildBehavior::EofAfterHandshake => "fake child later eof.cmd",
        FakeChildBehavior::StopReadingAfterHandshake => "fake child stopped reading.cmd",
        FakeChildBehavior::DelayedResponseAfterHandshake => "fake child delayed response.cmd",
    }
}

#[cfg(not(target_os = "windows"))]
fn fake_child_file_name(behavior: FakeChildBehavior) -> &'static str {
    match behavior {
        FakeChildBehavior::HandshakeOk => "fake child ok.sh",
        FakeChildBehavior::V1ErrorAfterHandshake => "fake child v1 error.sh",
        FakeChildBehavior::Eof => "fake child eof.sh",
        FakeChildBehavior::Malformed => "fake child malformed.sh",
        FakeChildBehavior::RemoteError => "fake child error.sh",
        FakeChildBehavior::Incompatible => "fake child incompatible.sh",
        FakeChildBehavior::Timeout => "fake child timeout.sh",
        FakeChildBehavior::MalformedAfterHandshake => "fake-child-later-malformed.sh",
        FakeChildBehavior::EofAfterHandshake => "fake-child-later-eof.sh",
        FakeChildBehavior::StopReadingAfterHandshake => "fake-child-stopped-reading.sh",
        FakeChildBehavior::DelayedResponseAfterHandshake => "fake-child-delayed-response.sh",
    }
}

#[cfg(target_os = "windows")]
fn fake_child_script(behavior: FakeChildBehavior) -> String {
    let response = fake_child_response(behavior);
    match behavior {
        FakeChildBehavior::Eof => "@echo off\r\nexit /b 0\r\n".to_string(),
        FakeChildBehavior::Timeout => "@echo off\r\nping -n 60 127.0.0.1 >nul\r\n".to_string(),
        FakeChildBehavior::V1ErrorAfterHandshake => format!(
            "@echo off\r\nset /p line=\r\necho {response}\r\nset /p line=\r\necho {{\"id\":\"2\",\"ok\":false,\"error\":{{\"kind\":\"shell_timeout\",\"message\":\"timed out\"}}}}\r\nping -n 60 127.0.0.1 >nul\r\n"
        ),
        FakeChildBehavior::MalformedAfterHandshake => format!(
            "@echo off\r\nset /p line=\r\necho {response}\r\nset /p line=\r\necho not-json\r\nping -n 60 127.0.0.1 >nul\r\n"
        ),
        FakeChildBehavior::EofAfterHandshake => {
            format!("@echo off\r\nset /p line=\r\necho {response}\r\nset /p line=\r\nexit /b 0\r\n")
        }
        FakeChildBehavior::StopReadingAfterHandshake => {
            format!("@echo off\r\nset /p line=\r\necho {response}\r\nping -n 60 127.0.0.1 >nul\r\n")
        }
        FakeChildBehavior::DelayedResponseAfterHandshake => format!(
            "@echo off\r\nset /p line=\r\necho {response}\r\nset /p line=\r\npowershell.exe -NoProfile -Command \"Start-Sleep -Milliseconds 250\"\r\necho {{\"id\":\"2\",\"ok\":true,\"result\":{{\"delayed\":true}}}}\r\nset /p line=\r\n"
        ),
        _ => {
            format!("@echo off\r\nset /p line=\r\necho {response}\r\nping -n 60 127.0.0.1 >nul\r\n")
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn fake_child_script(behavior: FakeChildBehavior) -> String {
    let response = fake_child_response(behavior);
    match behavior {
        FakeChildBehavior::Eof => "#!/bin/sh\nexit 0\n".to_string(),
        FakeChildBehavior::Timeout => "#!/bin/sh\nsleep 60\n".to_string(),
        FakeChildBehavior::V1ErrorAfterHandshake => format!(
            "#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' '{response}'\nIFS= read -r line\nprintf '%s\\n' '{{\"id\":\"2\",\"ok\":false,\"error\":{{\"kind\":\"shell_timeout\",\"message\":\"timed out\"}}}}'\nsleep 60\n"
        ),
        FakeChildBehavior::MalformedAfterHandshake => format!(
            "#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' '{response}'\nIFS= read -r line\nprintf '%s\\n' 'not-json'\nsleep 60\n"
        ),
        FakeChildBehavior::EofAfterHandshake => format!(
            "#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' '{response}'\nIFS= read -r line\nexit 0\n"
        ),
        FakeChildBehavior::StopReadingAfterHandshake => {
            format!("#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' '{response}'\nsleep 60\n")
        }
        FakeChildBehavior::DelayedResponseAfterHandshake => format!(
            "#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' '{response}'\nIFS= read -r line\nsleep 0.25\nprintf '%s\\n' '{{\"id\":\"2\",\"ok\":true,\"result\":{{\"delayed\":true}}}}'\nIFS= read -r line\n"
        ),
        _ => format!("#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' '{response}'\nsleep 60\n"),
    }
}

fn fake_child_response(behavior: FakeChildBehavior) -> String {
    match behavior {
        FakeChildBehavior::HandshakeOk
        | FakeChildBehavior::V1ErrorAfterHandshake
        | FakeChildBehavior::MalformedAfterHandshake
        | FakeChildBehavior::EofAfterHandshake
        | FakeChildBehavior::StopReadingAfterHandshake
        | FakeChildBehavior::DelayedResponseAfterHandshake => format!(
            "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{DIAGNOSTIC_CHILD_PROTOCOL_NAME}\",\"protocolVersion\":{DIAGNOSTIC_CHILD_PROTOCOL_VERSION}}}}}"
        ),
        FakeChildBehavior::Malformed => "not-json".to_string(),
        FakeChildBehavior::RemoteError => {
            "{\"id\":\"1\",\"ok\":false,\"error\":{\"kind\":\"unsupported_command\",\"message\":\"bad command\"}}"
                .to_string()
        }
        FakeChildBehavior::Incompatible => {
            "{\"id\":\"1\",\"ok\":true,\"result\":{\"protocol\":\"other\",\"protocolVersion\":999}}"
                .to_string()
        }
        FakeChildBehavior::Eof | FakeChildBehavior::Timeout => String::new(),
    }
}

#[cfg(unix)]
fn make_executable_for_test(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable_for_test(_path: &std::path::Path) {}

#[cfg(target_os = "windows")]
fn spawn_sleep_child() -> Child {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn powershell sleep child")
}

#[cfg(target_os = "windows")]
fn spawn_exit_child() -> Child {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn powershell exit child")
}

#[cfg(target_os = "windows")]
struct ExactWindowsProcess {
    handle: windows::Win32::Foundation::HANDLE,
    creation_identity: u64,
}

#[cfg(target_os = "windows")]
impl ExactWindowsProcess {
    fn open_while_known_live(pid: u32) -> Self {
        use windows::Win32::{
            Foundation::FILETIME,
            System::Threading::{
                GetProcessTimes, OpenProcess, PROCESS_ACCESS_RIGHTS,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };

        let handle = unsafe {
            OpenProcess(
                PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_LIMITED_INFORMATION.0 | 0x0010_0000),
                false,
                pid,
            )
        }
        .expect("open exact known-live process handle");
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }
            .expect("read exact process creation identity");
        Self {
            handle,
            creation_identity: u64::from(created.dwLowDateTime)
                | (u64::from(created.dwHighDateTime) << 32),
        }
    }

    fn creation_identity(&self) -> u64 {
        self.creation_identity
    }

    fn assert_still_active(&self) {
        use windows::Win32::{
            Foundation::{STILL_ACTIVE, WAIT_TIMEOUT},
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        };

        assert_eq!(unsafe { WaitForSingleObject(self.handle, 0) }, WAIT_TIMEOUT);
        let mut exit_code = 0;
        unsafe { GetExitCodeProcess(self.handle, &mut exit_code) }
            .expect("read exact process exit code");
        assert_eq!(exit_code, STILL_ACTIVE.0 as u32);
    }

    fn wait_until_exited(&self, timeout: Duration) {
        use windows::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject};

        assert_eq!(
            unsafe { WaitForSingleObject(self.handle, timeout.as_millis() as u32) },
            WAIT_OBJECT_0,
            "exact process handle did not become signaled within {timeout:?}"
        );
    }

    fn assert_exited(&self) {
        use windows::Win32::{
            Foundation::{STILL_ACTIVE, WAIT_OBJECT_0},
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        };

        assert_eq!(
            unsafe { WaitForSingleObject(self.handle, 0) },
            WAIT_OBJECT_0
        );
        let mut exit_code = STILL_ACTIVE.0 as u32;
        unsafe { GetExitCodeProcess(self.handle, &mut exit_code) }
            .expect("read exact process exit code");
        assert_ne!(exit_code, STILL_ACTIVE.0 as u32);
    }
}

#[cfg(target_os = "windows")]
impl Drop for ExactWindowsProcess {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
    }
}

#[cfg(not(target_os = "windows"))]
fn spawn_sleep_child() -> Child {
    Command::new("sh")
        .args(["-c", "sleep 60"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn shell sleep child")
}

#[cfg(not(target_os = "windows"))]
fn spawn_exit_child() -> Child {
    Command::new("sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn shell exit child")
}
