use super::*;

pub(super) fn encode_thread_record(value: &ThreadRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.id());
    enc_thread_rev(&mut e, value.revision());
    enc_opt(&mut e, value.committed_tail(), enc_turn);
    enc_draft(&mut e, value.current_draft_id());
    enc_opt(&mut e, value.parent_thread_id(), enc_thread);
    enc_opt(&mut e, value.lineage_ancestor_skip(), enc_thread);
    enc_thread_lineage_depth(&mut e, value.lineage_depth());
    enc_path_digest(&mut e, value.lineage_digest());
    enc_image_label_frontier(&mut e, value.image_label_frontiers().inherited());
    enc_image_label_frontier(&mut e, value.image_label_frontiers().current());
    enc_opt(&mut e, value.context_owner_id(), enc_context_owner);
    enc_path_digest(&mut e, value.selected_path_digest());
    Ok(e.finish())
}

pub(super) fn decode_thread_record(bytes: &[u8]) -> Result<ThreadRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let id = dec_thread(&mut d)?;
    let revision = dec_thread_rev(&mut d)?;
    let committed_tail = dec_opt(&mut d, "committed tail", dec_turn)?;
    let current_draft_id = dec_draft(&mut d)?;
    let parent_thread_id = dec_opt(&mut d, "parent thread", dec_thread)?;
    let lineage_ancestor_skip = dec_opt(&mut d, "thread-lineage ancestor skip", dec_thread)?;
    let lineage_depth = dec_thread_lineage_depth(&mut d)?;
    let lineage_digest = dec_path_digest(&mut d)?;
    let image_label_frontiers = crate::ThreadImageLabelFrontiers::new(
        dec_image_label_frontier(&mut d)?,
        dec_image_label_frontier(&mut d)?,
    )
    .map_err(|source| invalid("thread image-label frontiers", source))?;
    let context_owner_id = dec_opt(&mut d, "context owner", dec_context_owner)?;
    let selected_path_digest = dec_path_digest(&mut d)?;
    let value = ThreadRecord::new(
        id,
        SelectedPathProof::new(committed_tail, revision, selected_path_digest),
        current_draft_id,
        ThreadLineageProof::new(
            parent_thread_id,
            lineage_ancestor_skip,
            lineage_depth,
            lineage_digest,
        ),
        image_label_frontiers,
        context_owner_id,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_draft_record(value: &DraftRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_draft(&mut e, value.id());
    enc_thread(&mut e, value.thread_id());
    enc_draft_rev(&mut e, value.revision());
    match value.submission_intent() {
        DraftSubmissionIntent::Ordinary => e.u8(0),
        DraftSubmissionIntent::DiscussionContext(owner) => {
            e.u8(1);
            enc_context_owner(&mut e, owner);
        }
        DraftSubmissionIntent::Replacement(intent) => {
            e.u8(2);
            enc_turn(&mut e, intent.target_turn_id());
            enc_selected_path(&mut e, intent.selected_path());
            enc_transcript_generation(&mut e, intent.transcript_entry().generation());
            enc_transcript_pos(&mut e, intent.transcript_entry().position());
        }
    }
    enc_content_ref(&mut e, value.content());
    enc_timestamp(&mut e, value.created_at());
    enc_timestamp(&mut e, value.updated_at());
    Ok(e.finish())
}

pub(super) fn decode_draft_record(bytes: &[u8]) -> Result<DraftRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = DraftRecord::new(
        dec_draft(&mut d)?,
        dec_thread(&mut d)?,
        dec_draft_rev(&mut d)?,
        match d.u8()? {
            0 => DraftSubmissionIntent::Ordinary,
            1 => DraftSubmissionIntent::DiscussionContext(dec_context_owner(&mut d)?),
            2 => DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(
                dec_turn(&mut d)?,
                dec_selected_path(&mut d)?,
                CurrentTranscriptEntryProof::new(
                    dec_transcript_generation(&mut d)?,
                    dec_transcript_pos(&mut d)?,
                ),
            )),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "draft submission intent",
                    tag,
                });
            }
        },
        dec_content_ref(&mut d)?,
        dec_timestamp(&mut d)?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_context_record(value: &ContextEnvelopeRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_context_owner(&mut e, value.owner());
    enc_context_rev(&mut e, value.revision());
    enc_context_envelope(&mut e, value.envelope());
    Ok(e.finish())
}

pub(super) fn decode_context_record(bytes: &[u8]) -> Result<ContextEnvelopeRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ContextEnvelopeRecord::new(
        dec_context_owner(&mut d)?,
        dec_context_rev(&mut d)?,
        dec_context_envelope(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_turn_record(value: &TurnRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_turn(&mut e, value.id());
    enc_thread(&mut e, value.origin_thread_id());
    enc_turn_kind(&mut e, value.kind());
    enc_parent(&mut e, value.parent());
    enc_opt(&mut e, value.ancestor_skip(), enc_turn);
    enc_turn_depth(&mut e, value.depth());
    enc_path_digest(&mut e, value.chain_digest());
    enc_timestamp(&mut e, value.submitted_at());
    Ok(e.finish())
}

pub(super) fn decode_turn_record(bytes: &[u8]) -> Result<TurnRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = TurnRecord::new(
        dec_turn(&mut d)?,
        dec_thread(&mut d)?,
        dec_turn_kind(&mut d)?,
        dec_parent(&mut d)?,
        dec_opt(&mut d, "turn ancestor skip", dec_turn)?,
        dec_turn_depth(&mut d)?,
        dec_path_digest(&mut d)?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_turn_state(value: &TurnStateRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_turn(&mut e, value.turn_id());
    enc_turn_state_rev(&mut e, value.revision());
    enc_turn_lifecycle(&mut e, value.lifecycle());
    e.u64(value.source_event_count());
    e.u64(value.item_count());
    e.u64(value.finalized_item_count());
    e.u64(value.open_item_count());
    e.u64(value.history_blocking_item_count());
    enc_opt(
        &mut e,
        value.provider_observation_issue(),
        enc_provider_observation_issue_reason,
    );
    enc_opt(&mut e, value.end_status(), enc_turn_end_status);
    enc_timestamp(&mut e, value.updated_at());
    Ok(e.finish())
}

pub(super) fn decode_turn_state(bytes: &[u8]) -> Result<TurnStateRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = TurnStateRecord::with_capture_frontiers_and_issue(
        dec_turn(&mut d)?,
        dec_turn_state_rev(&mut d)?,
        dec_turn_lifecycle(&mut d)?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        dec_opt(
            &mut d,
            "provider-observation issue reason",
            dec_provider_observation_issue_reason,
        )?,
        dec_opt(&mut d, "turn end status", dec_turn_end_status)?,
        dec_timestamp(&mut d)?,
    )
    .map_err(|source| invalid("turn state", source))?;
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_accepted_input(value: &AcceptedInputRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_accepted(&mut e, value.id());
    enc_thread(&mut e, value.thread_id());
    enc_accepted_ord(&mut e, value.ordinal());
    let admission = value.admission();
    enc_thread_rev(&mut e, admission.expected_thread_revision());
    enc_draft(&mut e, admission.source_draft_id());
    enc_draft_rev(&mut e, admission.expected_draft_revision());
    enc_input_gate_rev(&mut e, admission.expected_gate_revision());
    enc_draft(&mut e, admission.replacement_draft_id());
    enc_route_generation(&mut e, value.route_generation());
    enc_content_ref(&mut e, value.content());
    enc_opt(
        &mut e,
        value.asset_reference_set(),
        enc_sealed_asset_reference_set_proof,
    );
    enc_timestamp(&mut e, value.admitted_at());
    Ok(e.finish())
}

pub(super) fn decode_accepted_input(bytes: &[u8]) -> Result<AcceptedInputRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = AcceptedInputRecord::new(
        dec_accepted(&mut d)?,
        dec_thread(&mut d)?,
        dec_accepted_ord(&mut d)?,
        AcceptedInputAdmissionProof::new(
            dec_thread_rev(&mut d)?,
            dec_draft(&mut d)?,
            dec_draft_rev(&mut d)?,
            dec_input_gate_rev(&mut d)?,
            dec_draft(&mut d)?,
        )
        .map_err(|source| invalid("accepted-input admission proof", source))?,
        dec_route_generation(&mut d)?,
        dec_content_ref(&mut d)?,
        dec_opt(
            &mut d,
            "accepted-input asset reference set",
            dec_sealed_asset_reference_set_proof,
        )?,
        dec_timestamp(&mut d)?,
    )
    .map_err(|source| invalid("accepted input", source))?;
    d.finish()?;
    Ok(value)
}
