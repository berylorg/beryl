use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

#[cfg(windows)]
use std::{path::PathBuf, time::Instant};

use beryl::acceptance_cli::{
    AcceptanceCli, AcceptanceCliRecoveryOutcome, AcceptanceCliStartupFailure,
    MAX_ACCEPTANCE_REQUEST_PLAN_BYTES,
};
use beryl_app::AcceptanceSession;
use clap::error::ErrorKind;
use serde_json::Value;

fn required_args(root: &Path) -> Vec<String> {
    vec![
        "--executable".to_string(),
        root.join("beryl.exe").display().to_string(),
        "--isolated-home".to_string(),
        root.join("home").display().to_string(),
        "--execution-workspace".to_string(),
        root.join("workspace").display().to_string(),
        "--evidence".to_string(),
        root.join("evidence.json").display().to_string(),
        "--run-identity".to_string(),
        "cli-run".to_string(),
        "--request-plan".to_string(),
        root.join("requests.json").display().to_string(),
    ]
}

fn parse(args: Vec<String>) -> Result<AcceptanceCli, clap::Error> {
    AcceptanceCli::try_parse_from(std::iter::once("beryl-acceptance".to_string()).chain(args))
}

#[test]
fn help_exposes_all_owned_paths_and_bounds() {
    let error = parse(vec!["--help".to_string()]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::DisplayHelp);
    let help = error.to_string();
    for flag in [
        "--executable <PATH>",
        "--isolated-home <PATH>",
        "--launch-mode <LAUNCH_MODE>",
        "--execution-workspace <PATH>",
        "--evidence <PATH>",
        "--run-identity <ID>",
        "--request-plan <PATH>",
        "--startup-timeout-ms <MS>",
        "--request-timeout-ms <MS>",
        "--runtime-timeout-ms <MS>",
        "--max-requests <COUNT>",
        "--max-output-bytes <BYTES>",
        "--cleanup-timeout-ms <MS>",
        "--recovery-cleanup-timeout-ms <MS>",
    ] {
        assert!(help.contains(flag), "missing {flag} from help: {help}");
    }
}

#[test]
fn required_inputs_and_nonzero_bounds_are_enforced_by_clap() {
    let missing = parse(Vec::new()).unwrap_err();
    assert_eq!(missing.kind(), ErrorKind::MissingRequiredArgument);

    let root = std::env::current_dir().unwrap();
    for flag in [
        "--startup-timeout-ms",
        "--request-timeout-ms",
        "--runtime-timeout-ms",
        "--max-requests",
        "--max-output-bytes",
        "--cleanup-timeout-ms",
        "--recovery-cleanup-timeout-ms",
    ] {
        let mut args = required_args(&root);
        args.extend([flag.to_string(), "0".to_string()]);
        assert_eq!(parse(args).unwrap_err().kind(), ErrorKind::ValueValidation);
    }
}

#[test]
fn parsed_identity_and_paths_are_typed() {
    let root = std::env::current_dir().unwrap();
    let cli = parse(required_args(&root)).unwrap();
    assert_eq!(cli.run_identity(), "cli-run");
    assert_eq!(cli.max_requests(), 64);
    assert_eq!(cli.executable(), root.join("beryl.exe"));
    assert_eq!(cli.request_plan(), root.join("requests.json"));
    assert_eq!(
        cli.launch_mode(),
        beryl_app::AcceptanceLaunchMode::FreshWorkspace
    );
}

#[test]
fn launch_mode_rejects_unknown_text() {
    let root = std::env::current_dir().unwrap();
    let mut args = required_args(&root);
    args.extend(["--launch-mode".to_string(), "recovery-ish".to_string()]);
    assert_eq!(parse(args).unwrap_err().kind(), ErrorKind::InvalidValue);
}

#[test]
fn recovery_cleanup_timeout_is_bounded_before_launch() {
    let root = std::env::current_dir().unwrap();
    let mut args = required_args(&root);
    args.extend([
        "--recovery-cleanup-timeout-ms".to_string(),
        "60001".to_string(),
    ]);
    let error = parse(args).unwrap().run().unwrap_err();
    assert!(error.to_string().contains("Beryl-owned limit"));
}

struct FakeOwnerFailure {
    observed_timeout: Arc<Mutex<Option<Duration>>>,
    observed_release: Arc<Mutex<Option<u32>>>,
    outcome: Option<Result<AcceptanceCliRecoveryOutcome, String>>,
    owner_retained: bool,
    released_pid: Option<u32>,
}

impl AcceptanceCliStartupFailure for FakeOwnerFailure {
    fn message(&self) -> String {
        "forced owner-bearing startup cause".to_string()
    }

    fn retry_cleanup(&mut self, timeout: Duration) -> Result<AcceptanceCliRecoveryOutcome, String> {
        *self.observed_timeout.lock().unwrap() = Some(timeout);
        let outcome = self.outcome.take().unwrap()?;
        if matches!(outcome, AcceptanceCliRecoveryOutcome::Reclaimed { .. }) {
            self.owner_retained = false;
        }
        Ok(outcome)
    }

    fn has_owner(&self) -> bool {
        self.owner_retained
    }

    fn release_owner_fail_safe_nonblocking(&mut self) -> Option<u32> {
        self.owner_retained = false;
        let released_pid = self.released_pid.take();
        *self.observed_release.lock().unwrap() = released_pid;
        released_pid
    }
}

#[cfg(windows)]
#[test]
fn launch_mode_path_combinations_reject_before_the_starter_runs() {
    for case in [
        "fresh-missing-workspace",
        "recovery-with-workspace",
        "recovery-missing-home",
        "recovery-file-home",
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("workspace")).unwrap();
        let executable = fake_executable(root.path());
        fs::write(
            root.path().join("requests.json"),
            r#"{"schemaVersion":1,"requests":[{"command":"read_process","params":{}}]}"#,
        )
        .unwrap();
        let mut args = required_args(root.path());
        let executable_value = args
            .iter()
            .position(|value| value == "--executable")
            .unwrap()
            + 1;
        args[executable_value] = executable.display().to_string();
        match case {
            "fresh-missing-workspace" => remove_flag_and_value(&mut args, "--execution-workspace"),
            "recovery-with-workspace" => args.extend([
                "--launch-mode".to_string(),
                "existing-home-recovery".to_string(),
            ]),
            "recovery-missing-home" => {
                remove_flag_and_value(&mut args, "--execution-workspace");
                args.extend([
                    "--launch-mode".to_string(),
                    "existing-home-recovery".to_string(),
                ]);
            }
            "recovery-file-home" => {
                remove_flag_and_value(&mut args, "--execution-workspace");
                fs::write(root.path().join("home"), b"not a directory").unwrap();
                args.extend([
                    "--launch-mode".to_string(),
                    "existing-home-recovery".to_string(),
                ]);
            }
            _ => unreachable!(),
        }
        let error = parse(args)
            .unwrap()
            .run_with_starter(|_| panic!("invalid mode fixture must not start a session"))
            .unwrap_err()
            .to_string();
        match case {
            "fresh-missing-workspace" => assert!(error.contains("requires an execution workspace")),
            "recovery-with-workspace" => assert!(error.contains("forbids an execution workspace")),
            "recovery-missing-home" | "recovery-file-home" => {
                assert!(
                    error.contains("inspect isolated home") || error.contains("existing directory"),
                    "unexpected error: {error}"
                )
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(windows)]
#[test]
fn fresh_and_recovery_configs_reach_the_starter_with_typed_workspace_state() {
    for mode in [
        beryl_app::AcceptanceLaunchMode::FreshWorkspace,
        beryl_app::AcceptanceLaunchMode::ExistingHomeRecovery,
    ] {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let executable = fake_executable(root.path());
        fs::write(
            root.path().join("requests.json"),
            r#"{"schemaVersion":1,"requests":[{"command":"read_process","params":{}}]}"#,
        )
        .unwrap();
        let mut args = required_args(root.path());
        let executable_value = args
            .iter()
            .position(|value| value == "--executable")
            .unwrap()
            + 1;
        args[executable_value] = executable.display().to_string();
        if mode == beryl_app::AcceptanceLaunchMode::ExistingHomeRecovery {
            fs::create_dir(root.path().join("home")).unwrap();
            remove_flag_and_value(&mut args, "--execution-workspace");
            args.extend([
                "--launch-mode".to_string(),
                "existing-home-recovery".to_string(),
            ]);
        }
        let observed = Arc::new(Mutex::new(None));
        let observed_for_starter = Arc::clone(&observed);
        let fake = || FakeOwnerFailure {
            observed_timeout: Arc::new(Mutex::new(None)),
            observed_release: Arc::new(Mutex::new(None)),
            outcome: Some(Ok(AcceptanceCliRecoveryOutcome::AlreadyReclaimed)),
            owner_retained: false,
            released_pid: None,
        };
        let error = parse(args)
            .unwrap()
            .run_with_starter(|config| {
                *observed_for_starter.lock().unwrap() = Some((
                    config.launch_mode(),
                    config.execution_workspace().map(Path::to_path_buf),
                ));
                Err(Box::new(fake()))
            })
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("forced owner-bearing startup cause")
        );
        assert_eq!(
            *observed.lock().unwrap(),
            Some((
                mode,
                (mode == beryl_app::AcceptanceLaunchMode::FreshWorkspace)
                    .then(|| workspace.clone()),
            ))
        );
    }
}

fn remove_flag_and_value(args: &mut Vec<String>, flag: &str) {
    let index = args
        .iter()
        .position(|value| value == flag)
        .expect("fixture includes requested flag");
    args.drain(index..=index + 1);
}

#[test]
fn deterministic_cli_recovery_uses_separate_timeout_and_reports_owner_outcome() {
    for still_retained in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let executable = fake_executable(root.path());
        fs::write(
            root.path().join("requests.json"),
            r#"{"schemaVersion":1,"requests":[{"command":"read_process","params":{}}]}"#,
        )
        .unwrap();
        let mut args = required_args(root.path());
        let executable_value = args
            .iter()
            .position(|value| value == "--executable")
            .unwrap()
            + 1;
        args[executable_value] = executable.display().to_string();
        args.extend([
            "--recovery-cleanup-timeout-ms".to_string(),
            "1234".to_string(),
        ]);
        let observed_timeout = Arc::new(Mutex::new(None));
        let observed_release = Arc::new(Mutex::new(None));
        let fake = FakeOwnerFailure {
            observed_timeout: Arc::clone(&observed_timeout),
            observed_release: Arc::clone(&observed_release),
            outcome: Some(Ok(if still_retained {
                AcceptanceCliRecoveryOutcome::StillRetained {
                    pid: 4242,
                    home_dir: root.path().join("home"),
                    executable_path: executable.clone(),
                    error: "forced retry expiry".to_string(),
                }
            } else {
                AcceptanceCliRecoveryOutcome::Reclaimed { pid: 4242 }
            })),
            owner_retained: true,
            released_pid: Some(4242),
        };
        let error = parse(args)
            .unwrap()
            .run_with_starter(|_| Err(Box::new(fake)))
            .unwrap_err()
            .to_string();
        assert_eq!(
            *observed_timeout.lock().unwrap(),
            Some(Duration::from_millis(1234))
        );
        assert!(error.contains("forced owner-bearing startup cause"));
        if still_retained {
            assert!(error.contains("remained indeterminate for process 4242"));
            assert!(error.contains("fail-safe closure for retained process 4242"));
            assert_eq!(*observed_release.lock().unwrap(), Some(4242));
        } else {
            assert!(error.contains("verified process 4242 stopped, reaped"));
            assert!(!error.contains("fail-safe closure"));
            assert_eq!(*observed_release.lock().unwrap(), None);
        }
    }
}

#[test]
fn deterministic_cli_recovery_rejection_releases_exact_owner_fail_safe() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("workspace")).unwrap();
    let executable = fake_executable(root.path());
    fs::write(
        root.path().join("requests.json"),
        r#"{"schemaVersion":1,"requests":[{"command":"read_process","params":{}}]}"#,
    )
    .unwrap();
    let mut args = required_args(root.path());
    let executable_value = args
        .iter()
        .position(|value| value == "--executable")
        .unwrap()
        + 1;
    args[executable_value] = executable.display().to_string();
    args.extend([
        "--recovery-cleanup-timeout-ms".to_string(),
        "1234".to_string(),
    ]);
    let observed_timeout = Arc::new(Mutex::new(None));
    let observed_release = Arc::new(Mutex::new(None));
    let fake = FakeOwnerFailure {
        observed_timeout: Arc::clone(&observed_timeout),
        observed_release: Arc::clone(&observed_release),
        outcome: Some(Err("forced recovery rejection".to_string())),
        owner_retained: true,
        released_pid: Some(4242),
    };

    let error = parse(args)
        .unwrap()
        .run_with_starter(|_| Err(Box::new(fake)))
        .unwrap_err()
        .to_string();

    assert_eq!(
        *observed_timeout.lock().unwrap(),
        Some(Duration::from_millis(1234))
    );
    assert_eq!(*observed_release.lock().unwrap(), Some(4242));
    assert!(error.contains("forced owner-bearing startup cause"));
    assert!(error.contains("explicit recovery cleanup was rejected: forced recovery rejection"));
    assert!(error.contains("fail-safe closure for retained process Some(4242)"));
}

#[test]
fn incompatible_request_plan_is_rejected_before_launch() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("requests.json"),
        r#"{"schemaVersion":99,"requests":[{"command":"read_process"}]}"#,
    )
    .unwrap();
    let error = parse(required_args(root.path()))
        .unwrap()
        .run()
        .unwrap_err();
    assert!(error.to_string().contains("incompatible"));
}

#[test]
fn unsupported_malformed_and_excess_timeout_plans_never_invoke_the_starter() {
    for plan in [
        r#"{"schemaVersion":1,"requests":[{"command":"not_a_command","params":{}}]}"#,
        r#"{"schemaVersion":1,"requests":[{"command":"start_turn","params":{"text":""}}]}"#,
        r#"{"schemaVersion":1,"requests":[{"command":"read_ui_state","params":{},"timeoutMillis":10001}]}"#,
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("requests.json"), plan).unwrap();
        let error = parse(required_args(root.path()))
            .unwrap()
            .run_with_starter(|_| panic!("invalid plan must not start a session"))
            .unwrap_err();
        assert!(
            error.to_string().contains("invalid diagnostic operation")
                || error
                    .to_string()
                    .contains("exceeds session request timeout"),
            "unexpected error: {error}",
        );
    }
}

#[test]
fn oversized_plan_and_oversized_operation_payload_never_invoke_the_starter() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("requests.json"),
        vec![b'x'; MAX_ACCEPTANCE_REQUEST_PLAN_BYTES + 1],
    )
    .unwrap();
    let error = parse(required_args(root.path()))
        .unwrap()
        .run_with_starter(|_| panic!("oversized plan must not start a session"))
        .unwrap_err();
    assert!(error.to_string().contains("exceeds 262144 bytes"));

    fs::write(
        root.path().join("requests.json"),
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 1,
            "requests": [{
                "command": "start_turn",
                "params": { "text": "x".repeat(32 * 1024) },
            }],
        }))
        .unwrap(),
    )
    .unwrap();
    let error = parse(required_args(root.path()))
        .unwrap()
        .run_with_starter(|_| panic!("oversized operation payload must not start a session"))
        .unwrap_err();
    assert!(error.to_string().contains("text exceeds"));
}

#[test]
fn cli_runs_bounded_sequential_requests_and_publishes_evidence() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let executable = fake_executable(root.path());
    let request_plan = root.path().join("requests.json");
    fs::write(
        &request_plan,
        r#"{"schemaVersion":1,"requests":[{"command":"read_process","params":{}},{"command":"read_ui_state","params":{},"timeoutMillis":500}]}"#,
    )
    .unwrap();
    let evidence = root.path().join("evidence.json");
    let args = vec![
        "--executable".to_string(),
        executable.display().to_string(),
        "--isolated-home".to_string(),
        root.path().join("home").display().to_string(),
        "--execution-workspace".to_string(),
        workspace.display().to_string(),
        "--evidence".to_string(),
        evidence.display().to_string(),
        "--run-identity".to_string(),
        "cli-sequential".to_string(),
        "--request-plan".to_string(),
        request_plan.display().to_string(),
        "--request-timeout-ms".to_string(),
        "1000".to_string(),
        "--cleanup-timeout-ms".to_string(),
        "1000".to_string(),
    ];
    parse(args).unwrap().run().unwrap();

    let evidence: Value = serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
    assert_eq!(evidence["schemaVersion"], 5);
    assert_eq!(evidence["launchMode"], "fresh_workspace");
    assert_eq!(
        evidence["fixture"]["executionWorkspace"],
        workspace.display().to_string()
    );
    assert_eq!(evidence["runIdentity"], "cli-sequential");
    assert_eq!(evidence["requests"].as_array().unwrap().len(), 2);
    assert_eq!(evidence["requests"][0]["requestId"], "2");
    assert_eq!(evidence["requests"][0]["protocolIdentityRange"]["count"], 1);
    assert_eq!(evidence["requests"][1]["requestId"], "3");
    assert_eq!(evidence["cleanup"]["finalState"], "verified_reclaimed");
    assert_eq!(evidence["cleanup"]["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(evidence["publication"]["outcome"], "published");
}

#[test]
fn phase_sixteen_shaped_plan_executes_in_order_with_exact_expanded_id_ranges() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let executable = phase_sixteen_fixture_executable(root.path());
    let request_plan = root.path().join("requests.json");
    fs::write(
        &request_plan,
        r#"{"schemaVersion":1,"requests":[{"command":"start_turn","params":{"text":"Continue from the root doc/plan.md."}},{"command":"wait_for_state","params":{"predicate":"selected_thread_active","timeoutMs":5000,"pollIntervalMs":25,"workspaceId":"workspace-1","threadId":"thread-1","turnId":"turn-1"},"timeoutMillis":5000},{"command":"read_ui_state","params":{"limit":64}},{"command":"hard_stop_turn","params":{"expectedThreadId":"thread-1","expectedTurnId":"turn-1"}},{"command":"wait_for_state","params":{"predicate":"selected_thread_idle","timeoutMs":5000,"pollIntervalMs":25,"workspaceId":"workspace-1","threadId":"thread-1"},"timeoutMillis":5000},{"command":"read_ui_state","params":{"limit":64}}]}"#,
    )
    .unwrap();
    let evidence_path = root.path().join("phase-16-evidence.json");
    let args = vec![
        "--executable".to_string(),
        executable.display().to_string(),
        "--isolated-home".to_string(),
        root.path().join("home").display().to_string(),
        "--execution-workspace".to_string(),
        workspace.display().to_string(),
        "--evidence".to_string(),
        evidence_path.display().to_string(),
        "--run-identity".to_string(),
        "phase-16-shaped".to_string(),
        "--request-plan".to_string(),
        request_plan.display().to_string(),
        "--request-timeout-ms".to_string(),
        "5000".to_string(),
        "--runtime-timeout-ms".to_string(),
        "30000".to_string(),
        "--cleanup-timeout-ms".to_string(),
        "1000".to_string(),
    ];

    parse(args).unwrap().run().unwrap();

    let evidence: Value = serde_json::from_slice(&fs::read(evidence_path).unwrap()).unwrap();
    assert_eq!(evidence["schemaVersion"], 5);
    assert_eq!(evidence["launchMode"], "fresh_workspace");
    let requests = evidence["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 6);
    assert_eq!(
        requests
            .iter()
            .map(|request| request["command"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "start_turn",
            "wait_for_state",
            "read_ui_state",
            "hard_stop_turn",
            "wait_for_state",
            "read_ui_state",
        ]
    );
    assert_eq!(requests[1]["requestId"], "4");
    assert_eq!(requests[1]["protocolIdentityRange"]["firstRequestId"], "3");
    assert_eq!(requests[1]["protocolIdentityRange"]["lastRequestId"], "4");
    assert_eq!(requests[1]["protocolIdentityRange"]["count"], 2);
    assert_eq!(requests[4]["requestId"], "7");
    assert_eq!(requests[4]["protocolIdentityRange"]["count"], 1);
    assert_eq!(evidence["cleanup"]["finalState"], "verified_reclaimed");
    assert_eq!(evidence["publication"]["outcome"], "published");
}

#[test]
fn cli_reports_publication_failure_and_preserves_racing_destination() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let executable = fake_executable(root.path());
    let request_plan = root.path().join("requests.json");
    fs::write(
        &request_plan,
        r#"{"schemaVersion":1,"requests":[{"command":"read_process","params":{}}]}"#,
    )
    .unwrap();
    let evidence = root.path().join("evidence.json");
    let args = vec![
        "--executable".to_string(),
        executable.display().to_string(),
        "--isolated-home".to_string(),
        root.path().join("home").display().to_string(),
        "--execution-workspace".to_string(),
        workspace.display().to_string(),
        "--evidence".to_string(),
        evidence.display().to_string(),
        "--run-identity".to_string(),
        "cli-publication-failure".to_string(),
        "--request-plan".to_string(),
        request_plan.display().to_string(),
        "--request-timeout-ms".to_string(),
        "1000".to_string(),
        "--cleanup-timeout-ms".to_string(),
        "1000".to_string(),
    ];

    let error = parse(args)
        .unwrap()
        .run_with_starter(|config| {
            let evidence_path = config.evidence_path().to_path_buf();
            let session = AcceptanceSession::start(config)
                .unwrap_or_else(|failure| panic!("fixture failed to start: {failure}"));
            fs::write(evidence_path, b"operator-owned").unwrap();
            Ok::<_, Box<dyn AcceptanceCliStartupFailure>>(session)
        })
        .unwrap_err();

    assert!(error.to_string().contains("evidence publication failed"));
    assert_eq!(fs::read(evidence).unwrap(), b"operator-owned");
}

#[cfg(windows)]
#[test]
fn cli_startup_failure_leaves_no_live_descendant() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let descendant_pid_path = root.path().join("descendant.pid");
    let release_path = root.path().join("release-incompatible-response");
    let executable =
        incompatible_descendant_executable(root.path(), &descendant_pid_path, &release_path);
    fs::write(
        root.path().join("requests.json"),
        r#"{"schemaVersion":1,"requests":[{"command":"read_process","params":{}}]}"#,
    )
    .unwrap();
    let args = vec![
        "--executable".to_string(),
        executable.display().to_string(),
        "--isolated-home".to_string(),
        root.path().join("home").display().to_string(),
        "--execution-workspace".to_string(),
        workspace.display().to_string(),
        "--evidence".to_string(),
        root.path().join("evidence.json").display().to_string(),
        "--run-identity".to_string(),
        "cli-startup-recovery".to_string(),
        "--request-plan".to_string(),
        root.path().join("requests.json").display().to_string(),
        "--cleanup-timeout-ms".to_string(),
        "3".to_string(),
        "--recovery-cleanup-timeout-ms".to_string(),
        "1000".to_string(),
    ];
    std::thread::scope(|scope| {
        let run = scope.spawn(move || parse(args).unwrap().run());
        let mut release = FixtureRelease::new(release_path);
        let descendant_pid = wait_for_descendant_pid(&descendant_pid_path, Duration::from_secs(5));
        let descendant = ExactWindowsProcess::open_while_known_live(descendant_pid);
        descendant.assert_still_active();

        release.release();
        let error = run.join().unwrap().unwrap_err().to_string();
        assert!(error.contains("startup failed"));
        descendant.assert_exited_within(Duration::from_secs(2));
    });
}

fn phase_sixteen_fixture_executable(root: &Path) -> std::path::PathBuf {
    let path = root.join(if cfg!(windows) {
        "phase-16-shaped.cmd"
    } else {
        "phase-16-shaped.sh"
    });
    #[cfg(windows)]
    let script = concat!(
        "@echo off\r\n",
        "set /p gate=\r\n",
        "powershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"handshake\",\"id\":\"1\",\"params\":{}}') { exit 91 }\"\r\n",
        "echo {\"id\":\"1\",\"ok\":true,\"result\":{\"protocol\":\"beryl_diagnostic_child\",\"protocolVersion\":1}}\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"start_turn\",\"id\":\"2\",\"params\":{\"text\":\"Continue from the root doc/plan.md.\"}}') { exit 92 }\"\r\n",
        "echo {\"id\":\"2\",\"ok\":true,\"result\":{\"threadId\":\"thread-1\",\"turnId\":\"turn-1\"}}\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"read_ui_state\",\"id\":\"3\",\"params\":{\"limit\":64}}') { exit 93 }\"\r\n",
        "echo {\"id\":\"3\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"wrong\",\"selectedThreadId\":\"thread-1\",\"turnState\":{\"selectedThreadState\":\"working\",\"cancellableActiveTurn\":{\"turnId\":\"turn-1\"}}}}\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"read_ui_state\",\"id\":\"4\",\"params\":{\"limit\":64}}') { exit 94 }\"\r\n",
        "echo {\"id\":\"4\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\",\"turnState\":{\"selectedThreadState\":\"working\",\"cancellableActiveTurn\":{\"turnId\":\"turn-1\"}}}}\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"read_ui_state\",\"id\":\"5\",\"params\":{\"limit\":64}}') { exit 95 }\"\r\n",
        "echo {\"id\":\"5\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\"}}\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"hard_stop_turn\",\"id\":\"6\",\"params\":{\"expectedThreadId\":\"thread-1\",\"expectedTurnId\":\"turn-1\"}}') { exit 96 }\"\r\n",
        "echo {\"id\":\"6\",\"ok\":true,\"result\":{\"status\":\"stopped\"}}\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"read_ui_state\",\"id\":\"7\",\"params\":{\"limit\":64}}') { exit 97 }\"\r\n",
        "echo {\"id\":\"7\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\",\"turnState\":{\"selectedThreadState\":\"idle\"}}}\r\n",
        "set /p line=\r\n",
        "powershell.exe -NoProfile -Command \"if ($env:line -cne '{\"command\":\"read_ui_state\",\"id\":\"8\",\"params\":{\"limit\":64}}') { exit 98 }\"\r\n",
        "echo {\"id\":\"8\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\"}}\r\n",
        "set \"line=\"\r\n",
        "set /p line=\r\n",
        "if not errorlevel 1 exit /b 99\r\n"
    );
    #[cfg(not(windows))]
    let script = concat!(
        "#!/bin/sh\n",
        "IFS= read -r gate\n",
        "printf '%s\\n' 'beryl_diagnostic_acceptance_ready_v1'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"handshake\",\"id\":\"1\",\"params\":{}}' ] || exit 91\n",
        "printf '%s\\n' '{\"id\":\"1\",\"ok\":true,\"result\":{\"protocol\":\"beryl_diagnostic_child\",\"protocolVersion\":1}}'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"start_turn\",\"id\":\"2\",\"params\":{\"text\":\"Continue from the root doc/plan.md.\"}}' ] || exit 92\n",
        "printf '%s\\n' '{\"id\":\"2\",\"ok\":true,\"result\":{\"threadId\":\"thread-1\",\"turnId\":\"turn-1\"}}'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"read_ui_state\",\"id\":\"3\",\"params\":{\"limit\":64}}' ] || exit 93\n",
        "printf '%s\\n' '{\"id\":\"3\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"wrong\",\"selectedThreadId\":\"thread-1\",\"turnState\":{\"selectedThreadState\":\"working\",\"cancellableActiveTurn\":{\"turnId\":\"turn-1\"}}}}'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"read_ui_state\",\"id\":\"4\",\"params\":{\"limit\":64}}' ] || exit 94\n",
        "printf '%s\\n' '{\"id\":\"4\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\",\"turnState\":{\"selectedThreadState\":\"working\",\"cancellableActiveTurn\":{\"turnId\":\"turn-1\"}}}}'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"read_ui_state\",\"id\":\"5\",\"params\":{\"limit\":64}}' ] || exit 95\n",
        "printf '%s\\n' '{\"id\":\"5\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\"}}'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"hard_stop_turn\",\"id\":\"6\",\"params\":{\"expectedThreadId\":\"thread-1\",\"expectedTurnId\":\"turn-1\"}}' ] || exit 96\n",
        "printf '%s\\n' '{\"id\":\"6\",\"ok\":true,\"result\":{\"status\":\"stopped\"}}'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"read_ui_state\",\"id\":\"7\",\"params\":{\"limit\":64}}' ] || exit 97\n",
        "printf '%s\\n' '{\"id\":\"7\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\",\"turnState\":{\"selectedThreadState\":\"idle\"}}}'\n",
        "IFS= read -r line\n",
        "[ \"$line\" = '{\"command\":\"read_ui_state\",\"id\":\"8\",\"params\":{\"limit\":64}}' ] || exit 98\n",
        "printf '%s\\n' '{\"id\":\"8\",\"ok\":true,\"result\":{\"selectedWorkspaceId\":\"workspace-1\",\"selectedThreadId\":\"thread-1\"}}'\n",
        "if IFS= read -r line; then exit 99; fi\n"
    );
    fs::write(&path, script).unwrap();
    make_executable(&path);
    path
}

fn fake_executable(root: &Path) -> std::path::PathBuf {
    let path = root.join(if cfg!(windows) {
        "frozen-beryl.cmd"
    } else {
        "frozen-beryl.sh"
    });
    #[cfg(windows)]
    let script = concat!(
        "@echo off\r\n",
        "set /p gate=\r\n",
        "powershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\n",
        "set /p line=\r\n",
        "echo {\"id\":\"1\",\"ok\":true,\"result\":{\"protocol\":\"beryl_diagnostic_child\",\"protocolVersion\":1}}\r\n",
        "set /p line=\r\n",
        "echo {\"id\":\"2\",\"ok\":true,\"result\":{\"pid\":42}}\r\n",
        "set /p line=\r\n",
        "echo {\"id\":\"3\",\"ok\":true,\"result\":{\"ready\":true}}\r\n",
        "set /p line=\r\n"
    );
    #[cfg(not(windows))]
    let script = concat!(
        "#!/bin/sh\n",
        "IFS= read -r gate\n",
        "printf '%s\\n' 'beryl_diagnostic_acceptance_ready_v1'\n",
        "IFS= read -r line\n",
        "printf '%s\\n' '{\"id\":\"1\",\"ok\":true,\"result\":{\"protocol\":\"beryl_diagnostic_child\",\"protocolVersion\":1}}'\n",
        "IFS= read -r line\n",
        "printf '%s\\n' '{\"id\":\"2\",\"ok\":true,\"result\":{\"pid\":42}}'\n",
        "IFS= read -r line\n",
        "printf '%s\\n' '{\"id\":\"3\",\"ok\":true,\"result\":{\"ready\":true}}'\n",
        "IFS= read -r line\n"
    );
    fs::write(&path, script).unwrap();
    make_executable(&path);
    path
}

#[cfg(windows)]
fn incompatible_descendant_executable(
    root: &Path,
    pid_path: &Path,
    release_path: &Path,
) -> std::path::PathBuf {
    let path = root.join("frozen-incompatible-beryl.cmd");
    let script = format!(
        "@echo off\r\nset /p gate=\r\npowershell.exe -NoProfile -Command \"[Console]::Out.Write('beryl_diagnostic_acceptance_ready_v1'+[char]10)\"\r\npowershell.exe -NoProfile -Command \"$p=Start-Process ping -ArgumentList '-n','60','127.0.0.1' -PassThru -WindowStyle Hidden; [IO.File]::WriteAllText('{}', ([string]$p.Id)+':ready')\"\r\npowershell.exe -NoProfile -Command \"while (-not (Test-Path -LiteralPath '{}')) {{ Start-Sleep -Milliseconds 10 }}\"\r\nset /p line=\r\necho {{\"id\":\"1\",\"ok\":true,\"result\":{{\"protocol\":\"beryl_diagnostic_child\",\"protocolVersion\":99}}}}\r\nping -n 60 127.0.0.1 >nul\r\n",
        pid_path.display(),
        release_path.display()
    );
    fs::write(&path, script).unwrap();
    path
}

#[cfg(windows)]
struct FixtureRelease {
    path: PathBuf,
    released: bool,
}

#[cfg(windows)]
impl FixtureRelease {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            released: false,
        }
    }

    fn release(&mut self) {
        fs::write(&self.path, b"release").unwrap();
        self.released = true;
    }
}

#[cfg(windows)]
impl Drop for FixtureRelease {
    fn drop(&mut self) {
        if !self.released {
            let _ = fs::write(&self.path, b"release");
        }
    }
}

#[cfg(windows)]
fn wait_for_descendant_pid(path: &Path, timeout: Duration) -> u32 {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(contents) = fs::read_to_string(path)
            && let Some(pid) = contents.strip_suffix(":ready")
            && let Ok(pid) = pid.parse()
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "descendant PID was not published within {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(windows)]
struct ExactWindowsProcess {
    handle: std::os::windows::io::OwnedHandle,
}

#[cfg(windows)]
impl ExactWindowsProcess {
    fn open_while_known_live(pid: u32) -> Self {
        use std::os::windows::io::FromRawHandle;

        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        let handle =
            unsafe { open_process(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        assert!(
            !handle.is_null(),
            "open exact known-live process {pid}: {}",
            std::io::Error::last_os_error()
        );
        Self {
            handle: unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle) },
        }
    }

    fn assert_still_active(&self) {
        use std::os::windows::io::AsRawHandle;

        const STILL_ACTIVE: u32 = 259;
        const WAIT_TIMEOUT: u32 = 258;
        assert_eq!(
            unsafe { wait_for_single_object(self.handle.as_raw_handle(), 0) },
            WAIT_TIMEOUT
        );
        assert_eq!(self.exit_code(), STILL_ACTIVE);
    }

    fn assert_exited_within(&self, timeout: Duration) {
        use std::os::windows::io::AsRawHandle;

        const STILL_ACTIVE: u32 = 259;
        const WAIT_OBJECT_0: u32 = 0;
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap();
        assert_eq!(
            unsafe { wait_for_single_object(self.handle.as_raw_handle(), timeout_ms) },
            WAIT_OBJECT_0,
            "exact process handle did not become signaled within {timeout:?}"
        );
        assert_ne!(self.exit_code(), STILL_ACTIVE);
    }

    fn exit_code(&self) -> u32 {
        use std::os::windows::io::AsRawHandle;

        let mut exit_code = 0;
        let succeeded =
            unsafe { get_exit_code_process(self.handle.as_raw_handle(), &mut exit_code) };
        assert_ne!(
            succeeded,
            0,
            "read exact process exit code: {}",
            std::io::Error::last_os_error()
        );
        exit_code
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "OpenProcess"]
    fn open_process(
        desired_access: u32,
        inherit_handle: i32,
        process_id: u32,
    ) -> *mut core::ffi::c_void;
    #[link_name = "WaitForSingleObject"]
    fn wait_for_single_object(handle: *mut core::ffi::c_void, milliseconds: u32) -> u32;
    #[link_name = "GetExitCodeProcess"]
    fn get_exit_code_process(handle: *mut core::ffi::c_void, exit_code: *mut u32) -> i32;
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
