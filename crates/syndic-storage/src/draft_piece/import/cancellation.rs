use super::super::*;
use super::{
    admission::required,
    cancellation_codec::canonical_cancellation_request,
    cancellation_model::*,
    read::{SourceReader, StoreSourceReader},
    request::*,
};
use crate::{
    DraftByThreadRecord, DraftRecord, DraftSubmissionIntent, InputGateState, SyndicStorage,
    codec::*, domain::SyndicDomain,
};
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};

#[derive(Clone)]
pub struct PreparedDraftReplacementCancellationV1 {
    pub(crate) observation: super::outcome_state::CommandObservation,
    pub(crate) scope: ImportCommandScope,
    pub(crate) receipt: Box<DraftReplacementCancellationReceiptV1>,
}
impl PreparedDraftReplacementCancellationV1 {
    pub fn receipt(&self) -> &DraftReplacementCancellationReceiptV1 {
        &self.receipt
    }
}
impl SyndicStorage {
    pub fn prepare_replacement_intent_cancellation(
        &self,
        store: &HomeStore,
        request: DraftReplacementCancellationRequestV1,
    ) -> Result<PreparedDraftReplacementCancellationV1, DraftReplacementErrorV1> {
        let revision = self.revision(store)?;
        let reader = StoreSourceReader {
            storage: self,
            store,
        };
        let key = DraftEditorCandidateSessionRecordKeyV1::replacement_intent_cancellation_receipt(
            request.selector.draft_id(),
            request.session,
            request.operation,
        );
        if let Some(stored) = reader.point::<DraftEditorCandidateSessionsFamily>(&key)? {
            let DraftEditorCandidateSessionRecordV1::OpenReceipt(stored) = stored else {
                return Err(DraftReplacementErrorV1::Conflict);
            };
            let stored = stored
                .replacement_cancellation()
                .ok_or(DraftReplacementErrorV1::Conflict)?;
            if stored.request != request || !stored.is_locally_exact() {
                return Err(DraftReplacementErrorV1::Conflict);
            }
            return Ok(PreparedDraftReplacementCancellationV1 {
                observation: Default::default(),
                scope: ImportCommandScope::capture(self, store)?,
                receipt: Box::new(stored.clone()),
            });
        }
        let DraftEditorCandidateSessionRecordV1::Head(before) = reader
            .point::<DraftEditorCandidateSessionsFamily>(
                &DraftEditorCandidateSessionRecordKeyV1::head(
                    request.selector.draft_id(),
                    request.session,
                ),
            )?
            .ok_or(DraftReplacementErrorV1::Conflict)?
        else {
            return Err(DraftReplacementErrorV1::Conflict);
        };
        let prior_draft = reader
            .point::<DraftsFamily>(&request.selector.draft_id())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let newest_history = reader
            .point::<DraftEditHistoryFrontiersFamily>(&before.newest_history().key())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let mutable_history = reader
            .point::<DraftEditHistoryFrontiersFamily>(&DraftEditHistoryFrontierKeyV1::session(
                before.draft_id(),
                before.session_id(),
            ))?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        let revision_next = prior_draft
            .revision()
            .checked_next()
            .map_err(crate::SyndicMutationError::from)?;
        let successor_selector = DraftEditorCurrentSelectorV1::new(
            request.selector.thread_id(),
            request.selector.thread_revision(),
            prior_draft.id(),
            revision_next,
            prior_draft.piece_root(),
            prior_draft.history(),
        );
        let after = cancelled_session(&before, successor_selector)
            .ok_or(DraftReplacementErrorV1::Ineligible)?;
        let successor_draft = DraftRecord::new(
            prior_draft.id(),
            prior_draft.thread_id(),
            revision_next,
            DraftSubmissionIntent::Ordinary,
            prior_draft.root_history(),
            prior_draft.created_at(),
            request.at,
        );
        let receipt = DraftReplacementCancellationReceiptV1 {
            request,
            request_bytes: canonical_cancellation_request(request),
            prior_selector: request.selector,
            successor_selector,
            before: Box::new(before),
            after,
            prior_draft: Box::new(prior_draft),
            successor_draft: Box::new(successor_draft),
            newest_history: Box::new(newest_history),
            mutable_history: Box::new(mutable_history),
        };
        if !receipt.is_locally_exact()
            || self.revision(store)? != revision
            || !session::idle_candidate_closure_is_exact_with_access(
                self,
                crate::read::access::ReadAccess::Ordinary(store),
                &receipt.before,
            )?
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(PreparedDraftReplacementCancellationV1 {
            observation: Default::default(),
            scope: ImportCommandScope::capture(self, store)?,
            receipt: Box::new(receipt),
        })
    }
    pub fn cancel_replacement_intent(
        &self,
        revision: beryl_model::DomainRevision,
        p: PreparedDraftReplacementCancellationV1,
    ) -> MutationContribution {
        p.scope
            .handle
            .clone()
            .contribution(revision, CancelIntent { p })
    }
}
struct CancelIntent {
    p: PreparedDraftReplacementCancellationV1,
}
impl DomainMutation<SyndicDomain> for CancelIntent {
    type Error = DraftReplacementErrorV1;
    type Prepared = Option<PreparedDraftReplacementCancellationV1>;
    fn prepare(self, r: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        let p = self.p;
        let c = &p.receipt;
        if let Some(stored) =
            SourceReader::point::<DraftEditorCandidateSessionsFamily>(r, &c.key())?
        {
            return if stored == record(c) && c.is_locally_exact() {
                Ok(None)
            } else {
                Err(DraftReplacementErrorV1::Conflict)
            };
        }
        let thread = required::<ThreadsFamily>(r, &c.prior_selector.thread_id())?;
        let reverse = required::<DraftByThreadFamily>(r, &thread.id())?;
        let gate = required::<InputGatesFamily>(r, &thread.id())?;
        crate::mutation::discussion_mutation::require_editable(r, thread.id())?;
        if !c.is_locally_exact()
            || thread.revision() != c.prior_selector.thread_revision()
            || thread.current_draft_id() != c.prior_selector.draft_id()
            || reverse.draft_id() != c.prior_selector.draft_id()
            || reverse.draft_revision() != c.prior_selector.selector_revision()
            || reverse.thread_revision() != thread.revision()
            || gate.state() != &InputGateState::Idle
            || gate.live_count() != 0
            || required::<DraftsFamily>(r, &c.prior_draft.id())? != *c.prior_draft
            || required::<DraftEditorCandidateSessionsFamily>(
                r,
                &super::command::head_key(&c.before),
            )? != DraftEditorCandidateSessionRecordV1::Head(*c.before.clone())
            || required::<DraftEditHistoryFrontiersFamily>(r, &c.newest_history.reference().key())?
                != *c.newest_history
            || required::<DraftEditHistoryFrontiersFamily>(r, &c.mutable_history.reference().key())?
                != *c.mutable_history
            || !session::idle_candidate_closure_is_exact_in_transaction(r, &c.before)?
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
        r.reserve_records::<DraftEditorCandidateSessionsCodec>(2)?;
        Ok(())
    }
    fn contribute(
        p: Self::Prepared,
        w: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(p) = p {
            let c = &p.receipt;
            w.put::<DraftsCodec>(&c.successor_draft.id(), &c.successor_draft)?;
            let reverse = DraftByThreadRecord::new(
                c.successor_selector.thread_id(),
                c.successor_selector.draft_id(),
                c.successor_selector.selector_revision(),
                c.successor_selector.thread_revision(),
            );
            w.put::<DraftByThreadCodec>(&reverse.thread_id(), &reverse)?;
            w.put::<DraftEditorCandidateSessionsCodec>(
                &super::command::head_key(&c.after),
                &DraftEditorCandidateSessionRecordV1::Head(*c.after.clone()),
            )?;
            w.put::<DraftEditorCandidateSessionsCodec>(&c.key(), &record(c))?;
        }
        Ok(())
    }
}
fn record(c: &DraftReplacementCancellationReceiptV1) -> DraftEditorCandidateSessionRecordV1 {
    DraftEditorCandidateSessionRecordV1::OpenReceipt(
        DraftEditorCandidateSessionOpenReceiptV1::from_replacement_cancellation(c.clone()),
    )
}
