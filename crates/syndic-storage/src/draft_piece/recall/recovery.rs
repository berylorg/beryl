use super::super::import::read::SourceReader;
use super::super::*;
use super::*;
use crate::{
    ContentByteSpanRecord, SyndicReadError, SyndicStorage, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{
    CommandOutcome, CursorDirection, CursorRange, CursorReadLimits, DomainHandle,
    HomeCandidateRecoveryAccess, MutationContribution,
};
use beryl_model::{BerylHomeId, DomainRevision, SyndicContentId};

#[derive(Clone)]
pub(crate) enum RecallTerminationScope {
    Ordinary(RecallScope),
    Candidate {
        home: BerylHomeId,
        handle: DomainHandle<SyndicDomain>,
    },
}
impl RecallTerminationScope {
    pub fn home_id(&self) -> BerylHomeId {
        match self {
            Self::Ordinary(s) => s.home,
            Self::Candidate { home, .. } => *home,
        }
    }
    pub fn handle(&self) -> &DomainHandle<SyndicDomain> {
        match self {
            Self::Ordinary(s) => &s.handle,
            Self::Candidate { handle, .. } => handle,
        }
    }
}
struct CandidateReader<'a, 'b> {
    storage: &'a SyndicStorage,
    access: &'a HomeCandidateRecoveryAccess<'b>,
}
impl SourceReader for CandidateReader<'_, '_> {
    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, SyndicReadError> {
        Ok(self.access.read_point::<SyndicDomain, ExactCodec<F>>(
            &self.storage.handle,
            key,
            family_point_limit::<F>(),
        )?)
    }
    fn byte_span(
        &self,
        owner: SyndicContentId,
        cursor: u64,
    ) -> Result<ContentByteSpanRecord, SyndicReadError> {
        let page = self
            .access
            .read_cursor::<SyndicDomain, ContentByteSpansCodec>(
                &self.storage.handle,
                &CursorRange::closed(
                    ContentByteSpanKey { owner, start: 0 },
                    ContentByteSpanKey {
                        owner,
                        start: cursor,
                    },
                ),
                CursorDirection::Reverse,
                CursorReadLimits::new(1, 512).unwrap(),
            )?;
        page.records()
            .first()
            .map(|r| *r.value())
            .ok_or(SyndicReadError::Invariant("recall source span unavailable"))
    }
}
impl SyndicStorage {
    pub fn observe_composer_root_replacement_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        p: &PreparedDraftComposerRootReplacementV1,
    ) -> Result<bool, DraftComposerRecallErrorV1> {
        outcome::require_home(access.home_id(), p.scope.home)?;
        p.observation.resume_candidate(access)
    }
    pub fn resume_composer_root_replacement_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        p: &PreparedDraftComposerRootReplacementV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        let committed = self.observe_composer_root_replacement_candidate(access, p)?;
        self.read_composer_root_replacement(
            &CandidateReader {
                storage: self,
                access,
            },
            p,
            committed,
        )
    }
    pub fn inspect_composer_root_replacement_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        p: &PreparedDraftComposerRootReplacementV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        self.resume_composer_root_replacement_candidate(access, p)
    }
    pub fn observe_composer_recall_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        p: &PreparedDraftComposerRecallTerminationV1,
    ) -> Result<bool, DraftComposerRecallErrorV1> {
        outcome::require_home(access.home_id(), p.scope.home_id())?;
        p.observation.resume_candidate(access)
    }
    pub fn resume_composer_recall_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        p: &PreparedDraftComposerRecallTerminationV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        let committed = self.observe_composer_recall_termination_candidate(access, p)?;
        self.read_composer_recall_termination(
            &CandidateReader {
                storage: self,
                access,
            },
            p,
            committed,
        )
    }
    pub fn inspect_composer_recall_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        p: &PreparedDraftComposerRecallTerminationV1,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        self.resume_composer_recall_termination_candidate(access, p)
    }
    pub fn reconcile_composer_recall_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        p: &PreparedDraftComposerRecallTerminationV1,
        outcome: CommandOutcome,
    ) -> Result<DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1> {
        p.observation.retain(outcome)?;
        self.resume_composer_recall_termination_candidate(access, p)
    }
    pub fn prepare_composer_recall_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        key: DraftPieceSettlementKeyV1,
    ) -> Result<PreparedDraftComposerRecallTerminationV1, DraftComposerRecallErrorV1> {
        self.prepare_composer_recall_termination_from_reader(
            &CandidateReader {
                storage: self,
                access,
            },
            key,
            RecallTerminationScope::Candidate {
                home: access.home_id(),
                handle: self.handle.clone(),
            },
        )
    }
    pub fn terminate_composer_recall_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        revision: DomainRevision,
        p: PreparedDraftComposerRecallTerminationV1,
    ) -> Result<MutationContribution, DraftComposerRecallErrorV1> {
        outcome::require_home(access.home_id(), p.scope.home_id())?;
        access.domain_revision(&self.handle)?;
        Ok(self.terminate_composer_recall(revision, p))
    }
}
