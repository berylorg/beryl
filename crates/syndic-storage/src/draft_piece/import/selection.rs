use super::super::*;
use super::{
    admission::{authenticate_source, required},
    command::{custody, head_key, put_transition},
    read::{SourceReader, StoreSourceReader},
    request::*,
};
use crate::{
    DraftByThreadRecord, DraftRecord, DraftSubmissionIntent, HistorySummaryRecord, SyndicStorage,
    SyndicTimestamp, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};

#[derive(Clone)]
pub struct PreparedDraftReplacementStartV1 {
    pub(crate) observation: super::outcome_state::CommandObservation,
    pub(crate) scope: ImportCommandScope,
    pub(crate) source: Box<DraftPieceBuildRecordV1>,
    pub(crate) selection: Box<DraftSealedImportSelectionV1>,
    pub(crate) terminal: Box<DraftPieceBuildRecordV1>,
    pub(crate) receipt: Box<DraftPieceBuildProgressReceiptV1>,
    pub(crate) settlement: Box<DraftPieceSettlementV1>,
    pub(crate) home_generation: beryl_home_store::HomeGeneration,
}
impl PreparedDraftReplacementStartV1 {
    pub fn basis(&self) -> DraftSealedImportBasisV1 {
        self.selection.basis
    }
    pub fn selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.selection.successor_selector
    }
    pub fn settlement(&self) -> &DraftPieceSettlementV1 {
        &self.settlement
    }
}
impl SyndicStorage {
    pub fn prepare_replacement_draft_start(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementImportV1,
        at: SyndicTimestamp,
    ) -> Result<PreparedDraftReplacementStartV1, DraftReplacementErrorV1> {
        p.scope.require_current(store)?;
        if p.home_generation != self.home_generation {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let key = DraftPieceSettlementKeyV1::new(
            p.basis.selector.draft_id(),
            p.session_id(),
            p.operation_id(),
        );
        let source = r
            .boxed_point::<DraftPieceBuildsFamily>(&key)?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        if source.canonical_header() != p.canonical_header
            || !import_build_is_exact(&source)
            || source.lifecycle() != DraftPieceBuildLifecycleV1::Complete
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let before = r.session_head(&DraftEditorCandidateSessionRecordKeyV1::head(
            source.draft_id(),
            source.session_id(),
        ))?;
        let prior = r
            .boxed_point::<DraftEditHistoryFrontiersFamily>(&source.predecessor_history().key())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let mutable = r
            .boxed_point::<DraftEditHistoryFrontiersFamily>(
                &DraftEditHistoryFrontierKeyV1::session(source.draft_id(), source.session_id()),
            )?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let draft = r
            .boxed_point::<DraftsFamily>(&source.draft_id())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        if at < draft.updated_at() {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let root = source
            .successor()
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let (baseline, fork) = mutable
            .sealed_import_baseline(source.session_id(), source.operation_id(), root)
            .ok_or(DraftReplacementErrorV1::CapacityUnavailable)?;
        let successor_selector = DraftEditorCurrentSelectorV1::new(
            p.basis.selector.thread_id(),
            p.basis.selector.thread_revision(),
            source.draft_id(),
            draft
                .revision()
                .checked_next()
                .map_err(crate::SyndicMutationError::from)?,
            root,
            baseline.reference(),
        );
        let after = imported_session(&before, successor_selector)
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let next_draft = DraftRecord::new(
            draft.id(),
            draft.thread_id(),
            successor_selector.selector_revision(),
            DraftSubmissionIntent::Replacement(p.basis.intent),
            DraftRootHistoryPairV1::new(root, baseline.reference()),
            draft.created_at(),
            at,
        );
        let prior_summary = r
            .boxed_point::<HistorySummariesFamily>(&draft.thread_id())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let successor_summary = HistorySummaryRecord::new(
            prior_summary.thread_id(),
            prior_summary
                .revision()
                .checked_next()
                .map_err(crate::SyndicMutationError::from)?,
            prior_summary.thread_revision(),
            prior_summary.committed_tail(),
            prior_summary.selected_path_digest(),
            prior_summary.complete(),
            prior_summary.last_activity_at().max(at),
        );
        let selection = DraftSealedImportSelectionV1 {
            prior_selector: p.basis.selector,
            successor_selector,
            before_session: before,
            after_session: after,
            imported_root: root,
            prior_history: prior,
            baseline: Box::new(baseline),
            before_mutable_history: mutable,
            after_mutable_history: Box::new(fork),
            basis: p.basis,
            prior_draft: draft,
            successor_draft: Box::new(next_draft),
            prior_summary: prior_summary,
            successor_summary: Box::new(successor_summary),
        };
        if !selection.is_locally_exact(&source) {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let (terminal, receipt) =
            terminal_transition(&source, DraftPieceBuildLifecycleV1::Committed)?;
        let settlement = make_settlement(
            &source,
            receipt.reference(),
            DraftPieceSettlementOutcomeV1::Committed {
                candidate_generation: selection.after_session.newest_candidate_generation(),
                successor: root,
                history: selection.baseline.reference(),
                caret: source.caret(),
                selection: source.selection(),
            },
            DraftPieceSettlementClosureV1::SealedImport(selection.clone()),
        )?;
        if self.revision(store)? != revision
            || store.health().generation() != Some(self.home_generation)
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(PreparedDraftReplacementStartV1 {
            observation: Default::default(),
            scope: p.scope.clone(),
            source,
            selection: Box::new(selection),
            terminal: Box::new(terminal),
            receipt: Box::new(receipt),
            settlement: Box::new(settlement),
            home_generation: self.home_generation,
        })
    }
    pub fn start_replacement_draft(
        &self,
        revision: beryl_model::DomainRevision,
        p: PreparedDraftReplacementStartV1,
    ) -> MutationContribution {
        let generation_matches = p.home_generation == self.home_generation;
        p.scope.handle.clone().contribution(
            revision,
            Start {
                p,
                generation_matches,
            },
        )
    }
}

struct Start {
    p: PreparedDraftReplacementStartV1,
    generation_matches: bool,
}
impl DomainMutation<SyndicDomain> for Start {
    type Error = DraftReplacementErrorV1;
    type Prepared = Option<PreparedDraftReplacementStartV1>;
    fn prepare(self, r: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        if !self.generation_matches {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let p = self.p;
        let c = &p.selection;
        if let Some(stored) =
            SourceReader::point::<DraftPieceSettlementsFamily>(r, &p.settlement.key())?
        {
            if stored != *p.settlement {
                return Err(DraftReplacementErrorV1::Conflict);
            }
            authenticate_selected_immutable(r, &p)?;
            return Ok(None);
        }
        authenticate_source(r, c.basis)?;
        if required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.source
            || required::<DraftsFamily>(r, &c.prior_draft.id())? != *c.prior_draft
            || required::<HistorySummariesFamily>(r, &c.prior_summary.thread_id())?
                != *c.prior_summary
            || required::<DraftEditorCandidateSessionsFamily>(r, &head_key(&c.before_session))?
                != DraftEditorCandidateSessionRecordV1::Head(*c.before_session.clone())
            || required::<DraftEditHistoryFrontiersFamily>(r, &c.prior_history.reference().key())?
                != *c.prior_history
            || required::<DraftEditHistoryFrontiersFamily>(
                r,
                &c.before_mutable_history.reference().key(),
            )? != *c.before_mutable_history
            || required::<DraftPieceRootsFamily>(r, &c.imported_root.key())?.reference()
                != c.imported_root
            || SourceReader::point::<DraftEditHistoryFrontiersFamily>(
                r,
                &c.baseline.reference().key(),
            )?
            .is_some()
            || SourceReader::point::<DraftPieceBuildProgressFamily>(r, &p.receipt.key())?.is_some()
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let source_receipt =
            required::<DraftPieceBuildProgressFamily>(r, &p.source.progress_receipt().key())?;
        mutation::authenticate_progress_receipt(r, &source_receipt)?;
        if !progress_receipt_is_exact(&source_receipt)
            || !progress_receipt_matches_build(&source_receipt, &p.source)
            || !super::super::checkpoint::saved_checkpoint_is_exact_in_transaction(
                r,
                &c.before_session,
                &c.prior_history,
            )?
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(Some(p))
    }
    fn reserve_reconciliation(
        &self,
        r: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        r.reserve_records::<DraftsCodec>(1)?;
        r.reserve_records::<DraftByThreadCodec>(1)?;
        r.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        r.reserve_records::<DraftEditHistoryFrontiersCodec>(2)?;
        r.reserve_records::<DraftPieceSettlementsCodec>(1)?;
        r.reserve_records::<DraftPieceBuildsCodec>(1)?;
        r.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        r.reserve_records::<HistorySummariesCodec>(1)?;
        Ok(())
    }
    fn contribute(
        p: Self::Prepared,
        w: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(p) = p {
            let c = &p.selection;
            w.put::<DraftsCodec>(&c.successor_draft.id(), &c.successor_draft)?;
            w.put::<HistorySummariesCodec>(&c.successor_summary.thread_id(), &c.successor_summary)?;
            let reverse = DraftByThreadRecord::new(
                c.successor_selector.thread_id(),
                c.successor_selector.draft_id(),
                c.successor_selector.selector_revision(),
                c.successor_selector.thread_revision(),
            );
            w.put::<DraftByThreadCodec>(&reverse.thread_id(), &reverse)?;
            w.put::<DraftEditHistoryFrontiersCodec>(&c.baseline.reference().key(), &c.baseline)?;
            w.put::<DraftEditHistoryFrontiersCodec>(
                &c.after_mutable_history.reference().key(),
                &c.after_mutable_history,
            )?;
            w.put::<DraftPieceSettlementsCodec>(&p.settlement.key(), &p.settlement)?;
            put_transition(w, &p.terminal, &p.receipt, &c.after_session)?;
        }
        Ok(())
    }
}
fn authenticate_selected_immutable(
    r: &DomainReader<'_, SyndicDomain>,
    p: &PreparedDraftReplacementStartV1,
) -> Result<(), DraftReplacementErrorV1> {
    if required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.terminal
        || required::<DraftPieceBuildProgressFamily>(r, &p.receipt.key())? != *p.receipt
        || required::<DraftEditHistoryFrontiersFamily>(r, &p.selection.baseline.reference().key())?
            != *p.selection.baseline
        || required::<DraftPieceRootsFamily>(r, &p.selection.imported_root.key())?.reference()
            != p.selection.imported_root
    {
        return Err(DraftReplacementErrorV1::Conflict);
    }
    Ok(())
}

pub(super) fn terminal_transition(
    source: &DraftPieceBuildRecordV1,
    lifecycle: DraftPieceBuildLifecycleV1,
) -> Result<(DraftPieceBuildRecordV1, DraftPieceBuildProgressReceiptV1), DraftReplacementErrorV1> {
    let target = DraftPieceBuildRecordV1::new(
        source.draft_id(),
        source.session_id(),
        source.predecessor_candidate_generation(),
        source.predecessor_root(),
        source.predecessor_history(),
        source.operation_id(),
        source.predecessor_caret(),
        source.predecessor_selection(),
        source.caret(),
        source.selection(),
        source.fragment_count(),
        source.fragment_chain(),
        source.canonical_header().to_vec(),
        source.staged_fragment_count(),
        source.staged_fragment_chain(),
        source.proposal_digest(),
        source.working_roots(),
        source.base_frontier(),
        source.successor_frontier(),
        source.next_record_ordinal(),
        source.frontier(),
        source.progress_receipt(),
        source.successor(),
        source.build_digest(),
        lifecycle,
    );
    authenticated_build_transition(target, Some(source.progress_receipt()), None)
        .map_err(|()| DraftReplacementErrorV1::Conflict)
}
pub(super) fn make_settlement(
    source: &DraftPieceBuildRecordV1,
    terminal: DraftPieceBuildProgressReceiptReferenceV1,
    outcome: DraftPieceSettlementOutcomeV1,
    closure: DraftPieceSettlementClosureV1,
) -> Result<DraftPieceSettlementV1, DraftReplacementErrorV1> {
    let result = DraftPieceSettlementV1::new(
        DraftPieceSettlementKeyV1::new(
            source.draft_id(),
            source.session_id(),
            source.operation_id(),
        ),
        source.proposal_digest(),
        source.predecessor_candidate_generation(),
        source.predecessor_root(),
        source.predecessor_history(),
        source.fragment_count(),
        source.fragment_chain(),
        source.predecessor_caret(),
        source.predecessor_selection(),
        source.caret(),
        source.selection(),
        source.build_digest(),
        source.canonical_header().to_vec(),
        Some(source.clone()),
        terminal,
        outcome,
        closure,
    );
    if !settlement_closure_is_exact(&result) {
        return Err(DraftReplacementErrorV1::Conflict);
    }
    Ok(result)
}
