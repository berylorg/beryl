use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn load_base(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    expected_content: crate::ContentReference,
    expected_gate_revision: InputGateRevision,
    next_draft_id: SyndicDraftId,
    admitted_at: SyndicTimestamp,
) -> Result<AdmissionBase, SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    if thread.revision() != expected_thread_revision {
        return Err(SyndicMutationError::ThreadRevisionConflict {
            expected: expected_thread_revision,
            current: thread.revision(),
        });
    }
    let draft = current_draft(reader, thread_id)?;
    if draft.id() != draft_id {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    if draft.revision() != expected_draft_revision {
        return Err(SyndicMutationError::DraftRevisionConflict {
            expected: expected_draft_revision,
            current: draft.revision(),
        });
    }
    if draft.content() != expected_content {
        return Err(SyndicMutationError::ContentManifestConflict);
    }
    let gate = required::<InputGatesFamily>(reader, &thread_id)?;
    if gate.revision() != expected_gate_revision {
        return Err(SyndicMutationError::InputGateRevisionConflict {
            expected: expected_gate_revision,
            current: gate.revision(),
        });
    }
    let summary = required::<HistorySummariesFamily>(reader, &thread_id)?;
    if admitted_at < draft.updated_at() || admitted_at < summary.last_activity_at() {
        return Err(SyndicMutationError::TimestampRegressed);
    }
    if draft.content().summary().atom_count() == 0 {
        return Err(SyndicMutationError::EmptySubmission);
    }
    require_sealed_composer(reader, draft.content())?;
    ensure_new_draft_identity(reader, draft.id(), next_draft_id)?;
    let empty_content = canonical_empty_content(reader)?;
    Ok(AdmissionBase {
        thread,
        draft,
        gate,
        summary,
        empty_content,
    })
}

pub(in crate::mutation) fn require_sealed_composer(
    reader: &DomainReader<'_, SyndicDomain>,
    content: crate::ContentReference,
) -> Result<(), SyndicMutationError> {
    let manifest = required::<ContentManifestsFamily>(reader, &content.id())?;
    if content.encoding() != ContentEncoding::ComposerV1
        || manifest.sealed_reference() != Some(content)
    {
        return Err(SyndicMutationError::ContentManifestConflict);
    }
    Ok(())
}

pub(in crate::mutation) fn canonical_empty_content(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<crate::ContentReference, SyndicMutationError> {
    let prepared = PreparedContent::composer(&crate::ComposerPayload::default())?;
    let manifest = required::<ContentManifestsFamily>(reader, &prepared.id())?;
    let Some(reference) = manifest.sealed_reference() else {
        return Err(SyndicMutationError::ContentManifestConflict);
    };
    if reference.summary() != prepared.summary()
        || reference.encoding() != ContentEncoding::ComposerV1
    {
        return Err(SyndicMutationError::ContentManifestConflict);
    }
    Ok(reference)
}

pub(super) fn ensure_new_draft_identity(
    reader: &DomainReader<'_, SyndicDomain>,
    old_draft_id: SyndicDraftId,
    next_draft_id: SyndicDraftId,
) -> Result<(), SyndicMutationError> {
    if next_draft_id == old_draft_id
        || point::<DraftsFamily>(reader, &next_draft_id)?.is_some()
        || point::<TurnsFamily>(reader, &next_draft_id.submitted_turn_id())?.is_some()
        || point::<AcceptedInputsFamily>(reader, &next_draft_id.accepted_input_id())?.is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    Ok(())
}

pub(in crate::mutation) fn validate_asset_reference_set(
    content: crate::ContentReference,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
) -> Result<(), SyndicMutationError> {
    let expected = content
        .sealed_marker_summary()
        .map_err(|_| SyndicMutationError::AssetReferenceSetConflict)?;
    let exact = match (content.summary().image_marker_count(), asset_reference_set) {
        (0, None) => true,
        (0, Some(_)) | (_, None) => false,
        (_, Some(proof)) => proof.source() == expected,
    };
    if !exact {
        return Err(SyndicMutationError::AssetReferenceSetConflict);
    }
    Ok(())
}

pub(super) fn advance_image_label_authority(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &ThreadRecord,
    owner: crate::ImageLabelOriginOwner,
    content: crate::ContentReference,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
) -> Result<
    (
        crate::ThreadImageLabelFrontiers,
        Option<crate::ImageLabelOriginSpanRecord>,
    ),
    SyndicMutationError,
> {
    validate_asset_reference_set(content, asset_reference_set)?;
    let frontiers = thread.image_label_frontiers();
    let Some(proof) = asset_reference_set else {
        return Ok((frontiers, None));
    };
    let end = proof
        .source()
        .maximum_image_label()
        .ok_or(SyndicMutationError::AssetReferenceSetConflict)?;
    if frontiers.current().contains(end) {
        return Ok((frontiers, None));
    }
    let start = crate::ImageLabelOrdinal::new(
        frontiers
            .current()
            .get()
            .checked_add(1)
            .ok_or(SyndicMutationError::AssetReferenceSetConflict)?,
    )
    .map_err(|_| SyndicMutationError::AssetReferenceSetConflict)?;
    let span = crate::ImageLabelOriginSpanRecord::new(thread.id(), start, end, owner, proof)?;
    if point::<ImageLabelOriginSpansFamily>(
        reader,
        &ImageLabelOriginSpanKey {
            thread: thread.id(),
            end_label: end,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::AssetReferenceSetConflict);
    }
    let advanced = crate::ThreadImageLabelFrontiers::new(
        frontiers.inherited(),
        crate::ImageLabelFrontier::from_raw(end.get()),
    )?;
    Ok((advanced, Some(span)))
}

pub(in crate::mutation) fn turn_shape(
    reader: &DomainReader<'_, SyndicDomain>,
    turn_id: SyndicTurnId,
    parent: ConversationParent,
) -> Result<
    (
        TurnDepth,
        beryl_model::SyndicPathDigest,
        Option<SyndicTurnId>,
    ),
    SyndicMutationError,
> {
    match parent {
        ConversationParent::Root => Ok((TurnDepth::FIRST, root_turn_chain_digest(turn_id), None)),
        ConversationParent::Turn(parent_id) => {
            let parent = required::<TurnsFamily>(reader, &parent_id)?;
            let depth = parent.depth().checked_next()?;
            let ancestor_skip = crate::selected_path::child_ancestor_skip(
                parent.clone(),
                depth,
                |turn_id| required::<TurnsFamily>(reader, &turn_id),
                |_| SyndicMutationError::SourceTailConflict,
            )?;
            Ok((
                depth,
                child_turn_chain_digest(turn_id, parent_id, parent.chain_digest()),
                Some(ancestor_skip),
            ))
        }
    }
}

pub(in crate::mutation) fn validate_replacement_intent(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &ThreadRecord,
    intent: crate::ReplacementEditIntent,
) -> Result<(TurnRecord, CanonicalItemRecord), SyndicMutationError> {
    let proof = intent.selected_path();
    if proof.tail() != thread.committed_tail()
        || proof.thread_revision() != thread.revision()
        || proof.digest() != thread.selected_path_digest()
    {
        return Err(SyndicMutationError::ReplacementTargetConflict);
    }
    let target = required::<TurnsFamily>(reader, &intent.target_turn_id())?;
    if target.kind() != TurnKind::OrdinaryUser {
        return Err(SyndicMutationError::ReplacementTargetConflict);
    }
    let head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
    let transcript = intent.transcript_entry();
    if head.lifecycle() != ProjectionLifecycle::Current
        || head.generation() != transcript.generation()
        || head.committed_tail() != proof.tail()
        || head.selected_path_digest() != proof.digest()
    {
        return Err(SyndicMutationError::ReplacementTargetConflict);
    }
    let entry = required::<TranscriptEntriesFamily>(
        reader,
        &ThreadTranscriptKey {
            thread: thread.id(),
            generation: transcript.generation(),
            position: transcript.position(),
        },
    )?;
    let item = required::<CanonicalItemsFamily>(reader, &entry.item_id())?;
    if entry.thread_id() != thread.id()
        || entry.generation() != transcript.generation()
        || entry.position() != transcript.position()
        || item.id() != entry.item_id()
        || item.revision() != entry.item_revision()
        || item.turn_id() != target.id()
        || item.kind() != crate::CanonicalItemKind::UserInput
    {
        return Err(SyndicMutationError::ReplacementTargetConflict);
    }
    Ok((target, item))
}

pub(in crate::mutation) fn thread_parent_index(
    thread: &ThreadRecord,
) -> Option<ThreadParentIndexRecord> {
    match (thread.parent_thread_id(), thread.context_owner_id()) {
        (Some(parent), Some(owner)) => Some(ThreadParentIndexRecord::new(
            parent,
            thread.id(),
            thread.revision(),
            owner,
        )),
        _ => None,
    }
}

pub(super) fn context_move(
    reader: &DomainReader<'_, SyndicDomain>,
    draft: &DraftRecord,
    turn_id: SyndicTurnId,
) -> Result<
    (
        Option<ContextMove>,
        Option<DiscussionContextOwnerId>,
        Option<ConversationParent>,
    ),
    SyndicMutationError,
> {
    let crate::DraftSubmissionIntent::DiscussionContext(owner) = draft.submission_intent() else {
        return Ok((None, None, None));
    };
    if owner != DiscussionContextOwnerId::Draft(draft.id()) {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    let record = required::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(owner))?;
    let submitted = DiscussionContextOwnerId::SubmittedTurn(turn_id);
    if point::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(submitted))?.is_some() {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    Ok((
        Some(ContextMove {
            old_owner: owner,
            new_record: crate::ContextEnvelopeRecord::new(
                submitted,
                record.revision(),
                record.envelope().clone(),
            ),
        }),
        Some(submitted),
        Some(ConversationParent::Turn(
            record.envelope().descriptor().source().turn_id(),
        )),
    ))
}
