use crate::codec::{CodecError, parts::*};
use crate::{CurrentTranscriptEntryProof, ReplacementEditIntent};

use super::super::{
    codec::{dec_current_selector, enc_current_selector},
    *,
};
use super::model::*;

pub(super) const IMPORT_HEADER_DOMAIN: &[u8] = b"syndic/draft-sealed-composer-import/v1";

pub(crate) fn enc_import_basis(e: &mut Encoder, basis: DraftSealedImportBasisV1) {
    enc_current_selector(e, basis.selector);
    enc_input_gate_rev(e, basis.gate_revision);
    enc_turn(e, basis.intent.target_turn_id());
    enc_selected_path(e, basis.intent.selected_path());
    enc_transcript_generation(e, basis.intent.transcript_entry().generation());
    enc_transcript_pos(e, basis.intent.transcript_entry().position());
    enc_item(e, basis.item_id);
    enc_projection_rev(e, basis.item_revision);
    enc_content_ref(e, basis.content);
    enc_opt(e, basis.assets, enc_sealed_asset_reference_set_proof);
}

pub(crate) fn dec_import_basis(
    d: &mut Decoder<'_>,
) -> Result<DraftSealedImportBasisV1, CodecError> {
    let basis = DraftSealedImportBasisV1 {
        selector: dec_current_selector(d)?,
        gate_revision: dec_input_gate_rev(d)?,
        intent: ReplacementEditIntent::new(
            dec_turn(d)?,
            dec_selected_path(d)?,
            CurrentTranscriptEntryProof::new(dec_transcript_generation(d)?, dec_transcript_pos(d)?),
        ),
        item_id: dec_item(d)?,
        item_revision: dec_projection_rev(d)?,
        content: dec_content_ref(d)?,
        assets: dec_opt(
            d,
            "import historical asset proof",
            dec_sealed_asset_reference_set_proof,
        )?,
    };
    if !basis.is_locally_exact() {
        return Err(CodecError::InvalidLength("sealed import basis"));
    }
    Ok(basis)
}

pub(crate) fn enc_import_frontier(e: &mut Encoder, value: DraftSealedImportFrontierV1) {
    enc_import_basis(e, value.basis);
    e.u8(match value.phase {
        DraftSealedImportPhaseV1::Streaming => 0,
        DraftSealedImportPhaseV1::CrossValidating => 1,
        DraftSealedImportPhaseV1::Complete => 2,
    });
    e.u64(value.encoded_cursor);
    e.u64(value.atoms);
    e.u64(value.logical_bytes);
    e.u64(value.markers);
    e.u8(match value.decoder {
        DraftSealedImportDecoderV1::BeforeHeader => 0,
        DraftSealedImportDecoderV1::BetweenAtoms => 1,
        DraftSealedImportDecoderV1::TextBody => 2,
        DraftSealedImportDecoderV1::End => 3,
    });
    e.u64(value.text_remaining);
    e.u64(value.next_piece);
    e.u64(value.piece_text_offset);
    e.u64(value.next_leaf_rank);
    e.u64(value.checked_markers);
    e.fixed32(&value.sequential_digest);
    enc_opt(e, value.maximum_label, enc_image_label);
    e.fixed32(&value.ordered_digest);
}

pub(crate) fn dec_import_frontier(
    d: &mut Decoder<'_>,
) -> Result<DraftSealedImportFrontierV1, CodecError> {
    let basis = dec_import_basis(d)?;
    let phase = match d.u8()? {
        0 => DraftSealedImportPhaseV1::Streaming,
        1 => DraftSealedImportPhaseV1::CrossValidating,
        2 => DraftSealedImportPhaseV1::Complete,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "sealed import phase",
                tag,
            });
        }
    };
    let encoded_cursor = d.u64()?;
    let atoms = d.u64()?;
    let logical_bytes = d.u64()?;
    let markers = d.u64()?;
    let decoder = match d.u8()? {
        0 => DraftSealedImportDecoderV1::BeforeHeader,
        1 => DraftSealedImportDecoderV1::BetweenAtoms,
        2 => DraftSealedImportDecoderV1::TextBody,
        3 => DraftSealedImportDecoderV1::End,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "sealed import decoder",
                tag,
            });
        }
    };
    Ok(DraftSealedImportFrontierV1 {
        basis,
        phase,
        encoded_cursor,
        atoms,
        logical_bytes,
        markers,
        decoder,
        text_remaining: d.u64()?,
        next_piece: d.u64()?,
        piece_text_offset: d.u64()?,
        next_leaf_rank: d.u64()?,
        checked_markers: d.u64()?,
        sequential_digest: d.fixed32()?,
        maximum_label: dec_opt(d, "sealed import maximum label", dec_image_label)?,
        ordered_digest: d.fixed32()?,
    })
}

pub(crate) fn canonical_import_header_bytes(
    header: DraftPieceEditHeaderV1,
    source_generation: u64,
    basis: DraftSealedImportBasisV1,
) -> Vec<u8> {
    let mut bytes = canonical_edit_command_bytes(header, source_generation);
    let mut e = Encoder::new();
    e.u8(1);
    e.bytes(IMPORT_HEADER_DOMAIN);
    enc_import_basis(&mut e, basis);
    bytes.extend_from_slice(&e.finish());
    bytes
}

pub(crate) fn import_header_generation(build: &DraftPieceBuildRecordV1) -> Option<u64> {
    let DraftPieceBuildFrontierV1::SealedImport(frontier) = build.frontier() else {
        return None;
    };
    let header = DraftPieceEditHeaderV1::new(
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
        build.fragment_count(),
        build.fragment_chain(),
    );
    let prefix = canonical_edit_header_bytes(header);
    let generation = u64::from_be_bytes(
        build
            .canonical_header()
            .get(prefix.len()..prefix.len() + 8)?
            .try_into()
            .ok()?,
    );
    (generation != 0
        && canonical_import_header_bytes(header, generation, frontier.basis)
            == build.canonical_header())
    .then_some(generation)
}
