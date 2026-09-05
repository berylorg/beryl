use super::*;
use beryl_backend::{
    ApprovalRequest, CompactionOperation, ManagedBackendClientConnector, ManagedBackendError,
    ManagedBackendSession, ThreadReadOptions,
};
use beryl_model::workspace::WorkspaceId;

/// Only explicit JSON-RPC rejection proves that a dispatched start was rejected.
pub(crate) fn classify_start_error(error: ManagedBackendError) -> PortError {
    match error {
        ManagedBackendError::RequestFailed { method, error }
            if method == "thread/compact/start" =>
        {
            PortError::Rejected(bounded(error.message))
        }
        _ => PortError::Unavailable,
    }
}

pub(super) struct ManagedPort {
    pub(super) connector: ManagedBackendClientConnector,
    pub(super) execution_target: WorkspaceId,
    pub(super) session: Option<ManagedBackendSession>,
    pub(super) operation: Option<CompactionOperation>,
}

impl ManagedPort {
    fn session(&mut self) -> Result<&mut ManagedBackendSession, PortError> {
        self.session.as_mut().ok_or(PortError::Unavailable)
    }
}

impl BackendPort for ManagedPort {
    fn connect(&mut self, timeout: Duration) -> Result<String, PortError> {
        if !connector_target_matches(self.connector.launch_spec(), &self.execution_target) {
            return Err(PortError::Rejected(
                "Compaction execution target does not match its backend connector.".into(),
            ));
        }
        let session = self
            .connector
            .connect_client(timeout)
            .map_err(|_| PortError::Unavailable)?;
        let id = session
            .compaction_observation()
            .session_id()
            .map_err(|error| PortError::Rejected(error.to_string()))?
            .to_string();
        self.session = Some(session);
        Ok(id)
    }
    fn disconnect(&mut self) {
        self.session = None;
    }
    fn prepare(&mut self, thread_id: &str) -> Result<OperationIdentity, PortError> {
        let operation = self
            .session()?
            .prepare_compaction(thread_id)
            .map_err(|error| PortError::Rejected(bounded(error.to_string())))?;
        let identity = OperationIdentity {
            thread_id: operation.thread_id().to_string(),
            operation_id: operation.operation_id().to_string(),
            observation_session_id: operation.observation_session_id().to_string(),
        };
        self.operation = Some(operation);
        Ok(identity)
    }
    fn subscribe(&mut self, thread_id: &str, timeout: Duration) -> Result<(), PortError> {
        let response = self
            .session()?
            .resume_thread_metadata(thread_id, timeout)
            .map_err(|_| PortError::Unavailable)?;
        if response.thread.summary().id != thread_id {
            return Err(PortError::Unavailable);
        }
        Ok(())
    }
    fn start(&mut self, timeout: Duration) -> Result<Receipt, PortError> {
        let operation = self.operation.clone().ok_or(PortError::Unavailable)?;
        self.session()?
            .start_compaction(&operation, timeout)
            .map(Into::into)
            .map_err(classify_start_error)
    }
    fn read(&mut self, turn_id: Option<&str>, timeout: Duration) -> Result<Receipt, PortError> {
        let operation = self.operation.clone().ok_or(PortError::Unavailable)?;
        self.session()?
            .read_compaction(&operation, turn_id, timeout)
            .map(Into::into)
            .map_err(|_| PortError::Unavailable)
    }
    fn status(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<(String, ThreadStatus), PortError> {
        let response = self
            .session()?
            .read_thread(thread_id, ThreadReadOptions::metadata_only(), timeout)
            .map_err(|_| PortError::Unavailable)?;
        Ok((response.thread.summary().id, response.thread.status))
    }
    fn poll(&mut self, timeout: Duration) -> Result<Option<TurnStreamEvent>, PortError> {
        self.session()?
            .next_turn_stream_event(timeout)
            .map_err(|_| PortError::Unavailable)
    }
    fn deny_approval(
        &mut self,
        request: &ApprovalRequest,
        thread_id: &str,
        turn_id: Option<&str>,
        timeout: Duration,
    ) -> Result<(), PortError> {
        let session = self.session()?;
        session
            .deny_approval_request_with_timeout(request, timeout)
            .map_err(|_| PortError::Unavailable)?;
        if !request.kind().denial_response_interrupts_turn()
            && request.thread_id() == Some(thread_id)
            && request.turn_id().is_some()
            && request.turn_id() == turn_id
        {
            session
                .interrupt_turn(thread_id, request.turn_id().unwrap_or_default(), timeout)
                .map_err(|_| PortError::Unavailable)?;
        }
        Ok(())
    }
}

pub(crate) fn connector_target_matches(
    launch: &beryl_backend::BackendLaunchSpec,
    target: &WorkspaceId,
) -> bool {
    launch.runtime_mode() == target.runtime_mode() && launch.cwd() == target.canonical_path()
}
