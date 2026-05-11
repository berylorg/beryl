use beryl_backend::{DynamicToolCallRequest, DynamicToolCallResponse};
use serde_json::{Value, json};
use tracing::warn;

use crate::{LifecycleYieldOutcome, YIELD_TOOL};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HandledDynamicToolCall {
    response: DynamicToolCallResponse,
    lifecycle_yield: Option<LifecycleYieldOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcceptedLifecycleYield {
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) outcome: LifecycleYieldOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveTurnDynamicToolCallResult {
    response: DynamicToolCallResponse,
    accepted_lifecycle_yield: Option<AcceptedLifecycleYield>,
}

#[derive(Default)]
pub(crate) struct ActiveTurnLifecycleYieldCapture {
    accepted: Option<AcceptedLifecycleYield>,
}

impl HandledDynamicToolCall {
    pub(crate) fn new(
        response: DynamicToolCallResponse,
        lifecycle_yield: Option<LifecycleYieldOutcome>,
    ) -> Self {
        Self {
            response,
            lifecycle_yield,
        }
    }

    pub(crate) fn lifecycle_yield(&self) -> Option<LifecycleYieldOutcome> {
        self.lifecycle_yield
    }

    pub(crate) fn into_response(self) -> DynamicToolCallResponse {
        self.response
    }
}

impl From<DynamicToolCallResponse> for HandledDynamicToolCall {
    fn from(response: DynamicToolCallResponse) -> Self {
        Self::new(response, None)
    }
}

impl ActiveTurnDynamicToolCallResult {
    pub(crate) fn new(
        response: DynamicToolCallResponse,
        accepted_lifecycle_yield: Option<AcceptedLifecycleYield>,
    ) -> Self {
        Self {
            response,
            accepted_lifecycle_yield,
        }
    }

    pub(crate) fn into_parts(self) -> (DynamicToolCallResponse, Option<AcceptedLifecycleYield>) {
        (self.response, self.accepted_lifecycle_yield)
    }
}

impl ActiveTurnLifecycleYieldCapture {
    pub(crate) fn handle_dynamic_tool_call(
        &mut self,
        active_thread_id: &str,
        active_turn_id: &str,
        request: &DynamicToolCallRequest,
        handled: HandledDynamicToolCall,
    ) -> ActiveTurnDynamicToolCallResult {
        let Some(outcome) = handled.lifecycle_yield() else {
            return ActiveTurnDynamicToolCallResult::new(handled.into_response(), None);
        };

        if request.thread_id() != active_thread_id || request.turn_id() != active_turn_id {
            warn!(
                tool_call = %request.summary(),
                active_thread_id,
                active_turn_id,
                "ignoring lifecycle yield for a non-active turn"
            );
            return ActiveTurnDynamicToolCallResult::new(
                uncorrelated_lifecycle_yield_response(request, active_thread_id, active_turn_id),
                None,
            );
        }

        if self.accepted.is_some() {
            warn!(
                tool_call = %request.summary(),
                "ignoring duplicate lifecycle yield for active turn"
            );
            return ActiveTurnDynamicToolCallResult::new(handled.into_response(), None);
        }

        let accepted = AcceptedLifecycleYield {
            thread_id: request.thread_id().to_string(),
            turn_id: request.turn_id().to_string(),
            outcome,
        };
        self.accepted = Some(accepted.clone());
        ActiveTurnDynamicToolCallResult::new(handled.into_response(), Some(accepted))
    }
}

fn uncorrelated_lifecycle_yield_response(
    request: &DynamicToolCallRequest,
    active_thread_id: &str,
    active_turn_id: &str,
) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(compact_json(json!({
        "ok": false,
        "error": {
            "kind": "uncorrelated_lifecycle_yield",
            "message": "Beryl ignored the lifecycle yield because it did not target the active streamed parent turn.",
            "tool": YIELD_TOOL,
            "callId": request.call_id(),
            "threadId": request.thread_id(),
            "turnId": request.turn_id(),
            "activeThreadId": active_thread_id,
            "activeTurnId": active_turn_id,
        }
    })))
}

fn compact_json(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":{\"kind\":\"internal\",\"message\":\"could not serialize dynamic tool response\"}}"
            .to_string()
    })
}
