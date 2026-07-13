use super::*;

impl BranchBootstrapBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TurnStartResponse, Self::Error> {
        ManagedBackendSession::start_turn_with_options(self, thread_id, text, options, timeout)
    }

    fn read_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, Self::Error> {
        ManagedBackendSession::read_thread_metadata(self, thread_id, timeout)
    }

    fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        ManagedBackendSession::next_turn_stream_event(self, idle_timeout)
    }

    fn deny_approval_request(&mut self, request: &ApprovalRequest) -> Result<(), Self::Error> {
        ManagedBackendSession::deny_approval_request(self, request)
    }

    fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), Self::Error> {
        ManagedBackendSession::respond_dynamic_tool_call(self, request, response)
    }
}
