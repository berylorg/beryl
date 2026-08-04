mod authentication;
mod observation;
mod reconciliation;

pub(in crate::read) use authentication::{
    observation_authenticates_record, steerable_target_matches,
};
pub(in crate::read) use observation::StopObservation;

/// Fixed-work reconciliation result for one exact durable stop transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopOperationTransitionStatus {
    /// The exact source authority still admits the requested transition.
    Prior,
    /// The retained operation receipt proves that the requested transition committed.
    Exact,
    /// Durable state proves neither the exact source nor the exact successor.
    Collision,
}
