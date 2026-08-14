use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse, DynamicToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::warn;

use crate::dynamic_tools::BERYL_DYNAMIC_TOOL_NAMESPACE;

pub const YIELD_TOOL: &str = "yield";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleYieldOutcome {
    PhaseNeedsReview,
    BlockedNeedsOperator,
    PhaseContinue,
    PlanComplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BerylLifecycleDynamicToolDispatch {
    response: DynamicToolCallResponse,
    outcome: Option<LifecycleYieldOutcome>,
}

pub fn beryl_lifecycle_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![DynamicToolSpec::new(
        YIELD_TOOL,
        "Yield control to Beryl with one semantic lifecycle outcome after the current turn reaches a natural boundary. Beryl owns all stop, notification, compaction, and resume policy.",
        yield_schema(),
    )
    .with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE)
    .with_defer_loading(false)]
}

pub fn dispatch_beryl_lifecycle_dynamic_tool_call(
    request: &DynamicToolCallRequest,
) -> DynamicToolCallResponse {
    dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata(request).into_response()
}

pub fn dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata(
    request: &DynamicToolCallRequest,
) -> BerylLifecycleDynamicToolDispatch {
    match dispatch_beryl_lifecycle_dynamic_tool_call_result(request) {
        Ok(outcome) => BerylLifecycleDynamicToolDispatch {
            response: lifecycle_yield_success(outcome),
            outcome: Some(outcome),
        },
        Err(error) => {
            warn!(
                tool_call = %request.summary(),
                error = %error,
                "Beryl lifecycle dynamic tool call failed"
            );
            BerylLifecycleDynamicToolDispatch {
                response: lifecycle_yield_failure(request, error),
                outcome: None,
            }
        }
    }
}

impl LifecycleYieldOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PhaseNeedsReview => "phase_needs_review",
            Self::BlockedNeedsOperator => "blocked_needs_operator",
            Self::PhaseContinue => "phase_continue",
            Self::PlanComplete => "plan_complete",
        }
    }
}

impl BerylLifecycleDynamicToolDispatch {
    pub fn response(&self) -> &DynamicToolCallResponse {
        &self.response
    }

    pub fn outcome(&self) -> Option<LifecycleYieldOutcome> {
        self.outcome
    }

    pub fn into_response(self) -> DynamicToolCallResponse {
        self.response
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifecycleYieldArguments {
    outcome: LifecycleYieldOutcome,
}

fn dispatch_beryl_lifecycle_dynamic_tool_call_result(
    request: &DynamicToolCallRequest,
) -> Result<LifecycleYieldOutcome, DynamicLifecycleToolError> {
    if let Some(namespace) = request.namespace()
        && namespace != BERYL_DYNAMIC_TOOL_NAMESPACE
    {
        return Err(DynamicLifecycleToolError::UnsupportedNamespace {
            namespace: namespace.to_string(),
        });
    }

    if request.tool() != YIELD_TOOL {
        return Err(DynamicLifecycleToolError::UnsupportedTool {
            tool: request.tool().to_string(),
        });
    }

    let arguments: LifecycleYieldArguments = parse_arguments(request.arguments())?;
    Ok(arguments.outcome)
}

fn parse_arguments<T>(arguments: &Value) -> Result<T, DynamicLifecycleToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone()).map_err(|source| {
        DynamicLifecycleToolError::InvalidArguments {
            detail: source.to_string(),
        }
    })
}

fn yield_schema() -> Value {
    json!({
        "type": "object",
        "required": ["outcome"],
        "properties": {
            "outcome": {
                "type": "string",
                "enum": LifecycleYieldOutcome::SUPPORTED
            }
        },
        "additionalProperties": false
    })
}

fn lifecycle_yield_success(outcome: LifecycleYieldOutcome) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(compact_json(json!({
        "ok": true,
        "result": {
            "outcome": outcome.as_str()
        }
    })))
}

fn lifecycle_yield_failure(
    request: &DynamicToolCallRequest,
    error: DynamicLifecycleToolError,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": error.kind(),
            "message": error.to_string(),
            "tool": request.tool(),
            "callId": request.call_id(),
        },
    })))
}

fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}

#[derive(Debug)]
enum DynamicLifecycleToolError {
    UnsupportedNamespace { namespace: String },
    UnsupportedTool { tool: String },
    InvalidArguments { detail: String },
}

impl DynamicLifecycleToolError {
    fn kind(&self) -> &'static str {
        match self {
            Self::UnsupportedNamespace { .. } => "unsupported_namespace",
            Self::UnsupportedTool { .. } => "unsupported_tool",
            Self::InvalidArguments { .. } => "invalid_arguments",
        }
    }
}

impl std::fmt::Display for DynamicLifecycleToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedNamespace { namespace } => {
                write!(
                    formatter,
                    "unsupported Beryl dynamic tool namespace {namespace:?}"
                )
            }
            Self::UnsupportedTool { tool } => {
                write!(
                    formatter,
                    "unsupported Beryl lifecycle dynamic tool {tool:?}"
                )
            }
            Self::InvalidArguments { detail } => {
                write!(formatter, "invalid lifecycle yield arguments: {detail}")
            }
        }
    }
}

impl std::error::Error for DynamicLifecycleToolError {}

impl LifecycleYieldOutcome {
    const SUPPORTED: [&'static str; 4] = [
        "phase_needs_review",
        "blocked_needs_operator",
        "phase_continue",
        "plan_complete",
    ];
}
