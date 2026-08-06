use std::time::{Duration, Instant};

use serde_json::Value;

use crate::{
    diagnostic_child_dynamic_tools::{
        DiagnosticAcceptanceOperation, diagnostic_wait_deadline, execute_diagnostic_wait_for_state,
    },
    diagnostic_child_protocol::DiagnosticChildCommand,
};

use super::{
    AcceptanceResponse, AcceptanceSession, AcceptanceSessionError, CompiledAcceptanceRequest,
    compile_acceptance_requests, duration_millis, evidence,
};

impl AcceptanceSession {
    pub fn request(
        &mut self,
        request: super::AcceptanceRequest,
    ) -> Result<AcceptanceResponse, AcceptanceSessionError> {
        let sequence = self.next_logical_sequence()?;
        let remaining = self.remaining_runtime()?;
        let compiled = compile_acceptance_requests([request], &self.config.limits)?;
        let compiled = compiled
            .into_iter()
            .next()
            .expect("one request compiles to one operation");
        self.execute_compiled_request_at_sequence(sequence, compiled, remaining)
    }

    pub fn execute_compiled_request(
        &mut self,
        request: CompiledAcceptanceRequest,
    ) -> Result<AcceptanceResponse, AcceptanceSessionError> {
        let sequence = self.next_logical_sequence()?;
        let remaining = self.remaining_runtime()?;
        self.execute_compiled_request_at_sequence(sequence, request, remaining)
    }

    fn next_logical_sequence(&self) -> Result<usize, AcceptanceSessionError> {
        let sequence = self
            .evidence
            .as_ref()
            .expect("active acceptance session has evidence")
            .evidence
            .requests
            .len()
            + 1;
        if sequence > self.config.limits.max_requests {
            return Err(AcceptanceSessionError::RequestLimit {
                limit: self.config.limits.max_requests,
            });
        }
        Ok(sequence)
    }

    fn remaining_runtime(&self) -> Result<Duration, AcceptanceSessionError> {
        self.config
            .limits
            .runtime_timeout
            .checked_sub(self.started_at.elapsed())
            .ok_or(AcceptanceSessionError::RuntimeLimit {
                limit: self.config.limits.runtime_timeout,
            })
    }

    fn execute_compiled_request_at_sequence(
        &mut self,
        sequence: usize,
        request: CompiledAcceptanceRequest,
        remaining: Duration,
    ) -> Result<AcceptanceResponse, AcceptanceSessionError> {
        let timeout = request.effective_timeout.min(remaining);
        match request.request.operation {
            DiagnosticAcceptanceOperation::Request { command, params } => self
                .execute_wire_request(sequence, &request.request.command, command, params, timeout),
            DiagnosticAcceptanceOperation::WaitForState { arguments, params } => self
                .execute_wait_for_state(
                    sequence,
                    &request.request.command,
                    arguments,
                    params,
                    timeout,
                ),
        }
    }

    #[cfg(test)]
    pub(crate) fn diagnostic_child_pid_for_test(&self) -> u32 {
        self.evidence
            .as_ref()
            .expect("active acceptance session has evidence")
            .evidence
            .process
            .diagnostic_child_pid
    }

    fn execute_wire_request(
        &mut self,
        sequence: usize,
        command_name: &str,
        command: DiagnosticChildCommand,
        params: Value,
        timeout: Duration,
    ) -> Result<AcceptanceResponse, AcceptanceSessionError> {
        let serialized_params = serialize_params(&params)?;
        let request_started = Instant::now();
        let (request_id, result) = self
            .supervisor
            .request_with_id_retaining_observed_exit(command, params, timeout);
        let mut identity_range = evidence::ProtocolIdentityRangeBuilder::default();
        identity_range.observe(request_id.as_deref());
        let request_duration = duration_millis(request_started.elapsed());
        let timeout_millis = duration_millis(timeout);
        match result {
            Ok(result) => {
                let serialized = serialize_response(&result, request_id.clone())?;
                self.evidence
                    .as_mut()
                    .expect("active acceptance session has evidence")
                    .record_success(
                        sequence,
                        request_id.clone(),
                        identity_range.finish(),
                        command_name,
                        &serialized_params,
                        timeout_millis,
                        request_duration,
                        &serialized,
                    );
                Ok(AcceptanceResponse { request_id, result })
            }
            Err(error) => {
                let message = error.to_string();
                self.evidence
                    .as_mut()
                    .expect("active acceptance session has evidence")
                    .record_error(
                        sequence,
                        request_id.clone(),
                        identity_range.finish(),
                        command_name,
                        &serialized_params,
                        timeout_millis,
                        request_duration,
                        &message,
                    );
                Err(AcceptanceSessionError::DiagnosticRequest {
                    request_id,
                    message,
                })
            }
        }
    }

    fn execute_wait_for_state(
        &mut self,
        sequence: usize,
        command_name: &str,
        arguments: crate::diagnostic_child_control::DiagnosticWaitForStateArguments,
        params: Value,
        timeout: Duration,
    ) -> Result<AcceptanceResponse, AcceptanceSessionError> {
        let serialized_params = serialize_params(&params)?;
        let request_started = Instant::now();
        let runtime_deadline = self.started_at + self.config.limits.runtime_timeout;
        let deadline = diagnostic_wait_deadline(
            request_started,
            arguments.timeout(),
            timeout,
            runtime_deadline,
        );
        let mut identity_range = evidence::ProtocolIdentityRangeBuilder::default();
        let result = execute_diagnostic_wait_for_state(&arguments, deadline, |limit, deadline| {
            let (request_id, result) = self
                .supervisor
                .request_with_id_retaining_observed_exit_until(
                    DiagnosticChildCommand::ReadUiState,
                    serde_json::json!({ "limit": limit }),
                    deadline,
                );
            identity_range.observe(request_id.as_deref());
            result
        });
        let request_duration = duration_millis(request_started.elapsed());
        let timeout_millis = duration_millis(timeout);
        match result {
            Ok(result) => {
                let request_id = identity_range.last_request_id().map(str::to_string);
                let serialized = serialize_response(&result, request_id.clone())?;
                self.evidence
                    .as_mut()
                    .expect("active acceptance session has evidence")
                    .record_success(
                        sequence,
                        request_id.clone(),
                        identity_range.finish(),
                        command_name,
                        &serialized_params,
                        timeout_millis,
                        request_duration,
                        &serialized,
                    );
                Ok(AcceptanceResponse { request_id, result })
            }
            Err(error) => {
                let request_id = identity_range.last_request_id().map(str::to_string);
                let message = error.to_string();
                self.evidence
                    .as_mut()
                    .expect("active acceptance session has evidence")
                    .record_error(
                        sequence,
                        request_id.clone(),
                        identity_range.finish(),
                        command_name,
                        &serialized_params,
                        timeout_millis,
                        request_duration,
                        &message,
                    );
                Err(AcceptanceSessionError::DiagnosticRequest {
                    request_id,
                    message,
                })
            }
        }
    }
}

fn serialize_params(params: &Value) -> Result<Vec<u8>, AcceptanceSessionError> {
    serde_json::to_vec(params).map_err(|error| {
        AcceptanceSessionError::InvalidConfiguration(format!(
            "could not serialize diagnostic request params: {error}"
        ))
    })
}

fn serialize_response(
    result: &Value,
    request_id: Option<String>,
) -> Result<Vec<u8>, AcceptanceSessionError> {
    serde_json::to_vec(result).map_err(|error| AcceptanceSessionError::DiagnosticRequest {
        request_id,
        message: format!("could not serialize response evidence: {error}"),
    })
}
