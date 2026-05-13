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

mod diagnostic_child_supervisor {
    use std::{fmt, path::PathBuf, time::Duration};

    use serde_json::Value;

    use crate::{BerylHomeDir, diagnostic_child_protocol::DiagnosticChildCommand};

    pub(crate) const DIAGNOSTIC_CHILD_STOP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);

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
        ProtocolEof,
        RequestTimeout {
            timeout: Duration,
        },
        ChildError {
            kind: String,
            message: String,
        },
        Protocol(crate::diagnostic_child_protocol::DiagnosticProtocolError),
        Other(String),
    }

    impl DiagnosticChildSupervisor {
        pub(crate) fn start(
            &mut self,
            _supervisor_home: &BerylHomeDir,
            child_home: impl Into<PathBuf>,
        ) -> Result<DiagnosticChildStartOutcome, DiagnosticChildSupervisorError> {
            if let Some(identity) = self.identity.clone() {
                return Ok(DiagnosticChildStartOutcome::AlreadyRunning(identity));
            }
            let identity = DiagnosticChildIdentity {
                pid: 42,
                home_dir: child_home.into(),
                executable_path: PathBuf::from("beryl-test.exe"),
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
                Self::ProtocolEof => write!(formatter, "diagnostic child protocol stream ended"),
                Self::RequestTimeout { timeout } => write!(
                    formatter,
                    "timed out waiting for diagnostic child response after {timeout:?}"
                ),
                Self::ChildError { kind, message } => {
                    write!(formatter, "diagnostic child returned {kind}: {message}")
                }
                Self::Protocol(error) => write!(formatter, "{error}"),
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
    BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE, DIAGNOSTIC_CHILD_READ_PROCESS_TOOL,
    DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL, DIAGNOSTIC_CHILD_START_TOOL,
    DIAGNOSTIC_CHILD_STATUS_TOOL, dispatch_beryl_diagnostic_child_dynamic_tool_call,
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
