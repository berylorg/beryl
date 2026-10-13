use super::super::*;
use super::{
    admission::required,
    command::{custody, head_key, put_transition},
    read::{SourceReader, StoreSourceReader},
    request::*,
    selection::{make_settlement, terminal_transition},
};
use crate::{SyndicStorage, codec::*, domain::SyndicDomain};
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};

#[derive(Clone)]
pub struct PreparedDraftReplacementImportTerminationV1 {
    pub(crate) observation: super::outcome_state::CommandObservation,
    pub(crate) scope: ImportCommandScope,
    pub(crate) source: Box<DraftPieceBuildRecordV1>,
    pub(crate) before: Box<DraftEditorCandidateSessionV1>,
    pub(crate) after: Box<DraftEditorCandidateSessionV1>,
    pub(crate) terminal: Box<DraftPieceBuildRecordV1>,
    pub(crate) receipt: Box<DraftPieceBuildProgressReceiptV1>,
    pub(crate) settlement: Box<DraftPieceSettlementV1>,
}
impl PreparedDraftReplacementImportTerminationV1 {
    pub fn settlement(&self) -> &DraftPieceSettlementV1 {
        &self.settlement
    }
}
impl SyndicStorage {
    pub fn prepare_replacement_import_termination(
        &self,
        store: &HomeStore,
        key: DraftPieceSettlementKeyV1,
    ) -> Result<PreparedDraftReplacementImportTerminationV1, DraftReplacementErrorV1> {
        let revision = self.revision(store)?;
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let source = r
            .point::<DraftPieceBuildsFamily>(&key)?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        if !import_build_is_exact(&source)
            || !matches!(
                source.lifecycle(),
                DraftPieceBuildLifecycleV1::Open | DraftPieceBuildLifecycleV1::Complete
            )
            || r.point::<DraftPieceSettlementsFamily>(&key)?.is_some()
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let DraftEditorCandidateSessionRecordV1::Head(before) = r
            .point::<DraftEditorCandidateSessionsFamily>(
                &DraftEditorCandidateSessionRecordKeyV1::head(key.draft_id(), key.session_id()),
            )?
            .ok_or(DraftReplacementErrorV1::Conflict)?
        else {
            return Err(DraftReplacementErrorV1::Conflict);
        };
        let after = before
            .clear_active_operation(&custody(&source))
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let history = r
            .point::<DraftEditHistoryFrontiersFamily>(&before.newest_history().key())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
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
        if self.revision(store)? != revision {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(PreparedDraftReplacementImportTerminationV1 {
            observation: Default::default(),
            scope: ImportCommandScope::capture(self, store)?,
            source: Box::new(source),
            before: Box::new(before),
            after: Box::new(after),
            terminal: Box::new(terminal),
            receipt: Box::new(receipt),
            settlement: Box::new(settlement),
        })
    }
    pub fn terminate_replacement_draft_import(
        &self,
        revision: beryl_model::DomainRevision,
        p: PreparedDraftReplacementImportTerminationV1,
    ) -> MutationContribution {
        p.scope
            .handle
            .clone()
            .contribution(revision, Terminate { p })
    }
}
struct Terminate {
    p: PreparedDraftReplacementImportTerminationV1,
}
impl DomainMutation<SyndicDomain> for Terminate {
    type Error = DraftReplacementErrorV1;
    type Prepared = Option<PreparedDraftReplacementImportTerminationV1>;
    fn prepare(self, r: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        let p = self.p;
        if let Some(stored) =
            SourceReader::point::<DraftPieceSettlementsFamily>(r, &p.settlement.key())?
        {
            if stored != *p.settlement
                || required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.terminal
                || required::<DraftPieceBuildProgressFamily>(r, &p.receipt.key())? != *p.receipt
            {
                return Err(DraftReplacementErrorV1::Conflict);
            }
            return Ok(None);
        }
        let prior =
            required::<DraftPieceBuildProgressFamily>(r, &p.source.progress_receipt().key())?;
        mutation::authenticate_progress_receipt(r, &prior)?;
        if required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.source
            || required::<DraftEditorCandidateSessionsFamily>(r, &head_key(&p.before))?
                != DraftEditorCandidateSessionRecordV1::Head(*p.before.clone())
            || p.before.active_operation() != Some(&custody(&p.source))
            || !progress_receipt_matches_build(&prior, &p.source)
            || !import_transition_is_exact(&prior, &p.receipt)
            || SourceReader::point::<DraftPieceBuildProgressFamily>(r, &p.receipt.key())?.is_some()
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let DraftPieceSettlementClosureV1::Noncommit(c) = p.settlement.closure() else {
            return Err(DraftReplacementErrorV1::Conflict);
        };
        if required::<DraftEditHistoryFrontiersFamily>(r, &c.observed_history().reference().key())?
            != *c.observed_history()
        {
            return Err(DraftReplacementErrorV1::Conflict);
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
