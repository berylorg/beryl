use beryl_home_store::RecordVersion;
use beryl_model::SyndicDraftId;
use sha2::{Digest, Sha256};

use crate::codec::parts::{Decoder, Encoder};
use crate::codec::{CodecError, ExactCodec, Family};

use super::super::{
    DraftEditorCandidateSessionIdV1, DraftPieceDigestV1, DraftPieceOperationIdV1, dec_position,
    dec_root_reference, enc_position, enc_root_reference,
};
use super::{records::*, references::*, witness::*};

pub(crate) struct DraftEditHistoryFrontiersFamily;
pub(crate) struct DraftEditHistoryTransitionsFamily;
pub(crate) type DraftEditHistoryFrontiersCodec = ExactCodec<DraftEditHistoryFrontiersFamily>;
pub(crate) type DraftEditHistoryTransitionsCodec = ExactCodec<DraftEditHistoryTransitionsFamily>;

impl Family for DraftEditHistoryFrontiersFamily {
    type Key = DraftEditHistoryFrontierKeyV1;
    type Value = DraftEditHistoryFrontierV1;
    const NAME: &'static str = "draft-edit-history-frontiers";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 33;
    const MAX_VALUE_BYTES: usize = 65_536;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_frontier_key(&mut e, *key);
        Ok(e.finish())
    }

    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = dec_frontier_key(&mut d)?;
        d.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        if !value.is_locally_valid() {
            return Err(CodecError::InvalidLength("draft edit-history frontier"));
        }
        Ok(encode_frontier_unchecked(value))
    }

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        decode_frontier(bytes)
    }
}

impl Family for DraftEditHistoryTransitionsFamily {
    type Key = DraftEditHistoryTransitionKeyV1;
    type Value = DraftEditHistoryTransitionV1;
    const NAME: &'static str = "draft-edit-history-transitions";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 40;
    const MAX_VALUE_BYTES: usize = 65_536;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        if key.cumulative_encoded_bytes() == 0 {
            return Err(CodecError::InvalidLength(
                "draft edit-history transition key",
            ));
        }
        let mut e = Encoder::new();
        enc_transition_key(&mut e, *key);
        Ok(e.finish())
    }

    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = dec_transition_key(&mut d)?;
        d.finish()?;
        if key.cumulative_encoded_bytes() == 0 {
            return Err(CodecError::InvalidLength(
                "draft edit-history transition key",
            ));
        }
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        if !value.is_locally_valid() {
            return Err(CodecError::InvalidLength("draft edit-history transition"));
        }
        Ok(encode_transition_unchecked(value))
    }

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        decode_transition(bytes)
    }
}

pub(super) fn authenticated_frontier(
    mut frontier: DraftEditHistoryFrontierV1,
) -> DraftEditHistoryFrontierV1 {
    frontier.reference.digest = frontier_digest(&frontier);
    frontier
}

pub(super) fn authenticated_transition(
    mut transition: DraftEditHistoryTransitionV1,
) -> DraftEditHistoryTransitionV1 {
    transition.digest = transition_digest(&transition);
    transition
}

fn hash_domain(domain: &[u8], parts: &[&[u8]]) -> DraftPieceDigestV1 {
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain);
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    DraftPieceDigestV1::from_bytes(hash.finalize().into())
}

pub(super) fn frontier_digest(frontier: &DraftEditHistoryFrontierV1) -> DraftPieceDigestV1 {
    let bytes = encode_frontier_preimage(frontier);
    hash_domain(b"syndic/draft-edit-history-frontier/v1", &[&bytes])
}

pub(super) fn transition_digest(transition: &DraftEditHistoryTransitionV1) -> DraftPieceDigestV1 {
    let bytes = encode_transition_preimage(transition);
    hash_domain(b"syndic/draft-edit-history-transition/v1", &[&bytes])
}

pub(super) fn enc_frontier_key(e: &mut Encoder, key: DraftEditHistoryFrontierKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    match key {
        DraftEditHistoryFrontierKeyV1::CanonicalEmpty { .. } => e.u8(0),
        DraftEditHistoryFrontierKeyV1::Session { session_id, .. } => {
            e.u8(1);
            e.fixed16(session_id.as_bytes());
        }
    }
}

fn dec_frontier_key(d: &mut Decoder<'_>) -> Result<DraftEditHistoryFrontierKeyV1, CodecError> {
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    match d.u8()? {
        0 => Ok(DraftEditHistoryFrontierKeyV1::canonical_empty(draft_id)),
        1 => Ok(DraftEditHistoryFrontierKeyV1::session(
            draft_id,
            DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft edit-history frontier key",
            tag,
        }),
    }
}

pub(super) fn enc_transition_key(e: &mut Encoder, key: DraftEditHistoryTransitionKeyV1) {
    e.fixed16(key.draft_id().as_bytes());
    e.u64(key.cumulative_encoded_bytes());
    e.fixed16(key.session_id().as_bytes());
}

fn dec_transition_key(d: &mut Decoder<'_>) -> Result<DraftEditHistoryTransitionKeyV1, CodecError> {
    let draft_id = SyndicDraftId::from_bytes(d.fixed16()?);
    let cumulative_encoded_bytes = d.u64()?;
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?);
    Ok(DraftEditHistoryTransitionKeyV1::new(
        draft_id,
        session_id,
        cumulative_encoded_bytes,
    ))
}

fn enc_transition_reference(e: &mut Encoder, reference: DraftEditHistoryTransitionReferenceV1) {
    enc_transition_key(e, reference.key());
    e.u64(reference.cumulative_encoded_bytes());
    e.u64(reference.journal_depth());
    e.fixed32(reference.digest().as_bytes());
}
fn dec_transition_reference(
    d: &mut Decoder<'_>,
) -> Result<DraftEditHistoryTransitionReferenceV1, CodecError> {
    Ok(DraftEditHistoryTransitionReferenceV1::new(
        dec_transition_key(d)?,
        d.u64()?,
        d.u64()?,
        DraftPieceDigestV1::from_bytes(d.fixed32()?),
    ))
}

fn enc_optional_transition_reference(
    e: &mut Encoder,
    value: Option<DraftEditHistoryTransitionReferenceV1>,
) {
    match value {
        None => e.u8(0),
        Some(value) => {
            e.u8(1);
            enc_transition_reference(e, value);
        }
    }
}

fn dec_optional_transition_reference(
    d: &mut Decoder<'_>,
) -> Result<Option<DraftEditHistoryTransitionReferenceV1>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(dec_transition_reference(d)?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft edit-history transition reference option",
            tag,
        }),
    }
}

pub(crate) fn enc_history_reference(e: &mut Encoder, value: DraftEditHistoryFrontierReferenceV1) {
    enc_frontier_key(e, value.key());
    e.u64(value.candidate_generation());
    enc_root_reference(e, value.root());
    e.u64(value.frontier_revision());
    e.u64(value.byte_budget());
    e.u64(value.retention_policy_revision());
    e.u8(u8::from(value.availability().undo_available()));
    e.u8(u8::from(value.availability().redo_available()));
    e.fixed32(value.digest().as_bytes());
}

pub(crate) fn canonical_history_reference_bytes(
    value: DraftEditHistoryFrontierReferenceV1,
) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_history_reference(&mut e, value);
    e.finish()
}

pub(crate) fn dec_history_reference(
    d: &mut Decoder<'_>,
) -> Result<DraftEditHistoryFrontierReferenceV1, CodecError> {
    let key = dec_frontier_key(d)?;
    let candidate_generation = d.u64()?;
    let root = dec_root_reference(d)?;
    let frontier_revision = d.u64()?;
    let byte_budget = d.u64()?;
    let retention_policy_revision = d.u64()?;
    let undo_available = dec_bool(d, "draft edit-history undo availability")?;
    let redo_available = dec_bool(d, "draft edit-history redo availability")?;
    let digest = DraftPieceDigestV1::from_bytes(d.fixed32()?);
    Ok(DraftEditHistoryFrontierReferenceV1::new(
        key,
        candidate_generation,
        root,
        frontier_revision,
        byte_budget,
        retention_policy_revision,
        DraftEditHistoryAvailabilityV1::new(undo_available, redo_available),
        digest,
    ))
}

fn enc_frontier_fields(e: &mut Encoder, frontier: &DraftEditHistoryFrontierV1) {
    let reference = frontier.reference();
    enc_frontier_key(e, reference.key());
    e.u64(reference.candidate_generation());
    enc_root_reference(e, reference.root());
    e.u64(reference.frontier_revision());
    e.u64(reference.byte_budget());
    e.u64(reference.retention_policy_revision());
    e.u8(u8::from(reference.availability().undo_available()));
    e.u8(u8::from(reference.availability().redo_available()));
    enc_optional_transition_reference(e, frontier.journal_head());
    enc_optional_transition_reference(e, frontier.undo_head());
    enc_optional_transition_reference(e, frontier.redo_head());
    enc_optional_transition_reference(e, frontier.oldest_eligible());
    e.u64(frontier.cumulative_encoded_bytes());
    e.u64(frontier.retained_encoded_bytes());
    e.u64(frontier.byte_budget());
    e.u64(frontier.retention_policy_revision());
}

fn encode_frontier_preimage(frontier: &DraftEditHistoryFrontierV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_frontier_fields(&mut e, frontier);
    e.finish()
}

pub(super) fn encode_frontier_unchecked(frontier: &DraftEditHistoryFrontierV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_frontier_fields(&mut e, frontier);
    e.fixed32(frontier.reference().digest().as_bytes());
    e.finish()
}

fn decode_frontier(bytes: &[u8]) -> Result<DraftEditHistoryFrontierV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_frontier_key(&mut d)?;
    let candidate_generation = d.u64()?;
    let root = dec_root_reference(&mut d)?;
    let frontier_revision = d.u64()?;
    let reference_byte_budget = d.u64()?;
    let reference_retention_policy_revision = d.u64()?;
    let availability = DraftEditHistoryAvailabilityV1::new(
        dec_bool(&mut d, "draft edit-history undo availability")?,
        dec_bool(&mut d, "draft edit-history redo availability")?,
    );
    let journal_head = dec_optional_transition_reference(&mut d)?;
    let undo_head = dec_optional_transition_reference(&mut d)?;
    let redo_head = dec_optional_transition_reference(&mut d)?;
    let oldest_eligible = dec_optional_transition_reference(&mut d)?;
    let cumulative_encoded_bytes = d.u64()?;
    let retained_encoded_bytes = d.u64()?;
    let byte_budget = d.u64()?;
    let retention_policy_revision = d.u64()?;
    let digest = DraftPieceDigestV1::from_bytes(d.fixed32()?);
    d.finish()?;
    let frontier = DraftEditHistoryFrontierV1::from_parts(
        DraftEditHistoryFrontierReferenceV1::new(
            key,
            candidate_generation,
            root,
            frontier_revision,
            reference_byte_budget,
            reference_retention_policy_revision,
            availability,
            digest,
        ),
        journal_head,
        undo_head,
        redo_head,
        oldest_eligible,
        cumulative_encoded_bytes,
        retained_encoded_bytes,
        byte_budget,
        retention_policy_revision,
    );
    if !frontier.is_locally_valid() {
        return Err(CodecError::InvalidLength("draft edit-history frontier"));
    }
    Ok(frontier)
}

pub(crate) fn enc_history_frontier(e: &mut Encoder, value: &DraftEditHistoryFrontierV1) {
    e.bytes(&encode_frontier_unchecked(value));
}

pub(crate) fn dec_history_frontier(
    d: &mut Decoder<'_>,
) -> Result<DraftEditHistoryFrontierV1, CodecError> {
    decode_frontier(d.bytes("draft edit-history frontier")?)
}

fn enc_transition_fields(e: &mut Encoder, transition: &DraftEditHistoryTransitionV1) {
    enc_transition_key(e, transition.key());
    enc_root_reference(e, transition.predecessor_root());
    enc_root_reference(e, transition.successor_root());
    enc_position(e, transition.before_caret());
    enc_position(e, transition.before_selection());
    enc_position(e, transition.after_caret());
    enc_position(e, transition.after_selection());
    e.u8(match transition.kind() {
        DraftEditHistoryTransitionKindV1::OrdinaryEdit => 0,
        DraftEditHistoryTransitionKindV1::Undo => 1,
        DraftEditHistoryTransitionKindV1::Redo => 2,
    });
    e.u64(transition.journal_depth());
    enc_optional_transition_reference(e, transition.prior_journal());
    enc_optional_transition_reference(e, transition.prior_undo());
    enc_optional_transition_reference(e, transition.prior_redo());
    e.u64(transition.cumulative_encoded_bytes());
    e.fixed16(transition.operation_id().as_bytes());
    e.u64(transition.ancestor_witness().bitmap());
    for ancestor in transition.ancestor_witness().slots() {
        enc_optional_transition_reference(e, *ancestor);
    }
}

fn encode_transition_preimage(transition: &DraftEditHistoryTransitionV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_transition_fields(&mut e, transition);
    e.finish()
}

pub(super) fn encode_transition_unchecked(transition: &DraftEditHistoryTransitionV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_transition_fields(&mut e, transition);
    e.fixed32(transition.digest().as_bytes());
    e.finish()
}

fn decode_transition(bytes: &[u8]) -> Result<DraftEditHistoryTransitionV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_transition_key(&mut d)?;
    let predecessor_root = dec_root_reference(&mut d)?;
    let successor_root = dec_root_reference(&mut d)?;
    let before_caret = dec_position(&mut d)?;
    let before_selection = dec_position(&mut d)?;
    let after_caret = dec_position(&mut d)?;
    let after_selection = dec_position(&mut d)?;
    let kind = match d.u8()? {
        0 => DraftEditHistoryTransitionKindV1::OrdinaryEdit,
        1 => DraftEditHistoryTransitionKindV1::Undo,
        2 => DraftEditHistoryTransitionKindV1::Redo,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft edit-history transition kind",
                tag,
            });
        }
    };
    let journal_depth = d.u64()?;
    let prior_journal = dec_optional_transition_reference(&mut d)?;
    let prior_undo = dec_optional_transition_reference(&mut d)?;
    let prior_redo = dec_optional_transition_reference(&mut d)?;
    let cumulative_encoded_bytes = d.u64()?;
    let operation_id = DraftPieceOperationIdV1::from_bytes(d.fixed16()?);
    let ancestor_bitmap = d.u64()?;
    let mut ancestor_slots = [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
    for slot in &mut ancestor_slots {
        *slot = dec_optional_transition_reference(&mut d)?;
    }
    let ancestor_witness =
        DraftEditHistoryAncestorWitnessV1::from_parts(ancestor_bitmap, ancestor_slots);
    let digest = DraftPieceDigestV1::from_bytes(d.fixed32()?);
    d.finish()?;
    let transition = DraftEditHistoryTransitionV1::from_parts(
        key,
        predecessor_root,
        successor_root,
        before_caret,
        before_selection,
        after_caret,
        after_selection,
        kind,
        journal_depth,
        prior_journal,
        prior_undo,
        prior_redo,
        operation_id,
        cumulative_encoded_bytes,
        ancestor_witness,
        digest,
    );
    if !transition.is_locally_valid() {
        return Err(CodecError::InvalidLength("draft edit-history transition"));
    }
    Ok(transition)
}

pub(crate) fn enc_history_transition(e: &mut Encoder, value: &DraftEditHistoryTransitionV1) {
    e.bytes(&encode_transition_unchecked(value));
}

pub(crate) fn dec_history_transition(
    d: &mut Decoder<'_>,
) -> Result<DraftEditHistoryTransitionV1, CodecError> {
    decode_transition(d.bytes("draft edit-history transition")?)
}

fn dec_bool(d: &mut Decoder<'_>, kind: &'static str) -> Result<bool, CodecError> {
    match d.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        tag => Err(CodecError::InvalidTag { kind, tag }),
    }
}

#[cfg(test)]
mod tests;
