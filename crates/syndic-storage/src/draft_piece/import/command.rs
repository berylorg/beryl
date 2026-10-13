use super::super::*;
use super::{
    admission::{authenticate_source, required},
    request::*,
};
use crate::{codec::*, domain::SyndicDomain};
use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

pub(super) struct BeginImport {
    pub prepared: PreparedDraftReplacementImportV1,
    pub generation_matches: bool,
}

type ImportTransition = (
    DraftPieceBuildRecordV1,
    DraftPieceBuildProgressReceiptV1,
    DraftEditorCandidateSessionV1,
);

impl DomainMutation<SyndicDomain> for BeginImport {
    type Error = DraftReplacementErrorV1;
    type Prepared = Option<Box<ImportTransition>>;
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.generation_matches {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let (build, receipt, head) = initial_transition(&self.prepared)?;
        let key = DraftPieceSettlementKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
        );
        let existing = super::read::SourceReader::point::<DraftPieceBuildsFamily>(reader, &key)?;
        if existing.as_ref() == Some(&build) {
            authenticate_transition(reader, &build, &receipt, &head)?;
            return Ok(None);
        }
        if existing.is_some()
            || super::read::SourceReader::point::<DraftPieceSettlementsFamily>(reader, &key)?
                .is_some()
            || super::read::SourceReader::point::<DraftPieceBuildProgressFamily>(
                reader,
                &receipt.key(),
            )?
            .is_some()
            || super::read::SourceReader::point::<DraftPieceRootsFamily>(
                reader,
                &DraftPieceRootKeyV1::editor_candidate(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                ),
            )?
            .is_some()
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        authenticate_source(reader, self.prepared.basis)?;
        let current = required::<DraftEditorCandidateSessionsFamily>(
            reader,
            &head_key(&self.prepared.source_session),
        )?;
        if current
            != DraftEditorCandidateSessionRecordV1::Head(*self.prepared.source_session.clone())
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        if !checkpoint::saved_checkpoint_is_exact_in_transaction(
            reader,
            &self.prepared.source_session,
            &required::<DraftEditHistoryFrontiersFamily>(
                reader,
                &self.prepared.source_session.newest_history().key(),
            )?,
        )? {
            return Err(DraftReplacementErrorV1::Ineligible);
        }
        Ok(Some(Box::new((build, receipt, head))))
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

pub(super) fn initial_transition(
    prepared: &PreparedDraftReplacementImportV1,
) -> Result<ImportTransition, DraftReplacementErrorV1> {
    let h = prepared.header;
    let zero = DraftPieceBuildBoundaryV1::new(0, 0);
    let frontier = DraftSealedImportFrontierV1 {
        basis: prepared.basis,
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
        DraftPieceBuildRootsV1::from_root(h.predecessor_root()),
        zero,
        zero,
        1,
        DraftPieceBuildFrontierV1::SealedImport(frontier),
        DraftPieceBuildProgressReceiptReferenceV1::new(
            key,
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        None,
        None,
        DraftPieceBuildLifecycleV1::Open,
    );
    let (build, receipt) = authenticated_build_transition(raw, None, None)
        .map_err(|()| DraftReplacementErrorV1::Conflict)?;
    let head = prepared
        .source_session
        .with_active_operation(custody(&build))
        .ok_or(DraftReplacementErrorV1::Conflict)?;
    Ok((build, receipt, head))
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
) -> Result<(), DraftReplacementErrorV1> {
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
) -> Result<(), DraftReplacementErrorV1> {
    if required::<DraftPieceBuildProgressFamily>(reader, &receipt.key())? != *receipt
        || required::<DraftEditorCandidateSessionsFamily>(reader, &head_key(head))?
            != DraftEditorCandidateSessionRecordV1::Head(head.clone())
        || !progress_receipt_matches_build(receipt, build)
    {
        return Err(DraftReplacementErrorV1::Conflict);
    }
    Ok(())
}
