use std::time::Duration;

use beryl_backend::ApprovalRequest;
use tracing::warn;

use super::TurnStreamBackend;

pub(super) fn deny_backend_approval_request<B>(
    backend: &mut B,
    request: &ApprovalRequest,
    request_timeout: Duration,
) -> Result<(), String>
where
    B: TurnStreamBackend,
{
    warn!(
        approval = %request.summary(),
        approval_payload = %request.pretty_params(),
        "auto-denying unsupported backend approval request"
    );
    backend
        .deny_approval_request(request)
        .map_err(|error| format!("Beryl could not deny the backend approval request: {error}"))?;

    if request.kind().denial_response_interrupts_turn() {
        return Ok(());
    }

    let Some(thread_id) = request.thread_id() else {
        return Err(
            "Beryl denied a backend approval request but could not interrupt the turn because the request did not include a thread id."
                .to_string(),
        );
    };
    let Some(turn_id) = request.turn_id() else {
        return Err(
            "Beryl denied a backend approval request but could not interrupt the turn because the request did not include a turn id."
                .to_string(),
        );
    };

    backend
        .interrupt_turn(thread_id, turn_id, request_timeout)
        .map_err(|error| {
            format!("Beryl denied the backend approval request but could not interrupt the turn: {error}")
        })
}
