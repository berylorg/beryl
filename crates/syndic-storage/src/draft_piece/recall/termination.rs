use super::super::*;
use super::{
    super::import::read::{SourceReader, StoreSourceReader},
    command::{custody, head_key, put_transition},
    request::*,
    source::required,
    writer::terminal_transition,
};
use crate::{SyndicStorage, domain::SyndicDomain};
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};

#[derive(Clone)]
pub struct PreparedDraftComposerRecallTerminationV1 {
    pub(crate) observation: super::outcome_state::CommandObservation,
    pub(crate) scope: super::recovery::RecallTerminationScope,
    pub(crate) source: Box<DraftPieceBuildRecordV1>,
    pub(crate) before: Box<DraftEditorCandidateSessionV1>,
    pub(crate) after: Box<DraftEditorCandidateSessionV1>,
    pub(crate) terminal: Box<DraftPieceBuildRecordV1>,
    pub(crate) receipt: Box<DraftPieceBuildProgressReceiptV1>,
    pub(crate) settlement: Box<DraftPieceSettlementV1>,
}
impl PreparedDraftComposerRecallTerminationV1 {
    pub fn candidate(&self) -> Option<DraftEditorCandidateActivationBindingV1> {
        Some(DraftEditorCandidateActivationBindingV1::from_head(
            &self.after,
        ))
    }
    pub fn settlement(&self) -> &DraftPieceSettlementV1 {
        &self.settlement
    }
}
impl SyndicStorage {
    pub fn prepare_composer_recall_termination(
        &self,
        store: &HomeStore,
        key: DraftPieceSettlementKeyV1,
    ) -> Result<PreparedDraftComposerRecallTerminationV1, DraftComposerRecallErrorV1> {
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let prepared = self.prepare_composer_recall_termination_from_reader(
            &r,
            key,
            super::recovery::RecallTerminationScope::Ordinary(RecallScope::capture(self, store)?),
        )?;
        if self.revision(store)? != revision {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(prepared)
    }
    pub(super) fn prepare_composer_recall_termination_from_reader(
        &self,
        r: &impl SourceReader,
        key: DraftPieceSettlementKeyV1,
        scope: super::recovery::RecallTerminationScope,
    ) -> Result<PreparedDraftComposerRecallTerminationV1, DraftComposerRecallErrorV1> {
        let source = r
            .boxed_point::<DraftPieceBuildsFamily>(&key)?
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        if !recall_build_is_exact(&source)
            || !matches!(
                source.lifecycle(),
                DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
            )
            || r.contains::<DraftPieceSettlementsFamily>(&key)?
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        let before = r.session_head(&DraftEditorCandidateSessionRecordKeyV1::head(
            key.draft_id(),
            key.session_id(),
        ))?;
        let after = before
            .clear_active_operation(&custody(&source))
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        let history = r
            .point::<DraftEditHistoryFrontiersFamily>(&before.newest_history().key())?
            .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        let (terminal, receipt) =
            terminal_transition(&source, DraftPieceBuildLifecycleV1::Cancelled)?;
        let settlement = make_settlement(
            &source,
            receipt.reference(),
            DraftPieceSettlementOutcomeV1::Cancelled,
            DraftPieceSettlementClosureV1::Noncommit(DraftPieceNoncommitClosureV1::new(
                after.clone(),
                history,
                source.successor(),
            )),
        )?;
        Ok(PreparedDraftComposerRecallTerminationV1 {
            observation: Default::default(),
            scope,
            source,
            before,
            after: Box::new(after),
            terminal: Box::new(terminal),
            receipt: Box::new(receipt),
            settlement: Box::new(settlement),
        })
    }
    pub fn terminate_composer_recall(
        &self,
        revision: beryl_model::DomainRevision,
        p: PreparedDraftComposerRecallTerminationV1,
    ) -> MutationContribution {
        p.scope
            .handle()
            .clone()
            .contribution(revision, Terminate { p })
    }
}

fn make_settlement(
    source: &DraftPieceBuildRecordV1,
    terminal: DraftPieceBuildProgressReceiptReferenceV1,
    outcome: DraftPieceSettlementOutcomeV1,
    closure: DraftPieceSettlementClosureV1,
) -> Result<DraftPieceSettlementV1, DraftComposerRecallErrorV1> {
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
        0,
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
    if !recall_settlement_is_exact(&result) {
        return Err(DraftComposerRecallErrorV1::Conflict);
    }
    Ok(result)
}
struct Terminate {
    p: PreparedDraftComposerRecallTerminationV1,
}
impl DomainMutation<SyndicDomain> for Terminate {
    type Error = DraftComposerRecallErrorV1;
    type Prepared = Option<PreparedDraftComposerRecallTerminationV1>;
    fn prepare(self, r: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        let p = self.p;
        if let Some(stored) =
            SourceReader::point::<DraftPieceSettlementsFamily>(r, &p.settlement.key())?
        {
            if stored != *p.settlement
                || required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.terminal
                || required::<DraftPieceBuildProgressFamily>(r, &p.receipt.key())? != *p.receipt
            {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            return Ok(None);
        }
        let prior =
            required::<DraftPieceBuildProgressFamily>(r, &p.source.progress_receipt().key())?;
        mutation::authenticate_progress_receipt_destination(r, &prior)?;
        if required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.source
            || required::<DraftEditorCandidateSessionsFamily>(r, &head_key(&p.before))?
                != DraftEditorCandidateSessionRecordV1::Head(*p.before.clone())
            || p.before.active_operation() != Some(&custody(&p.source))
            || !progress_receipt_matches_build(&prior, &p.source)
            || !recall_transition_is_exact(&prior, &p.receipt)
            || SourceReader::point::<DraftPieceBuildProgressFamily>(r, &p.receipt.key())?.is_some()
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        let DraftPieceSettlementClosureV1::Noncommit(c) = p.settlement.closure() else {
            return Err(DraftComposerRecallErrorV1::Conflict);
        };
        if required::<DraftEditHistoryFrontiersFamily>(r, &c.observed_history().reference().key())?
            != *c.observed_history()
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(Some(p))
    }
    fn reserve_reconciliation(
        &self,
        r: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        r.reserve_records::<DraftPieceBuildsCodec>(1)?;
        r.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        r.reserve_records::<DraftPieceSettlementsCodec>(1)?;
        r.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }
    fn contribute(
        p: Self::Prepared,
        w: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(p) = p {
            w.put::<DraftPieceSettlementsCodec>(&p.settlement.key(), &p.settlement)?;
            put_transition(w, &p.terminal, &p.receipt, &p.after)?;
        }
        Ok(())
    }
}
