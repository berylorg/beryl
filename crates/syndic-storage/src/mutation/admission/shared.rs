use super::*;
use beryl_home_store::{DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{DiscussionContextOwnerId, DraftRevision};

use crate::{
    AcceptedInputRecord, AcceptedNextSourceRecord, AcceptedOrderIndexRecord,
    AcceptedReadySourceRecord, AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord,
    AcceptedRouteLeafRecord, ActivityQueryHeadRecord, ActivityQuerySourceRecord, BindingHeadRecord,
    BindingRecord, CanonicalItemRecord, ContextEnvelopeRecord, DraftByThreadRecord,
    DraftComposerMaterializationsFamily, DraftEditHistoryFrontierV1,
    DraftEditHistoryFrontiersCodec, DraftEditHistoryFrontiersFamily, DraftEditHistoryPolicyV1,
    DraftEditorCandidateSessionLifecycleV1, DraftEditorCandidateSessionRecordKeyV1,
    DraftEditorCandidateSessionRecordV1, DraftEditorCandidateSessionV1,
    DraftEditorCandidateSessionsCodec, DraftEditorCandidateSessionsFamily, DraftPieceRootRecordV1,
    DraftPieceRootsCodec, DraftPieceRootsFamily, DraftRecord, DraftRootHistoryPairV1,
    DraftSubmissionIntent, HistorySummaryRecord, ImageLabelAuthorityHeadV1,
    ImageLabelOriginSpanRecord, InputGateRecord, ThreadParentIndexRecord, ThreadRecord,
    TranscriptBuildRecord, TranscriptViewHeadRecord, TurnChildIndexRecord, TurnItemIndexRecord,
    TurnRecord, TurnStateRecord, authenticate_draft_edit_history_frontier_v1,
    canonical_empty_draft_edit_history_v1, canonical_empty_draft_piece_root_v1,
    canonical_empty_draft_root_operation_id_v1,
};

pub(super) struct AcceptanceBase {
    pub(super) thread: ThreadRecord,
    pub(super) image_label_authority: ImageLabelAuthorityHeadV1,
    pub(super) draft: DraftRecord,
    pub(super) gate: InputGateRecord,
    pub(super) summary: HistorySummaryRecord,
    pub(super) disposed_session: DraftEditorCandidateSessionV1,
    pub(super) fresh_root: DraftPieceRootRecordV1,
    pub(super) fresh_history: DraftEditHistoryFrontierV1,
}

pub(super) enum AcceptanceRecords {
    Idle(super::idle::IdleRecords),
    Accepted(super::accepted::AcceptedRecords),
}

impl AcceptanceRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        match self {
            Self::Idle(records) => records.contribute(mutations),
            Self::Accepted(records) => records.contribute(mutations),
        }
    }
}

pub(super) fn reserve_acceptance_records(
    reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    reservation.reserve_records::<DraftsCodec>(2)?;
    reservation.reserve_records::<DraftPieceRootsCodec>(1)?;
    reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
    reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
    reservation.reserve_records::<ThreadsCodec>(1)?;
    reservation.reserve_records::<DraftByThreadCodec>(1)?;
    reservation.reserve_records::<TurnsCodec>(1)?;
    reservation.reserve_records::<TurnStatesCodec>(1)?;
    reservation.reserve_records::<TurnChildrenCodec>(1)?;
    reservation.reserve_records::<CanonicalItemsCodec>(1)?;
    reservation.reserve_records::<TurnItemsCodec>(1)?;
    reservation.reserve_records::<AcceptedInputsCodec>(1)?;
    reservation.reserve_records::<AcceptedOrderCodec>(1)?;
    reservation.reserve_records::<AcceptedRouteGenerationHeadsCodec>(1)?;
    reservation.reserve_records::<AcceptedRouteGenerationsCodec>(1)?;
    reservation.reserve_records::<AcceptedRouteLeavesCodec>(1)?;
    reservation.reserve_records::<AcceptedReadySourcesCodec>(1)?;
    reservation.reserve_records::<AcceptedNextSourcesCodec>(1)?;
    reservation.reserve_records::<ImageLabelOriginSpansCodec>(1)?;
    reservation.reserve_records::<ImageLabelAuthorityHeadsCodec>(1)?;
    reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
    reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
    reservation.reserve_records::<HistorySummariesCodec>(1)?;
    reservation.reserve_records::<InputGatesCodec>(1)?;
    reservation.reserve_records::<ActivityQueryHeadsCodec>(1)?;
    reservation.reserve_records::<ActivityQuerySourcesCodec>(1)?;
    reservation.reserve_records::<BindingsCodec>(1)?;
    reservation.reserve_records::<BindingHeadsCodec>(1)?;
    reservation.reserve_records::<ContextEnvelopesCodec>(2)?;
    reservation.reserve_records::<ThreadParentCodec>(1)?;
    Ok(())
}

pub(super) fn load_base(
    reader: &DomainReader<'_, SyndicDomain>,
    acceptance: &FirstAcceptance,
) -> Result<AcceptanceBase, SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &acceptance.thread_id())?;
    if thread.revision() != acceptance.expected_thread_revision() {
        return Err(SyndicMutationError::ThreadRevisionConflict {
            expected: acceptance.expected_thread_revision(),
            current: thread.revision(),
        });
    }
    let image_label_authority =
        required::<ImageLabelAuthorityHeadsFamily>(reader, &acceptance.thread_id())?;
    if !image_label_authority.is_exact()
        || image_label_authority.thread_id() != thread.id()
        || image_label_authority != acceptance.expected_image_label_authority()
    {
        return Err(SyndicMutationError::ImageLabelAuthorityConflict);
    }
    let draft = current_draft(reader, acceptance.thread_id())?;
    if draft.id() != acceptance.draft_id() {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    if draft.revision() != acceptance.expected_draft_revision() {
        return Err(SyndicMutationError::DraftRevisionConflict {
            expected: acceptance.expected_draft_revision(),
            current: draft.revision(),
        });
    }
    let expected_pair = DraftRootHistoryPairV1::new(
        acceptance.candidate().root(),
        acceptance.candidate().history(),
    );
    if draft.root_history() != expected_pair
        || acceptance.candidate().draft_id() != draft.id()
        || acceptance.materialization().key().source() != acceptance.candidate().root()
    {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    let root = required::<DraftPieceRootsFamily>(reader, &acceptance.candidate().root().key())?;
    if root.reference() != acceptance.candidate().root() {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    let source_history = required::<DraftEditHistoryFrontiersFamily>(
        reader,
        &acceptance.candidate().history().key(),
    )?;
    if source_history.reference() != acceptance.candidate().history() {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    authenticate_draft_edit_history_frontier_v1(reader, &source_history)?;

    let session_key = DraftEditorCandidateSessionRecordKeyV1::Head {
        draft_id: acceptance.draft_id(),
        session_id: acceptance.candidate().session_id(),
    };
    let DraftEditorCandidateSessionRecordV1::Head(session) =
        required::<DraftEditorCandidateSessionsFamily>(reader, &session_key)?
    else {
        return Err(SyndicMutationError::IdentityCollision);
    };
    if session.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
        || session.thread_id() != acceptance.thread_id()
        || session.active_operation().is_some()
        || DraftEditorCandidateActivationBindingV1::from_head(&session) != acceptance.candidate()
        || session.published_candidate_generation() != session.newest_candidate_generation()
        || session.published_root() != session.newest_root()
        || session.published_history() != session.newest_history()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let disposed_session = session
        .disposed(acceptance.session_disposal_operation_id())
        .ok_or(SyndicMutationError::IdentityCollision)?;

    let mapping = required::<DraftComposerMaterializationsFamily>(
        reader,
        &acceptance.materialization().key(),
    )?;
    if mapping != acceptance.materialization()
        || mapping.content().summary().atom_count() == 0
        || mapping.source_digest() != acceptance.candidate().root().combined_digest()
        || mapping.source_piece_count() != acceptance.candidate().root().summary().piece_count()
        || mapping.source_utf8_bytes()
            != acceptance.candidate().root().summary().logical_utf8_bytes()
        || mapping.source_marker_count() != acceptance.candidate().root().summary().marker_count()
    {
        return Err(SyndicMutationError::ContentManifestConflict);
    }
    validate_asset_proof(acceptance)?;

    let gate = required::<InputGatesFamily>(reader, &acceptance.thread_id())?;
    if gate.revision() != acceptance.expected_gate_revision() {
        return Err(SyndicMutationError::InputGateRevisionConflict {
            expected: acceptance.expected_gate_revision(),
            current: gate.revision(),
        });
    }
    if gate.state() != acceptance.expected_gate_state() {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let summary = required::<HistorySummariesFamily>(reader, &acceptance.thread_id())?;
    if acceptance.admitted_at() < draft.updated_at()
        || acceptance.admitted_at() < summary.last_activity_at()
    {
        return Err(SyndicMutationError::TimestampRegressed);
    }
    ensure_new_identities(reader, acceptance)?;

    let draft_revision = DraftRevision::new(1)?;
    let fresh_root = canonical_empty_draft_piece_root_v1(
        acceptance.next_draft_id(),
        draft_revision,
        canonical_empty_draft_root_operation_id_v1(acceptance.next_draft_id()),
    );
    if point::<DraftPieceRootsFamily>(reader, &fresh_root.reference().key())?.is_some() {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let history_policy = DraftEditHistoryPolicyV1::new(
        source_history.byte_budget(),
        source_history.retention_policy_revision(),
    )
    .ok_or(SyndicMutationError::IdentityCollision)?;
    let fresh_history =
        canonical_empty_draft_edit_history_v1(fresh_root.reference(), history_policy);
    if point::<DraftEditHistoryFrontiersFamily>(reader, &fresh_history.reference().key())?.is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    Ok(AcceptanceBase {
        thread,
        image_label_authority,
        draft,
        gate,
        summary,
        disposed_session,
        fresh_root,
        fresh_history,
    })
}

fn validate_asset_proof(acceptance: &FirstAcceptance) -> Result<(), SyndicMutationError> {
    let content = acceptance.materialization().content();
    let sequential = content
        .sealed_marker_summary()
        .map_err(|_| SyndicMutationError::AssetReferenceSetConflict)?
        .sequential();
    match acceptance.asset_reference_set() {
        None if content.summary().image_marker_count() == 0 && sequential.marker_count() == 0 => {
            Ok(())
        }
        Some(proof)
            if content.summary().image_marker_count() != 0
                && proof.sequential() == sequential
                && proof.ordered_assets().marker_count()
                    == content.summary().image_marker_count() =>
        {
            Ok(())
        }
        None | Some(_) => Err(SyndicMutationError::AssetReferenceSetConflict),
    }
}

fn ensure_new_identities(
    reader: &DomainReader<'_, SyndicDomain>,
    acceptance: &FirstAcceptance,
) -> Result<(), SyndicMutationError> {
    if acceptance.next_draft_id() == acceptance.draft_id()
        || point::<DraftsFamily>(reader, &acceptance.next_draft_id())?.is_some()
        || point::<TurnsFamily>(reader, &acceptance.next_draft_id().submitted_turn_id())?.is_some()
        || point::<AcceptedInputsFamily>(reader, &acceptance.next_draft_id().accepted_input_id())?
            .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    Ok(())
}

pub(super) struct CommonRecords {
    pub(super) old_draft_id: SyndicDraftId,
    pub(super) thread: ThreadRecord,
    pub(super) draft: DraftRecord,
    pub(super) draft_index: DraftByThreadRecord,
    pub(super) fresh_root: DraftPieceRootRecordV1,
    pub(super) fresh_history: DraftEditHistoryFrontierV1,
    pub(super) disposed_session: DraftEditorCandidateSessionV1,
    pub(super) origin_span: Option<ImageLabelOriginSpanRecord>,
    pub(super) advanced_image_label_authority: Option<ImageLabelAuthorityHeadV1>,
    pub(super) summary: HistorySummaryRecord,
    pub(super) gate: InputGateRecord,
    pub(super) thread_parent_index: Option<ThreadParentIndexRecord>,
}

impl CommonRecords {
    pub(super) fn contribute(
        &self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.delete::<DraftsCodec>(&self.old_draft_id)?;
        mutations.put::<ThreadsCodec>(&self.thread.id(), &self.thread)?;
        mutations
            .put::<DraftPieceRootsCodec>(&self.fresh_root.reference().key(), &self.fresh_root)?;
        mutations.put::<DraftEditHistoryFrontiersCodec>(
            &self.fresh_history.reference().key(),
            &self.fresh_history,
        )?;
        mutations.put::<DraftsCodec>(&self.draft.id(), &self.draft)?;
        mutations.put::<DraftByThreadCodec>(&self.thread.id(), &self.draft_index)?;
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &DraftEditorCandidateSessionRecordKeyV1::Head {
                draft_id: self.disposed_session.draft_id(),
                session_id: self.disposed_session.session_id(),
            },
            &DraftEditorCandidateSessionRecordV1::Head(self.disposed_session.clone()),
        )?;
        if let Some(span) = &self.origin_span {
            mutations.put::<ImageLabelOriginSpansCodec>(
                &ImageLabelOriginSpanKey {
                    thread: span.thread_id(),
                    end_label: span.end_label(),
                },
                span,
            )?;
        }
        if let Some(head) = &self.advanced_image_label_authority {
            mutations.put::<ImageLabelAuthorityHeadsCodec>(&head.thread_id(), head)?;
        }
        mutations.put::<HistorySummariesCodec>(&self.thread.id(), &self.summary)?;
        mutations.put::<InputGatesCodec>(&self.thread.id(), &self.gate)?;
        if let Some(index) = &self.thread_parent_index {
            mutations.put::<ThreadParentCodec>(
                &ThreadPairKey {
                    first: index.parent_thread_id(),
                    second: index.child_thread_id(),
                },
                index,
            )?;
        }
        Ok(())
    }
}

pub(super) struct IdleSpecificRecords {
    pub(super) turn: TurnRecord,
    pub(super) turn_state: TurnStateRecord,
    pub(super) child_index: Option<TurnChildIndexRecord>,
    pub(super) item: CanonicalItemRecord,
    pub(super) item_index: TurnItemIndexRecord,
    pub(super) transcript_head: TranscriptViewHeadRecord,
    pub(super) transcript_build: Option<TranscriptBuildRecord>,
    pub(super) activity_head: ActivityQueryHeadRecord,
    pub(super) activity_source: ActivityQuerySourceRecord,
    pub(super) binding: BindingRecord,
    pub(super) binding_head: BindingHeadRecord,
    pub(super) context_move: Option<(DiscussionContextOwnerId, ContextEnvelopeRecord)>,
}

pub(super) struct AcceptedSpecificRecords {
    pub(super) input: AcceptedInputRecord,
    pub(super) order_index: AcceptedOrderIndexRecord,
    pub(super) route_head: Option<AcceptedRouteGenerationHeadRecord>,
    pub(super) route_generation: AcceptedRouteGenerationRecord,
    pub(super) route_leaf: AcceptedRouteLeafRecord,
    pub(super) ready_source: Option<AcceptedReadySourceRecord>,
    pub(super) next_source: Option<AcceptedNextSourceRecord>,
}

pub(super) fn fresh_draft(
    acceptance: &FirstAcceptance,
    base: &AcceptanceBase,
    thread: &ThreadRecord,
) -> Result<(DraftRecord, DraftByThreadRecord), SyndicMutationError> {
    let revision = DraftRevision::new(1)?;
    let draft = DraftRecord::new(
        acceptance.next_draft_id(),
        thread.id(),
        revision,
        DraftSubmissionIntent::Ordinary,
        DraftRootHistoryPairV1::new(base.fresh_root.reference(), base.fresh_history.reference()),
        acceptance.admitted_at(),
        acceptance.admitted_at(),
    );
    let index = DraftByThreadRecord::new(thread.id(), draft.id(), revision, thread.revision());
    Ok((draft, index))
}

pub(super) fn thread_parent_index(thread: &ThreadRecord) -> Option<ThreadParentIndexRecord> {
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

pub(super) fn advance_image_label_authority(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &ThreadRecord,
    head: ImageLabelAuthorityHeadV1,
    owner: crate::ImageLabelOriginOwner,
    proof: Option<SealedAssetReferenceSetProof>,
) -> Result<
    (
        Option<ImageLabelAuthorityHeadV1>,
        Option<ImageLabelOriginSpanRecord>,
    ),
    SyndicMutationError,
> {
    if !head.is_exact() || head.thread_id() != thread.id() {
        return Err(SyndicMutationError::ImageLabelAuthorityConflict);
    }
    let Some(proof) = proof else {
        return Ok((None, None));
    };
    let end = proof
        .sequential()
        .maximum_image_label()
        .ok_or(SyndicMutationError::AssetReferenceSetConflict)?;
    if head.permanent().contains(end) {
        return Ok((None, None));
    }
    let start = crate::ImageLabelOrdinal::new(
        head.permanent()
            .get()
            .checked_add(1)
            .ok_or(SyndicMutationError::AssetReferenceSetConflict)?,
    )
    .map_err(|_| SyndicMutationError::AssetReferenceSetConflict)?;
    let span = ImageLabelOriginSpanRecord::new(thread.id(), start, end, owner, proof)?;
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
    let advanced = head.advanced(crate::ImageLabelFrontier::from_raw(end.get()))?;
    Ok((Some(advanced), Some(span)))
}

pub(super) fn validate_replacement_intent(
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
    if target.kind() != crate::TurnKind::OrdinaryUser {
        return Err(SyndicMutationError::ReplacementTargetConflict);
    }
    let head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
    let transcript = intent.transcript_entry();
    if head.lifecycle() != crate::ProjectionLifecycle::Current
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

fn required<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<F::Value, SyndicMutationError>
where
    F::Key: std::fmt::Debug,
{
    crate::mutation::required::<F>(reader, key)
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, SyndicMutationError> {
    crate::mutation::point::<F>(reader, key)
}

fn current_draft(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: SyndicThreadId,
) -> Result<DraftRecord, SyndicMutationError> {
    crate::mutation::current_draft(reader, thread)
}
