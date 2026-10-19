use super::super::import::read::{SourceReader, StoreSourceReader};
use super::super::*;
use super::*;
use crate::SyndicStorage;
use beryl_home_store::{CommandOutcome, HomeStore};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftComposerRecallCommandOutcomeV1 {
    Adopted(DraftEditorCandidateActivationBindingV1),
    Terminated,
    NotCommitted,
}

impl SyndicStorage {
    pub(crate) fn read_composer_root_replacement(
        &self,
        r: &impl SourceReader,
        p: &PreparedDraftComposerRootReplacementV1,
        committed: bool,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        if let DraftComposerRootReplacementPurposeV1::AcceptedRecall(basis) = &p.purpose {
            authenticate_basis(r, basis)?;
        }
        let settlement = r.point::<DraftPieceSettlementsFamily>(&p.key())?;
        if committed {
            let s = settlement.ok_or(DraftComposerRecallErrorV1::Conflict)?;
            let DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) = s.closure() else {
                return Err(DraftComposerRecallErrorV1::Conflict);
            };
            if s.canonical_header() != p.canonical_header
                || c.purpose != p.purpose
                || c.adoption.predecessor_session() != p.before.as_ref()
                || !recall_settlement_is_exact(&s)
                || required::<DraftPieceRootsFamily>(r, &p.root.key())?.reference() != p.root
                || required::<DraftEditHistoryTransitionsFamily>(r, &c.adoption.transition().key())?
                    != *c.adoption.transition()
            {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            if let Some(source) = &p.source {
                let build = required::<DraftPieceBuildsFamily>(r, &p.key())?;
                if !settlement_terminal_build_is_exact(&s, Some(&build))
                    || !progress_receipt_matches_build(
                        &required::<DraftPieceBuildProgressFamily>(
                            r,
                            &build.progress_receipt().key(),
                        )?,
                        &build,
                    )
                    || source.canonical_header() != s.canonical_header()
                {
                    return Err(DraftComposerRecallErrorV1::Conflict);
                }
            }
            let candidate =
                DraftEditorCandidateActivationBindingV1::from_head(c.adoption.adopted_session());
            *p.result
                .lock()
                .map_err(|_| DraftComposerRecallErrorV1::Unavailable)? = Some(s);
            return Ok(DraftComposerRecallCommandOutcomeV1::Adopted(candidate));
        }
        if settlement.is_some()
            || required::<DraftEditorCandidateSessionsFamily>(r, &command::head_key(&p.before))?
                != DraftEditorCandidateSessionRecordV1::Head(*p.before.clone())
            || required::<DraftEditHistoryFrontiersFamily>(r, &p.history.reference().key())?
                != *p.history
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(DraftComposerRecallCommandOutcomeV1::NotCommitted)
    }
    pub(crate) fn read_composer_recall_termination(
        &self,
        r: &impl SourceReader,
        p: &PreparedDraftComposerRecallTerminationV1,
        committed: bool,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        if committed {
            if required::<DraftPieceSettlementsFamily>(r, &p.settlement.key())? != *p.settlement
                || required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.terminal
                || required::<DraftPieceBuildProgressFamily>(r, &p.receipt.key())? != *p.receipt
            {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            return Ok(DraftComposerRecallCommandOutcomeV1::Terminated);
        }
        if r.point::<DraftPieceSettlementsFamily>(&p.settlement.key())?
            .is_some()
            || required::<DraftPieceBuildsFamily>(r, &p.settlement.key())? != *p.source
            || required::<DraftEditorCandidateSessionsFamily>(r, &command::head_key(&p.before))?
                != DraftEditorCandidateSessionRecordV1::Head(*p.before.clone())
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(DraftComposerRecallCommandOutcomeV1::NotCommitted)
    }
    pub fn reconcile_composer_root_replacement(
        &self,
        store: &HomeStore,
        p: &PreparedDraftComposerRootReplacementV1,
        outcome: CommandOutcome,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        p.observation.retain(outcome)?;
        self.resume_composer_root_replacement(store, p)
    }
    pub fn resume_composer_root_replacement(
        &self,
        store: &HomeStore,
        p: &PreparedDraftComposerRootReplacementV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        require_home(store.home_id(), p.scope.home)?;
        self.read_composer_root_replacement(
            &StoreSourceReader {
                storage: self,
                store,
            },
            p,
            p.observation.resume(store)?,
        )
    }
    pub fn inspect_composer_root_replacement(
        &self,
        store: &HomeStore,
        p: &PreparedDraftComposerRootReplacementV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        require_home(store.home_id(), p.scope.home)?;
        self.read_composer_root_replacement(
            &StoreSourceReader {
                storage: self,
                store,
            },
            p,
            p.observation.known()?,
        )
    }
    pub fn reconcile_composer_recall_termination(
        &self,
        store: &HomeStore,
        p: &PreparedDraftComposerRecallTerminationV1,
        outcome: CommandOutcome,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        p.observation.retain(outcome)?;
        self.resume_composer_recall_termination(store, p)
    }
    pub fn resume_composer_recall_termination(
        &self,
        store: &HomeStore,
        p: &PreparedDraftComposerRecallTerminationV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        require_home(store.home_id(), p.scope.home_id())?;
        self.read_composer_recall_termination(
            &StoreSourceReader {
                storage: self,
                store,
            },
            p,
            p.observation.resume(store)?,
        )
    }
    pub fn inspect_composer_recall_termination(
        &self,
        store: &HomeStore,
        p: &PreparedDraftComposerRecallTerminationV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        require_home(store.home_id(), p.scope.home_id())?;
        self.read_composer_recall_termination(
            &StoreSourceReader {
                storage: self,
                store,
            },
            p,
            p.observation.known()?,
        )
    }
}
pub(crate) fn require_home(
    actual: beryl_model::BerylHomeId,
    expected: beryl_model::BerylHomeId,
) -> Result<(), DraftComposerRecallErrorV1> {
    if actual == expected {
        Ok(())
    } else {
        Err(DraftComposerRecallErrorV1::Proof)
    }
}
