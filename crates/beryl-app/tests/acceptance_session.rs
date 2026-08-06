use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use beryl_app::{
    AcceptanceDiagnosticStartupCauseKind, AcceptanceLaunchMode, AcceptanceLimits,
    AcceptancePublicationState, AcceptanceRequest, AcceptanceSession, AcceptanceSessionConfig,
    AcceptanceSessionError, AcceptanceSessionStartCause, MAX_ACCEPTANCE_EXPANDED_REQUESTS,
    MAX_ACCEPTANCE_OUTPUT_BYTES, MAX_ACCEPTANCE_REQUESTS, compile_acceptance_requests,
};
use serde_json::json;

#[path = "support/tempdir.rs"]
mod tempdir_support;

const PROTOCOL: &str = "beryl_diagnostic_child";

fn limits(max_requests: usize, max_output_bytes: usize) -> AcceptanceLimits {
    limits_with_request_timeout(max_requests, max_output_bytes, Duration::from_millis(150))
}

fn limits_with_request_timeout(
    max_requests: usize,
    max_output_bytes: usize,
    request_timeout: Duration,
) -> AcceptanceLimits {
    AcceptanceLimits::new(
        Duration::from_secs(5),
        request_timeout,
        Duration::from_secs(10),
        max_requests,
        max_output_bytes,
        Duration::from_secs(1),
    )
    .unwrap()
}

struct Fixture {
    root: tempdir_support::TestTempDir,
    executable: PathBuf,
    home: PathBuf,
    workspace: PathBuf,
    evidence: PathBuf,
}

impl Fixture {
    fn new(behavior: FakeBehavior) -> Self {
        let root = tempdir_support::temp_dir("beryl-acceptance-session-");
        let workspace = root.join("workspace");
        let evidence_dir = root.join("evidence");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&evidence_dir).unwrap();
        let executable = fake_executable(root.path(), behavior);
        Self {
            home: root.join("isolated-home"),
            evidence: evidence_dir.join("run.json"),
            root,
            executable,
            workspace,
        }
    }

    fn config(&self, limits: AcceptanceLimits) -> AcceptanceSessionConfig {
        AcceptanceSessionConfig::new(
            &self.executable,
            &self.home,
            AcceptanceLaunchMode::FreshWorkspace,
            Some(self.workspace.clone()),
            &self.evidence,
            "deterministic-run",
            limits,
            Duration::from_secs(1),
        )
        .unwrap()
    }
}

#[derive(Clone, Copy)]
enum FakeBehavior {
    Success,
    InvalidStderr,
    LongStderr,
    MultibyteStderr,
    Incompatible,
    TimeoutAfterHandshake,
    MalformedAfterHandshake,
    PartialAfterHandshake,
    OversizeAfterHandshake,
    IgnoreEof,
    GateReadyEof,
    GateReadyMalformed,
    GateReadyTimeout,
    #[cfg(windows)]
    SpawnDescendant,
}

#[test]
fn limits_reject_zero_and_values_above_owned_bounds() {
    assert!(
        AcceptanceLimits::new(
            Duration::ZERO,
            Duration::from_secs(1),
            Duration::from_secs(1),
            1,
            1,
            Duration::from_millis(3),
        )
        .is_err()
    );
    assert!(
        AcceptanceLimits::new(
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_secs(1),
            MAX_ACCEPTANCE_REQUESTS + 1,
            MAX_ACCEPTANCE_OUTPUT_BYTES + 1,
            Duration::from_millis(3),
        )
        .is_err()
    );
}

#[test]
fn compiled_operations_normalize_waits_and_resolve_exact_timeouts_before_launch() {
    let limits = limits(4, 64);
    let compiled = compile_acceptance_requests(
        vec![
            AcceptanceRequest::new("read_ui_state", json!({})).unwrap(),
            AcceptanceRequest::new(
                "wait_for_state",
                json!({
                    "predicate": "thread_selected",
                    "threadId": "thread-1",
                    "timeoutMs": 100,
                    "pollIntervalMs": 1,
                    "limit": 999,
                }),
            )
            .unwrap()
            .with_timeout(Duration::from_millis(120))
            .unwrap(),
            AcceptanceRequest::new("start_turn", json!({ "text": "phase work" })).unwrap(),
            AcceptanceRequest::new(
                "hard_stop_turn",
                json!({ "expectedThreadId": "thread-1", "expectedTurnId": "turn-1" }),
            )
            .unwrap(),
        ],
        &limits,
    )
    .unwrap();

    assert_eq!(compiled.len(), 4);
    assert_eq!(compiled[0].effective_timeout(), Duration::from_millis(150));
    assert_eq!(compiled[1].effective_timeout(), Duration::from_millis(120));
    assert_eq!(compiled[1].command(), "wait_for_state");
    assert!(AcceptanceRequest::new("not_a_command", json!({})).is_err());
    assert!(AcceptanceRequest::new("start_turn", json!({ "text": "" })).is_err());
    let nested = compile_acceptance_requests(
        vec![
            AcceptanceRequest::new(
                "wait_for_state",
                json!({ "predicate": "ready", "timeoutMs": 200, "pollIntervalMs": 25 }),
            )
            .unwrap()
            .with_timeout(Duration::from_millis(150))
            .unwrap(),
        ],
        &limits,
    )
    .unwrap();
    assert_eq!(nested[0].maximum_wire_requests(), 8);
}

#[test]
fn compiler_bounds_aggregate_runtime_expansion_and_plan_wide_request_identity() {
    let short_runtime = AcceptanceLimits::new(
        Duration::from_secs(1),
        Duration::from_millis(150),
        Duration::from_millis(200),
        2,
        64,
        Duration::from_millis(3),
    )
    .unwrap();
    let error = compile_acceptance_requests(
        vec![
            AcceptanceRequest::new("read_process", json!({})).unwrap(),
            AcceptanceRequest::new("read_ui_state", json!({})).unwrap(),
        ],
        &short_runtime,
    )
    .unwrap_err();
    assert!(error.to_string().contains("worst-case operation budget"));

    let maximum_limits = AcceptanceLimits::new(
        Duration::from_secs(1),
        Duration::from_secs(600),
        Duration::from_secs(24 * 60 * 60),
        MAX_ACCEPTANCE_REQUESTS,
        64,
        Duration::from_millis(3),
    )
    .unwrap();
    let maximum_plan = (0..MAX_ACCEPTANCE_REQUESTS)
        .map(|_| {
            AcceptanceRequest::new(
                "wait_for_state",
                json!({
                    "predicate": "ready",
                    "timeoutMs": 10_000,
                    "pollIntervalMs": 25,
                }),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let compiled = compile_acceptance_requests(maximum_plan, &maximum_limits).unwrap();
    assert_eq!(
        compiled
            .iter()
            .map(|request| request.maximum_wire_requests())
            .sum::<usize>(),
        MAX_ACCEPTANCE_EXPANDED_REQUESTS
    );
    assert!(
        compiled
            .iter()
            .all(|request| request.largest_qualified_request_id()
                == 1 + MAX_ACCEPTANCE_EXPANDED_REQUESTS as u64)
    );
}

#[cfg(not(windows))]
#[test]
fn config_rejects_unsupported_host_before_launch() {
    let error = AcceptanceSessionConfig::new(
        "/unused/beryl",
        "/unused/home",
        AcceptanceLaunchMode::FreshWorkspace,
        Some("/unused/workspace".into()),
        "/unused/evidence.json",
        "unsupported-host",
        limits(1, 1),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(matches!(error, AcceptanceSessionError::UnsupportedPlatform));
}

#[cfg(windows)]
#[test]
fn config_rejects_relative_missing_and_colliding_paths_before_launch() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let relative = AcceptanceSessionConfig::new(
        "relative.exe",
        &fixture.home,
        AcceptanceLaunchMode::FreshWorkspace,
        Some(fixture.workspace.clone()),
        &fixture.evidence,
        "run",
        limits(1, 1),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(relative.to_string().contains("absolute"));

    let missing_workspace = AcceptanceSessionConfig::new(
        &fixture.executable,
        &fixture.home,
        AcceptanceLaunchMode::FreshWorkspace,
        Some(fixture.root.join("missing-workspace")),
        &fixture.evidence,
        "run",
        limits(1, 1),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(matches!(
        missing_workspace,
        AcceptanceSessionError::PathIo { .. }
    ));

    let collision = AcceptanceSessionConfig::new(
        &fixture.executable,
        &fixture.workspace,
        AcceptanceLaunchMode::FreshWorkspace,
        Some(fixture.workspace.clone()),
        &fixture.evidence,
        "run",
        limits(1, 1),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(collision.to_string().contains("collides"));
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn fresh_workspace_accepts_a_frozen_executable_nested_under_a_canonical_workspace_alias() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let executable_dir = fixture.workspace.join("acceptance");
    fs::create_dir(&executable_dir).unwrap();
    let executable = executable_dir.join("frozen.cmd");
    fs::copy(&fixture.executable, &executable).unwrap();
    make_executable(&executable);
    let canonical_executable = fs::canonicalize(&executable).unwrap();
    let workspace_alias = executable_dir.join("..");

    let session = AcceptanceSession::start(
        AcceptanceSessionConfig::new(
            &executable,
            &fixture.home,
            AcceptanceLaunchMode::FreshWorkspace,
            Some(workspace_alias),
            &fixture.evidence,
            "nested-executable",
            limits(1, 64),
            Duration::from_secs(1),
        )
        .unwrap(),
    )
    .unwrap();
    let outcome = session.finish();
    assert_eq!(
        outcome.evidence().fixture.executable_path,
        canonical_executable
    );
    assert_eq!(
        outcome.evidence().process.executable_path,
        canonical_executable
    );
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn config_rejects_each_cleanup_or_publication_sensitive_path_overlap() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let assert_collision = |executable: &Path,
                            home: &Path,
                            evidence: &Path,
                            workspace: &Path,
                            expected_left: &str,
                            expected_right: &str| {
        let error = AcceptanceSessionConfig::new(
            executable,
            home,
            AcceptanceLaunchMode::FreshWorkspace,
            Some(workspace.to_path_buf()),
            evidence,
            "path-overlap",
            limits(1, 64),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains(expected_left) && error.to_string().contains(expected_right),
            "expected {expected_left}/{expected_right} collision, got {error}"
        );
    };

    assert_collision(
        &fixture.executable,
        &fixture.workspace.join("isolated-home"),
        &fixture.evidence,
        &fixture.workspace,
        "isolated home path",
        "execution workspace path",
    );
    assert_collision(
        &fixture.executable,
        &fixture.home,
        &fixture.workspace.join("evidence.json"),
        &fixture.workspace,
        "evidence path",
        "execution workspace path",
    );

    let shared = fixture.root.join("shared");
    fs::create_dir(&shared).unwrap();
    assert_collision(
        &fixture.executable,
        &shared,
        &shared.join("evidence.json"),
        &fixture.workspace,
        "isolated home path",
        "evidence path",
    );

    let executable_home = fixture.root.join("executable-home");
    fs::create_dir(&executable_home).unwrap();
    let nested_executable = executable_home.join("frozen.cmd");
    fs::copy(&fixture.executable, &nested_executable).unwrap();
    make_executable(&nested_executable);
    assert_collision(
        &nested_executable,
        &executable_home,
        &fixture.evidence,
        &fixture.workspace,
        "executable path",
        "isolated home path",
    );
    assert_collision(
        &fixture.executable,
        &fixture.home,
        &fixture.executable,
        &fixture.workspace,
        "executable path",
        "evidence path",
    );
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn config_rejects_a_home_with_multiple_missing_components_under_the_workspace() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let home = fixture
        .workspace
        .join("missing-parent")
        .join("isolated-home");

    let error = AcceptanceSessionConfig::new(
        &fixture.executable,
        &home,
        AcceptanceLaunchMode::FreshWorkspace,
        Some(fixture.workspace.clone()),
        &fixture.evidence,
        "missing-home-components",
        limits(1, 64),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("isolated home path")
            && error.to_string().contains("execution workspace path"),
        "expected isolated home/workspace collision, got {error}"
    );
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn launch_modes_validate_workspace_and_recovery_home_state_before_launch() {
    let fixture = Fixture::new(FakeBehavior::Success);

    let fresh_missing_workspace = AcceptanceSessionConfig::new(
        &fixture.executable,
        &fixture.home,
        AcceptanceLaunchMode::FreshWorkspace,
        None,
        &fixture.evidence,
        "fresh-missing-workspace",
        limits(1, 64),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(
        fresh_missing_workspace
            .to_string()
            .contains("requires an execution workspace")
    );

    let recovery_with_workspace = AcceptanceSessionConfig::new(
        &fixture.executable,
        &fixture.home,
        AcceptanceLaunchMode::ExistingHomeRecovery,
        Some(fixture.workspace.clone()),
        &fixture.evidence,
        "recovery-with-workspace",
        limits(1, 64),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(
        recovery_with_workspace
            .to_string()
            .contains("forbids an execution workspace")
    );

    let missing_recovery_home = AcceptanceSessionConfig::new(
        &fixture.executable,
        &fixture.home,
        AcceptanceLaunchMode::ExistingHomeRecovery,
        None,
        &fixture.evidence,
        "missing-recovery-home",
        limits(1, 64),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(matches!(
        missing_recovery_home,
        AcceptanceSessionError::PathIo { .. }
    ));

    fs::write(&fixture.home, b"not a directory").unwrap();
    let file_recovery_home = AcceptanceSessionConfig::new(
        &fixture.executable,
        &fixture.home,
        AcceptanceLaunchMode::ExistingHomeRecovery,
        None,
        &fixture.evidence,
        "file-recovery-home",
        limits(1, 64),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(matches!(
        file_recovery_home,
        AcceptanceSessionError::InvalidConfiguration(_)
    ));
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn valid_launch_modes_reach_the_starter_and_record_mode_valid_evidence() {
    for mode in [
        AcceptanceLaunchMode::FreshWorkspace,
        AcceptanceLaunchMode::ExistingHomeRecovery,
    ] {
        let fixture = Fixture::new(FakeBehavior::Success);
        let workspace = match mode {
            AcceptanceLaunchMode::FreshWorkspace => Some(fixture.workspace.clone()),
            AcceptanceLaunchMode::ExistingHomeRecovery => {
                fs::create_dir(&fixture.home).unwrap();
                None
            }
        };
        let config = AcceptanceSessionConfig::new(
            &fixture.executable,
            &fixture.home,
            mode,
            workspace,
            &fixture.evidence,
            "mode-starter",
            limits(1, 64),
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(config.launch_mode(), mode);
        let session = AcceptanceSession::start(config).unwrap();
        let outcome = session.finish();
        let evidence = outcome.evidence();
        assert_eq!(evidence.launch_mode, mode);
        assert_eq!(
            evidence.fixture.execution_workspace.is_some(),
            mode == AcceptanceLaunchMode::FreshWorkspace
        );
        assert_eq!(
            evidence.process.execution_workspace.is_some(),
            mode == AcceptanceLaunchMode::FreshWorkspace
        );
        let serialized = serde_json::to_value(evidence).unwrap();
        assert_eq!(serialized["launchMode"], mode.as_str());
        assert_eq!(
            serialized["fixture"].get("executionWorkspace").is_some(),
            mode == AcceptanceLaunchMode::FreshWorkspace
        );
        assert_eq!(
            serialized["process"].get("executionWorkspace").is_some(),
            mode == AcceptanceLaunchMode::FreshWorkspace
        );
        fixture.root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn acceptance_launch_modes_forward_exact_mode_specific_host_arguments() {
    for mode in [
        AcceptanceLaunchMode::FreshWorkspace,
        AcceptanceLaunchMode::ExistingHomeRecovery,
    ] {
        let fixture = Fixture::new(FakeBehavior::Success);
        let args_path = fixture.root.join(format!("{mode:?}-arguments.txt"));
        let executable = fake_argument_capture_executable(fixture.root.path(), &args_path);
        let workspace = match mode {
            AcceptanceLaunchMode::FreshWorkspace => Some(fixture.workspace.clone()),
            AcceptanceLaunchMode::ExistingHomeRecovery => {
                fs::create_dir(&fixture.home).unwrap();
                None
            }
        };
        let config = AcceptanceSessionConfig::new(
            executable,
            &fixture.home,
            mode,
            workspace.clone(),
            &fixture.evidence,
            "acceptance-launch-arguments",
            limits(1, 64),
            Duration::from_secs(1),
        )
        .unwrap();

        let session = AcceptanceSession::start(config).unwrap();
        let outcome = session.finish();
        assert_eq!(outcome.evidence().publication.outcome, "published");

        let arguments = fs::read_to_string(&args_path).unwrap();
        assert!(arguments.contains("--diagnostic-target-stdio"));
        assert!(arguments.contains("--beryl-home-dir"));
        assert!(arguments.contains("--diagnostic-acceptance-startup-gate"));
        assert_eq!(
            arguments.matches("--host-path").count(),
            usize::from(mode == AcceptanceLaunchMode::FreshWorkspace)
        );
        if let Some(workspace) = workspace {
            let canonical_workspace = fs::canonicalize(workspace).unwrap();
            assert!(
                arguments.contains(&format!("--host-path {}", canonical_workspace.display())),
                "fresh launch did not forward the canonical workspace argument: {arguments}"
            );
        }
        fixture.root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn config_rejects_invalid_recovery_cleanup_budget_before_launch() {
    for recovery_cleanup_timeout in [
        Duration::ZERO,
        beryl_app::MAX_ACCEPTANCE_CLEANUP_TIMEOUT + Duration::from_millis(1),
    ] {
        let fixture = Fixture::new(FakeBehavior::Success);
        let error = AcceptanceSessionConfig::new(
            &fixture.executable,
            &fixture.home,
            AcceptanceLaunchMode::FreshWorkspace,
            Some(fixture.workspace.clone()),
            &fixture.evidence,
            "invalid-recovery-budget",
            limits(1, 64),
            recovery_cleanup_timeout,
        )
        .unwrap_err();
        assert!(error.to_string().contains("recovery cleanup timeout"));
        assert!(!fixture.home.exists());
        assert!(!fixture.evidence.exists());
        fixture.root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn compatible_session_records_bounded_requests_and_publishes_after_cleanup() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let mut session = AcceptanceSession::start(fixture.config(limits(1, 8))).unwrap();
    assert!(!fixture.evidence.exists());

    let response = session
        .request(AcceptanceRequest::new("read_process", json!({})).unwrap())
        .unwrap();
    assert_eq!(response.request_id(), Some("2"));
    assert_eq!(response.result(), &json!({"value":"abcdefghijk"}));
    assert!(matches!(
        session.request(AcceptanceRequest::new("read_process", json!({})).unwrap()),
        Err(AcceptanceSessionError::RequestLimit { limit: 1 })
    ));

    let outcome = session.finish();
    let evidence = outcome.evidence();
    assert!(fixture.evidence.is_file());
    assert_eq!(evidence.schema_version, 5);
    assert_eq!(evidence.launch_mode, AcceptanceLaunchMode::FreshWorkspace);
    assert_eq!(
        evidence.fixture.execution_workspace.as_deref(),
        Some(fixture.workspace.as_path())
    );
    assert_eq!(
        evidence.process.execution_workspace.as_deref(),
        Some(fixture.workspace.as_path())
    );
    assert_eq!(evidence.requests.len(), 1);
    assert_eq!(evidence.requests[0].request_id.as_deref(), Some("2"));
    assert_eq!(
        evidence.requests[0].protocol_identity_range.as_ref(),
        Some(&beryl_app::AcceptanceProtocolIdentityRangeEvidence {
            first_request_id: "2".to_string(),
            last_request_id: "2".to_string(),
            count: 1,
        })
    );
    assert_eq!(
        evidence.requests[0].params_sha256,
        "44136FA355B3678A1146AD16F7E8649E94FB4FC21FE77E8310C060F61CAAFF8A"
    );
    let response = evidence.requests[0].response.as_ref().unwrap();
    assert_eq!(response.serialized_bytes, 23);
    assert_eq!(
        response.sha256,
        "F95A083C2E22D56D5924CDA5B188C54E409BF54E776E0BAE41832D29B1253053"
    );
    assert_eq!(response.bounded_prefix, "{\"value\"");
    assert_eq!(response.prefix_bytes, 8);
    assert!(response.truncated);
    assert_eq!(evidence.cleanup.final_state, "verified_reclaimed");
    assert_eq!(evidence.cleanup.attempts.len(), 1);
    assert_eq!(
        evidence.cleanup.attempts[0].termination_method,
        "graceful_eof"
    );
    assert_eq!(evidence.publication.outcome, "published");
    assert_eq!(evidence.fixture.executable_bytes, 355);
    assert_eq!(
        evidence.fixture.executable_sha256,
        "3C4E2F6A78CBA591E6C6A96196A61AD47C37472C46FBA72ADCAF56D3B06B0F4F"
    );
    assert!(evidence.stderr.total_bytes > 0);
    assert_eq!(evidence.stderr.sha256.len(), 64);
    assert!(evidence.stderr.bounded_prefix.is_empty());
    assert_eq!(evidence.stderr.prefix_bytes, 0);
    assert!(evidence.stderr.truncated);
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn aggregate_budget_follows_request_observation_order_then_stderr() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let mut session = AcceptanceSession::start(fixture.config(limits(2, 8))).unwrap();
    session
        .request(AcceptanceRequest::new("read_process", json!({})).unwrap())
        .unwrap();
    let request_error = session
        .request(AcceptanceRequest::new("read_process", json!({})).unwrap())
        .unwrap_err()
        .to_string();
    let request_error_source = request_error
        .split_once(" failed: ")
        .expect("public request error wraps its recorded source")
        .1;

    let outcome = session.finish();
    let evidence = outcome.evidence();

    assert_eq!(evidence.requests.len(), 2);
    assert_eq!(
        evidence.requests[0].response.as_ref().unwrap().prefix_bytes,
        8
    );
    let error = evidence.requests[1].error.as_ref().unwrap();
    assert_eq!(error.total_bytes, request_error_source.len());
    assert_eq!(error.prefix_bytes, 0);
    assert!(error.bounded_prefix.is_empty());
    assert!(error.truncated);
    assert_eq!(error.sha256.len(), 64);
    assert!(evidence.stderr.total_bytes > 0);
    assert_eq!(evidence.stderr.prefix_bytes, 0);
    assert!(evidence.stderr.bounded_prefix.is_empty());
    assert!(evidence.stderr.truncated);
    drop(outcome);
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn invalid_stderr_lossy_prefix_obeys_tiny_encoded_budgets_and_hashes_full_stream() {
    for (budget, expected_prefix, expected_prefix_bytes) in [(1, "", 0), (3, "�", 3)] {
        let fixture = Fixture::new(FakeBehavior::InvalidStderr);
        let session = AcceptanceSession::start(fixture.config(limits(1, budget))).unwrap();

        let outcome = session.finish();
        let evidence = outcome.evidence();

        assert_eq!(evidence.stderr.total_bytes, 6);
        assert_eq!(
            evidence.stderr.sha256,
            "E431490617C7C1C4C45DF4891993BCE858B9FDDF3F9920C8CCD3E7B3888BCD02"
        );
        assert_eq!(evidence.stderr.bounded_prefix, expected_prefix);
        assert_eq!(evidence.stderr.prefix_bytes, expected_prefix_bytes);
        assert!(evidence.stderr.truncated);
        assert!(evidence.stderr.capture_complete);
        drop(outcome);
        fixture.root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn multibyte_stderr_never_splits_a_scalar_and_records_exact_boundary() {
    for (budget, expected_prefix, truncated) in [(2, "", true), (3, "€", true), (4, "€x", false)]
    {
        let fixture = Fixture::new(FakeBehavior::MultibyteStderr);
        let session = AcceptanceSession::start(fixture.config(limits(1, budget))).unwrap();

        let outcome = session.finish();
        let evidence = outcome.evidence();

        assert_eq!(evidence.stderr.total_bytes, 4);
        assert_eq!(
            evidence.stderr.sha256,
            "0E5A5F40123B30ADE3F28BE30DB9E75EE42FCED66ABD89D4FCA27F90683AC015"
        );
        assert_eq!(evidence.stderr.bounded_prefix, expected_prefix);
        assert_eq!(evidence.stderr.prefix_bytes, expected_prefix.len());
        assert_eq!(evidence.stderr.truncated, truncated);
        assert!(evidence.stderr.capture_complete);
        drop(outcome);
        fixture.root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn stderr_digest_continues_after_raw_prefix_retention_is_exhausted() {
    let fixture = Fixture::new(FakeBehavior::LongStderr);
    let long_stderr_limits = AcceptanceLimits::new(
        Duration::from_secs(5),
        Duration::from_secs(2),
        Duration::from_secs(10),
        1,
        18,
        Duration::from_secs(1),
    )
    .unwrap();
    let mut session = AcceptanceSession::start(fixture.config(long_stderr_limits)).unwrap();
    session
        .request(AcceptanceRequest::new("read_process", json!({})).unwrap())
        .unwrap();

    let outcome = session.finish();
    let evidence = outcome.evidence();

    assert_eq!(evidence.stderr.total_bytes, 5_001);
    assert_eq!(
        evidence.stderr.sha256,
        "417FE8F1539D8521DBE20B3320775C6B95ECE73E0D369A80B71C4CCB31EBD13E"
    );
    assert_eq!(evidence.stderr.bounded_prefix, "aaaa");
    assert_eq!(evidence.stderr.prefix_bytes, 4);
    assert!(evidence.stderr.truncated);
    assert!(evidence.stderr.capture_complete);
    drop(outcome);
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn evidence_publication_never_replaces_a_racing_destination() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let session = AcceptanceSession::start(fixture.config(limits(1, 64))).unwrap();
    fs::write(&fixture.evidence, b"operator-owned").unwrap();
    let outcome = session.finish();
    assert!(matches!(
        outcome.publication(),
        AcceptancePublicationState::Failed { .. }
    ));
    assert_eq!(outcome.evidence().publication.outcome, "failed");
    assert_eq!(fs::read(&fixture.evidence).unwrap(), b"operator-owned");
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn incompatible_handshake_is_rejected_without_publishing_evidence() {
    let fixture = Fixture::new(FakeBehavior::Incompatible);
    let error = match AcceptanceSession::start(fixture.config(limits(1, 64))) {
        Ok(_) => panic!("incompatible handshake unexpectedly launched"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("incompatible"));
    assert!(!fixture.evidence.exists());
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn gate_ready_failures_are_public_startup_protocol_causes() {
    for behavior in [
        FakeBehavior::GateReadyEof,
        FakeBehavior::GateReadyMalformed,
        FakeBehavior::GateReadyTimeout,
    ] {
        let fixture = Fixture::new(behavior);
        let gate_limits = AcceptanceLimits::new(
            Duration::from_millis(200),
            Duration::from_millis(150),
            Duration::from_secs(10),
            1,
            64,
            Duration::from_secs(1),
        )
        .unwrap();
        let failure = match AcceptanceSession::start(fixture.config(gate_limits)) {
            Ok(_) => panic!("gate-ready failure unexpectedly launched"),
            Err(failure) => failure,
        };
        let AcceptanceSessionStartCause::Diagnostic(cause) = failure.cause() else {
            panic!("gate-ready failure must be a typed diagnostic startup cause");
        };
        assert_eq!(
            cause.kind(),
            AcceptanceDiagnosticStartupCauseKind::StartupProtocol
        );
        assert!(!failure.has_owner());
        assert!(!fixture.evidence.exists());
        fixture.root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn executable_identity_failure_is_typed_before_spawn() {
    let fixture = Fixture::new(FakeBehavior::Success);
    let config = fixture.config(limits(1, 64));
    fs::remove_file(&fixture.executable).unwrap();

    let failure = match AcceptanceSession::start(config) {
        Ok(_) => panic!("missing executable unexpectedly launched"),
        Err(failure) => failure,
    };

    let AcceptanceSessionStartCause::Diagnostic(cause) = failure.cause() else {
        panic!("executable identity failure must be a typed diagnostic startup cause");
    };
    assert_eq!(
        cause.kind(),
        AcceptanceDiagnosticStartupCauseKind::ExecutableIdentity
    );
    assert!(!failure.has_owner());
    assert!(!fixture.home.exists());
    assert!(!fixture.evidence.exists());
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn timeout_is_recorded_and_cleanup_can_still_publish() {
    let fixture = Fixture::new(FakeBehavior::TimeoutAfterHandshake);
    let mut session = AcceptanceSession::start(fixture.config(limits(2, 8))).unwrap();
    let error = session
        .request(AcceptanceRequest::new("read_process", json!({})).unwrap())
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    let outcome = session.finish();
    let evidence = outcome.evidence();
    assert_eq!(
        evidence.cleanup.attempts[0].termination_method,
        "direct_kill"
    );
    assert_eq!(evidence.requests[0].outcome, "error");
    let request_error = evidence.requests[0].error.as_ref().unwrap();
    assert_eq!(request_error.total_bytes, 68);
    assert_eq!(
        request_error.sha256,
        "DBB0D39458240603CD8B69F1F9A69DB94AFD767CF8D24D6562618C95CF46B5FA"
    );
    assert_eq!(request_error.bounded_prefix, "timed ou");
    assert_eq!(request_error.prefix_bytes, 8);
    assert!(request_error.truncated);
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn malformed_partial_and_oversize_output_fail_boundedly() {
    for (behavior, request_timeout, expected_source_prefix) in [
        (
            FakeBehavior::MalformedAfterHandshake,
            Duration::from_secs(1),
            "diagnostic child protocol error: diagnostic protocol frame was not valid JSON",
        ),
        (
            FakeBehavior::PartialAfterHandshake,
            Duration::from_secs(1),
            "diagnostic child protocol error: diagnostic protocol frame was not valid JSON",
        ),
        (
            FakeBehavior::OversizeAfterHandshake,
            Duration::from_secs(3),
            "diagnostic child protocol error: diagnostic protocol frame exceeded 262144 bytes",
        ),
    ] {
        let fixture = Fixture::new(behavior);
        let mut session = AcceptanceSession::start(fixture.config(limits_with_request_timeout(
            1,
            128,
            request_timeout,
        )))
        .unwrap();
        let error = session
            .request(AcceptanceRequest::new("read_process", json!({})).unwrap())
            .unwrap_err();
        let message = error.to_string();
        let source_message = match &error {
            AcceptanceSessionError::DiagnosticRequest { message, .. } => message.as_str(),
            unexpected => panic!(
                "expected a diagnostic request failure, got {unexpected:?}; full request error: {message}"
            ),
        };
        assert!(
            source_message.starts_with(expected_source_prefix),
            "unexpected bounded diagnostic failure; expected source prefix {expected_source_prefix:?}, full request error: {message}"
        );
        let outcome = session.finish();
        let error = outcome.evidence().requests[0].error.as_ref().unwrap();
        assert!(
            error.prefix_bytes <= 128,
            "bounded error prefix exceeded its fixture limit; full request error: {message}"
        );
        assert_eq!(
            error.prefix_bytes,
            error.bounded_prefix.len(),
            "error evidence prefix byte count disagreed with retained text; full request error: {message}"
        );
        assert_eq!(
            error.sha256.len(),
            64,
            "error evidence digest length was invalid; full request error: {message}"
        );
        assert_eq!(
            error.total_bytes,
            source_message.len(),
            "error evidence byte count did not match the recorded source; full request error: {message}"
        );
        fixture.root.close().unwrap();
    }
}

#[cfg(windows)]
#[test]
fn drop_interrupts_session_without_publishing_and_reclaims_child() {
    let fixture = Fixture::new(FakeBehavior::IgnoreEof);
    let started = Instant::now();
    let session = AcceptanceSession::start(fixture.config(limits(1, 64))).unwrap();
    drop(session);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!fixture.evidence.exists());
    fixture.root.close().unwrap();
}

#[cfg(windows)]
#[test]
fn forced_cleanup_reclaims_the_exact_job_descendant() {
    let fixture = Fixture::new(FakeBehavior::SpawnDescendant);
    let session = AcceptanceSession::start(fixture.config(limits(1, 64))).unwrap();
    let pid_path = fixture.root.join("descendant.pid");
    let deadline = Instant::now() + Duration::from_secs(2);
    let pid = loop {
        if let Ok(pid) = fs::read_to_string(&pid_path)
            && let Ok(pid) = pid.trim().parse::<u32>()
        {
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "descendant PID was not published within the setup deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let descendant = ExactWindowsProcess::open_while_known_live(pid);
    assert_ne!(descendant.creation_identity(), 0);
    descendant.assert_still_active();
    drop(session);
    descendant.wait_until_exited(Duration::from_secs(2));
    descendant.assert_exited();
    fixture.root.close().unwrap();
}

fn fake_executable(root: &Path, behavior: FakeBehavior) -> PathBuf {
    let oversize = root.join("oversize-response.txt");
    if matches!(behavior, FakeBehavior::OversizeAfterHandshake) {
        fs::write(&oversize, format!("{}\n", "x".repeat(270_000))).unwrap();
    }
    let path = root.join(fake_file_name(behavior));
    fs::write(
        &path,
        fake_script(behavior, &oversize, &root.join("descendant.pid")),
    )
    .unwrap();
    make_executable(&path);
    path
}

#[cfg(windows)]
fn fake_argument_capture_executable(root: &Path, args_path: &Path) -> PathBuf {
    let path = root.join("fake argument capture.cmd");
    let script = format!(
        "@echo off\r\necho %*>\"{}\"\r\nset /p gate=\r\npowershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\nset /p line=\r\necho {{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{PROTOCOL}\",\"protocolVersion\":1}}}}\r\nset /p line=\r\n",
        args_path.display()
    );
    fs::write(&path, script).unwrap();
    path
}

#[cfg(windows)]
fn fake_file_name(behavior: FakeBehavior) -> &'static str {
    match behavior {
        FakeBehavior::Success => "fake success.cmd",
        FakeBehavior::InvalidStderr => "fake invalid stderr.cmd",
        FakeBehavior::LongStderr => "fake long stderr.cmd",
        FakeBehavior::MultibyteStderr => "fake multibyte stderr.cmd",
        FakeBehavior::Incompatible => "fake incompatible.cmd",
        FakeBehavior::TimeoutAfterHandshake => "fake timeout.cmd",
        FakeBehavior::MalformedAfterHandshake => "fake malformed.cmd",
        FakeBehavior::PartialAfterHandshake => "fake partial.cmd",
        FakeBehavior::OversizeAfterHandshake => "fake oversize.cmd",
        FakeBehavior::IgnoreEof => "fake ignore eof.cmd",
        FakeBehavior::GateReadyEof => "fake gate ready eof.cmd",
        FakeBehavior::GateReadyMalformed => "fake gate ready malformed.cmd",
        FakeBehavior::GateReadyTimeout => "fake gate ready timeout.cmd",
        FakeBehavior::SpawnDescendant => "fake descendant.cmd",
    }
}

#[cfg(not(windows))]
fn fake_file_name(behavior: FakeBehavior) -> &'static str {
    match behavior {
        FakeBehavior::Success => "fake-success.sh",
        FakeBehavior::InvalidStderr => "fake-invalid-stderr.sh",
        FakeBehavior::LongStderr => "fake-long-stderr.sh",
        FakeBehavior::MultibyteStderr => "fake-multibyte-stderr.sh",
        FakeBehavior::Incompatible => "fake-incompatible.sh",
        FakeBehavior::TimeoutAfterHandshake => "fake-timeout.sh",
        FakeBehavior::MalformedAfterHandshake => "fake-malformed.sh",
        FakeBehavior::PartialAfterHandshake => "fake-partial.sh",
        FakeBehavior::OversizeAfterHandshake => "fake-oversize.sh",
        FakeBehavior::IgnoreEof => "fake-ignore-eof.sh",
        FakeBehavior::GateReadyEof => "fake-gate-ready-eof.sh",
        FakeBehavior::GateReadyMalformed => "fake-gate-ready-malformed.sh",
        FakeBehavior::GateReadyTimeout => "fake-gate-ready-timeout.sh",
    }
}

fn handshake(version: u64) -> String {
    format!(
        "{{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"{PROTOCOL}\",\"protocolVersion\":{version}}}}}"
    )
}

#[cfg(windows)]
fn fake_script(behavior: FakeBehavior, oversize: &Path, descendant_pid: &Path) -> String {
    match behavior {
        FakeBehavior::GateReadyEof => {
            return "@echo off\r\nset /p gate=\r\nexit /b 0\r\n".to_string();
        }
        FakeBehavior::GateReadyMalformed => {
            return "@echo off\r\nset /p gate=\r\necho not-ready\r\nping -n 60 127.0.0.1 >nul\r\n"
                .to_string();
        }
        FakeBehavior::GateReadyTimeout => {
            return "@echo off\r\nset /p gate=\r\nping -n 60 127.0.0.1 >nul\r\n".to_string();
        }
        _ => {}
    }
    let handshake = handshake(if matches!(behavior, FakeBehavior::Incompatible) {
        99
    } else {
        1
    });
    let before_handshake = match behavior {
        FakeBehavior::InvalidStderr => {
            "powershell.exe -NoProfile -Command \"$stderr=[Console]::OpenStandardError(); $bytes=[byte[]](0xFF,0x61,0xE2,0x82,0xAC,0x62); $stderr.Write($bytes,0,$bytes.Length)\"\r\n"
        }
        FakeBehavior::MultibyteStderr => {
            "powershell.exe -NoProfile -Command \"$stderr=[Console]::OpenStandardError(); $bytes=[byte[]](0xE2,0x82,0xAC,0x78); $stderr.Write($bytes,0,$bytes.Length)\"\r\n"
        }
        _ => "",
    };
    let tail = match behavior {
        FakeBehavior::Success => concat!(
            "echo bounded-stderr 1>&2\r\n",
            "set /p line=\r\n",
            "echo {\"id\":\"2\",\"ok\":true,\"result\":{\"value\":\"abcdefghijk\"}}\r\n",
            "set /p line=\r\n"
        )
        .to_string(),
        FakeBehavior::InvalidStderr => "set /p line=\r\n".to_string(),
        FakeBehavior::LongStderr => concat!(
            "powershell.exe -NoProfile -Command \"$stderr=[Console]::OpenStandardError(); $bytes=[Text.Encoding]::ASCII.GetBytes(('a' * 5000) + 'b'); $stderr.Write($bytes,0,$bytes.Length)\"\r\n",
            "set /p line=\r\n",
            "echo {\"id\":\"2\",\"ok\":true,\"result\":{\"ready\":true}}\r\n",
            "set /p line=\r\n"
        )
        .to_string(),
        FakeBehavior::MultibyteStderr => "set /p line=\r\n".to_string(),
        FakeBehavior::Incompatible
        | FakeBehavior::IgnoreEof
        | FakeBehavior::GateReadyEof
        | FakeBehavior::GateReadyMalformed
        | FakeBehavior::GateReadyTimeout => "ping -n 60 127.0.0.1 >nul\r\n".to_string(),
        FakeBehavior::TimeoutAfterHandshake => {
            "set /p line=\r\nping -n 60 127.0.0.1 >nul\r\n".to_string()
        }
        FakeBehavior::MalformedAfterHandshake => "set /p line=\r\necho not-json\r\n".to_string(),
        FakeBehavior::PartialAfterHandshake => {
            "set /p line=\r\n<nul set /p ={\"id\":\r\n".to_string()
        }
        FakeBehavior::OversizeAfterHandshake => {
            format!("set /p line=\r\ntype \"{}\"\r\n", oversize.display())
        }
        FakeBehavior::SpawnDescendant => format!(
            "powershell -NoProfile -Command \"$p=Start-Process ping -ArgumentList '-n','60','127.0.0.1' -PassThru -WindowStyle Hidden; [IO.File]::WriteAllText('{}', [string]$p.Id); Wait-Process -Id $p.Id\"\r\n",
            descendant_pid.display()
        ),
    };
    format!(
        "@echo off\r\nset /p gate=\r\npowershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\nset /p line=\r\n{before_handshake}echo {handshake}\r\n{tail}"
    )
}

#[cfg(not(windows))]
fn fake_script(behavior: FakeBehavior, oversize: &Path, _descendant_pid: &Path) -> String {
    match behavior {
        FakeBehavior::GateReadyEof => {
            return "#!/bin/sh\nIFS= read -r gate\nexit 0\n".to_string();
        }
        FakeBehavior::GateReadyMalformed => {
            return "#!/bin/sh\nIFS= read -r gate\nprintf '%s\\n' 'not-ready'\nsleep 60\n"
                .to_string();
        }
        FakeBehavior::GateReadyTimeout => {
            return "#!/bin/sh\nIFS= read -r gate\nsleep 60\n".to_string();
        }
        _ => {}
    }
    let handshake = handshake(if matches!(behavior, FakeBehavior::Incompatible) {
        99
    } else {
        1
    });
    let tail = match behavior {
        FakeBehavior::Success => concat!(
            "printf '%s\\n' 'bounded-stderr' >&2\n",
            "IFS= read -r line\n",
            "printf '%s\\n' '{\"id\":\"2\",\"ok\":true,\"result\":{\"value\":\"abcdefghijk\"}}'\n",
            "IFS= read -r line\n"
        )
        .to_string(),
        FakeBehavior::InvalidStderr => {
            "printf '\\377a\\342\\202\\254b' >&2\nIFS= read -r line\n".to_string()
        }
        FakeBehavior::LongStderr => {
            "i=0; while [ $i -lt 5000 ]; do printf 'a' >&2; i=$((i + 1)); done; printf 'b' >&2\nIFS= read -r line\nprintf '%s\n' '{\"id\":\"2\",\"ok\":true,\"result\":{\"ready\":true}}'\nIFS= read -r line\n".to_string()
        }
        FakeBehavior::MultibyteStderr => {
            "printf '€x' >&2\nIFS= read -r line\n".to_string()
        }
        FakeBehavior::Incompatible
        | FakeBehavior::IgnoreEof
        | FakeBehavior::GateReadyEof
        | FakeBehavior::GateReadyMalformed
        | FakeBehavior::GateReadyTimeout => "sleep 60\n".to_string(),
        FakeBehavior::TimeoutAfterHandshake => "IFS= read -r line\nsleep 60\n".to_string(),
        FakeBehavior::MalformedAfterHandshake => {
            "IFS= read -r line\nprintf '%s\\n' 'not-json'\n".to_string()
        }
        FakeBehavior::PartialAfterHandshake => {
            "IFS= read -r line\nprintf '%s' '{\"id\":'\n".to_string()
        }
        FakeBehavior::OversizeAfterHandshake => {
            format!("IFS= read -r line\ncat '{}'\n", oversize.display())
        }
    };
    format!(
        "#!/bin/sh\nIFS= read -r gate\nprintf '%s\\n' 'beryl_diagnostic_acceptance_ready_v1'\nIFS= read -r line\nprintf '%s\\n' '{handshake}'\n{tail}"
    )
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

#[cfg(windows)]
struct ExactWindowsProcess {
    handle: windows::Win32::Foundation::HANDLE,
    creation_identity: u64,
}

#[cfg(windows)]
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

#[cfg(windows)]
impl Drop for ExactWindowsProcess {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
    }
}
