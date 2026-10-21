mod capture;
pub(crate) mod codec;
mod command;
mod extraction;
mod read;
mod source;
pub(crate) mod validation;

pub use read::{GeneratedMediaPendingEntry, GeneratedMediaPendingPage, GeneratedMediaSweep};
pub use source::{GeneratedMediaReadError, GeneratedMediaSourceProof, GeneratedMediaSourceState};

use beryl_model::{
    GeneratedSourceIncarnation, SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceRecord {
    sequence: u64,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    snapshot: SyndicExecutionSnapshotId,
    initial_event: u64,
    source_incarnation: Option<GeneratedSourceIncarnation>,
    completion: Option<(u64, [u8; 32])>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceValue {
    Allocator(u64),
    Source(SourceRecord),
    Owner(GeneratedSourceIncarnation),
}

pub(crate) use capture::CaptureEffect;
