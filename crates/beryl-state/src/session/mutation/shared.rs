use beryl_home_store::{DomainReader, MutationBuilder, PointReadLimit};
use beryl_model::{ClaimRevision, SessionRevision, SyndicThreadId, WindowId};

use crate::RecordRevision;

use super::super::{
    codec::{
        ClaimByThreadCodec, ClaimByWindowCodec, SessionHeaderCodec, SessionWindowCodec, HEADER_KEY,
    },
    SessionDomain, SessionExitIntent, SessionHeader, SessionMutationError, SessionWindowRecord,
    ThreadClaimRecord, WindowClaimSelection, CLAIM_V1_BYTES, SESSION_HEADER_V1_BYTES,
    SESSION_WINDOW_V1_BYTES,
};

pub(super) fn header(
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<Option<SessionHeader>, SessionMutationError> {
    reader
        .point::<SessionHeaderCodec>(&HEADER_KEY, point_limit(SESSION_HEADER_V1_BYTES))
        .map_err(Into::into)
}

pub(super) fn required_header(
    reader: &DomainReader<'_, SessionDomain>,
    expected: SessionRevision,
    require_running: bool,
) -> Result<SessionHeader, SessionMutationError> {
    let header = header(reader)?.ok_or(SessionMutationError::NotInitialized)?;
    if header.revision != expected {
        return Err(SessionMutationError::SessionRevisionConflict {
            expected,
            current: header.revision,
        });
    }
    if require_running && header.exit_intent == SessionExitIntent::OrderlyExit {
        return Err(SessionMutationError::OrderlyExitInProgress);
    }
    Ok(header)
}

pub(super) fn required_window(
    reader: &DomainReader<'_, SessionDomain>,
    header: &SessionHeader,
    window_id: WindowId,
    expected: RecordRevision,
) -> Result<SessionWindowRecord, SessionMutationError> {
    let reference = header
        .windows
        .iter()
        .find(|reference| reference.window_id == window_id)
        .ok_or(SessionMutationError::WindowMissing { window_id })?;
    if reference.record_revision != expected {
        return Err(SessionMutationError::WindowRevisionConflict {
            window_id,
            expected,
            current: reference.record_revision,
        });
    }
    let window = reader
        .point::<SessionWindowCodec>(&window_id, point_limit(SESSION_WINDOW_V1_BYTES))?
        .ok_or(SessionMutationError::WindowMissing { window_id })?;
    if window.revision != expected {
        return Err(SessionMutationError::WindowRevisionConflict {
            window_id,
            expected,
            current: window.revision,
        });
    }
    Ok(window)
}

pub(super) fn claim_by_window(
    reader: &DomainReader<'_, SessionDomain>,
    window_id: WindowId,
) -> Result<Option<ThreadClaimRecord>, SessionMutationError> {
    reader
        .point::<ClaimByWindowCodec>(&window_id, point_limit(CLAIM_V1_BYTES))
        .map_err(Into::into)
}

pub(super) fn claim_by_thread(
    reader: &DomainReader<'_, SessionDomain>,
    thread_id: SyndicThreadId,
) -> Result<Option<ThreadClaimRecord>, SessionMutationError> {
    reader
        .point::<ClaimByThreadCodec>(&thread_id, point_limit(CLAIM_V1_BYTES))
        .map_err(Into::into)
}

pub(super) fn required_claim(
    reader: &DomainReader<'_, SessionDomain>,
    window_id: WindowId,
    selection: WindowClaimSelection,
) -> Result<ThreadClaimRecord, SessionMutationError> {
    let by_window = claim_by_window(reader, window_id)?
        .ok_or(SessionMutationError::ClaimMissing { window_id })?;
    let by_thread = claim_by_thread(reader, selection.thread_id)?
        .ok_or(SessionMutationError::ClaimMissing { window_id })?;
    if by_window != by_thread
        || by_window.window_id != window_id
        || by_window.selection() != selection
    {
        return Err(SessionMutationError::ClaimCopiesDisagree { window_id });
    }
    Ok(by_window)
}

pub(super) fn ensure_claim_expectation(
    window_id: WindowId,
    expected: Option<WindowClaimSelection>,
    current: Option<WindowClaimSelection>,
) -> Result<(), SessionMutationError> {
    if expected == current {
        Ok(())
    } else {
        Err(SessionMutationError::ClaimExpectationConflict {
            window_id,
            expected,
            current,
        })
    }
}

pub(super) fn put_claim(
    mutations: &mut MutationBuilder<'_, SessionDomain>,
    claim: ThreadClaimRecord,
) -> Result<(), SessionMutationError> {
    mutations.put::<ClaimByWindowCodec>(&claim.window_id, &claim)?;
    mutations.put::<ClaimByThreadCodec>(&claim.thread_id, &claim)?;
    Ok(())
}

pub(super) fn delete_claim(
    mutations: &mut MutationBuilder<'_, SessionDomain>,
    claim: ThreadClaimRecord,
    delete_window_copy: bool,
) -> Result<(), SessionMutationError> {
    if delete_window_copy {
        mutations.delete::<ClaimByWindowCodec>(&claim.window_id)?;
    }
    mutations.delete::<ClaimByThreadCodec>(&claim.thread_id)?;
    Ok(())
}

pub(super) fn put_header(
    mutations: &mut MutationBuilder<'_, SessionDomain>,
    header: &SessionHeader,
) -> Result<(), SessionMutationError> {
    mutations.put::<SessionHeaderCodec>(&HEADER_KEY, header)?;
    Ok(())
}

pub(super) fn put_window(
    mutations: &mut MutationBuilder<'_, SessionDomain>,
    window: &SessionWindowRecord,
) -> Result<(), SessionMutationError> {
    mutations.put::<SessionWindowCodec>(&window.window_id, window)?;
    Ok(())
}

pub(super) fn replace_reference(
    header: &mut SessionHeader,
    window_id: WindowId,
    revision: RecordRevision,
) -> Result<(), SessionMutationError> {
    let reference = header
        .windows
        .iter_mut()
        .find(|reference| reference.window_id == window_id)
        .ok_or(SessionMutationError::WindowMissing { window_id })?;
    reference.record_revision = revision;
    Ok(())
}

pub(super) fn insert_reference(
    header: &mut SessionHeader,
    window_id: WindowId,
    revision: RecordRevision,
) -> Result<(), SessionMutationError> {
    match header
        .windows
        .binary_search_by_key(&window_id, |reference| reference.window_id)
    {
        Ok(_) => Err(SessionMutationError::WindowExists { window_id }),
        Err(index) => {
            header.windows.insert(
                index,
                super::super::SessionWindowReference::new(window_id, revision),
            );
            Ok(())
        }
    }
}

pub(super) fn initial_session_revision() -> SessionRevision {
    SessionRevision::new(1).expect("initial session revision is nonzero")
}

pub(super) fn initial_claim_revision() -> ClaimRevision {
    ClaimRevision::new(1).expect("initial claim revision is nonzero")
}

pub(super) fn point_limit(payload: usize) -> PointReadLimit {
    PointReadLimit::new(payload + 4).expect("fixed session point limit is nonzero")
}
