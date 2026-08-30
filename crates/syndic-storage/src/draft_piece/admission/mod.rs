mod codec;
mod model;
mod readiness_source;
mod tree;

pub use model::*;
pub use readiness_source::*;
pub use tree::*;

pub(crate) use codec::{
    DraftMarkerAdmissionCapacityCodec, DraftMarkerAdmissionCapacityFamily,
    DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
    DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionNodesFamily,
    DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReceiptsFamily,
    encoded_head_record_charge, encoded_node_record_charge, encoded_receipt_record_charge,
};

pub const DRAFT_MARKER_ADMISSION_MAX_HEADS: u64 = 64;
pub const DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS: u64 = 65_536;
pub const DRAFT_MARKER_ADMISSION_MAX_ENCODED_BYTES: u64 = 67_108_864;
pub const DRAFT_MARKER_ADMISSION_TREE_FANOUT: usize = 128;
pub const DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT: u8 = 64;
pub const DRAFT_MARKER_ADMISSION_PAGE_MAX_ASSOCIATIONS: u64 = 256;
pub const DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES: u64 = 4_194_304;
pub const DRAFT_MARKER_ADMISSION_MAX_EVIDENCE_BYTES: usize = 65_536;

#[cfg(feature = "test-faults")]
pub use codec::{
    DraftMarkerAdmissionCodecFixtureV1, draft_marker_admission_codec_accepts,
    draft_marker_admission_corrupted_value_rejected,
};
