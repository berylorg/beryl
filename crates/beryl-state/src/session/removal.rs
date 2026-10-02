use std::path::PathBuf;

use beryl_home_store::{
    DomainHandle, HomeCandidateRecoveryAccess, HomeStore, MutationContribution, PointReadLimit,
    ReadError, RecordCodec,
};
use beryl_model::{BerylHomeId, DomainRevision, WindowId};

use super::{
    CLAIM_V1_BYTES, MAX_RESTORABLE_WINDOWS, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES,
    SessionDomain, SessionExitIntent, SessionHeader, SessionMutationError, SessionState,
    SessionWindowRecord, SessionWindowReference, ThreadClaimRecord, ThreadClaimState,
    codec::{
        ClaimByThreadCodec, ClaimByWindowCodec, HEADER_KEY, SessionHeaderCodec, SessionWindowCodec,
    },
};

mod mutation;

#[derive(Clone, Debug)]
pub struct SessionWindowRemovalEvidence {
    home_id: BerylHomeId,
    canonical_path: PathBuf,
    header: SessionHeader,
    window: SessionWindowRecord,
    claim: Option<ThreadClaimRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionWindowRemovalState {
    Original,
    Removed,
    Recovered,
    Collision,
}

impl SessionWindowRemovalEvidence {
    pub fn header(&self) -> &SessionHeader {
        &self.header
    }

    pub fn window(&self) -> &SessionWindowRecord {
        &self.window
    }

    pub fn claim(&self) -> Option<ThreadClaimRecord> {
        self.claim
    }

    fn removed_header(&self) -> Result<SessionHeader, SessionMutationError> {
        let mut header = self.header.clone();
        header.revision = header.revision.checked_next()?;
        header
            .windows
            .retain(|reference| reference.window_id != self.window.window_id);
        Ok(header)
    }

    fn recovered(&self) -> Result<RecoveredWindow, SessionMutationError> {
        let mut header = self.header.clone();
        header.revision = header.revision.checked_next()?.checked_next()?;
        let mut window = self.window.clone();
        window.revision = window.revision.checked_next()?;
        let claim = self
            .claim
            .map(|mut claim| {
                claim.revision = claim.revision.checked_next()?;
                Ok::<_, SessionMutationError>(claim)
            })
            .transpose()?;
        window.selected_thread = claim.map(|claim| claim.selection());
        let reference = header
            .windows
            .iter_mut()
            .find(|reference| reference.window_id == window.window_id)
            .ok_or(invalid("removal evidence has no source reference"))?;
        *reference = SessionWindowReference::new(window.window_id, window.revision);
        Ok(RecoveredWindow {
            header,
            window,
            claim,
        })
    }
}

pub(super) struct RecoveredWindow {
    header: SessionHeader,
    window: SessionWindowRecord,
    claim: Option<ThreadClaimRecord>,
}

pub(super) struct RemovalSource {
    header: Option<SessionHeader>,
    window: Option<SessionWindowRecord>,
    by_window: Option<ThreadClaimRecord>,
    by_thread: Option<ThreadClaimRecord>,
}

impl SessionState {
    pub fn capture_window_removal(
        &self,
        store: &HomeStore,
        window_id: WindowId,
    ) -> Result<SessionWindowRemovalEvidence, SessionMutationError> {
        let revision = store.home_revision()?;
        let access = ReadAccess::Store(store);
        let header = access.point::<SessionHeaderCodec>(
            &self.handle,
            &HEADER_KEY,
            SESSION_HEADER_V1_BYTES,
        )?;
        let window = access.point::<SessionWindowCodec>(
            &self.handle,
            &window_id,
            SESSION_WINDOW_V1_BYTES,
        )?;
        let by_window =
            access.point::<ClaimByWindowCodec>(&self.handle, &window_id, CLAIM_V1_BYTES)?;
        let thread_id = window
            .as_ref()
            .and_then(|window| window.selected_thread)
            .map(|selection| selection.thread_id);
        let by_thread = thread_id
            .map(|id| access.point::<ClaimByThreadCodec>(&self.handle, &id, CLAIM_V1_BYTES))
            .transpose()?
            .flatten();
        if store.home_revision()? != revision {
            return Err(invalid("session changed during removal capture"));
        }
        let header = header.ok_or(SessionMutationError::NotInitialized)?;
        let window = window.ok_or(SessionMutationError::WindowMissing { window_id })?;
        if header.exit_intent != SessionExitIntent::Running {
            return Err(SessionMutationError::OrderlyExitInProgress);
        }
        if window.window_id != window_id
            || !header
                .windows
                .contains(&SessionWindowReference::new(window_id, window.revision))
        {
            return Err(invalid(
                "removal source window does not match the active header",
            ));
        }
        match window.selected_thread {
            Some(selection) => {
                let claim = by_window.ok_or(SessionMutationError::ClaimMissing { window_id })?;
                if by_thread != Some(claim)
                    || claim.window_id != window_id
                    || claim.selection() != selection
                    || claim.state != ThreadClaimState::Active
                    || claim.generation > header.revision
                    || window.remembered_target.is_none()
                    || header.fallback.is_none()
                {
                    return Err(invalid(
                        "removal source is not an exact active paired claim",
                    ));
                }
            }
            None => {
                if header.windows.len() != 1
                    || header.fallback.is_some()
                    || window.remembered_target.is_some()
                    || by_window.is_some()
                {
                    return Err(invalid(
                        "removal source is not the sole valid threadless window",
                    ));
                }
            }
        }
        let evidence = SessionWindowRemovalEvidence {
            home_id: store.home_id(),
            canonical_path: store.canonical_path().to_owned(),
            header,
            window,
            claim: by_window,
        };
        evidence.recovered()?;
        Ok(evidence)
    }

    pub fn remove_captured_window(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        evidence: &SessionWindowRemovalEvidence,
    ) -> Result<MutationContribution, SessionMutationError> {
        store.domain_revision(&self.handle)?;
        qualify(store.home_id(), store.canonical_path(), evidence)?;
        Ok(self.handle.contribution(
            expected_revision,
            mutation::RemoveCapturedWindow(evidence.clone()),
        ))
    }

    pub fn classify_window_removal_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        evidence: &SessionWindowRemovalEvidence,
    ) -> Result<SessionWindowRemovalState, SessionMutationError> {
        let revision = access.home_revision()?;
        qualify(access.home_id(), access.canonical_path(), evidence)?;
        let source = read_source(&self.handle, ReadAccess::Candidate(access), evidence)?;
        if access.home_revision()? != revision {
            return Err(invalid("session changed during removal classification"));
        }
        classify(&source, evidence)
    }

    pub fn recover_removed_window_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        expected_revision: DomainRevision,
        evidence: &SessionWindowRemovalEvidence,
    ) -> Result<MutationContribution, SessionMutationError> {
        if self.classify_window_removal_candidate(access, evidence)?
            != SessionWindowRemovalState::Removed
        {
            return Err(invalid("window recovery requires the exact removed source"));
        }
        if evidence.header.windows.len() > MAX_RESTORABLE_WINDOWS {
            return Err(SessionMutationError::WindowLimit);
        }
        evidence.recovered()?;
        Ok(self.handle.contribution(
            expected_revision,
            mutation::RecoverRemovedWindow(evidence.clone()),
        ))
    }
}

fn qualify(
    home_id: BerylHomeId,
    path: &std::path::Path,
    evidence: &SessionWindowRemovalEvidence,
) -> Result<(), SessionMutationError> {
    if home_id != evidence.home_id || path != evidence.canonical_path {
        Err(invalid(
            "removal evidence belongs to another configured home",
        ))
    } else {
        Ok(())
    }
}

fn classify(
    source: &RemovalSource,
    evidence: &SessionWindowRemovalEvidence,
) -> Result<SessionWindowRemovalState, SessionMutationError> {
    if source.header.as_ref() == Some(&evidence.header)
        && source.window.as_ref() == Some(&evidence.window)
        && source.by_window == evidence.claim
        && source.by_thread == evidence.claim
    {
        return Ok(SessionWindowRemovalState::Original);
    }
    if source.header.as_ref() == Some(&evidence.removed_header()?)
        && source.window.is_none()
        && source.by_window.is_none()
        && source.by_thread.is_none()
    {
        return Ok(SessionWindowRemovalState::Removed);
    }
    let recovered = evidence.recovered()?;
    if source.header.as_ref() == Some(&recovered.header)
        && source.window.as_ref() == Some(&recovered.window)
        && source.by_window == recovered.claim
        && source.by_thread == recovered.claim
    {
        return Ok(SessionWindowRemovalState::Recovered);
    }
    Ok(SessionWindowRemovalState::Collision)
}

fn read_source(
    handle: &DomainHandle<SessionDomain>,
    access: ReadAccess<'_, '_>,
    evidence: &SessionWindowRemovalEvidence,
) -> Result<RemovalSource, SessionMutationError> {
    Ok(RemovalSource {
        header: access.point::<SessionHeaderCodec>(handle, &HEADER_KEY, SESSION_HEADER_V1_BYTES)?,
        window: access.point::<SessionWindowCodec>(
            handle,
            &evidence.window.window_id,
            SESSION_WINDOW_V1_BYTES,
        )?,
        by_window: access.point::<ClaimByWindowCodec>(
            handle,
            &evidence.window.window_id,
            CLAIM_V1_BYTES,
        )?,
        by_thread: evidence
            .claim
            .map(|claim| {
                access.point::<ClaimByThreadCodec>(handle, &claim.thread_id, CLAIM_V1_BYTES)
            })
            .transpose()?
            .flatten(),
    })
}

#[derive(Clone, Copy)]
enum ReadAccess<'a, 'b> {
    Store(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'b>),
}

impl ReadAccess<'_, '_> {
    fn point<R: RecordCodec<SessionDomain>>(
        &self,
        handle: &DomainHandle<SessionDomain>,
        key: &R::Key,
        bytes: usize,
    ) -> Result<Option<R::Value>, ReadError> {
        match self {
            Self::Store(store) => store.read_point::<SessionDomain, R>(handle, key, limit(bytes)),
            Self::Candidate(access) => {
                access.read_point::<SessionDomain, R>(handle, key, limit(bytes))
            }
        }
    }
}

fn limit(bytes: usize) -> PointReadLimit {
    PointReadLimit::new(bytes + 4).expect("fixed session point limit is nonzero")
}

fn invalid(message: &'static str) -> SessionMutationError {
    SessionMutationError::InvalidCurrentState(message)
}
