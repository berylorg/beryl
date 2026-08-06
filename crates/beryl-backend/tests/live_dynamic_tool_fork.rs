use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use beryl_backend::{
    DynamicToolCallResponse, DynamicToolSpec, ManagedBackendLaunchOptions, ManagedBackendServer,
    ManagedBackendSession, ThreadInfo, ThreadItem, ThreadListOptions, ThreadReadOptions,
    ThreadReadResponse, ThreadSessionMetadata, ThreadSessionResponse, ThreadStartOptions,
    ThreadStatus, ThreadSummary, TurnInfo, TurnStatus, TurnStreamEvent, canonicalize_host_path,
};
use beryl_model::workspace::RuntimeMode;
use serde_json::json;
use tempfile::TempDir;
use tokio::{io::AsyncReadExt, process::Command as TokioCommand, time};

const LIVE_PROBE_ENV: &str = "BERYL_RUN_LIVE_DYNAMIC_TOOL_FORK_PROBE";
const LIVE_CODEX_EXECUTABLE_ENV: &str = "BERYL_LIVE_CODEX_EXECUTABLE";
const EXPECTED_CODEX_VERSION: &str = "codex-cli 0.146.0";
const EXPECTED_INITIALIZE_USER_AGENT_PREFIX: &str = "beryl/0.146.0 ";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const TURN_TIMEOUT: Duration = Duration::from_secs(90);
const STREAM_POLL_TIMEOUT: Duration = Duration::from_millis(250);
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const VERSION_CLEANUP_TIMEOUT: Duration = Duration::from_secs(15);
const VERSION_STDOUT_LIMIT: usize = 1024;
const VERSION_READ_CHUNK_SIZE: usize = 256;
const CLEANUP_DISCOVERY_PAGE_SIZE: u32 = 100;
const CLEANUP_DISCOVERY_MAX_PAGES: usize = 4;
const PROBE_TOOL_NAME: &str = "beryl_lifecycle_probe";
const PROBE_TOOL_NAMESPACE: &str = "beryl";

/// Requires an authenticated local Codex account and intentionally remains opt-in.
///
/// Run only with `BERYL_RUN_LIVE_DYNAMIC_TOOL_FORK_PROBE=1` and `--run-ignored`.
#[test]
#[ignore = "launches the local codex app-server and performs live model work"]
fn forked_child_retains_dynamic_tool_after_rollback_and_resume_live() {
    if std::env::var(LIVE_PROBE_ENV).as_deref() != Ok("1") {
        return;
    }

    let executable = required_live_codex_executable();
    require_expected_codex_version(&executable);
    let workspace =
        TempDir::new().unwrap_or_else(|_| panic!("create the disposable live-probe workspace"));
    let canonical_workspace = canonicalize_host_path(workspace.path())
        .unwrap_or_else(|_| panic!("canonicalize the disposable live-probe workspace"));
    let launch_options = ManagedBackendLaunchOptions::with_exact_host_windows_program(&executable)
        .unwrap_or_else(|_| panic!("accept the operator-supplied exact Codex executable"));
    let server = ManagedBackendServer::launch_with_options(
        RuntimeMode::HostWindows,
        workspace.path(),
        launch_options,
    )
    .unwrap_or_else(|_| panic!("launch the managed local codex app-server"));
    let (server, session, report) = connect_and_probe_guarded(server);
    let mut probe = LiveProbe::new(server, session, canonical_workspace.clone());

    let probe_result = catch_unwind(AssertUnwindSafe(|| {
        assert!(
            report
                .initialize()
                .user_agent
                .starts_with(EXPECTED_INITIALIZE_USER_AGENT_PREFIX),
            "the initialized app-server user agent must combine Beryl's project name with the expected CAS version"
        );
        run_live_probe(&mut probe, workspace.path(), &canonical_workspace);
    }));
    let cleanup_result = probe.cleanup();

    match probe_result {
        Ok(()) => {
            cleanup_result.unwrap_or_else(|error| panic!("live-probe cleanup failed: {error}"))
        }
        Err(payload) => {
            if let Err(error) = cleanup_result {
                eprintln!("live-probe cleanup also failed while unwinding: {error}");
            }
            resume_unwind(payload);
        }
    }
}

fn connect_and_probe_guarded(
    mut server: ManagedBackendServer,
) -> (
    ManagedBackendServer,
    ManagedBackendSession,
    beryl_backend::ManagedBackendProbeReport,
) {
    let connect_result = catch_unwind(AssertUnwindSafe(|| {
        server.connect_and_probe(REQUEST_TIMEOUT)
    }));

    match connect_result {
        Ok(Ok((session, report))) => (server, session, report),
        Ok(Err(probe_error)) => {
            let cleanup_result = server.shutdown();
            match cleanup_result {
                Ok(()) => panic!(
                    "connect, initialize, and probe the managed local codex app-server: {probe_error}"
                ),
                Err(cleanup_error) => panic!(
                    "connect, initialize, and probe the managed local codex app-server: {probe_error}; managed process/auth cleanup also failed: {cleanup_error}"
                ),
            }
        }
        Err(payload) => {
            if let Err(error) = server.shutdown() {
                eprintln!(
                    "managed process/auth cleanup failed while unwinding guarded app-server probing: {error}"
                );
            }
            resume_unwind(payload);
        }
    }
}

fn run_live_probe(probe: &mut LiveProbe, workspace: &Path, canonical_workspace: &Path) {
    let root = probe.start_persistent_root(workspace);
    let root_id = root.thread.summary().id;
    let root_model_provider = required_summary_model_provider(
        &root.thread.summary(),
        "the persistent root start response",
    );
    let root_start_runtime = runtime_metadata_from_session(&root);
    probe.track_thread(&root_id, None);
    assert!(
        !root.thread.summary().ephemeral,
        "the root must be persistent"
    );
    assert_eq!(
        root.thread.summary().cwd,
        canonical_workspace,
        "the persistent root must report the canonical disposable workspace"
    );

    let root_turn = probe.start_required_tool_turn(&root_id, "root");
    probe.answer_exactly_one_required_tool_call(&root_id, &root_turn);
    probe.wait_for_completed_turn_without_another_tool_call(&root_id, &root_turn);

    let root_snapshot = probe.read_full_history(&root_id);
    assert_root_snapshot_identity(
        &root_snapshot.thread,
        &root_id,
        canonical_workspace,
        "the initial full root snapshot",
    );
    assert_required_summary_model_provider_matches(
        &root_model_provider,
        &root_snapshot.thread.summary(),
        "the initial full root snapshot",
    );
    let root_read_runtime = root_snapshot.read_metadata().session_metadata;
    let root_runtime = root_runtime_metadata(&root_start_runtime, &root_read_runtime);

    let fork = probe
        .session
        .fork_thread(&root_id, REQUEST_TIMEOUT)
        .unwrap_or_else(|_| panic!("fork the completed persistent root"));
    let child_id = fork.thread.summary().id;
    probe.track_thread(&child_id, Some(&root_id));
    assert_prepared_child_identity(
        &fork.thread,
        &child_id,
        &root_id,
        canonical_workspace,
        "the populated fork response",
    );
    assert_required_summary_model_provider_matches(
        &root_model_provider,
        &fork.thread.summary(),
        "the populated fork response",
    );
    assert_runtime_matches_root(
        &root_runtime,
        &runtime_metadata_from_fork(&fork),
        "the populated fork response",
    );
    assert!(
        !fork.thread.turns.is_empty(),
        "the fork response must populate inherited turns"
    );

    let inherited_user_turns = inherited_user_turn_count(&fork.thread.turns);
    assert!(
        inherited_user_turns > 0,
        "the populated fork response must include an inherited user turn"
    );
    let rollback = probe
        .session
        .rollback_thread(&child_id, inherited_user_turns, REQUEST_TIMEOUT)
        .unwrap_or_else(|_| panic!("roll back every inherited user turn from the child"));
    assert_prepared_child_identity(
        &rollback.thread,
        &child_id,
        &root_id,
        canonical_workspace,
        "the rollback response",
    );
    assert_required_summary_model_provider_matches(
        &root_model_provider,
        &rollback.thread.summary(),
        "the rollback response",
    );
    assert!(
        rollback.thread.turns.is_empty(),
        "rollback must return an empty effective child history"
    );

    let prepared_child = probe.read_full_history(&child_id);
    assert_prepared_child_identity(
        &prepared_child.thread,
        &child_id,
        &root_id,
        canonical_workspace,
        "the prepared child full-history snapshot",
    );
    assert_required_summary_model_provider_matches(
        &root_model_provider,
        &prepared_child.thread.summary(),
        "the prepared child full-history snapshot",
    );
    assert_runtime_does_not_conflict_when_exposed(
        &root_runtime,
        &prepared_child.read_metadata().session_metadata,
        "the prepared child full-history snapshot",
    );
    assert!(
        prepared_child.thread.turns.is_empty(),
        "the prepared child must have no inherited effective history"
    );

    let root_after_child_rollback = probe.read_full_history(&root_id);
    assert_eq!(
        root_after_child_rollback, root_snapshot,
        "fork and rollback must leave the exact full root snapshot unchanged"
    );

    probe.mark_may_be_archived(&child_id);
    probe
        .session
        .archive_thread(&child_id, REQUEST_TIMEOUT)
        .unwrap_or_else(|_| panic!("archive the prepared child"));
    probe.wait_for_exact_archive_notification(&child_id);
    let unarchived_child = probe
        .session
        .unarchive_thread(&child_id, REQUEST_TIMEOUT)
        .unwrap_or_else(|_| panic!("unarchive the prepared child"));
    assert_prepared_child_identity(
        &unarchived_child,
        &child_id,
        &root_id,
        canonical_workspace,
        "the unarchived child",
    );
    assert_required_summary_model_provider_matches(
        &root_model_provider,
        &unarchived_child.summary(),
        "the unarchived child",
    );
    assert_eq!(
        unarchived_child.status,
        ThreadStatus::NotLoaded,
        "unarchive must return the prepared child as not loaded"
    );
    let resumed_child = probe
        .session
        .resume_thread_metadata(&child_id, REQUEST_TIMEOUT)
        .unwrap_or_else(|_| panic!("resume the prepared child through Beryl's activation path"));
    assert_prepared_child_identity(
        &resumed_child.thread,
        &child_id,
        &root_id,
        canonical_workspace,
        "the resumed child",
    );
    assert_required_summary_model_provider_matches(
        &root_model_provider,
        &resumed_child.thread.summary(),
        "the resumed child",
    );
    assert_runtime_matches_root(
        &root_runtime,
        &runtime_metadata_from_session(&resumed_child),
        "the resumed child",
    );
    assert!(
        matches!(resumed_child.thread.status, ThreadStatus::Idle),
        "the resumed child must be idle before its continuation turn"
    );

    let resumed_child_history = probe.read_full_history(&child_id);
    assert_prepared_child_identity(
        &resumed_child_history.thread,
        &child_id,
        &root_id,
        canonical_workspace,
        "the resumed child full-history snapshot",
    );
    assert_required_summary_model_provider_matches(
        &root_model_provider,
        &resumed_child_history.thread.summary(),
        "the resumed child full-history snapshot",
    );
    assert_runtime_does_not_conflict_when_exposed(
        &root_runtime,
        &resumed_child_history.read_metadata().session_metadata,
        "the resumed child full-history snapshot",
    );
    assert!(
        resumed_child_history.thread.turns.is_empty(),
        "the resumed child must remain empty before its continuation turn"
    );

    let child_turn = probe.start_required_tool_turn(&child_id, "child");
    probe.answer_exactly_one_required_tool_call(&child_id, &child_turn);
    probe.wait_for_completed_turn_without_another_tool_call(&child_id, &child_turn);
}

fn required_live_codex_executable() -> PathBuf {
    let executable = std::env::var_os(LIVE_CODEX_EXECUTABLE_ENV).unwrap_or_else(|| {
        panic!(
            "set {LIVE_CODEX_EXECUTABLE_ENV} to the absolute exact executable for {EXPECTED_CODEX_VERSION}"
        )
    });
    let executable = PathBuf::from(executable);
    assert!(
        executable.is_absolute(),
        "{LIVE_CODEX_EXECUTABLE_ENV} must be an absolute exact executable path"
    );
    executable
}

fn require_expected_codex_version(executable: &Path) {
    let mut command = TokioCommand::new(executable);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = run_bounded_version_command(command, VersionProcessBounds::live())
        .unwrap_or_else(|error| panic!("validate the exact executable --version process: {error}"));
    assert_expected_codex_version_output(&output);
}

#[derive(Clone, Copy)]
struct VersionProcessBounds {
    process_timeout: Duration,
    cleanup_timeout: Duration,
    stdout_limit: usize,
}

impl VersionProcessBounds {
    const fn live() -> Self {
        Self {
            process_timeout: VERSION_TIMEOUT,
            cleanup_timeout: VERSION_CLEANUP_TIMEOUT,
            stdout_limit: VERSION_STDOUT_LIMIT,
        }
    }
}

fn run_bounded_version_command(
    command: TokioCommand,
    bounds: VersionProcessBounds,
) -> Result<Vec<u8>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| format!("create the bounded --version runtime: {error}"))?;
    runtime.block_on(collect_bounded_version_output(command, bounds))
}

async fn collect_bounded_version_output(
    mut command: TokioCommand,
    bounds: VersionProcessBounds,
) -> Result<Vec<u8>, String> {
    command.kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn the exact executable --version process: {error}"))?;
    let process_label = version_process_label(child.id());
    let Some(mut stdout) = child.stdout.take() else {
        let cleanup = reap_bounded_version_process(&mut child, &process_label, bounds).await;
        return Err(combine_version_failure(
            format!("{process_label} did not provide captured stdout"),
            cleanup,
        ));
    };

    let collection =
        collect_bounded_version_stdout(&mut child, &mut stdout, &process_label, bounds).await;
    match collection {
        Ok((status, output)) => {
            if status.success() {
                Ok(output)
            } else {
                Err(format!("{process_label} exited unsuccessfully: {status}"))
            }
        }
        Err(error) => {
            let cleanup = reap_bounded_version_process(&mut child, &process_label, bounds).await;
            Err(combine_version_failure(error, cleanup))
        }
    }
}

async fn collect_bounded_version_stdout(
    child: &mut tokio::process::Child,
    stdout: &mut tokio::process::ChildStdout,
    process_label: &str,
    bounds: VersionProcessBounds,
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    let deadline = time::Instant::now() + bounds.process_timeout;
    let mut status = None;
    let mut output = Vec::with_capacity(bounds.stdout_limit.saturating_add(1));
    let mut buffer = [0_u8; VERSION_READ_CHUNK_SIZE];

    loop {
        if status.is_none() {
            status = child.try_wait().map_err(|error| {
                format!("query {process_label} status while collecting stdout: {error}")
            })?;
        }

        let remaining = deadline
            .checked_duration_since(time::Instant::now())
            .ok_or_else(|| {
                format!(
                    "{process_label} exceeded its {:?} stdout-collection deadline",
                    bounds.process_timeout
                )
            })?;
        let maximum_read = bounds
            .stdout_limit
            .saturating_add(1)
            .saturating_sub(output.len())
            .min(buffer.len());
        if maximum_read == 0 {
            return Err(format!(
                "{process_label} stdout exceeded the {}-byte probe limit",
                bounds.stdout_limit
            ));
        }

        let bytes_read = time::timeout(remaining, stdout.read(&mut buffer[..maximum_read]))
            .await
            .map_err(|_| {
                format!(
                    "{process_label} stdout did not reach EOF within its {:?} collection deadline",
                    bounds.process_timeout
                )
            })?
            .map_err(|error| format!("read bounded stdout from {process_label}: {error}"))?;
        if bytes_read == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..bytes_read]);
        if output.len() > bounds.stdout_limit {
            return Err(format!(
                "{process_label} stdout exceeded the {}-byte probe limit",
                bounds.stdout_limit
            ));
        }
    }

    let status =
        match status {
            Some(status) => status,
            None => {
                let remaining = deadline
                .checked_duration_since(time::Instant::now())
                .ok_or_else(|| {
                    format!(
                        "{process_label} exceeded its {:?} process deadline after closing stdout",
                        bounds.process_timeout
                    )
                })?;
                time::timeout(remaining, child.wait())
                .await
                .map_err(|_| {
                    format!(
                        "{process_label} exceeded its {:?} process deadline after closing stdout",
                        bounds.process_timeout
                    )
                })?
                .map_err(|error| format!("wait for {process_label} after closing stdout: {error}"))?
            }
        };
    Ok((status, output))
}

async fn reap_bounded_version_process(
    child: &mut tokio::process::Child,
    process_label: &str,
    bounds: VersionProcessBounds,
) -> Result<(), String> {
    match child
        .try_wait()
        .map_err(|error| format!("query {process_label} before cleanup: {error}"))?
    {
        Some(_) => return Ok(()),
        None => {}
    }

    child
        .start_kill()
        .map_err(|error| format!("terminate timed-out {process_label}: {error}"))?;
    time::timeout(bounds.cleanup_timeout, child.wait())
        .await
        .map_err(|_| {
            format!(
                "{process_label} remained unreaped after its {:?} cleanup deadline",
                bounds.cleanup_timeout
            )
        })?
        .map_err(|error| format!("reap terminated {process_label}: {error}"))?;
    Ok(())
}

fn combine_version_failure(primary: String, cleanup: Result<(), String>) -> String {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) => format!("{primary}; direct-process residue is unverified: {cleanup}"),
    }
}

fn version_process_label(process_id: Option<u32>) -> String {
    match process_id {
        Some(process_id) => format!("exact executable --version process pid {process_id}"),
        None => "exact executable --version process with unavailable pid".to_string(),
    }
}

fn assert_expected_codex_version_output(output: &[u8]) {
    let output = String::from_utf8(output.to_vec())
        .unwrap_or_else(|_| panic!("decode the exact executable --version stdout as UTF-8"));
    assert!(
        matches!(
            output.as_str(),
            EXPECTED_CODEX_VERSION | "codex-cli 0.146.0\n" | "codex-cli 0.146.0\r\n"
        ),
        "the exact executable --version stdout must be exactly {EXPECTED_CODEX_VERSION:?}"
    );
}

fn assert_root_snapshot_identity(
    thread: &ThreadInfo,
    expected_root_id: &str,
    canonical_workspace: &Path,
    context: &str,
) {
    let summary = thread.summary();
    assert_eq!(
        summary.id, expected_root_id,
        "{context} must retain the exact root id"
    );
    assert!(!summary.ephemeral, "{context} root must remain persistent");
    assert_eq!(
        summary.forked_from_id, None,
        "{context} root must not become a fork"
    );
    assert_eq!(
        summary.cwd, canonical_workspace,
        "{context} must retain the root canonical working directory"
    );
}

fn assert_prepared_child_identity(
    thread: &ThreadInfo,
    expected_child_id: &str,
    expected_root_id: &str,
    canonical_workspace: &Path,
    context: &str,
) {
    let summary = thread.summary();
    assert_eq!(
        summary.id, expected_child_id,
        "{context} must retain the child id"
    );
    assert_ne!(
        summary.id, expected_root_id,
        "{context} child id must differ from the root"
    );
    assert!(!summary.ephemeral, "{context} child must remain persistent");
    assert_eq!(
        summary.forked_from_id.as_deref(),
        Some(expected_root_id),
        "{context} must retain direct root backend lineage"
    );
    assert_eq!(
        summary.cwd, canonical_workspace,
        "{context} must retain the root canonical working directory"
    );
}

fn required_summary_model_provider(summary: &ThreadSummary, context: &str) -> String {
    assert!(
        !summary.model_provider.trim().is_empty(),
        "{context} must expose a non-empty required summary-level model provider"
    );
    summary.model_provider.clone()
}

fn assert_required_summary_model_provider_matches(
    expected: &str,
    actual: &ThreadSummary,
    context: &str,
) {
    assert_eq!(
        required_summary_model_provider(actual, context),
        expected,
        "{context} must retain the exact required summary-level model provider"
    );
}

fn runtime_metadata_from_session(session: &ThreadSessionResponse) -> ThreadSessionMetadata {
    ThreadSessionMetadata {
        model: session.model.clone(),
        model_provider: session.model_provider.clone(),
        reasoning_effort: session.reasoning_effort.clone(),
    }
}

fn runtime_metadata_from_fork(fork: &beryl_backend::ThreadForkResponse) -> ThreadSessionMetadata {
    ThreadSessionMetadata {
        model: fork.model.clone(),
        model_provider: fork.model_provider.clone(),
        reasoning_effort: fork.reasoning_effort.clone(),
    }
}

fn root_runtime_metadata(
    started: &ThreadSessionMetadata,
    read: &ThreadSessionMetadata,
) -> ThreadSessionMetadata {
    ThreadSessionMetadata {
        model: merge_root_runtime_value("model", &started.model, &read.model),
        model_provider: merge_root_runtime_value(
            "model provider",
            &started.model_provider,
            &read.model_provider,
        ),
        reasoning_effort: merge_root_runtime_value(
            "reasoning effort",
            &started.reasoning_effort,
            &read.reasoning_effort,
        ),
    }
}

fn merge_root_runtime_value(
    field: &str,
    started: &Option<String>,
    read: &Option<String>,
) -> Option<String> {
    if let (Some(started), Some(read)) = (started, read) {
        assert_eq!(
            started, read,
            "root start and full-read runtime {field} values must not conflict"
        );
    }
    started.clone().or_else(|| read.clone())
}

fn assert_runtime_matches_root(
    expected: &ThreadSessionMetadata,
    actual: &ThreadSessionMetadata,
    context: &str,
) {
    assert_eq!(
        actual.model, expected.model,
        "{context} must exactly retain the effective root runtime model"
    );
    assert_eq!(
        actual.model_provider, expected.model_provider,
        "{context} must exactly retain the effective root runtime model provider"
    );
    assert_eq!(
        actual.reasoning_effort, expected.reasoning_effort,
        "{context} must exactly retain the effective root runtime reasoning effort"
    );
}

fn assert_runtime_does_not_conflict_when_exposed(
    expected: &ThreadSessionMetadata,
    actual: &ThreadSessionMetadata,
    context: &str,
) {
    if let Some(actual_model) = actual.model.as_deref() {
        assert_eq!(
            expected.model.as_deref(),
            Some(actual_model),
            "{context} must not report a runtime model that conflicts with the root"
        );
    }
    if let Some(actual_provider) = actual.model_provider.as_deref() {
        assert_eq!(
            expected.model_provider.as_deref(),
            Some(actual_provider),
            "{context} must not report a runtime model provider that conflicts with the root"
        );
    }
    if let Some(actual_reasoning) = actual.reasoning_effort.as_deref() {
        assert_eq!(
            expected.reasoning_effort.as_deref(),
            Some(actual_reasoning),
            "{context} must not report runtime reasoning that conflicts with the root"
        );
    }
}

#[test]
fn root_runtime_metadata_rejects_conflicting_start_and_full_read_values() {
    let started = ThreadSessionMetadata {
        model: Some("gpt-5.5".to_string()),
        ..ThreadSessionMetadata::default()
    };
    let read = ThreadSessionMetadata {
        model: Some("gpt-5.6".to_string()),
        ..ThreadSessionMetadata::default()
    };

    assert!(catch_unwind(|| root_runtime_metadata(&started, &read)).is_err());
}

#[test]
fn root_snapshot_identity_rejects_non_root_or_mismatched_metadata() {
    let canonical_workspace = PathBuf::from(r"C:\beryl-live-probe");
    let root_thread: ThreadInfo = serde_json::from_value(json!({
        "id": "root-id",
        "cwd": canonical_workspace,
        "preview": "",
        "createdAt": 1,
        "updatedAt": 2,
        "modelProvider": "openai",
        "ephemeral": false,
        "status": { "type": "idle" },
        "turns": []
    }))
    .unwrap();

    assert_root_snapshot_identity(
        &root_thread,
        "root-id",
        &canonical_workspace,
        "the deterministic root snapshot",
    );

    let forked_root: ThreadInfo = serde_json::from_value(json!({
        "id": "root-id",
        "forkedFromId": "unexpected-parent",
        "cwd": canonical_workspace,
        "preview": "",
        "createdAt": 1,
        "updatedAt": 2,
        "modelProvider": "openai",
        "ephemeral": false,
        "status": { "type": "idle" },
        "turns": []
    }))
    .unwrap();
    assert!(
        catch_unwind(|| {
            assert_root_snapshot_identity(
                &forked_root,
                "root-id",
                &canonical_workspace,
                "the deterministic root snapshot",
            )
        })
        .is_err()
    );
}

#[test]
fn fork_and_resume_runtime_must_match_root_optional_values_exactly() {
    let root = ThreadSessionMetadata::default();
    let child = ThreadSessionMetadata {
        reasoning_effort: Some("xhigh".to_string()),
        ..ThreadSessionMetadata::default()
    };

    assert!(
        catch_unwind(|| {
            assert_runtime_matches_root(&root, &child, "the deterministic runtime helper")
        })
        .is_err()
    );
}

#[test]
fn full_read_runtime_rejects_values_that_conflict_with_the_effective_root() {
    let root = ThreadSessionMetadata {
        model: Some("gpt-5.6".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("high".to_string()),
    };
    let conflicting_read = ThreadSessionMetadata {
        reasoning_effort: Some("xhigh".to_string()),
        ..root.clone()
    };

    assert!(
        catch_unwind(|| {
            assert_runtime_does_not_conflict_when_exposed(
                &root,
                &conflicting_read,
                "the deterministic full-read runtime helper",
            )
        })
        .is_err()
    );
}

#[test]
fn full_read_runtime_omissions_do_not_conflict_with_the_effective_root() {
    let root = ThreadSessionMetadata {
        model: Some("gpt-5.6".to_string()),
        model_provider: Some("openai".to_string()),
        reasoning_effort: Some("high".to_string()),
    };

    assert_runtime_does_not_conflict_when_exposed(
        &root,
        &ThreadSessionMetadata::default(),
        "the deterministic runtime helper",
    );
}

#[test]
fn exact_codex_version_output_accepts_only_the_pinned_banner() {
    assert_expected_codex_version_output(EXPECTED_CODEX_VERSION.as_bytes());
    assert_expected_codex_version_output(format!("{EXPECTED_CODEX_VERSION}\n").as_bytes());
    assert_expected_codex_version_output(format!("{EXPECTED_CODEX_VERSION}\r\n").as_bytes());

    assert!(catch_unwind(|| assert_expected_codex_version_output(b"codex-cli 0.146.1\n")).is_err());
}

#[test]
fn bounded_version_command_rejects_stdout_beyond_its_capture_limit() {
    let executable = std::env::current_exe()
        .unwrap_or_else(|_| panic!("locate the deterministic bounded-version test executable"));
    let mut command = TokioCommand::new(executable);
    command
        .arg("--list")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let error = run_bounded_version_command(
        command,
        VersionProcessBounds {
            process_timeout: REQUEST_TIMEOUT,
            cleanup_timeout: REQUEST_TIMEOUT,
            stdout_limit: 0,
        },
    )
    .unwrap_err();
    assert!(error.contains("stdout exceeded the 0-byte probe limit"));
}

const BOUNDED_VERSION_TEST_SLEEP_ENV: &str = "BERYL_BOUNDED_VERSION_TEST_SLEEP";

#[test]
fn bounded_version_command_reaps_a_timed_out_direct_process() {
    let executable = std::env::current_exe()
        .unwrap_or_else(|_| panic!("locate the deterministic bounded-version test executable"));
    let mut command = TokioCommand::new(executable);
    command
        .args([
            "--exact",
            "bounded_version_test_child_sleeps_when_requested",
            "--nocapture",
        ])
        .env(BOUNDED_VERSION_TEST_SLEEP_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let error = run_bounded_version_command(
        command,
        VersionProcessBounds {
            process_timeout: Duration::from_millis(50),
            cleanup_timeout: REQUEST_TIMEOUT,
            stdout_limit: VERSION_STDOUT_LIMIT,
        },
    )
    .unwrap_err();
    assert!(error.contains("stdout did not reach EOF"));
    assert!(
        !error.contains("direct-process residue is unverified"),
        "the timed-out deterministic child must be explicitly reaped: {error}"
    );
}

#[test]
fn bounded_version_test_child_sleeps_when_requested() {
    if std::env::var_os(BOUNDED_VERSION_TEST_SLEEP_ENV).is_some() {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn deterministic_thread_with_model_provider(model_provider: &str) -> ThreadInfo {
    serde_json::from_value(json!({
        "id": "thread-id",
        "cwd": r"C:\\beryl-live-probe",
        "preview": "",
        "createdAt": 1,
        "updatedAt": 2,
        "modelProvider": model_provider,
        "ephemeral": false,
        "status": { "type": "idle" },
        "turns": []
    }))
    .unwrap()
}

#[test]
fn required_summary_model_provider_rejects_conflicting_identity() {
    let root = deterministic_thread_with_model_provider("openai");
    let child = deterministic_thread_with_model_provider("other-provider");
    let root_provider = required_summary_model_provider(&root.summary(), "the deterministic root");

    assert!(
        catch_unwind(|| {
            assert_required_summary_model_provider_matches(
                &root_provider,
                &child.summary(),
                "the deterministic child",
            )
        })
        .is_err()
    );
}

#[test]
fn required_summary_model_provider_accepts_consistent_identity() {
    let root = deterministic_thread_with_model_provider("openai");
    let child = deterministic_thread_with_model_provider("openai");
    let root_provider = required_summary_model_provider(&root.summary(), "the deterministic root");

    assert_required_summary_model_provider_matches(
        &root_provider,
        &child.summary(),
        "the deterministic child",
    );
}

fn inherited_user_turn_count(turns: &[TurnInfo]) -> u32 {
    turns
        .iter()
        .filter(|turn| {
            turn.items
                .iter()
                .any(|item| matches!(item, ThreadItem::UserMessage(_)))
        })
        .count()
        .try_into()
        .unwrap_or_else(|_| panic!("the inherited user-turn count must fit the rollback protocol"))
}

#[derive(Clone, Debug)]
struct KnownProbeThread {
    id: String,
    parent_id: Option<String>,
    may_be_archived: bool,
}

#[derive(Clone, Debug)]
struct CleanupCandidate {
    parent_id: Option<String>,
    discovered_active: bool,
    may_be_archived: bool,
    lineage_established: bool,
}

#[derive(Clone, Debug)]
struct DiscoveredProbeThread {
    id: String,
    forked_from_id: Option<String>,
    lineage_established: bool,
}

struct LiveProbe {
    server: ManagedBackendServer,
    session: ManagedBackendSession,
    canonical_workspace: PathBuf,
    known_threads: Vec<KnownProbeThread>,
    cleanup_attempted: bool,
}

impl LiveProbe {
    fn new(
        server: ManagedBackendServer,
        session: ManagedBackendSession,
        canonical_workspace: PathBuf,
    ) -> Self {
        Self {
            server,
            session,
            canonical_workspace,
            known_threads: Vec::new(),
            cleanup_attempted: false,
        }
    }

    fn track_thread(&mut self, thread_id: &str, parent_id: Option<&str>) {
        if let Some(known) = self
            .known_threads
            .iter_mut()
            .find(|known| known.id == thread_id)
        {
            if known.parent_id.is_none() {
                known.parent_id = parent_id.map(str::to_string);
            }
            return;
        }
        self.known_threads.push(KnownProbeThread {
            id: thread_id.to_string(),
            parent_id: parent_id.map(str::to_string),
            may_be_archived: false,
        });
    }

    fn mark_may_be_archived(&mut self, thread_id: &str) {
        let known = self
            .known_threads
            .iter_mut()
            .find(|known| known.id == thread_id)
            .unwrap_or_else(|| panic!("track the child before attempting to archive it"));
        known.may_be_archived = true;
    }

    fn start_persistent_root(&mut self, workspace: &Path) -> beryl_backend::ThreadSessionResponse {
        let options = ThreadStartOptions::persistent().with_dynamic_tool(probe_tool_spec());
        self.session
            .start_thread_with_options(workspace, options, REQUEST_TIMEOUT)
            .unwrap_or_else(|_| panic!("start the disposable persistent root with the probe tool"))
    }

    fn start_required_tool_turn(&mut self, thread_id: &str, phase: &str) -> String {
        let prompt = format!(
            "This is the {phase} lifecycle contract probe. Call the Beryl-owned dynamic tool \"{PROBE_TOOL_NAME}\" exactly once now with an empty object. Tool use is the sole required action: do not use any other tool and do not produce a normal answer before the tool response."
        );
        self.session
            .start_turn(thread_id, &prompt, REQUEST_TIMEOUT)
            .unwrap_or_else(|_| panic!("start the required dynamic-tool probe turn"))
            .turn
            .id
    }

    fn answer_exactly_one_required_tool_call(
        &mut self,
        expected_thread_id: &str,
        expected_turn_id: &str,
    ) {
        let deadline = Instant::now() + TURN_TIMEOUT;
        loop {
            let event = self.next_event_until(deadline, "wait for the required dynamic-tool call");
            match event {
                TurnStreamEvent::DynamicToolCallRequested(request) => {
                    assert_eq!(request.thread_id(), expected_thread_id);
                    assert_eq!(request.turn_id(), expected_turn_id);
                    assert_eq!(request.namespace(), Some(PROBE_TOOL_NAMESPACE));
                    assert_eq!(request.tool(), PROBE_TOOL_NAME);
                    assert_eq!(request.arguments(), &json!({}));
                    self.session
                        .respond_dynamic_tool_call(
                            &request,
                            &DynamicToolCallResponse::success_text("probe acknowledged"),
                        )
                        .unwrap_or_else(|_| {
                            panic!("respond successfully to the required dynamic-tool call")
                        });
                    return;
                }
                TurnStreamEvent::TurnCompleted { thread_id, turn }
                    if thread_id == expected_thread_id && turn.id == expected_turn_id =>
                {
                    panic!(
                        "the probe turn completed before issuing its required dynamic-tool call"
                    );
                }
                _ => {}
            };
        }
    }

    fn wait_for_completed_turn_without_another_tool_call(
        &mut self,
        expected_thread_id: &str,
        expected_turn_id: &str,
    ) {
        let deadline = Instant::now() + TURN_TIMEOUT;
        loop {
            let event = self.next_event_until(deadline, "wait for the probe turn to complete");
            match event {
                TurnStreamEvent::DynamicToolCallRequested(request) => {
                    panic!(
                        "the probe turn issued a second dynamic-tool request before completion: {}",
                        request.summary()
                    );
                }
                TurnStreamEvent::TurnCompleted { thread_id, turn }
                    if thread_id == expected_thread_id && turn.id == expected_turn_id =>
                {
                    assert_eq!(
                        turn.status,
                        TurnStatus::Completed,
                        "the probe turn must complete successfully"
                    );
                    return;
                }
                _ => {}
            }
        }
    }

    fn read_full_history(&mut self, thread_id: &str) -> ThreadReadResponse {
        self.session
            .read_thread(
                thread_id,
                ThreadReadOptions::include_turns(),
                REQUEST_TIMEOUT,
            )
            .unwrap_or_else(|_| panic!("read the bounded live-probe thread history"))
    }

    fn wait_for_exact_archive_notification(&mut self, expected_thread_id: &str) {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let event =
                self.next_event_until(deadline, "wait for the exact child archive notification");
            if let TurnStreamEvent::ThreadArchived { thread_id } = event {
                assert_eq!(
                    thread_id, expected_thread_id,
                    "the archive notification must identify the prepared child exactly"
                );
                return;
            }
        }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        if self.cleanup_attempted {
            return Ok(());
        }
        self.cleanup_attempted = true;

        let mut failures = Vec::new();
        let known_ids = self.known_thread_ids();
        let mut candidates = self.known_cleanup_candidates();
        match self.discover_disposable_active_threads_with_lineage() {
            Ok(threads) => {
                for thread in threads {
                    if let Err(error) = insert_discovered_candidate(
                        &mut candidates,
                        thread.id,
                        thread.forked_from_id,
                        thread.lineage_established,
                    ) {
                        failures.push(format!(
                            "initial cleanup discovery is contradictory ({error}); known thread ids: {known_ids:?}; additional disposable-workspace residue is unverified"
                        ));
                    }
                }
            }
            Err(error) => failures.push(format!(
                "initial cleanup discovery failed ({error}); known thread ids: {known_ids:?}; additional disposable-workspace residue is unverified"
            )),
        }

        let mut delete_failures = BTreeMap::new();
        match cleanup_deletion_order(&candidates) {
            Ok(thread_ids) => {
                for thread_id in thread_ids {
                    if let Err(error) = self.session.delete_thread(&thread_id, REQUEST_TIMEOUT) {
                        delete_failures.insert(thread_id, error.to_string());
                    }
                }
            }
            Err(error) => failures.push(format!(
                "cannot establish descendant-first cleanup order ({error}); known thread ids: {known_ids:?}; additional disposable-workspace residue is unverified"
            )),
        }

        match self.list_disposable_active_threads() {
            Ok(final_threads) => {
                let final_ids = final_threads
                    .iter()
                    .map(|thread| thread.id.clone())
                    .collect::<BTreeSet<_>>();
                if !final_ids.is_empty() {
                    failures.push(format!(
                        "final cleanup discovery found persistent disposable-workspace residue: {:?}",
                        final_ids
                    ));
                }
                for (thread_id, error) in &delete_failures {
                    let Some(candidate) = candidates.get(thread_id.as_str()) else {
                        failures.push(format!(
                            "delete persistent thread {thread_id} failed ({error}) and its cleanup identity is unverified"
                        ));
                        continue;
                    };
                    let final_absence_verifies_delete = !final_ids.contains(thread_id.as_str())
                        && (candidate.discovered_active || !candidate.may_be_archived);
                    let ancestor_cascade_verifies_delete = has_successfully_deleted_ancestor(
                        thread_id,
                        &candidates,
                        &delete_failures,
                    );
                    if !final_absence_verifies_delete && !ancestor_cascade_verifies_delete {
                        failures.push(format!(
                            "delete persistent thread {thread_id} failed ({error}); known thread ids: {known_ids:?}; additional disposable-workspace residue is unverified"
                        ));
                    }
                }
            }
            Err(error) => failures.push(format!(
                "final cleanup discovery failed ({error}); known thread ids: {known_ids:?}; additional disposable-workspace residue is unverified"
            )),
        }

        if let Err(error) = self.session.shutdown() {
            failures.push(format!("close live-probe backend session: {error}"));
        }
        if let Err(error) = self.server.shutdown() {
            failures.push(format!(
                "managed live-probe app-server process/auth cleanup is unverified: {error}"
            ));
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn known_thread_ids(&self) -> Vec<String> {
        self.known_threads
            .iter()
            .map(|thread| thread.id.clone())
            .collect()
    }

    fn known_cleanup_candidates(&self) -> BTreeMap<String, CleanupCandidate> {
        self.known_threads
            .iter()
            .map(|thread| {
                (
                    thread.id.clone(),
                    CleanupCandidate {
                        parent_id: thread.parent_id.clone(),
                        discovered_active: false,
                        may_be_archived: thread.may_be_archived,
                        lineage_established: true,
                    },
                )
            })
            .collect()
    }

    fn discover_disposable_active_threads_with_lineage(
        &mut self,
    ) -> Result<Vec<DiscoveredProbeThread>, String> {
        let threads = self.list_disposable_active_threads()?;
        let mut discovered = Vec::with_capacity(threads.len());
        for thread in threads {
            let mut forked_from_id = thread.forked_from_id;
            let mut lineage_established = forked_from_id.is_some();
            if forked_from_id.is_none() {
                let metadata = self
                    .session
                    .read_thread_metadata(&thread.id, REQUEST_TIMEOUT)
                    .map_err(|error| {
                        format!(
                            "read disposable-workspace thread {} metadata for cleanup lineage: {error}",
                            thread.id
                        )
                    })?;
                if metadata.id != thread.id || metadata.cwd != self.canonical_workspace {
                    return Err(format!(
                        "thread/read metadata for {} did not retain its exact disposable-workspace identity",
                        thread.id
                    ));
                }
                forked_from_id = metadata.forked_from_id;
                lineage_established = forked_from_id.is_some();
            }
            discovered.push(DiscoveredProbeThread {
                id: thread.id,
                forked_from_id,
                lineage_established,
            });
        }
        Ok(discovered)
    }

    fn list_disposable_active_threads(&mut self) -> Result<Vec<ThreadSummary>, String> {
        let mut options = ThreadListOptions::page(CLEANUP_DISCOVERY_PAGE_SIZE)
            .with_cwd(self.canonical_workspace.clone())
            .updated_descending();
        let mut threads = Vec::new();

        for page in 0..CLEANUP_DISCOVERY_MAX_PAGES {
            let response = self
                .session
                .list_thread_page(&options, REQUEST_TIMEOUT)
                .map_err(|error| {
                    format!(
                        "list disposable-workspace thread page {}: {error}",
                        page + 1
                    )
                })?;
            for thread in response.data {
                if thread.cwd != self.canonical_workspace {
                    return Err(format!(
                        "thread/list returned {} with unexpected cwd {:?}",
                        thread.id, thread.cwd
                    ));
                }
                threads.push(thread);
            }
            let Some(cursor) = response.next_cursor else {
                return Ok(threads);
            };
            if page + 1 == CLEANUP_DISCOVERY_MAX_PAGES {
                return Err(format!(
                    "thread/list exceeded the {}-page bounded cleanup inventory",
                    CLEANUP_DISCOVERY_MAX_PAGES
                ));
            }
            options.cursor = Some(cursor);
        }

        unreachable!("the bounded cleanup inventory returns or errors from its final page");
    }

    fn next_event_until(&mut self, deadline: Instant, operation: &str) -> TurnStreamEvent {
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("timed out while attempting to {operation}"));
            let event = self
                .session
                .next_turn_stream_event(remaining.min(STREAM_POLL_TIMEOUT))
                .unwrap_or_else(|_| panic!("receive a bounded live-probe stream event"));
            if let Some(event) = event {
                return event;
            }
        }
    }
}

fn insert_discovered_candidate(
    candidates: &mut BTreeMap<String, CleanupCandidate>,
    thread_id: String,
    discovered_parent_id: Option<String>,
    discovered_lineage_established: bool,
) -> Result<(), String> {
    match candidates.get_mut(&thread_id) {
        Some(candidate) => {
            if candidate.parent_id.is_some()
                && discovered_parent_id.is_some()
                && candidate.parent_id != discovered_parent_id
            {
                return Err(format!(
                    "thread {} disagrees about fork parent: known {:?}, discovered {:?}",
                    thread_id, candidate.parent_id, discovered_parent_id
                ));
            }
            candidate.parent_id = candidate.parent_id.clone().or(discovered_parent_id);
            candidate.discovered_active = true;
            candidate.lineage_established |= discovered_lineage_established;
        }
        None => {
            candidates.insert(
                thread_id,
                CleanupCandidate {
                    parent_id: discovered_parent_id,
                    discovered_active: true,
                    may_be_archived: false,
                    lineage_established: discovered_lineage_established,
                },
            );
        }
    }
    Ok(())
}

fn cleanup_deletion_order(
    candidates: &BTreeMap<String, CleanupCandidate>,
) -> Result<Vec<String>, String> {
    if candidates.len() > 1 {
        let unresolved = candidates
            .iter()
            .filter_map(|(thread_id, candidate)| {
                (!candidate.lineage_established).then_some(thread_id.clone())
            })
            .collect::<Vec<_>>();
        if !unresolved.is_empty() {
            return Err(format!(
                "metadata-only lineage remained absent for multiple disposable-workspace candidates {unresolved:?}"
            ));
        }
    }
    let mut ordered = candidates
        .keys()
        .map(|thread_id| {
            let mut visiting = BTreeSet::new();
            cleanup_depth(thread_id, candidates, &mut visiting)
                .map(|depth| (thread_id.clone(), depth))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ordered.sort_by(|(left_id, left_depth), (right_id, right_depth)| {
        right_depth
            .cmp(left_depth)
            .then_with(|| left_id.cmp(right_id))
    });
    Ok(ordered
        .into_iter()
        .map(|(thread_id, _)| thread_id)
        .collect())
}

fn cleanup_depth(
    thread_id: &str,
    candidates: &BTreeMap<String, CleanupCandidate>,
    visiting: &mut BTreeSet<String>,
) -> Result<usize, String> {
    if !visiting.insert(thread_id.to_string()) {
        return Err(format!("cycle at thread {thread_id}"));
    }
    let depth = candidates
        .get(thread_id)
        .and_then(|candidate| candidate.parent_id.as_deref())
        .filter(|parent_id| candidates.contains_key(*parent_id))
        .map(|parent_id| cleanup_depth(parent_id, candidates, visiting).map(|depth| depth + 1))
        .transpose()?
        .unwrap_or(0);
    visiting.remove(thread_id);
    Ok(depth)
}

fn has_successfully_deleted_ancestor(
    thread_id: &str,
    candidates: &BTreeMap<String, CleanupCandidate>,
    delete_failures: &BTreeMap<String, String>,
) -> bool {
    let mut parent_id = candidates
        .get(thread_id)
        .and_then(|candidate| candidate.parent_id.as_deref());
    let mut seen = BTreeSet::new();
    while let Some(parent) = parent_id {
        if !seen.insert(parent.to_string()) {
            return false;
        }
        if candidates.contains_key(parent) && !delete_failures.contains_key(parent) {
            return true;
        }
        parent_id = candidates
            .get(parent)
            .and_then(|candidate| candidate.parent_id.as_deref());
    }
    false
}

#[test]
fn cleanup_orders_descendants_first_and_reconciles_root_delete_cascades() {
    let mut candidates = BTreeMap::new();
    candidates.insert(
        "root".to_string(),
        CleanupCandidate {
            parent_id: None,
            discovered_active: true,
            may_be_archived: false,
            lineage_established: true,
        },
    );
    candidates.insert(
        "child".to_string(),
        CleanupCandidate {
            parent_id: Some("root".to_string()),
            discovered_active: false,
            may_be_archived: true,
            lineage_established: true,
        },
    );

    insert_discovered_candidate(&mut candidates, "child".to_string(), None, false).unwrap();
    assert_eq!(
        candidates["child"].parent_id.as_deref(),
        Some("root"),
        "optional list lineage must not erase the known child parent"
    );
    assert!(
        insert_discovered_candidate(
            &mut candidates,
            "child".to_string(),
            Some("other-root".to_string()),
            true,
        )
        .is_err()
    );

    insert_discovered_candidate(
        &mut candidates,
        "enriched-child".to_string(),
        Some("root".to_string()),
        true,
    )
    .unwrap();

    assert_eq!(
        cleanup_deletion_order(&candidates).unwrap(),
        vec![
            "child".to_string(),
            "enriched-child".to_string(),
            "root".to_string(),
        ]
    );

    let delete_failures = BTreeMap::from([("child".to_string(), "not found".to_string())]);
    assert!(has_successfully_deleted_ancestor(
        "child",
        &candidates,
        &delete_failures,
    ));
}

impl Drop for LiveProbe {
    fn drop(&mut self) {
        if self.cleanup_attempted {
            return;
        }
        if let Err(error) = self.cleanup() {
            if std::thread::panicking() {
                eprintln!("live-probe cleanup failed during drop: {error}");
            } else {
                panic!("live-probe cleanup failed during drop: {error}");
            }
        }
    }
}

fn probe_tool_spec() -> DynamicToolSpec {
    DynamicToolSpec::new(
        PROBE_TOOL_NAME,
        "Acknowledges the bounded Beryl lifecycle contract probe.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        }),
    )
    .with_namespace(PROBE_TOOL_NAMESPACE)
}
