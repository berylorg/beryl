use super::*;
use beryl_model::{DiscussionContextDigest, JobId, ResolutionIntentId, SyndicItemId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscussionHandoffReceipt {
    pub parent_thread_revision: ThreadRevision,
    pub parent_gate_revision: InputGateRevision,
    pub child_thread_id: SyndicThreadId,
    pub intent_id: ResolutionIntentId,
    pub job_id: JobId,
    pub context_owner: DiscussionContextOwnerId,
    pub context_digest: DiscussionContextDigest,
    pub resolving_turn_id: SyndicTurnId,
    pub resolution_digest: [u8; 32],
    pub parent_turn_id: SyndicTurnId,
    pub canonical_item_id: SyndicItemId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedInputSource {
    Composer {
        admission: AcceptedInputAdmissionProof,
        route_generation: AcceptedRouteGeneration,
    },
    DiscussionHandoff(DiscussionHandoffReceipt),
}

impl AcceptedInputSource {
    pub const fn thread_revision(self) -> ThreadRevision {
        match self {
            Self::Composer { admission, .. } => admission.expected_thread_revision(),
            Self::DiscussionHandoff(receipt) => receipt.parent_thread_revision,
        }
    }
    pub const fn gate_revision(self) -> InputGateRevision {
        match self {
            Self::Composer { admission, .. } => admission.expected_gate_revision(),
            Self::DiscussionHandoff(receipt) => receipt.parent_gate_revision,
        }
    }
    pub const fn order_source(self) -> crate::AcceptedOrderSource {
        match self {
            Self::Composer {
                route_generation, ..
            } => crate::AcceptedOrderSource::Composer(route_generation),
            Self::DiscussionHandoff(_) => crate::AcceptedOrderSource::DiscussionHandoff,
        }
    }
}
