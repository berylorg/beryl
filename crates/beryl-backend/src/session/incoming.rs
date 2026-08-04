use crate::{ApprovalRequest, turn::ApprovalResponder};

pub(super) enum IncomingMessage {
    Approval {
        request: ApprovalRequest,
        responder: ApprovalResponder,
    },
}

pub(super) enum ReceiveOutcome {
    Quiet,
    Message(IncomingMessage),
    OrderedProgress,
    Response(crate::BoundedResponseResult),
    Rejection(crate::JsonRpcError),
}

impl IncomingMessage {
    pub(super) fn bind_approval_response_authority(
        &self,
        generation: u64,
    ) -> Result<(), crate::ApprovalRequestSchemaError> {
        let Self::Approval { responder, .. } = self;
        responder.bind_response_authority(generation)
    }

    pub(super) const fn approval_parts(&self) -> (&ApprovalRequest, &ApprovalResponder) {
        let Self::Approval { request, responder } = self;
        (request, responder)
    }
}
