use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{SessionRevision, WindowId, WindowPlacement};

use crate::RecordRevision;

use crate::session::{
    SessionDomain, SessionExitIntent, SessionHeader, SessionMutationError, SessionWindowRecord,
    ThreadClaimRecord, WindowClaimSelection,
    codec::{ClaimByThreadCodec, ClaimByWindowCodec, SessionHeaderCodec, SessionWindowCodec},
};

use super::shared::{
    delete_claim, ensure_claim_expectation, put_header, put_window, replace_reference,
    required_claim, required_header, required_window,
};

/// Persist one changed placement under exact session and window revisions.
pub struct UpdateWindowPlacement {
    expected_session_revision: SessionRevision,
    window_id: WindowId,
    expected_window_revision: RecordRevision,
    placement: WindowPlacement,
}

pub(crate) struct UpdateWindowPlacementPrepared {
    header: SessionHeader,
    window: SessionWindowRecord,
}

impl UpdateWindowPlacement {
    #[must_use]
    pub const fn new(
        expected_session_revision: SessionRevision,
        window_id: WindowId,
        expected_window_revision: RecordRevision,
        placement: WindowPlacement,
    ) -> Self {
        Self {
            expected_session_revision,
            window_id,
            expected_window_revision,
            placement,
        }
    }
}

impl DomainMutation<SessionDomain> for UpdateWindowPlacement {
    type Error = SessionMutationError;
    type Prepared = UpdateWindowPlacementPrepared;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let (mut header, mut window) = prepare_placement(&self, reader)?;
        window.placement = self.placement;
        window.revision = window.revision.checked_next()?;
        header.revision = header.revision.checked_next()?;
        replace_reference(&mut header, self.window_id, window.revision)?;
        Ok(UpdateWindowPlacementPrepared { header, window })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        reservation.reserve_records::<SessionWindowCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        put_window(mutations, &prepared.window)?;
        put_header(mutations, &prepared.header)
    }
}

fn prepare_placement(
    command: &UpdateWindowPlacement,
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<(SessionHeader, SessionWindowRecord), SessionMutationError> {
    let header = required_header(reader, command.expected_session_revision, true)?;
    let window = required_window(
        reader,
        &header,
        command.window_id,
        command.expected_window_revision,
    )?;
    if window.placement == command.placement {
        return Err(SessionMutationError::PlacementUnchanged {
            window_id: command.window_id,
        });
    }
    Ok((header, window))
}

/// Remove one active window and its claim, retaining the session fallback.
pub struct RemoveSessionWindow {
    expected_session_revision: SessionRevision,
    window_id: WindowId,
    expected_window_revision: RecordRevision,
    expected_claim: Option<WindowClaimSelection>,
}

pub(crate) struct RemoveSessionWindowPrepared {
    header: SessionHeader,
    window_id: WindowId,
    claim: Option<ThreadClaimRecord>,
}

impl RemoveSessionWindow {
    #[must_use]
    pub const fn new(
        expected_session_revision: SessionRevision,
        window_id: WindowId,
        expected_window_revision: RecordRevision,
        expected_claim: Option<WindowClaimSelection>,
    ) -> Self {
        Self {
            expected_session_revision,
            window_id,
            expected_window_revision,
            expected_claim,
        }
    }
}

impl DomainMutation<SessionDomain> for RemoveSessionWindow {
    type Error = SessionMutationError;
    type Prepared = RemoveSessionWindowPrepared;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let (mut header, _window, claim) = prepare_remove(&self, reader)?;
        let index = header
            .windows
            .binary_search_by_key(&self.window_id, |reference| reference.window_id)
            .map_err(|_| SessionMutationError::WindowMissing {
                window_id: self.window_id,
            })?;
        header.windows.remove(index);
        header.revision = header.revision.checked_next()?;
        Ok(RemoveSessionWindowPrepared {
            header,
            window_id: self.window_id,
            claim,
        })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        reservation.reserve_records::<SessionWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByThreadCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<SessionWindowCodec>(&prepared.window_id)?;
        if let Some(claim) = prepared.claim {
            delete_claim(mutations, claim, true)?;
        }
        put_header(mutations, &prepared.header)
    }
}

fn prepare_remove(
    command: &RemoveSessionWindow,
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<
    (
        SessionHeader,
        SessionWindowRecord,
        Option<ThreadClaimRecord>,
    ),
    SessionMutationError,
> {
    let header = required_header(reader, command.expected_session_revision, true)?;
    let window = required_window(
        reader,
        &header,
        command.window_id,
        command.expected_window_revision,
    )?;
    ensure_claim_expectation(
        command.window_id,
        command.expected_claim,
        window.selected_thread,
    )?;
    let claim = window
        .selected_thread
        .map(|selection| required_claim(reader, command.window_id, selection))
        .transpose()?;
    Ok((header, window, claim))
}

/// Mark dedicated application Exit without processing windows as ordinary closes.
pub struct MarkOrderlyExit {
    expected_session_revision: SessionRevision,
}

pub(crate) struct MarkOrderlyExitPrepared {
    header: SessionHeader,
}

impl MarkOrderlyExit {
    #[must_use]
    pub const fn new(expected_session_revision: SessionRevision) -> Self {
        Self {
            expected_session_revision,
        }
    }
}

impl DomainMutation<SessionDomain> for MarkOrderlyExit {
    type Error = SessionMutationError;
    type Prepared = MarkOrderlyExitPrepared;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut header = required_header(reader, self.expected_session_revision, false)?;
        if header.exit_intent == SessionExitIntent::OrderlyExit {
            return Err(SessionMutationError::AlreadyOrderlyExit);
        }
        header.exit_intent = SessionExitIntent::OrderlyExit;
        header.revision = header.revision.checked_next()?;
        Ok(MarkOrderlyExitPrepared { header })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        put_header(mutations, &prepared.header)
    }
}
