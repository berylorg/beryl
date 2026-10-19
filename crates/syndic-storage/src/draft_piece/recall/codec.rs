use super::super::import::model::SealedComposerBasisV1;
use super::super::*;
use super::*;
use crate::codec::{CodecError, parts::*};

pub(crate) fn enc_recall_basis(e: &mut Encoder, basis: &DraftComposerRecallBasisV1) {
    super::super::codec::enc_session_key(e, basis.key);
    e.fixed32(&basis.acceptance_digest);
    enc_thread(e, basis.thread);
    enc_content_ref(e, basis.content);
    enc_opt(e, basis.assets, enc_sealed_asset_reference_set_proof);
}
pub(crate) fn dec_recall_basis(
    d: &mut Decoder<'_>,
) -> Result<DraftComposerRecallBasisV1, CodecError> {
    let basis = DraftComposerRecallBasisV1 {
        key: super::super::codec::dec_session_key(d)?,
        acceptance_digest: d.fixed32()?,
        thread: dec_thread(d)?,
        content: dec_content_ref(d)?,
        assets: dec_opt(d, "recall assets", dec_sealed_asset_reference_set_proof)?,
    };
    if !basis.is_locally_exact() {
        return Err(CodecError::InvalidLength("recall source basis"));
    }
    Ok(basis)
}
pub(crate) fn enc_recall_frontier(e: &mut Encoder, frontier: &DraftComposerRecallFrontierV1) {
    enc_recall_basis(e, &frontier.basis);
    super::super::import::codec::enc_sealed_stream_frontier(e, frontier);
}
pub(crate) fn dec_recall_frontier(
    d: &mut Decoder<'_>,
) -> Result<DraftComposerRecallFrontierV1, CodecError> {
    let basis = dec_recall_basis(d)?;
    super::super::import::codec::dec_sealed_stream_frontier(d, basis)
}
pub(crate) fn enc_replacement_purpose(
    e: &mut Encoder,
    purpose: &DraftComposerRootReplacementPurposeV1,
) {
    match purpose {
        DraftComposerRootReplacementPurposeV1::AcceptedRecall(basis) => {
            e.u8(0);
            enc_recall_basis(e, basis);
        }
        DraftComposerRootReplacementPurposeV1::CheckpointRestore {
            activation,
            caret,
            selection,
        } => {
            e.u8(1);
            enc_recall_activation(e, *activation);
            enc_position(e, *caret);
            enc_position(e, *selection);
        }
    }
}
pub(crate) fn dec_replacement_purpose(
    d: &mut Decoder<'_>,
) -> Result<DraftComposerRootReplacementPurposeV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftComposerRootReplacementPurposeV1::AcceptedRecall(
            dec_recall_basis(d)?,
        )),
        1 => Ok(DraftComposerRootReplacementPurposeV1::CheckpointRestore {
            activation: dec_recall_activation(d)?,
            caret: dec_position(d)?,
            selection: dec_position(d)?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "ordinary root replacement purpose",
            tag,
        }),
    }
}
pub(crate) fn canonical_recall_header(
    header: DraftPieceEditHeaderV1,
    generation: u64,
    purpose: &DraftComposerRootReplacementPurposeV1,
) -> Vec<u8> {
    let mut bytes = canonical_edit_command_bytes(header, generation);
    let mut e = Encoder::new();
    match purpose {
        DraftComposerRootReplacementPurposeV1::AcceptedRecall(basis) => {
            e.u8(2);
            e.bytes(b"syndic/draft-composer-recall/v1");
            enc_recall_basis(&mut e, basis);
        }
        DraftComposerRootReplacementPurposeV1::CheckpointRestore {
            activation,
            caret,
            selection,
        } => {
            e.u8(3);
            e.bytes(b"syndic/draft-composer-checkpoint-restore/v1");
            enc_recall_activation(&mut e, *activation);
            enc_position(&mut e, *caret);
            enc_position(&mut e, *selection);
        }
    }
    bytes.extend_from_slice(&e.finish());
    bytes
}
pub(crate) fn recall_header_generation(build: &DraftPieceBuildRecordV1) -> Option<u64> {
    let DraftPieceBuildFrontierV1::OrdinaryRecall(f) = build.frontier() else {
        return None;
    };
    let h = DraftPieceEditHeaderV1::new(
        build.draft_id(),
        build.session_id(),
        build.predecessor_candidate_generation(),
        build.predecessor_root(),
        build.predecessor_history(),
        build.operation_id(),
        build.predecessor_caret(),
        build.predecessor_selection(),
        build.caret(),
        build.selection(),
        0,
        canonical_empty_draft_piece_fragment_chain_v1(),
    );
    let prefix = canonical_edit_header_bytes(h);
    let generation = u64::from_be_bytes(
        build
            .canonical_header()
            .get(prefix.len()..prefix.len() + 8)?
            .try_into()
            .ok()?,
    );
    (generation != 0
        && canonical_recall_header(
            h,
            generation,
            &DraftComposerRootReplacementPurposeV1::AcceptedRecall(f.basis),
        ) == build.canonical_header())
    .then_some(generation)
}
pub(crate) fn header_has_restore_suffix(bytes: &[u8], header: DraftPieceEditHeaderV1) -> bool {
    bytes.get(canonical_edit_header_bytes(header).len() + 8) == Some(&3)
}
