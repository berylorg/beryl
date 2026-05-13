#[path = "support/tempdir.rs"]
mod tempdir_support;

pub use beryl_app::BerylHomeDir;

mod dynamic_tools {
    pub const BERYL_DYNAMIC_TOOL_NAMESPACE: &str = "beryl";
}

mod diagnostic_dynamic_tools {
    use serde::Serialize;

    #[derive(Clone, Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct RuntimeTargetDiagnostic {
        pub runtime: String,
        pub canonical_path: String,
        pub display_label: String,
    }

    #[derive(Clone, Debug, Default, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct VisibleMediaSnapshot {}
}

#[path = "../src/diagnostic_child_protocol.rs"]
mod diagnostic_child_protocol;

#[path = "../src/diagnostic_child_control.rs"]
mod diagnostic_child_control;

mod diagnostic_child_supervisor {
    use std::{fmt, io, path::PathBuf, time::Duration};

    use serde_json::Value;

    use crate::{BerylHomeDir, diagnostic_child_protocol::DiagnosticChildCommand};

    pub(crate) const DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);
    pub(crate) const MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES: usize = 1024;

    #[derive(Default)]
    pub(crate) struct DiagnosticChildSupervisor {
        identity: Option<DiagnosticChildIdentity>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct DiagnosticChildIdentity {
        pub pid: u32,
        pub home_dir: PathBuf,
        pub executable_path: PathBuf,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct DiagnosticChildLaunch {
        child_home: PathBuf,
        executable_path: PathBuf,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) enum DiagnosticChildStartOutcome {
        Started(DiagnosticChildIdentity),
        AlreadyRunning(DiagnosticChildIdentity),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) enum DiagnosticChildStopOutcome {
        Stopped(DiagnosticChildIdentity),
        NotRunning,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) enum DiagnosticChildStatus {
        Running(DiagnosticChildIdentity),
        NotRunning,
    }

    #[derive(Debug)]
    pub(crate) enum DiagnosticChildSupervisorError {
        BerylHomeDir(beryl_app::BerylHomeDirError),
        HomeCollidesWithSupervisor {
            child_home: PathBuf,
            supervisor_home: PathBuf,
        },
        CurrentExecutable {
            source: io::Error,
        },
        InvalidExecutablePath {
            path: PathBuf,
            reason: &'static str,
        },
        ExecutablePathAccess {
            path: PathBuf,
            source: io::Error,
        },
        Spawn {
            executable_path: PathBuf,
            source: io::Error,
        },
        ProtocolEof,
        RequestTimeout {
            timeout: Duration,
        },
        ChildError {
            kind: String,
            message: String,
        },
        Protocol(crate::diagnostic_child_protocol::DiagnosticProtocolError),
        StartupProtocolTimeout {
            timeout: Duration,
        },
        StartupProtocolEof,
        StartupProtocolMalformed {
            source: crate::diagnostic_child_protocol::DiagnosticProtocolError,
        },
        StartupProtocolRejected {
            kind: String,
            message: String,
        },
        StartupProtocolIncompatible {
            message: String,
        },
        Other(String),
    }

    impl DiagnosticChildLaunch {
        pub(crate) fn new(
            child_home: impl Into<PathBuf>,
            executable_path: impl Into<PathBuf>,
        ) -> Self {
            Self {
                child_home: child_home.into(),
                executable_path: executable_path.into(),
            }
        }

        pub(crate) fn current_executable(
            child_home: impl Into<PathBuf>,
        ) -> Result<Self, io::Error> {
            Ok(Self::new(child_home, PathBuf::from("beryl-current.exe")))
        }
    }

    impl DiagnosticChildSupervisor {
        pub(crate) fn start(
            &mut self,
            _supervisor_home: &BerylHomeDir,
            launch: DiagnosticChildLaunch,
        ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
            if let Some(identity) = self.identity.clone() {
                return Ok(DiagnosticChildStartOutcome::AlreadyRunning(identity));
            }
            let identity = DiagnosticChildIdentity {
                pid: 42,
                home_dir: launch.child_home,
                executable_path: launch.executable_path,
            };
            self.identity = Some(identity.clone());
            Ok(DiagnosticChildStartOutcome::Started(identity))
        }

        pub(crate) fn stop(
            &mut self,
        ) -> Result<DiagnosticChildStopOutcome, DiagnosticChildSupervisorError> {
            Ok(self
                .identity
                .take()
                .map(DiagnosticChildStopOutcome::Stopped)
                .unwrap_or(DiagnosticChildStopOutcome::NotRunning))
        }

        pub(crate) fn status(
            &mut self,
        ) -> Result<DiagnosticChildStatus, DiagnosticChildSupervisorError> {
            Ok(self
                .identity
                .clone()
                .map(DiagnosticChildStatus::Running)
                .unwrap_or(DiagnosticChildStatus::NotRunning))
        }

        pub(crate) fn request(
            &mut self,
            command: DiagnosticChildCommand,
            params: Value,
            _timeout: Duration,
        ) -> Result<Value, DiagnosticChildSupervisorError> {
            if self.identity.is_none() {
                return Err(DiagnosticChildSupervisorError::ProtocolEof);
            }
            Ok(serde_json::json!({
                "command": command.as_str(),
                "params": params,
            }))
        }
    }

    impl fmt::Display for DiagnosticChildSupervisorError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::BerylHomeDir(error) => write!(formatter, "{error}"),
                Self::HomeCollidesWithSupervisor {
                    child_home,
                    supervisor_home,
                } => write!(
                    formatter,
                    "diagnostic child home {} must be isolated from supervisor home {}",
                    child_home.display(),
                    supervisor_home.display()
                ),
                Self::CurrentExecutable { source } => {
                    write!(
                        formatter,
                        "failed to resolve current Beryl executable path: {source}"
                    )
                }
                Self::InvalidExecutablePath { path, reason } => write!(
                    formatter,
                    "invalid diagnostic child executable path {}: {reason}",
                    path.display()
                ),
                Self::ExecutablePathAccess { path, source } => write!(
                    formatter,
                    "failed to inspect diagnostic child executable path {}: {source}",
                    path.display()
                ),
                Self::Spawn {
                    executable_path,
                    source,
                } => write!(
                    formatter,
                    "failed to spawn diagnostic child Beryl process from {}: {source}",
                    executable_path.display()
                ),
                Self::ProtocolEof => write!(formatter, "diagnostic child protocol stream ended"),
                Self::RequestTimeout { timeout } => write!(
                    formatter,
                    "timed out waiting for diagnostic child response after {timeout:?}"
                ),
                Self::ChildError { kind, message } => {
                    write!(formatter, "diagnostic child returned {kind}: {message}")
                }
                Self::Protocol(error) => write!(formatter, "{error}"),
                Self::StartupProtocolTimeout { timeout } => write!(
                    formatter,
                    "timed out waiting for diagnostic child startup protocol after {timeout:?}"
                ),
                Self::StartupProtocolEof => write!(
                    formatter,
                    "diagnostic child startup protocol stream ended before readiness"
                ),
                Self::StartupProtocolMalformed { source } => write!(
                    formatter,
                    "diagnostic child startup protocol returned malformed response: {source}"
                ),
                Self::StartupProtocolRejected { kind, message } => write!(
                    formatter,
                    "diagnostic child startup protocol returned {kind}: {message}"
                ),
                Self::StartupProtocolIncompatible { message } => write!(
                    formatter,
                    "diagnostic child startup protocol is incompatible: {message}"
                ),
                Self::Other(message) => write!(formatter, "{message}"),
            }
        }
    }
}

#[path = "../src/gui_control_dynamic_tools.rs"]
mod gui_control_dynamic_tools;

#[path = "../src/diagnostic_child_dynamic_tools.rs"]
mod diagnostic_child_dynamic_tools;

use beryl_backend::{
    DynamicToolCallOutputContentItem, DynamicToolCallRequest, DynamicToolCallResponse,
    parse_dynamic_tool_call_request,
};
use diagnostic_child_dynamic_tools::{
    BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE, DIAGNOSTIC_CHILD_HARD_STOP_TURN_TOOL,
    DIAGNOSTIC_CHILD_LIST_WORKSPACE_THREADS_TOOL, DIAGNOSTIC_CHILD_PREPARE_RENDERER_WINDOW_TOOL,
    DIAGNOSTIC_CHILD_READ_PROCESS_TOOL, DIAGNOSTIC_CHILD_READ_RENDERER_TOOL,
    DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL, DIAGNOSTIC_CHILD_SOFT_STOP_TURN_TOOL,
    DIAGNOSTIC_CHILD_START_TOOL, DIAGNOSTIC_CHILD_START_TURN_TOOL, DIAGNOSTIC_CHILD_STATUS_TOOL,
    DIAGNOSTIC_CHILD_WAIT_FOR_STATE_TOOL, beryl_diagnostic_child_dynamic_tool_specs,
    dispatch_beryl_diagnostic_child_dynamic_tool_call,
};
use diagnostic_child_supervisor::DiagnosticChildSupervisor;
use serde_json::{Value, json};

#[test]
fn diagnostic_child_read_before_start_returns_not_running_failure() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    let request = tool_request(DIAGNOSTIC_CHILD_READ_PROCESS_TOOL, json!({}));

    let response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &request,
    );
    let payload = response_json(&response);

    assert!(!response.success);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "diagnostic_child_not_running");

    root.close().unwrap();
}

#[test]
fn diagnostic_child_status_reports_not_running_as_success() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    let request = tool_request(DIAGNOSTIC_CHILD_STATUS_TOOL, json!({}));

    let response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &request,
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["result"]["status"], "not_running");

    root.close().unwrap();
}

#[test]
fn diagnostic_child_start_returns_identity_and_running_status() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    let request = tool_request(
        DIAGNOSTIC_CHILD_START_TOOL,
        json!({ "berylHomeDir": child.path().display().to_string() }),
    );

    let response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &request,
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["result"]["status"], "started");
    assert_eq!(payload["result"]["child"]["pid"], 42);
    assert!(payload["result"]["child"]["home"].as_str().unwrap().len() <= 1024);
    assert_eq!(
        payload["result"]["child"]["executablePath"],
        "beryl-current.exe"
    );

    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn diagnostic_child_start_forwards_custom_executable_and_preserves_running_identity() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let second_child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let executable_path = child.path().join(format!("custom b{}ryl.exe", '\u{00e9}'));
    let second_executable_path = second_child.path().join("other beryl.exe");
    let mut supervisor = DiagnosticChildSupervisor::default();
    let first_request = tool_request(
        DIAGNOSTIC_CHILD_START_TOOL,
        json!({
            "berylHomeDir": child.path().display().to_string(),
            "executablePath": executable_path.display().to_string()
        }),
    );
    let second_request = tool_request(
        DIAGNOSTIC_CHILD_START_TOOL,
        json!({
            "berylHomeDir": second_child.path().display().to_string(),
            "executablePath": second_executable_path.display().to_string()
        }),
    );

    let first_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &first_request,
    );
    let second_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &second_request,
    );
    let first_payload = response_json(&first_response);
    let second_payload = response_json(&second_response);

    assert!(first_response.success);
    assert_eq!(first_payload["result"]["status"], "started");
    assert_eq!(
        first_payload["result"]["child"]["executablePath"]
            .as_str()
            .unwrap(),
        executable_path.display().to_string()
    );
    assert!(second_response.success);
    assert_eq!(second_payload["result"]["status"], "already_running");
    assert_eq!(
        second_payload["result"]["child"]["home"].as_str().unwrap(),
        first_payload["result"]["child"]["home"].as_str().unwrap()
    );
    assert_eq!(
        second_payload["result"]["child"]["executablePath"]
            .as_str()
            .unwrap(),
        first_payload["result"]["child"]["executablePath"]
            .as_str()
            .unwrap()
    );

    second_child.close().unwrap();
    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn diagnostic_child_control_params_are_normalized_before_protocol_request() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    let start_request = tool_request(
        DIAGNOSTIC_CHILD_START_TOOL,
        json!({ "berylHomeDir": child.path().display().to_string() }),
    );
    let scroll_request = tool_request(
        DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL,
        json!({ "command": "page_down", "repeat": 99 }),
    );

    let _ = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &start_request,
    );
    let response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &scroll_request,
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["result"]["command"], "scroll_transcript");
    assert_eq!(payload["result"]["params"]["repeat"], 8);

    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn diagnostic_child_new_control_tools_are_mapped_to_protocol_commands() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    let start_request = tool_request(
        DIAGNOSTIC_CHILD_START_TOOL,
        json!({ "berylHomeDir": child.path().display().to_string() }),
    );
    let list_request = tool_request(
        DIAGNOSTIC_CHILD_LIST_WORKSPACE_THREADS_TOOL,
        json!({ "limit": 999 }),
    );
    let turn_request = tool_request(
        DIAGNOSTIC_CHILD_START_TURN_TOOL,
        json!({ "text": "diagnostic turn" }),
    );
    let renderer_request = tool_request(DIAGNOSTIC_CHILD_READ_RENDERER_TOOL, json!({}));
    let renderer_prepare_alias_request = tool_request(
        DIAGNOSTIC_CHILD_READ_RENDERER_TOOL,
        json!({ "prepareWindow": true }),
    );
    let prepare_renderer_request =
        tool_request(DIAGNOSTIC_CHILD_PREPARE_RENDERER_WINDOW_TOOL, json!({}));

    let _ = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &start_request,
    );
    let list_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &list_request,
    );
    let turn_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &turn_request,
    );
    let renderer_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &renderer_request,
    );
    let renderer_prepare_alias_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &renderer_prepare_alias_request,
    );
    let prepare_renderer_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &prepare_renderer_request,
    );
    let list_payload = response_json(&list_response);
    let turn_payload = response_json(&turn_response);
    let renderer_payload = response_json(&renderer_response);
    let renderer_prepare_alias_payload = response_json(&renderer_prepare_alias_response);
    let prepare_renderer_payload = response_json(&prepare_renderer_response);

    assert!(list_response.success);
    assert_eq!(list_payload["result"]["command"], "list_workspace_threads");
    assert_eq!(list_payload["result"]["params"]["limit"], 128);
    assert!(turn_response.success);
    assert_eq!(turn_payload["result"]["command"], "start_turn");
    assert_eq!(turn_payload["result"]["params"]["text"], "diagnostic turn");
    assert!(renderer_response.success);
    assert_eq!(renderer_payload["result"]["command"], "read_renderer");
    assert!(renderer_prepare_alias_response.success);
    assert_eq!(
        renderer_prepare_alias_payload["result"]["command"],
        "prepare_renderer_window"
    );
    assert!(prepare_renderer_response.success);
    assert_eq!(
        prepare_renderer_payload["result"]["command"],
        "prepare_renderer_window"
    );

    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn diagnostic_child_stop_tools_require_and_forward_expected_turn_identity() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    let start_request = tool_request(
        DIAGNOSTIC_CHILD_START_TOOL,
        json!({ "berylHomeDir": child.path().display().to_string() }),
    );
    let missing_identity_request = tool_request(DIAGNOSTIC_CHILD_SOFT_STOP_TURN_TOOL, json!({}));
    let soft_request = tool_request(
        DIAGNOSTIC_CHILD_SOFT_STOP_TURN_TOOL,
        json!({ "expectedThreadId": "thread-a", "expectedTurnId": "turn-a" }),
    );
    let hard_request = tool_request(
        DIAGNOSTIC_CHILD_HARD_STOP_TURN_TOOL,
        json!({ "expectedThreadId": "thread-b", "expectedTurnId": "turn-b" }),
    );

    let _ = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &start_request,
    );
    let missing_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &missing_identity_request,
    );
    let soft_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &soft_request,
    );
    let hard_response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &hard_request,
    );
    let missing_payload = response_json(&missing_response);
    let soft_payload = response_json(&soft_response);
    let hard_payload = response_json(&hard_response);

    assert!(!missing_response.success);
    assert_eq!(missing_payload["error"]["kind"], "invalid_arguments");
    assert!(soft_response.success);
    assert_eq!(soft_payload["result"]["command"], "soft_stop_turn");
    assert_eq!(
        soft_payload["result"]["params"]["expectedThreadId"],
        "thread-a"
    );
    assert_eq!(soft_payload["result"]["params"]["expectedTurnId"], "turn-a");
    assert!(hard_response.success);
    assert_eq!(hard_payload["result"]["command"], "hard_stop_turn");
    assert_eq!(
        hard_payload["result"]["params"]["expectedThreadId"],
        "thread-b"
    );
    assert_eq!(hard_payload["result"]["params"]["expectedTurnId"], "turn-b");

    child.close().unwrap();
    root.close().unwrap();
}

#[test]
fn diagnostic_child_limit_schemas_match_their_runtime_caps() {
    let specs = beryl_diagnostic_child_dynamic_tool_specs();
    let start_schema = specs
        .iter()
        .find(|spec| spec.name == DIAGNOSTIC_CHILD_START_TOOL)
        .unwrap();
    let list_schema = specs
        .iter()
        .find(|spec| spec.name == DIAGNOSTIC_CHILD_LIST_WORKSPACE_THREADS_TOOL)
        .unwrap();
    let renderer_schema = specs
        .iter()
        .find(|spec| spec.name == DIAGNOSTIC_CHILD_READ_RENDERER_TOOL)
        .unwrap();
    let wait_schema = specs
        .iter()
        .find(|spec| spec.name == DIAGNOSTIC_CHILD_WAIT_FOR_STATE_TOOL)
        .unwrap();

    assert_eq!(
        renderer_schema.input_schema["properties"]["prepareWindow"]["type"],
        "boolean"
    );
    assert_eq!(renderer_schema.input_schema["additionalProperties"], false);
    assert_eq!(
        list_schema.input_schema["properties"]["limit"]["maximum"],
        diagnostic_child_control::MAX_DIAGNOSTIC_THREAD_LIST_LIMIT
    );
    assert_eq!(
        wait_schema.input_schema["properties"]["limit"]["maximum"],
        diagnostic_child_control::MAX_DIAGNOSTIC_WAIT_VISIBLE_ROW_LIMIT
    );
    assert_eq!(
        start_schema.input_schema["properties"]["executablePath"]["maxLength"],
        diagnostic_child_supervisor::MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES
    );
    assert_eq!(
        start_schema.input_schema["required"],
        json!(["berylHomeDir"])
    );
}

#[test]
fn diagnostic_stop_turn_arguments_match_exact_thread_and_turn_identity() {
    let arguments = diagnostic_child_control::DiagnosticStopTurnArguments {
        expected_thread_id: "thread-a".to_string(),
        expected_turn_id: "turn-a".to_string(),
    };

    assert!(arguments.validate().is_ok());
    assert!(arguments.matches("thread-a", "turn-a"));
    assert!(!arguments.matches("thread-b", "turn-a"));
    assert!(!arguments.matches("thread-a", "turn-b"));
}

#[test]
fn diagnostic_child_wait_for_state_polls_ui_state_until_timeout() {
    let root = tempdir_support::temp_dir("beryl-diagnostic-child-dynamic-tools-");
    let child = tempdir_support::temp_dir("beryl-diagnostic-child-home-");
    let supervisor_home = BerylHomeDir::from_explicit_path(root.path()).unwrap();
    let mut supervisor = DiagnosticChildSupervisor::default();
    let start_request = tool_request(
        DIAGNOSTIC_CHILD_START_TOOL,
        json!({ "berylHomeDir": child.path().display().to_string() }),
    );
    let wait_request = tool_request(
        DIAGNOSTIC_CHILD_WAIT_FOR_STATE_TOOL,
        json!({ "predicate": "ready", "timeoutMs": 0, "pollIntervalMs": 25, "limit": 999 }),
    );

    let _ = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &start_request,
    );
    let response = dispatch_beryl_diagnostic_child_dynamic_tool_call(
        &mut supervisor,
        &supervisor_home,
        &wait_request,
    );
    let payload = response_json(&response);

    assert!(response.success);
    assert_eq!(payload["result"]["status"], "timeout");
    assert_eq!(payload["result"]["predicate"], "ready");
    assert_eq!(payload["result"]["uiState"]["command"], "read_ui_state");
    assert_eq!(payload["result"]["uiState"]["params"]["limit"], 64);

    child.close().unwrap();
    root.close().unwrap();
}

fn tool_request(tool: &str, arguments: Value) -> DynamicToolCallRequest {
    parse_dynamic_tool_call_request(
        json!("dynamic-request-1"),
        "item/tool/call",
        Some(json!({
            "threadId": "thread_1",
            "turnId": "turn_1",
            "callId": "call_1",
            "namespace": BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE,
            "tool": tool,
            "arguments": arguments
        })),
    )
    .unwrap()
    .unwrap()
}

fn response_json(response: &DynamicToolCallResponse) -> Value {
    let Some(DynamicToolCallOutputContentItem::InputText { text }) = response.content_items.first()
    else {
        panic!("expected a single text content item")
    };
    serde_json::from_str(text).unwrap()
}
