use beryl_model::CasThreadId;

use super::router::LiveEventTargetCloseReason;

#[derive(Clone, Debug)]
pub(in crate::cas_projection) enum ConnectionRoutingFailure {
    Backend,
    Router,
    Target {
        thread_id: CasThreadId,
        reason: LiveEventTargetCloseReason,
    },
}

#[derive(Debug)]
pub(in crate::cas_projection) struct ConnectionCommandOutcome<T> {
    operation: T,
    routing_failure: Option<ConnectionRoutingFailure>,
}

impl<T> ConnectionCommandOutcome<T> {
    pub(super) const fn new(
        operation: T,
        routing_failure: Option<ConnectionRoutingFailure>,
    ) -> Self {
        Self {
            operation,
            routing_failure,
        }
    }

    pub(in crate::cas_projection) fn into_parts(self) -> (T, Option<ConnectionRoutingFailure>) {
        (self.operation, self.routing_failure)
    }

    pub(in crate::cas_projection) const fn operation(&self) -> &T {
        &self.operation
    }

    pub(super) fn record_routing_failure(&mut self, failure: ConnectionRoutingFailure) {
        if self.routing_failure.is_none() {
            self.routing_failure = Some(failure);
        }
    }
}
