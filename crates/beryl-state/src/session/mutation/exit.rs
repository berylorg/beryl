use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{SessionRevision, WindowId, WindowPlacement};

use crate::RecordRevision;
use crate::session::{
    MAX_RESTORABLE_WINDOWS, SessionDomain, SessionExitIntent, SessionHeader, SessionMutationError,
    SessionWindowRecord,
    codec::{SessionHeaderCodec, SessionWindowCodec},
};

use super::shared::{put_header, put_window, replace_reference, required_header, required_window};

pub struct ExitWindowPlacement {
    window_id: WindowId,
    expected_revision: RecordRevision,
    placement: WindowPlacement,
}

impl ExitWindowPlacement {
    #[must_use]
    pub const fn new(
        window_id: WindowId,
        expected_revision: RecordRevision,
        placement: WindowPlacement,
    ) -> Self {
        Self {
            window_id,
            expected_revision,
            placement,
        }
    }
}

pub struct PublishExitSession {
    expected_session_revision: SessionRevision,
    windows: Vec<ExitWindowPlacement>,
}

impl PublishExitSession {
    pub fn new(
        expected_session_revision: SessionRevision,
        mut windows: Vec<ExitWindowPlacement>,
    ) -> Result<Self, SessionMutationError> {
        if windows.is_empty() || windows.len() > MAX_RESTORABLE_WINDOWS {
            return Err(SessionMutationError::InvalidExitWindowSet);
        }
        windows.sort_unstable_by_key(|window| window.window_id);
        if windows
            .windows(2)
            .any(|pair| pair[0].window_id == pair[1].window_id)
        {
            return Err(SessionMutationError::InvalidExitWindowSet);
        }
        Ok(Self {
            expected_session_revision,
            windows,
        })
    }
}

pub(crate) struct PublishExitSessionPrepared {
    header: SessionHeader,
    windows: Vec<SessionWindowRecord>,
}

impl DomainMutation<SessionDomain> for PublishExitSession {
    type Error = SessionMutationError;
    type Prepared = PublishExitSessionPrepared;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut header = required_header(reader, self.expected_session_revision, true)?;
        if header.windows.len() != self.windows.len()
            || header
                .windows
                .iter()
                .zip(&self.windows)
                .any(|(reference, supplied)| reference.window_id != supplied.window_id)
        {
            return Err(SessionMutationError::InvalidExitWindowSet);
        }
        let mut windows = Vec::with_capacity(self.windows.len());
        for supplied in self.windows {
            let mut window = required_window(
                reader,
                &header,
                supplied.window_id,
                supplied.expected_revision,
            )?;
            window.placement = supplied.placement;
            window.revision = window.revision.checked_next()?;
            replace_reference(&mut header, window.window_id, window.revision)?;
            windows.push(window);
        }
        header.revision = header.revision.checked_next()?;
        header.exit_intent = SessionExitIntent::OrderlyExit;
        Ok(PublishExitSessionPrepared { header, windows })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        reservation.reserve_records::<SessionWindowCodec>(self.windows.len())?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        for window in prepared.windows {
            put_window(mutations, &window)?;
        }
        put_header(mutations, &prepared.header)
    }
}
