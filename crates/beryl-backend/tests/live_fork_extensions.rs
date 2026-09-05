#![cfg(windows)]

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
    time::Duration,
};

use beryl_backend::{
    JsonRpcError, ManagedBackendError, ManagedBackendLaunchOptions, ManagedBackendProbeReport,
    ManagedBackendServer, ManagedBackendSession, ThreadStartOptions, UsageTreeAccountingStatus,
    UsageTreeReadOutcome, UsageTreeTokenUsageBreakdown,
};
use beryl_model::workspace::RuntimeMode;
use tempfile::TempDir;

const LIVE_TEST_ENABLE_ENV: &str = "BERYL_RUN_LIVE_FORK_EXTENSIONS_TEST";
const LIVE_STANDALONE_APP_SERVER_ENV: &str = "BERYL_LIVE_STANDALONE_APP_SERVER_EXECUTABLE";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Exercises only the managed standalone-app-server launch boundary; it never starts a turn.
///
/// The currently public launch API has no task-local `CODEX_HOME` injection. This test instead
/// creates an ephemeral no-turn root in a task-owned temporary workspace, so it never reads a
/// thread inventory or transcript and leaves no persistent thread. The launched app-server still
/// inherits its normal `CODEX_HOME` for bootstrap configuration and authentication.
///
/// Run only with `BERYL_RUN_LIVE_FORK_EXTENSIONS_TEST=1`,
/// `BERYL_LIVE_STANDALONE_APP_SERVER_EXECUTABLE=<absolute codex-app-server.exe>`, and
/// `--run-ignored`.
#[test]
#[ignore = "launches an operator-supplied standalone Codex app-server executable"]
fn standalone_fork_usage_tree_smoke_uses_beryl_session_boundary() {
    if std::env::var(LIVE_TEST_ENABLE_ENV).as_deref() != Ok("1") {
        return;
    }

    let executable = required_standalone_app_server();
    let workspace =
        TempDir::new().unwrap_or_else(|_| panic!("create the task-owned live-smoke workspace"));
    let launch_options =
        ManagedBackendLaunchOptions::with_exact_host_windows_standalone_app_server(&executable)
            .unwrap_or_else(|_| panic!("accept the exact standalone app-server executable"));
    let server = ManagedBackendServer::launch_with_options(
        RuntimeMode::HostWindows,
        workspace.path(),
        launch_options,
    )
    .unwrap_or_else(|_| panic!("launch the managed standalone app-server"));
    let (mut server, mut session, report) = connect_and_probe_guarded(server);

    let smoke_result = catch_unwind(AssertUnwindSafe(|| {
        assert!(
            report.initialize().user_agent.starts_with("beryl/"),
            "the real initialize response must identify the Beryl session, whose public initialization path requests savedPathOnly"
        );
        let expected_codex_home = std::env::var_os("CODEX_HOME")
            .filter(|home| !home.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| panic!("resolve the inherited Host-Windows user profile"))
                    .join(".codex")
            });
        assert_eq!(
            PathBuf::from(&report.initialize().codex_home),
            expected_codex_home,
            "the live smoke must inherit the active default Codex home"
        );
        eprintln!("live_fork_extensions_smoke_codex_home=inherited");

        let root = session
            .start_thread_with_options(
                workspace.path(),
                ThreadStartOptions::ephemeral(),
                REQUEST_TIMEOUT,
            )
            .unwrap_or_else(|_| panic!("start an isolated ephemeral no-turn root"));
        let root_id = root.thread.summary().id;
        assert!(
            root.thread.summary().ephemeral,
            "the live smoke must use an app-server ephemeral thread"
        );

        let outcome = session.read_token_usage_tree(&root_id, REQUEST_TIMEOUT);
        if is_unused_ephemeral_root_without_usage_tree(&outcome, &root_id) {
            eprintln!("live_fork_extensions_smoke_usage_tree=exact_no_tree");
            return;
        }
        let outcome = outcome.unwrap_or_else(|error| {
            panic!("read the fork usage tree through Beryl's public API: {error}")
        });
        let UsageTreeReadOutcome::Snapshot(snapshot) = outcome else {
            panic!(
                "the maintained standalone fork must recognize thread/tokenUsageTree/read rather than reporting it unsupported"
            );
        };

        assert_eq!(snapshot.schema_version, 1);
        assert_eq!(snapshot.root_thread_id, root_id);
        assert_eq!(
            snapshot.accounting_status,
            UsageTreeAccountingStatus::Complete,
            "the unused ephemeral root must expose a complete, not legacy-partial, tree"
        );
        assert_zero_usage(&snapshot.self_usage.total, "root cumulative usage");
        assert_zero_usage(&snapshot.self_usage.last, "root last-turn usage");
        assert_zero_usage(&snapshot.descendants_total, "descendant usage");
        assert_zero_usage(&snapshot.tree_total, "tree usage");
        eprintln!("live_fork_extensions_smoke_usage_tree=snapshot");
    }));
    let cleanup_result = server.shutdown();

    match smoke_result {
        Ok(()) => cleanup_result.unwrap_or_else(|error| {
            panic!("managed standalone app-server cleanup failed: {error}")
        }),
        Err(payload) => {
            if let Err(error) = cleanup_result {
                eprintln!(
                    "managed standalone app-server cleanup also failed while unwinding: {error}"
                );
            }
            resume_unwind(payload);
        }
    }
}

fn is_unused_ephemeral_root_without_usage_tree(
    outcome: &Result<UsageTreeReadOutcome, ManagedBackendError>,
    expected_root_id: &str,
) -> bool {
    matches!(
        outcome,
        Err(ManagedBackendError::RequestFailed { method, error })
            if method == "thread/tokenUsageTree/read"
                && error.code == -32600
                && error.message == format!("usage tree not found for thread {expected_root_id}")
    )
}

#[test]
fn unused_ephemeral_root_recognizes_only_exact_no_usage_tree_response() {
    let root_id = "ephemeral-root";
    let exact = Err(ManagedBackendError::RequestFailed {
        method: "thread/tokenUsageTree/read".to_string(),
        error: JsonRpcError {
            code: -32600,
            message: format!("usage tree not found for thread {root_id}"),
            data: None,
        },
    });
    assert!(is_unused_ephemeral_root_without_usage_tree(&exact, root_id));

    for error in [
        ManagedBackendError::RequestFailed {
            method: "thread/tokenUsageTree/read".to_string(),
            error: JsonRpcError {
                code: -32601,
                message: format!("usage tree not found for thread {root_id}"),
                data: None,
            },
        },
        ManagedBackendError::RequestFailed {
            method: "thread/tokenUsageTree/read".to_string(),
            error: JsonRpcError {
                code: -32600,
                message: "usage tree not found for thread different-root".to_string(),
                data: None,
            },
        },
        ManagedBackendError::RequestFailed {
            method: "thread/tokenUsageTree/read".to_string(),
            error: JsonRpcError {
                code: -32600,
                message: format!("usage tree not found for thread {root_id}.unexpected"),
                data: None,
            },
        },
        ManagedBackendError::RequestFailed {
            method: "thread/tokenUsageTree/read".to_string(),
            error: JsonRpcError {
                code: -32600,
                message: "usage tree missing".to_string(),
                data: None,
            },
        },
        ManagedBackendError::RequestFailed {
            method: "thread/read".to_string(),
            error: JsonRpcError {
                code: -32600,
                message: format!("usage tree not found for thread {root_id}"),
                data: None,
            },
        },
        ManagedBackendError::RequestTimeout {
            method: "thread/tokenUsageTree/read".to_string(),
            timeout: REQUEST_TIMEOUT,
        },
    ] {
        assert!(!is_unused_ephemeral_root_without_usage_tree(
            &Err(error),
            root_id
        ));
    }
}

fn required_standalone_app_server() -> PathBuf {
    let executable = std::env::var_os(LIVE_STANDALONE_APP_SERVER_ENV).unwrap_or_else(|| {
        panic!(
            "set {LIVE_STANDALONE_APP_SERVER_ENV} to the absolute exact standalone codex-app-server.exe path"
        )
    });
    let executable = PathBuf::from(executable);
    assert!(
        executable.is_absolute(),
        "{LIVE_STANDALONE_APP_SERVER_ENV} must be an absolute exact executable path"
    );
    assert!(
        executable.is_file(),
        "{LIVE_STANDALONE_APP_SERVER_ENV} must name an existing standalone executable"
    );
    executable
}

fn connect_and_probe_guarded(
    mut server: ManagedBackendServer,
) -> (
    ManagedBackendServer,
    ManagedBackendSession,
    ManagedBackendProbeReport,
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
                    "connect, initialize, and probe the managed standalone app-server: {probe_error}"
                ),
                Err(cleanup_error) => panic!(
                    "connect, initialize, and probe the managed standalone app-server: {probe_error}; managed process/auth cleanup also failed: {cleanup_error}"
                ),
            }
        }
        Err(payload) => {
            if let Err(error) = server.shutdown() {
                eprintln!(
                    "managed standalone app-server cleanup failed while unwinding guarded probing: {error}"
                );
            }
            resume_unwind(payload);
        }
    }
}

fn assert_zero_usage(usage: &UsageTreeTokenUsageBreakdown, label: &str) {
    assert_eq!(
        usage.total_tokens, 0,
        "{label} totalTokens must remain zero"
    );
    assert_eq!(
        usage.input_tokens, 0,
        "{label} inputTokens must remain zero"
    );
    assert_eq!(
        usage.cached_input_tokens, 0,
        "{label} cachedInputTokens must remain zero"
    );
    assert_eq!(
        usage.cache_write_input_tokens, 0,
        "{label} cacheWriteInputTokens must remain zero"
    );
    assert_eq!(
        usage.output_tokens, 0,
        "{label} outputTokens must remain zero"
    );
    assert_eq!(
        usage.reasoning_output_tokens, 0,
        "{label} reasoningOutputTokens must remain zero"
    );
}
