use super::super::*;
use super::{
    cancellation::PreparedDraftReplacementCancellationV1,
    read::{SourceReader, StoreSourceReader},
    request::DraftReplacementErrorV1,
    selection::PreparedDraftReplacementStartV1,
    termination::PreparedDraftReplacementImportTerminationV1,
};
use crate::{DraftByThreadRecord, SyndicStorage, codec::*};
use beryl_home_store::{CommandOutcome, HomeStore};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftReplacementCommandOutcomeV1 {
    Started(DraftEditorCurrentSelectorV1),
    IntentCancelled(DraftEditorCurrentSelectorV1),
    ImportTerminated,
    NotCommitted,
}

impl SyndicStorage {
    fn read_replacement_draft_start(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementStartV1,
        committed: bool,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        if store.home_id() != p.scope.home_id {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        if committed {
            if get::<DraftPieceSettlementsFamily>(&r, &p.settlement.key())? != *p.settlement
                || get::<DraftPieceBuildsFamily>(&r, &p.settlement.key())? != *p.terminal
                || get::<DraftPieceBuildProgressFamily>(&r, &p.receipt.key())? != *p.receipt
                || get::<DraftEditHistoryFrontiersFamily>(
                    &r,
                    &p.selection.baseline.reference().key(),
                )? != *p.selection.baseline
                || get::<DraftPieceRootsFamily>(&r, &p.selection.imported_root.key())?.reference()
                    != p.selection.imported_root
            {
                return Err(DraftReplacementErrorV1::Conflict);
            }
            return Ok(DraftReplacementCommandOutcomeV1::Started(
                p.selection.successor_selector,
            ));
        }
        let c = &p.selection;
        if r.point::<DraftPieceSettlementsFamily>(&p.settlement.key())?
            .is_some()
            || r.point::<DraftPieceBuildProgressFamily>(&p.receipt.key())?
                .is_some()
            || r.point::<DraftEditHistoryFrontiersFamily>(&c.baseline.reference().key())?
                .is_some()
            || get::<DraftPieceBuildsFamily>(&r, &p.settlement.key())? != *p.source
            || get::<DraftEditorCandidateSessionsFamily>(
                &r,
                &super::command::head_key(&c.before_session),
            )? != DraftEditorCandidateSessionRecordV1::Head(*c.before_session.clone())
            || get::<DraftsFamily>(&r, &c.prior_draft.id())? != *c.prior_draft
            || get::<HistorySummariesFamily>(&r, &c.prior_summary.thread_id())? != *c.prior_summary
            || get::<DraftEditHistoryFrontiersFamily>(
                &r,
                &c.before_mutable_history.reference().key(),
            )? != *c.before_mutable_history
            || get::<DraftByThreadFamily>(&r, &c.prior_selector.thread_id())?
                != reverse(c.prior_selector)
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(DraftReplacementCommandOutcomeV1::NotCommitted)
    }
    fn read_replacement_intent_cancellation(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementCancellationV1,
        committed: bool,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        if store.home_id() != p.scope.home_id {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let c = &p.receipt;
        let receipt = r.point::<DraftEditorCandidateSessionsFamily>(&c.key())?;
        if committed {
            let Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt)) = receipt else {
                return Err(DraftReplacementErrorV1::Conflict);
            };
            if receipt.replacement_cancellation() != Some(c.as_ref()) || !c.is_locally_exact() {
                return Err(DraftReplacementErrorV1::Conflict);
            }
            return Ok(DraftReplacementCommandOutcomeV1::IntentCancelled(
                c.successor_selector,
            ));
        }
        if receipt.is_some()
            || get::<DraftsFamily>(&r, &c.prior_draft.id())? != *c.prior_draft
            || get::<DraftByThreadFamily>(&r, &c.prior_selector.thread_id())?
                != reverse(c.prior_selector)
            || get::<DraftEditorCandidateSessionsFamily>(&r, &super::command::head_key(&c.before))?
                != DraftEditorCandidateSessionRecordV1::Head(*c.before.clone())
            || get::<DraftEditHistoryFrontiersFamily>(&r, &c.newest_history.reference().key())?
                != *c.newest_history
            || get::<DraftEditHistoryFrontiersFamily>(&r, &c.mutable_history.reference().key())?
                != *c.mutable_history
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(DraftReplacementCommandOutcomeV1::NotCommitted)
    }
    fn read_replacement_import_termination(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementImportTerminationV1,
        committed: bool,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        if store.home_id() != p.scope.home_id {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        if committed {
            if get::<DraftPieceSettlementsFamily>(&r, &p.settlement.key())? != *p.settlement
                || get::<DraftPieceBuildsFamily>(&r, &p.settlement.key())? != *p.terminal
                || get::<DraftPieceBuildProgressFamily>(&r, &p.receipt.key())? != *p.receipt
            {
                return Err(DraftReplacementErrorV1::Conflict);
            }
            return Ok(DraftReplacementCommandOutcomeV1::ImportTerminated);
        }
        if r.point::<DraftPieceSettlementsFamily>(&p.settlement.key())?
            .is_some()
            || r.point::<DraftPieceBuildProgressFamily>(&p.receipt.key())?
                .is_some()
            || get::<DraftPieceBuildsFamily>(&r, &p.settlement.key())? != *p.source
            || get::<DraftEditorCandidateSessionsFamily>(&r, &super::command::head_key(&p.before))?
                != DraftEditorCandidateSessionRecordV1::Head(*p.before.clone())
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(DraftReplacementCommandOutcomeV1::NotCommitted)
    }
}
impl SyndicStorage {
    pub fn reconcile_replacement_draft_start(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementStartV1,
        outcome: CommandOutcome,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        p.observation.retain(outcome)?;
        require_home(store, p.scope.home_id)?;
        self.read_replacement_draft_start(store, p, p.observation.resume(store)?)
    }
    pub fn resume_replacement_draft_start(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementStartV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        require_home(store, p.scope.home_id)?;
        self.read_replacement_draft_start(store, p, p.observation.resume(store)?)
    }
    pub fn inspect_replacement_draft_start(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementStartV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        require_home(store, p.scope.home_id)?;
        self.read_replacement_draft_start(store, p, p.observation.known()?)
    }
    pub fn reconcile_replacement_intent_cancellation(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementCancellationV1,
        outcome: CommandOutcome,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        p.observation.retain(outcome)?;
        require_home(store, p.scope.home_id)?;
        self.read_replacement_intent_cancellation(store, p, p.observation.resume(store)?)
    }
    pub fn resume_replacement_intent_cancellation(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementCancellationV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        require_home(store, p.scope.home_id)?;
        self.read_replacement_intent_cancellation(store, p, p.observation.resume(store)?)
    }
    pub fn inspect_replacement_intent_cancellation(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementCancellationV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        require_home(store, p.scope.home_id)?;
        self.read_replacement_intent_cancellation(store, p, p.observation.known()?)
    }
    pub fn reconcile_replacement_import_termination(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementImportTerminationV1,
        outcome: CommandOutcome,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        p.observation.retain(outcome)?;
        require_home(store, p.scope.home_id)?;
        self.read_replacement_import_termination(store, p, p.observation.resume(store)?)
    }
    pub fn resume_replacement_import_termination(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementImportTerminationV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        require_home(store, p.scope.home_id)?;
        self.read_replacement_import_termination(store, p, p.observation.resume(store)?)
    }
    pub fn inspect_replacement_import_termination(
        &self,
        store: &HomeStore,
        p: &PreparedDraftReplacementImportTerminationV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        require_home(store, p.scope.home_id)?;
        self.read_replacement_import_termination(store, p, p.observation.known()?)
    }
}
fn require_home(
    store: &HomeStore,
    home: beryl_model::BerylHomeId,
) -> Result<(), DraftReplacementErrorV1> {
    if store.home_id() == home {
        Ok(())
    } else {
        Err(DraftReplacementErrorV1::Proof)
    }
}
fn reverse(selector: DraftEditorCurrentSelectorV1) -> DraftByThreadRecord {
    DraftByThreadRecord::new(
        selector.thread_id(),
        selector.draft_id(),
        selector.selector_revision(),
        selector.thread_revision(),
    )
}
fn get<F: Family>(
    r: &impl SourceReader,
    key: &F::Key,
) -> Result<F::Value, DraftReplacementErrorV1> {
    r.point::<F>(key)?.ok_or(DraftReplacementErrorV1::Conflict)
}
