use std::time::Duration;

use serde_json::Value;

use crate::{
    diagnostic_child_dynamic_tools::{
        DiagnosticAcceptanceOperation, compile_diagnostic_acceptance_operation,
    },
    diagnostic_child_protocol::{
        DiagnosticChildCommand, MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES, request_frame,
    },
};

use super::{
    AcceptanceLimits, AcceptanceSessionError, MAX_ACCEPTANCE_EXPANDED_REQUESTS,
    MAX_ACCEPTANCE_REQUEST_TIMEOUT, validation::validate_duration,
};

#[derive(Clone, Debug)]
pub struct AcceptanceRequest {
    pub(super) command: String,
    pub(super) operation: DiagnosticAcceptanceOperation,
    timeout: Option<Duration>,
}

impl AcceptanceRequest {
    pub fn new(command: impl Into<String>, params: Value) -> Result<Self, AcceptanceSessionError> {
        let command = command.into();
        if !params.is_object() {
            return Err(AcceptanceSessionError::InvalidConfiguration(
                "diagnostic request params must be a JSON object".to_string(),
            ));
        }
        let operation =
            compile_diagnostic_acceptance_operation(&command, &params).map_err(|error| {
                AcceptanceSessionError::InvalidConfiguration(format!(
                    "invalid diagnostic operation {command:?}: {error}"
                ))
            })?;
        Ok(Self {
            command,
            operation,
            timeout: None,
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, AcceptanceSessionError> {
        validate_duration(
            "per-request timeout",
            timeout,
            MAX_ACCEPTANCE_REQUEST_TIMEOUT,
        )?;
        self.timeout = Some(timeout);
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub struct CompiledAcceptanceRequest {
    pub(super) request: AcceptanceRequest,
    pub(super) effective_timeout: Duration,
    maximum_wire_requests: usize,
    largest_qualified_request_id: u64,
}

impl CompiledAcceptanceRequest {
    pub fn command(&self) -> &str {
        &self.request.command
    }

    pub fn effective_timeout(&self) -> Duration {
        self.effective_timeout
    }

    pub fn maximum_wire_requests(&self) -> usize {
        self.maximum_wire_requests
    }

    pub fn largest_qualified_request_id(&self) -> u64 {
        self.largest_qualified_request_id
    }
}

pub fn compile_acceptance_requests(
    requests: impl IntoIterator<Item = AcceptanceRequest>,
    limits: &AcceptanceLimits,
) -> Result<Vec<CompiledAcceptanceRequest>, AcceptanceSessionError> {
    let mut compiled = Vec::new();
    let mut expanded_request_count = 0_usize;
    let mut worst_case_budget = Duration::ZERO;
    for request in requests {
        if compiled.len() >= limits.max_requests {
            return Err(AcceptanceSessionError::RequestLimit {
                limit: limits.max_requests,
            });
        }
        let effective_timeout = request.timeout.unwrap_or(limits.request_timeout);
        if effective_timeout > limits.request_timeout {
            return Err(AcceptanceSessionError::InvalidConfiguration(format!(
                "per-request timeout {effective_timeout:?} exceeds session request timeout {:?}",
                limits.request_timeout
            )));
        }
        let (maximum_wire_requests, operation_budget) = match &request.operation {
            DiagnosticAcceptanceOperation::Request { .. } => (1, effective_timeout),
            DiagnosticAcceptanceOperation::WaitForState { arguments, .. } => (
                maximum_wait_poll_count(arguments.timeout(), arguments.poll_interval()),
                arguments.timeout().min(effective_timeout),
            ),
        };
        expanded_request_count = expanded_request_count
            .checked_add(maximum_wire_requests)
            .ok_or_else(|| invalid("expanded diagnostic request count overflowed"))?;
        if expanded_request_count > MAX_ACCEPTANCE_EXPANDED_REQUESTS {
            return Err(invalid(format!(
                "request plan expands to {expanded_request_count} protocol requests, exceeding the Beryl-owned limit of {MAX_ACCEPTANCE_EXPANDED_REQUESTS}"
            )));
        }
        worst_case_budget = worst_case_budget
            .checked_add(operation_budget)
            .ok_or_else(|| invalid("request plan worst-case runtime budget overflowed"))?;
        if worst_case_budget > limits.runtime_timeout {
            return Err(invalid(format!(
                "request plan worst-case operation budget {worst_case_budget:?} exceeds runtime timeout {:?}",
                limits.runtime_timeout
            )));
        }
        compiled.push(CompiledAcceptanceRequest {
            request,
            effective_timeout,
            maximum_wire_requests,
            largest_qualified_request_id: 0,
        });
    }
    let largest_request_id = 1_u64
        .checked_add(
            u64::try_from(expanded_request_count)
                .map_err(|_| invalid("expanded request count does not fit request identity"))?,
        )
        .ok_or_else(|| invalid("largest diagnostic request identity overflowed"))?;
    for request in &compiled {
        prove_operation_frame(&request.request.operation, largest_request_id)?;
    }
    for request in &mut compiled {
        request.largest_qualified_request_id = largest_request_id;
    }
    Ok(compiled)
}

fn maximum_wait_poll_count(timeout: Duration, poll_interval: Duration) -> usize {
    if timeout.is_zero() {
        return 0;
    }
    usize::try_from(timeout.as_nanos().div_ceil(poll_interval.as_nanos()))
        .expect("bounded diagnostic wait poll count fits usize")
}

fn prove_operation_frame(
    operation: &DiagnosticAcceptanceOperation,
    largest_request_id: u64,
) -> Result<(), AcceptanceSessionError> {
    let (command, params) = match operation {
        DiagnosticAcceptanceOperation::Request { command, params } => (*command, params.clone()),
        DiagnosticAcceptanceOperation::WaitForState { arguments, .. } => (
            DiagnosticChildCommand::ReadUiState,
            serde_json::json!({ "limit": arguments.visible_row_limit() }),
        ),
    };
    request_frame(&largest_request_id.to_string(), command, params).map_err(|error| {
        invalid(format!(
            "diagnostic request frame, including its newline, exceeds the Beryl-owned {MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES}-byte limit: {error}"
        ))
    })?;
    Ok(())
}

fn invalid(message: impl Into<String>) -> AcceptanceSessionError {
    AcceptanceSessionError::InvalidConfiguration(message.into())
}
