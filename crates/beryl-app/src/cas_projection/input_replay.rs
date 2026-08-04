mod accepted;
mod authority;
#[cfg(feature = "test-faults")]
mod diagnostics;
mod error;
mod marker;
mod page;
mod prepared;
mod record;

#[allow(unused_imports, reason = "Phase 52 mounts this prepared app boundary")]
pub(in crate::cas_projection) use accepted::{
    AcceptedInputReplayContext, AcceptedInputReplayError, AcceptedInputReplayFactory,
    AcceptedInputReplaySource, AcceptedInputSteeringCorrelationError,
    decode_accepted_input_steering_correlation, encode_accepted_input_steering_correlation,
};
pub(in crate::cas_projection) use authority::{InputReplayAuthority, InputReplayFactory};
#[cfg(feature = "test-faults")]
pub use diagnostics::{
    OrdinaryInputReplayDiagnostics, OrdinaryInputReplayDiagnosticsSnapshot,
    SourcePageHandoffBarrierController,
};
pub(in crate::cas_projection) use error::InputReplayPrepareError;
pub(in crate::cas_projection) use prepared::check_cancelled;
pub(in crate::cas_projection) use record::{InputReplayContext, InputReplayRecord};

use syndic_storage::SyndicPointReadLimit;

const POINT_READ_BYTES: usize = 2 * 1024 * 1024;

pub(in crate::cas_projection) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_BYTES).expect("input replay point-read bound is nonzero")
}
