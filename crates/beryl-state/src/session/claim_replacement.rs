use std::path::PathBuf;

use beryl_home_store::{
    HomeCandidateRecoveryAccess, HomeGenerationIdentity, HomeStore, MutationContribution,
    PointReadLimit,
};
use beryl_model::{BerylHomeId, DomainRevision, SyndicThreadId, WindowId};

use super::{
    CLAIM_V1_BYTES, RememberedTarget, ReplaceWindowClaim, SESSION_HEADER_V1_BYTES,
    SESSION_WINDOW_V1_BYTES, SessionDomain, SessionHeader, SessionMutationError, SessionState,
    SessionWindowRecord, SessionWindowRemovalEvidence, ThreadClaimRecord, WindowClaimSelection,
    codec::{
        ClaimByThreadCodec, ClaimByWindowCodec, HEADER_KEY, SessionHeaderCodec, SessionWindowCodec,
    },
};

#[derive(Debug)]
pub enum WindowClaimReplacementPreparation {
    Current {
        window: SessionWindowRecord,
        claim: ThreadClaimRecord,
    },
    ClaimedElsewhere {
        claim: ThreadClaimRecord,
    },
    Prepared(PreparedWindowClaimReplacement),
}

#[derive(Debug)]
pub struct PreparedWindowClaimReplacement {
    home_id: BerylHomeId,
    canonical_path: PathBuf,
    generation: HomeGenerationIdentity,
    session_revision: DomainRevision,
    original: SessionWindowRemovalEvidence,
    header: SessionHeader,
    window: SessionWindowRecord,
    claim: ThreadClaimRecord,
    target: RememberedTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowClaimReplacementState {
    Original,
    Committed,
    Collision,
}

impl PreparedWindowClaimReplacement {
    pub fn catalog_claim(&self) -> crate::catalog::CatalogWindowClaim {
        crate::catalog::CatalogWindowClaim::active(
            self.claim.thread_id(),
            self.claim.window_id(),
            self.claim.revision(),
        )
    }

    pub fn future_selection(&self) -> WindowClaimSelection {
        self.claim.selection()
    }
    pub fn future_window(&self) -> &SessionWindowRecord {
        &self.window
    }
    pub fn prior_claim(&self) -> Option<ThreadClaimRecord> {
        self.original.claim()
    }
    pub fn future_claim(&self) -> ThreadClaimRecord {
        self.claim
    }
    pub fn target(&self) -> RememberedTarget {
        self.target
    }
    pub fn session_revision(&self) -> DomainRevision {
        self.session_revision
    }

    fn command(&self) -> ReplaceWindowClaim {
        ReplaceWindowClaim::new(
            self.original.header().revision(),
            self.window.window_id(),
            self.original.window().revision(),
            self.original.window().selected_thread(),
            self.target,
            self.claim.thread_id(),
        )
    }

    fn qualify(&self, store: &HomeStore) -> Result<(), SessionMutationError> {
        if store.home_id() != self.home_id
            || store.canonical_path() != self.canonical_path
            || store.generation_identity()? != self.generation
        {
            return Err(invalid(
                "claim replacement belongs to another home generation",
            ));
        }
        Ok(())
    }

    pub fn contribution(
        &self,
        state: &SessionState,
        store: &HomeStore,
    ) -> Result<MutationContribution, SessionMutationError> {
        self.qualify(store)?;
        if state.revision(store)? != self.session_revision {
            return Err(invalid("claim replacement session source changed"));
        }
        Ok(state.replace_claim(self.session_revision, self.command()))
    }
}

impl SessionState {
    pub fn prepare_window_claim_replacement(
        &self,
        store: &HomeStore,
        window_id: WindowId,
        expected_selected: Option<WindowClaimSelection>,
        target: RememberedTarget,
        thread_id: SyndicThreadId,
    ) -> Result<WindowClaimReplacementPreparation, SessionMutationError> {
        let home_revision = store.home_revision()?;
        let generation = store.generation_identity()?;
        let session_revision = self.revision(store)?;
        let original = self.capture_window_removal(store, window_id)?;
        if original.window().selected_thread() != expected_selected {
            return Err(invalid("claim replacement predecessor selection changed"));
        }
        let target_claim = self.replacement_thread_claim(store, thread_id)?;
        if let Some(claim) = target_claim {
            let occupied = self.capture_window_removal(store, claim.window_id())?;
            if occupied.claim() != Some(claim)
                || occupied.window().remembered_target() != Some(target)
            {
                return Err(invalid(
                    "claim replacement target window does not select its exact binding",
                ));
            }
        }
        if store.home_revision()? != home_revision || store.generation_identity()? != generation {
            return Err(invalid("claim replacement sources changed during capture"));
        }
        if let Some(claim) = target_claim {
            if original.claim() == Some(claim) {
                return Ok(WindowClaimReplacementPreparation::Current {
                    window: original.window().clone(),
                    claim,
                });
            }
            return Ok(WindowClaimReplacementPreparation::ClaimedElsewhere { claim });
        }
        let command = ReplaceWindowClaim::new(
            original.header().revision(),
            window_id,
            original.window().revision(),
            expected_selected,
            target,
            thread_id,
        );
        let (header, window, claim) = command.planned_records(
            original.header().clone(),
            original.window().clone(),
            original.claim(),
        )?;
        Ok(WindowClaimReplacementPreparation::Prepared(
            PreparedWindowClaimReplacement {
                home_id: store.home_id(),
                canonical_path: store.canonical_path().to_owned(),
                generation,
                session_revision,
                original,
                header,
                window,
                claim,
                target,
            },
        ))
    }

    pub fn classify_window_claim_replacement(
        &self,
        store: &HomeStore,
        prepared: &PreparedWindowClaimReplacement,
    ) -> Result<WindowClaimReplacementState, SessionMutationError> {
        prepared.qualify(store)?;
        let home_revision = store.home_revision()?;
        let current = self.capture_window_removal(store, prepared.window.window_id())?;
        let target = self.replacement_thread_claim(store, prepared.claim.thread_id())?;
        let prior = prepared
            .original
            .claim()
            .map(|claim| self.replacement_thread_claim(store, claim.thread_id()))
            .transpose()?
            .flatten();
        if store.home_revision()? != home_revision {
            return Err(invalid("claim replacement sources changed during audit"));
        }
        if current.header() == prepared.original.header()
            && current.window() == prepared.original.window()
            && current.claim() == prepared.original.claim()
            && prior == prepared.original.claim()
            && target.is_none()
        {
            return Ok(WindowClaimReplacementState::Original);
        }
        if current.header() == &prepared.header
            && current.window() == &prepared.window
            && current.claim() == Some(prepared.claim)
            && target == Some(prepared.claim)
            && prior.is_none()
        {
            return Ok(WindowClaimReplacementState::Committed);
        }
        Ok(WindowClaimReplacementState::Collision)
    }

    pub fn classify_window_claim_replacement_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        prepared: &PreparedWindowClaimReplacement,
    ) -> Result<WindowClaimReplacementState, SessionMutationError> {
        if access.home_id() != prepared.home_id
            || access.canonical_path() != prepared.canonical_path
        {
            return Err(invalid("claim replacement belongs to another home"));
        }
        let revision = access.home_revision()?;
        let header = access.read_point::<SessionDomain, SessionHeaderCodec>(
            &self.handle,
            &HEADER_KEY,
            PointReadLimit::new(SESSION_HEADER_V1_BYTES + 4).expect("fixed header limit"),
        )?;
        let window = access.read_point::<SessionDomain, SessionWindowCodec>(
            &self.handle,
            &prepared.window.window_id(),
            PointReadLimit::new(SESSION_WINDOW_V1_BYTES + 4).expect("fixed window limit"),
        )?;
        let limit = PointReadLimit::new(CLAIM_V1_BYTES + 4).expect("fixed claim limit");
        let by_window = access.read_point::<SessionDomain, ClaimByWindowCodec>(
            &self.handle,
            &prepared.window.window_id(),
            limit,
        )?;
        let target = access.read_point::<SessionDomain, ClaimByThreadCodec>(
            &self.handle,
            &prepared.claim.thread_id(),
            limit,
        )?;
        let prior = prepared
            .original
            .claim()
            .map(|claim| {
                access.read_point::<SessionDomain, ClaimByThreadCodec>(
                    &self.handle,
                    &claim.thread_id(),
                    limit,
                )
            })
            .transpose()?
            .flatten();
        if access.home_revision()? != revision {
            return Err(invalid(
                "claim replacement sources changed during candidate audit",
            ));
        }
        if header.as_ref() == Some(prepared.original.header())
            && window.as_ref() == Some(prepared.original.window())
            && by_window == prepared.original.claim()
            && prior == prepared.original.claim()
            && target.is_none()
        {
            return Ok(WindowClaimReplacementState::Original);
        }
        if header.as_ref() == Some(&prepared.header)
            && window.as_ref() == Some(&prepared.window)
            && by_window == Some(prepared.claim)
            && target == Some(prepared.claim)
            && prior.is_none()
        {
            return Ok(WindowClaimReplacementState::Committed);
        }
        Ok(WindowClaimReplacementState::Collision)
    }

    fn replacement_thread_claim(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<Option<ThreadClaimRecord>, SessionMutationError> {
        let limit = PointReadLimit::new(CLAIM_V1_BYTES + 4).expect("fixed claim limit");
        let claim = store.read_point::<SessionDomain, ClaimByThreadCodec>(
            &self.handle,
            &thread_id,
            limit,
        )?;
        if let Some(claim) = claim {
            let reverse = store.read_point::<SessionDomain, ClaimByWindowCodec>(
                &self.handle,
                &claim.window_id(),
                limit,
            )?;
            if claim.thread_id() != thread_id || reverse != Some(claim) {
                return Err(invalid("claim replacement target copies disagree"));
            }
        }
        Ok(claim)
    }
}

fn invalid(message: &'static str) -> SessionMutationError {
    SessionMutationError::InvalidCurrentState(message)
}
