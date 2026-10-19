use super::super::*;
use super::{
    super::import::read::SourceReader,
    request::*,
    source::{authenticate_basis, require_writer_destination, required},
};
use crate::domain::SyndicDomain;
use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

pub(super) struct BeginRecall {
    pub prepared: PreparedDraftComposerRecallV1,
    pub generation_matches: bool,
}

type RecallTransition = (
    DraftPieceBuildRecordV1,
    DraftPieceBuildProgressReceiptV1,
    DraftEditorCandidateSessionV1,
);

impl DomainMutation<SyndicDomain> for BeginRecall {
    type Error = DraftComposerRecallErrorV1;
    type Prepared = Option<Box<RecallTransition>>;
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.generation_matches {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        authenticate_basis(reader, &self.prepared.basis)?;
        let transition = initial_transition(&self.prepared)?;
        let (build, receipt, head) = transition.as_ref();
        let key = DraftPieceSettlementKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
        );
        let existing = reader.boxed_point::<DraftPieceBuildsFamily>(&key)?;
        if existing.as_deref() == Some(build) {
            authenticate_transition(reader, build, receipt, head)?;
            return Ok(None);
        }
        if existing.is_some()
            || reader.contains::<DraftPieceSettlementsFamily>(&key)?
            || reader.contains::<DraftPieceBuildProgressFamily>(&receipt.key())?
            || reader.contains::<DraftPieceRootsFamily>(&DraftPieceRootKeyV1::editor_candidate(
                build.draft_id(),
                build.session_id(),
                build.operation_id(),
            ))?
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        let current = reader
            .boxed_point::<DraftEditorCandidateSessionsFamily>(&head_key(&self.prepared.before))?
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        if !matches!(current.as_ref(), DraftEditorCandidateSessionRecordV1::Head(head)
            if head == self.prepared.before.as_ref())
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        require_writer_destination(reader, &self.prepared.before)?;
        if !session::idle_candidate_closure_is_exact_in_transaction(reader, &self.prepared.before)?
        {
            return Err(DraftComposerRecallErrorV1::Ineligible);
        }
        Ok(Some(transition))
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }
    fn contribute(
        prepared: Self::Prepared,
        writer: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(transition) = prepared {
            put_transition(writer, &transition.0, &transition.1, &transition.2)?;
        }
        Ok(())
    }
}

#[inline(never)]
fn initial_transition(
    prepared: &PreparedDraftComposerRecallV1,
) -> Result<Box<RecallTransition>, DraftComposerRecallErrorV1> {
    let h = prepared.header;
    let zero = DraftPieceBuildBoundaryV1::new(0, 0);
    let frontier = DraftComposerRecallFrontierV1 {
        basis: prepared.basis.clone(),
        phase: DraftSealedImportPhaseV1::Streaming,
        encoded_cursor: 0,
        atoms: 0,
        logical_bytes: 0,
        markers: 0,
        decoder: DraftSealedImportDecoderV1::BeforeHeader,
        text_remaining: 0,
        next_piece: 1,
        piece_text_offset: 0,
        next_leaf_rank: 0,
        checked_markers: 0,
        sequential_digest: beryl_model::sequential_marker_digest_seed(),
        maximum_label: None,
        ordered_digest: beryl_model::ordered_marker_asset_digest_seed(),
    };
    let key =
        DraftPieceBuildProgressReceiptKeyV1::new(h.draft_id(), h.session_id(), h.operation_id(), 1);
    let raw = DraftPieceBuildRecordV1::new(
        h.draft_id(),
        h.session_id(),
        h.predecessor_candidate_generation(),
        h.predecessor_root(),
        h.predecessor_history(),
        h.operation_id(),
        h.predecessor_caret(),
        h.predecessor_selection(),
        h.caret(),
        h.selection(),
        0,
        canonical_empty_draft_piece_fragment_chain_v1(),
        prepared.canonical_header.clone(),
        0,
        canonical_empty_draft_piece_fragment_chain_v1(),
        canonical_proposal_digest(&prepared.canonical_header),
        DraftPieceBuildRootsV1::from_root(
            canonical_empty_draft_piece_root_v1(
                h.draft_id(),
                beryl_model::DraftRevision::new(1).unwrap(),
                canonical_empty_draft_root_operation_id_v1(h.draft_id()),
            )
            .reference(),
        ),
        zero,
        zero,
        1,
        DraftPieceBuildFrontierV1::OrdinaryRecall(frontier),
        DraftPieceBuildProgressReceiptReferenceV1::new(
            key,
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        None,
        None,
        DraftPieceBuildLifecycleV1::Open,
    );
    let (build, receipt) = authenticated_build_transition(raw, None, None)
        .map_err(|()| DraftComposerRecallErrorV1::Conflict)?;
    let head = prepared
        .before
        .with_active_operation(custody(&build))
        .ok_or(DraftComposerRecallErrorV1::Conflict)?;
    Ok(Box::new((build, receipt, head)))
}

pub(super) fn custody(build: &DraftPieceBuildRecordV1) -> DraftEditorActiveOperationV1 {
    DraftEditorActiveOperationV1::building(
        build.operation_id(),
        build.proposal_digest(),
        build.predecessor_candidate_generation(),
        build.predecessor_root(),
        build.predecessor_history(),
        build.progress_receipt(),
    )
}
pub(super) fn head_key(
    head: &DraftEditorCandidateSessionV1,
) -> DraftEditorCandidateSessionRecordKeyV1 {
    DraftEditorCandidateSessionRecordKeyV1::head(head.draft_id(), head.session_id())
}
pub(super) fn put_transition(
    writer: &mut MutationBuilder<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
    receipt: &DraftPieceBuildProgressReceiptV1,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), DraftComposerRecallErrorV1> {
    writer.put::<DraftPieceBuildProgressCodec>(&receipt.key(), receipt)?;
    writer.put::<DraftPieceBuildsCodec>(
        &DraftPieceSettlementKeyV1::new(build.draft_id(), build.session_id(), build.operation_id()),
        build,
    )?;
    writer.put::<DraftEditorCandidateSessionsCodec>(
        &head_key(head),
        &DraftEditorCandidateSessionRecordV1::Head(head.clone()),
    )?;
    Ok(())
}
pub(super) fn authenticate_transition(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &DraftPieceBuildRecordV1,
    receipt: &DraftPieceBuildProgressReceiptV1,
    head: &DraftEditorCandidateSessionV1,
) -> Result<(), DraftComposerRecallErrorV1> {
    if required::<DraftPieceBuildProgressFamily>(reader, &receipt.key())? != *receipt
        || required::<DraftEditorCandidateSessionsFamily>(reader, &head_key(head))?
            != DraftEditorCandidateSessionRecordV1::Head(head.clone())
        || !progress_receipt_matches_build(receipt, build)
    {
        return Err(DraftComposerRecallErrorV1::Conflict);
    }
    Ok(())
}
