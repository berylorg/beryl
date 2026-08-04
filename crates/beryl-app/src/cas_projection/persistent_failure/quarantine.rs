//! Inert ownership normalization for one sealed persistent-failure inventory.

mod conversion;
mod disposition;
mod model;

pub use model::{
    PersistentFailurePendingProjectionQuarantine,
    PersistentFailurePendingProjectionQuarantineError,
    PersistentFailurePendingProjectionQuarantineMetadata,
    PersistentFailurePendingProjectionQuarantineReason,
};

pub(in crate::cas_projection) use model::{
    PendingProjectionAdoptionTopology, PendingProjectionCandidateGroup,
    PendingProjectionGroupIdentity, PendingProjectionWitness,
};

pub(in crate::cas_projection::persistent_failure) use model::{
    PendingProjectionQuarantineAuthority, PendingProjectionQuarantineOwnedTopology,
};
