use beryl_backend::{
    ApprovalOperationCompletion, ApprovalRequest, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSubmitCause,
};

use super::{BrokerReply, Ingester, WholeConnectionRoutingFailure};
use crate::cas_projection::{
    connection::{
        registry,
        router::{ApprovalRouteOutcome, PreparedApprovalInterruption},
    },
    stop::StopOwnership,
};

impl Ingester {
    pub(super) fn route_approval(&self, request: ApprovalRequest) -> (BrokerReply, bool) {
        let needs_interruption = request.kind().separate_interruption_required();
        #[cfg(feature = "test-faults")]
        let approval_thread = request.thread_id().cloned();
        #[cfg(feature = "test-faults")]
        if needs_interruption && let Some(thread_id) = approval_thread.as_ref() {
            crate::cas_projection::test_faults::pause_approval_slot_admission(thread_id);
        }
        if needs_interruption && !self.approval.reserve() {
            self.retire_for(
                WholeConnectionRoutingFailure::Router,
                crate::cas_projection::connection::router::LiveEventTargetCloseReason::StreamFailure,
            );
            return (
                BrokerReply::Rejected(
                    OrderedTurnStreamOperation::Approval(request),
                    OrderedTurnStreamSubmitCause::CapacityFull,
                ),
                true,
            );
        }
        let prepared = if needs_interruption {
            let proof = match self.router.approval_stop_target(&request) {
                Ok(proof) => proof,
                Err(_) => {
                    self.approval.cancel_reservation();
                    self.retire();
                    return (
                        BrokerReply::Rejected(
                            OrderedTurnStreamOperation::Approval(request),
                            OrderedTurnStreamSubmitCause::Unavailable,
                        ),
                        true,
                    );
                }
            };
            match self.stop_coordinator.coordinate(
                &self.router,
                proof,
                syndic_storage::StopCause::InterruptingApproval,
            ) {
                Ok(StopOwnership::Primary(owner)) => Some(PreparedApprovalInterruption::new(
                    owner.interruption(),
                    Some(owner),
                )),
                Ok(StopOwnership::Joined {
                    interruption,
                    operation_id: _,
                }) => Some(PreparedApprovalInterruption::new(interruption, None)),
                Err(_) => {
                    self.approval.cancel_reservation();
                    self.retire();
                    return (
                        BrokerReply::Rejected(
                            OrderedTurnStreamOperation::Approval(request),
                            OrderedTurnStreamSubmitCause::Unavailable,
                        ),
                        true,
                    );
                }
            }
        } else {
            None
        };
        let outcome = self
            .router
            .route_approval(self.live_command(), request, prepared);
        #[cfg(feature = "test-faults")]
        if needs_interruption && let Some(thread_id) = approval_thread.as_ref() {
            crate::cas_projection::test_faults::pause_approval_install(
                thread_id,
                std::sync::Arc::as_ptr(&self.approval) as usize,
            );
        }
        match outcome {
            ApprovalRouteOutcome::Routed {
                interruption,
                obligation,
            } => {
                let terminal = install_obligation(self, needs_interruption, obligation);
                (
                    BrokerReply::Applied(OrderedTurnStreamCompletion::Approval(
                        ApprovalOperationCompletion::Routed { interruption },
                    )),
                    terminal,
                )
            }
            ApprovalRouteOutcome::TargetFailed {
                request,
                cause,
                target,
                obligation,
            } => {
                if registry::invalidate_exact_generation(
                    &target.key,
                    self.authority.generation,
                    target.owner,
                    target.loaded_generation,
                )
                .is_err()
                {
                    self.approval.cancel_reservation();
                    self.retire();
                    return (
                        BrokerReply::Rejected(
                            OrderedTurnStreamOperation::Approval(request),
                            OrderedTurnStreamSubmitCause::Unavailable,
                        ),
                        true,
                    );
                }
                let terminal = install_obligation(self, needs_interruption, obligation);
                (
                    BrokerReply::Applied(OrderedTurnStreamCompletion::Approval(
                        ApprovalOperationCompletion::TargetFailed { request, cause },
                    )),
                    terminal,
                )
            }
            ApprovalRouteOutcome::Rejected {
                request,
                cause,
                reason,
            } => {
                self.approval.cancel_reservation();
                self.retire_for(WholeConnectionRoutingFailure::Router, reason);
                (
                    BrokerReply::Rejected(OrderedTurnStreamOperation::Approval(request), cause),
                    true,
                )
            }
        }
    }
}

fn install_obligation(
    ingester: &Ingester,
    needs_interruption: bool,
    obligation: Option<crate::cas_projection::connection::router::ApprovalInterruptionObligation>,
) -> bool {
    match (needs_interruption, obligation) {
        (true, Some(obligation)) => !ingester.approval.install(obligation),
        (false, None) => false,
        (true, None) | (false, Some(_)) => {
            unreachable!("approval route and interruption admission must agree")
        }
    }
}
