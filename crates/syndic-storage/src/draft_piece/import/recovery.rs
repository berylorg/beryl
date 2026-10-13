use super::super::*;
use super::{
    cancellation::PreparedDraftReplacementCancellationV1,
    outcome::DraftReplacementCommandOutcomeV1,
    read::SourceReader,
    request::{DraftReplacementErrorV1, ImportCommandScope, PreparedDraftReplacementImportV1},
    selection::PreparedDraftReplacementStartV1,
    termination::PreparedDraftReplacementImportTerminationV1,
};
use crate::{
    ContentByteSpanRecord, SyndicReadError, SyndicStorage, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{
    CommandOutcome, CursorDirection, CursorRange, CursorReadLimits, DomainHandle,
    HomeCandidateRecoveryAccess, MutationContribution,
};
use beryl_model::{BerylHomeId, DomainRevision, SyndicContentId};

#[derive(Clone)]
pub(crate) enum TerminationScope {
    Ordinary(ImportCommandScope),
    Candidate {
        home: BerylHomeId,
        handle: DomainHandle<SyndicDomain>,
    },
}

impl ImportCommandScope {
    pub(crate) fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
}

impl TerminationScope {
    pub(crate) fn home_id(&self) -> BerylHomeId {
        match self {
            Self::Ordinary(scope) => scope.home_id,
            Self::Candidate { home, .. } => *home,
        }
    }
    pub(crate) fn handle(&self) -> &DomainHandle<SyndicDomain> {
        match self {
            Self::Ordinary(scope) => &scope.handle,
            Self::Candidate { handle, .. } => handle,
        }
    }
}

struct CandidateSourceReader<'a, 'b> {
    storage: &'a SyndicStorage,
    access: &'a HomeCandidateRecoveryAccess<'b>,
}

impl SourceReader for CandidateSourceReader<'_, '_> {
    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, SyndicReadError> {
        self.access
            .read_point::<SyndicDomain, ExactCodec<F>>(
                &self.storage.handle,
                key,
                family_point_limit::<F>(),
            )
            .map_err(Into::into)
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
            .map(|record| *record.value())
            .ok_or(SyndicReadError::Invariant(
                "import source byte span missing",
            ))
    }
}

impl SyndicStorage {
    pub fn observe_replacement_draft_start_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementStartV1,
    ) -> Result<bool, DraftReplacementErrorV1> {
        require_home(access, prepared.scope.home_id)?;
        prepared.observation.resume_candidate(access)
    }

    pub fn observe_replacement_intent_cancellation_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementCancellationV1,
    ) -> Result<bool, DraftReplacementErrorV1> {
        require_home(access, prepared.scope.home_id)?;
        prepared.observation.resume_candidate(access)
    }

    pub fn observe_replacement_import_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementImportTerminationV1,
    ) -> Result<bool, DraftReplacementErrorV1> {
        require_home(access, prepared.scope.home_id())?;
        prepared.observation.resume_candidate(access)
    }

    pub fn resume_replacement_draft_start_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementStartV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        let committed = self.observe_replacement_draft_start_candidate(access, prepared)?;
        self.read_replacement_draft_start(
            &CandidateSourceReader {
                storage: self,
                access,
            },
            prepared,
            committed,
        )
    }

    pub fn reconcile_replacement_draft_start_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementStartV1,
        outcome: CommandOutcome,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        prepared.observation.retain(outcome)?;
        self.resume_replacement_draft_start_candidate(access, prepared)
    }

    pub fn resume_replacement_intent_cancellation_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementCancellationV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        let committed = self.observe_replacement_intent_cancellation_candidate(access, prepared)?;
        self.read_replacement_intent_cancellation(
            &CandidateSourceReader {
                storage: self,
                access,
            },
            prepared,
            committed,
        )
    }

    pub fn reconcile_replacement_intent_cancellation_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementCancellationV1,
        outcome: CommandOutcome,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        prepared.observation.retain(outcome)?;
        self.resume_replacement_intent_cancellation_candidate(access, prepared)
    }

    pub fn resume_replacement_import_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementImportTerminationV1,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        let committed = self.observe_replacement_import_termination_candidate(access, prepared)?;
        self.read_replacement_import_termination(
            &CandidateSourceReader {
                storage: self,
                access,
            },
            prepared,
            committed,
        )
    }

    pub fn reconcile_replacement_import_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedDraftReplacementImportTerminationV1,
        outcome: CommandOutcome,
    ) -> Result<DraftReplacementCommandOutcomeV1, DraftReplacementErrorV1> {
        prepared.observation.retain(outcome)?;
        self.resume_replacement_import_termination_candidate(access, prepared)
    }

    pub fn prepare_replacement_import_termination_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        original: &PreparedDraftReplacementImportV1,
    ) -> Result<PreparedDraftReplacementImportTerminationV1, DraftReplacementErrorV1> {
        require_home(access, original.scope.home_id)?;
        let revision = self.revision_candidate(access)?;
        let reader = CandidateSourceReader {
            storage: self,
            access,
        };
        let key = DraftPieceSettlementKeyV1::new(
            original.basis.selector().draft_id(),
            original.session_id(),
            original.operation_id(),
        );
        let prepared = self.prepare_replacement_import_termination_from_reader(
            &reader,
            key,
            TerminationScope::Candidate {
                home: access.home_id(),
                handle: self.handle.clone(),
            },
        )?;
        if prepared.source.canonical_header() != original.canonical_header()
            || prepared.before.thread_id() != original.basis.selector().thread_id()
            || self.revision_candidate(access)? != revision
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(prepared)
    }

    pub fn terminate_replacement_draft_import_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        revision: DomainRevision,
        prepared: PreparedDraftReplacementImportTerminationV1,
    ) -> Result<MutationContribution, DraftReplacementErrorV1> {
        require_home(access, prepared.scope.home_id())?;
        if !matches!(prepared.scope, TerminationScope::Candidate { .. }) {
            return Err(DraftReplacementErrorV1::Proof);
        }
        if access.domain_revision(prepared.scope.handle())? != revision
            || self.revision_candidate(access)? != revision
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(self.terminate_replacement_draft_import(revision, prepared))
    }
}

fn require_home(
    access: &HomeCandidateRecoveryAccess<'_>,
    home: BerylHomeId,
) -> Result<(), DraftReplacementErrorV1> {
    if access.home_id() == home {
        Ok(())
    } else {
        Err(DraftReplacementErrorV1::Proof)
    }
}
