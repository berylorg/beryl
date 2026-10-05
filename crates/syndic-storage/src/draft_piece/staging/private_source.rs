use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPrivateClipboardSourceV1 {
    draft_id: beryl_model::SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    candidate_generation: u64,
    candidate_root: DraftPieceRootReferenceV1,
    content_root: DraftPieceRootReferenceV1,
    cut_settlement: Option<DraftPieceSettlementKeyV1>,
}

impl DraftPrivateClipboardSourceV1 {
    pub fn from_candidate(
        draft_id: beryl_model::SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        candidate_generation: u64,
        root: DraftPieceRootReferenceV1,
    ) -> Option<Self> {
        (root.key().draft_id() == draft_id).then_some(Self {
            draft_id,
            session_id,
            candidate_generation,
            candidate_root: root,
            content_root: root,
            cut_settlement: None,
        })
    }

    pub fn from_committed_cut(
        settlement: DraftPieceSettlementKeyV1,
        successor_generation: u64,
        successor_root: DraftPieceRootReferenceV1,
        predecessor_root: DraftPieceRootReferenceV1,
    ) -> Option<Self> {
        let mut source = Self::from_candidate(
            settlement.draft_id(),
            settlement.session_id(),
            successor_generation,
            successor_root,
        )?;
        if predecessor_root.key().draft_id() != settlement.draft_id() {
            return None;
        }
        source.content_root = predecessor_root;
        source.cut_settlement = Some(settlement);
        Some(source)
    }

    pub fn candidate(session: &DraftEditorCandidateSessionV1) -> Option<Self> {
        (session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
            && session.is_coherent())
        .then_some(Self {
            draft_id: session.draft_id(),
            session_id: session.session_id(),
            candidate_generation: session.newest_candidate_generation(),
            candidate_root: session.newest_root(),
            content_root: session.newest_root(),
            cut_settlement: None,
        })
    }

    pub fn committed_cut(
        session: &DraftEditorCandidateSessionV1,
        settlement: &DraftPieceSettlementV1,
    ) -> Option<Self> {
        let mut source = Self::candidate(session)?;
        if !source.settlement_is_exact(settlement) {
            return None;
        }
        source.content_root = settlement.predecessor_root();
        source.cut_settlement = Some(settlement.key());
        Some(source)
    }

    pub const fn content_root(self) -> DraftPieceRootReferenceV1 {
        self.content_root
    }

    pub fn marker_selector(
        self,
        marker_id: beryl_model::SyndicDraftMarkerId,
    ) -> DraftMarkerReadinessSourceSelectorV1 {
        match self.cut_settlement {
            Some(settlement) => {
                DraftMarkerReadinessSourceSelectorV1::Cut(DraftMarkerReadinessCutSourceV1::new(
                    settlement,
                    self.candidate_generation,
                    self.candidate_root,
                    marker_id,
                ))
            }
            None => DraftMarkerReadinessSourceSelectorV1::Candidate(
                DraftMarkerReadinessCandidateSourceV1::new(
                    self.draft_id,
                    self.session_id,
                    self.candidate_generation,
                    self.candidate_root,
                    marker_id,
                ),
            ),
        }
    }

    fn session_is_exact(self, session: &DraftEditorCandidateSessionV1) -> bool {
        session.draft_id() == self.draft_id
            && session.session_id() == self.session_id
            && session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active
            && session.is_coherent()
            && session.newest_candidate_generation() == self.candidate_generation
            && session.newest_root() == self.candidate_root
    }

    fn settlement_is_exact(self, settlement: &DraftPieceSettlementV1) -> bool {
        settlement.key().draft_id() == self.draft_id
            && settlement.key().session_id() == self.session_id
            && self
                .cut_settlement
                .is_none_or(|key| key == settlement.key())
            && settlement_closure_is_exact(settlement)
            && matches!(settlement.outcome(), DraftPieceSettlementOutcomeV1::Committed {
                candidate_generation, successor, ..
            } if *candidate_generation == self.candidate_generation && *successor == self.candidate_root)
            && self
                .cut_settlement
                .is_none_or(|_| settlement.predecessor_root() == self.content_root)
    }
}

#[derive(Clone)]
pub struct DraftPrivateClipboardSourceFenceV1 {
    source: DraftPrivateClipboardSourceV1,
    begin: DraftMutationBeginV1,
    pub(super) home_generation: u64,
}

impl SyndicStorage {
    pub fn draft_private_clipboard_source_is_current(
        &self,
        store: &beryl_home_store::HomeStore,
        source: DraftPrivateClipboardSourceV1,
    ) -> Result<bool, SyndicReadError> {
        let session = self.point::<DraftEditorCandidateSessionsFamily>(
            store,
            DraftEditorCandidateSessionRecordKeyV1::head(source.draft_id, source.session_id),
            crate::SyndicPointReadLimit::new(DRAFT_PIECE_PAGE_MAX_BYTES).unwrap(),
        )?;
        if !matches!(session, Some(DraftEditorCandidateSessionRecordV1::Head(ref session)) if source.session_is_exact(session))
        {
            return Ok(false);
        }
        match source.cut_settlement {
            None => Ok(true),
            Some(key) => Ok(self
                .point::<DraftPieceSettlementsFamily>(
                    store,
                    key,
                    crate::SyndicPointReadLimit::new(DRAFT_PIECE_PAGE_MAX_BYTES).unwrap(),
                )?
                .as_ref()
                .is_some_and(|settlement| source.settlement_is_exact(settlement))),
        }
    }

    pub fn prepare_draft_private_clipboard_source_fence(
        &self,
        begin: DraftMutationBeginV1,
        source: DraftPrivateClipboardSourceV1,
    ) -> DraftPrivateClipboardSourceFenceV1 {
        DraftPrivateClipboardSourceFenceV1 {
            source,
            begin,
            home_generation: self.home_generation.get(),
        }
    }

    pub fn prepare_draft_mutation_staging_private_begin(
        &self,
        begin: DraftMutationBeginV1,
        session: &DraftEditorCandidateSessionV1,
        fence: DraftPrivateClipboardSourceFenceV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        if fence.home_generation != self.home_generation.get() || fence.begin != begin {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let mut prepared = self.prepare_draft_mutation_staging_begin(begin, session)?;
        prepared.private_source_fence = Some(fence);
        Ok(prepared)
    }

    pub fn prepare_draft_mutation_staging_private_marker_begin(
        &self,
        begin: DraftMutationBeginV1,
        session: &DraftEditorCandidateSessionV1,
        readiness: DraftMarkerLabelReadinessProofV1,
        source: DraftPrivateClipboardSourceV1,
    ) -> Result<PreparedDraftMutationStagingCommandV1, DraftMutationStagingErrorV1> {
        let admission = readiness
            .into_writer_admission(begin)
            .ok_or(DraftMutationStagingErrorV1::Invalid)?;
        let begin = begin.with_writer_admission(admission);
        let fence = self.prepare_draft_private_clipboard_source_fence(begin, source);
        self.prepare_draft_mutation_staging_private_begin(begin, session, fence)
    }
}

pub(super) fn validate_private_source_fence(
    reader: &DomainReader<'_, SyndicDomain>,
    prepared: &PreparedDraftMutationStagingCommandV1,
    work: Option<&AdmissionWorkLedger>,
) -> Result<(), SyndicMutationError> {
    let Some(fence) = prepared.private_source_fence.as_ref() else {
        return Ok(());
    };
    if prepared.source_head.is_some() || fence.begin != prepared.target_head.begin() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let source = fence.source;
    let session = draft_marker_writer_point::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::head(source.draft_id, source.session_id),
        work,
    )?;
    if !matches!(session, Some(DraftEditorCandidateSessionRecordV1::Head(ref session)) if source.session_is_exact(session))
    {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    if let Some(key) = source.cut_settlement {
        if !draft_marker_writer_point::<DraftPieceSettlementsFamily>(reader, &key, work)?
            .as_ref()
            .is_some_and(|settlement| source.settlement_is_exact(settlement))
        {
            return Err(SyndicMutationError::CurrentDraftConflict);
        }
    }
    Ok(())
}
