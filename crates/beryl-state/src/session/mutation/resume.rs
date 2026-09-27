use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{SessionRevision, WindowId};

use crate::RecordRevision;
use crate::session::{
    MAX_RESTORABLE_WINDOWS, SessionDomain, SessionExitIntent, SessionHeader, SessionMutationError,
    codec::SessionHeaderCodec,
};

use super::shared::{put_header, required_header};

pub struct ResumeSessionAfterExit {
    expected_session_revision: SessionRevision,
    windows: Vec<(WindowId, RecordRevision)>,
}

impl ResumeSessionAfterExit {
    pub fn new(
        expected_session_revision: SessionRevision,
        mut windows: Vec<(WindowId, RecordRevision)>,
    ) -> Result<Self, SessionMutationError> {
        if windows.is_empty() || windows.len() > MAX_RESTORABLE_WINDOWS {
            return Err(SessionMutationError::InvalidResumeWindowSet);
        }
        windows.sort_unstable_by_key(|(id, _)| *id);
        if windows.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(SessionMutationError::InvalidResumeWindowSet);
        }
        Ok(Self {
            expected_session_revision,
            windows,
        })
    }
}

impl DomainMutation<SessionDomain> for ResumeSessionAfterExit {
    type Error = SessionMutationError;
    type Prepared = SessionHeader;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut header = required_header(reader, self.expected_session_revision, false)?;
        if header.exit_intent != SessionExitIntent::OrderlyExit {
            return Err(SessionMutationError::NotOrderlyExit);
        }
        if header.windows.len() != self.windows.len()
            || header
                .windows
                .iter()
                .zip(&self.windows)
                .any(|(reference, supplied)| {
                    (reference.window_id, reference.record_revision) != *supplied
                })
        {
            return Err(SessionMutationError::InvalidResumeWindowSet);
        }
        header.revision = header.revision.checked_next()?;
        header.exit_intent = SessionExitIntent::Running;
        Ok(header)
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
        put_header(mutations, &prepared)
    }
}
