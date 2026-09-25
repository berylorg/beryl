use super::shared::{
    claim_by_window, header, initial_session_revision, point_limit, put_header, put_window,
};
use crate::{
    RecordRevision,
    session::{
        SESSION_WINDOW_V1_BYTES, SessionDomain, SessionExitIntent, SessionHeader,
        SessionMutationError, SessionWindowRecord, SessionWindowReference,
        codec::{SessionHeaderCodec, SessionWindowCodec},
    },
};
use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{SessionRevision, WindowId, WindowPlacement};

#[cfg(test)]
mod tests;

pub struct InitializeThreadlessWindow {
    expected_session_revision: Option<SessionRevision>,
    window_id: WindowId,
    placement: WindowPlacement,
}

pub(crate) struct InitializeThreadlessWindowPrepared {
    header: SessionHeader,
    window: SessionWindowRecord,
}

impl InitializeThreadlessWindow {
    pub const fn new(window_id: WindowId, placement: WindowPlacement) -> Self {
        Self {
            expected_session_revision: None,
            window_id,
            placement,
        }
    }

    pub const fn for_empty_session(
        expected_session_revision: SessionRevision,
        window_id: WindowId,
        placement: WindowPlacement,
    ) -> Self {
        Self {
            expected_session_revision: Some(expected_session_revision),
            window_id,
            placement,
        }
    }
}

impl DomainMutation<SessionDomain> for InitializeThreadlessWindow {
    type Error = SessionMutationError;
    type Prepared = InitializeThreadlessWindowPrepared;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let revision = match (self.expected_session_revision, header(reader)?) {
            (None, None) => initial_session_revision(),
            (None, Some(_)) => return Err(SessionMutationError::AlreadyInitialized),
            (Some(_), None) => return Err(SessionMutationError::NotInitialized),
            (Some(expected), Some(current)) => {
                if current.revision != expected {
                    return Err(SessionMutationError::SessionRevisionConflict {
                        expected,
                        current: current.revision,
                    });
                }
                if !current.windows.is_empty() || current.fallback.is_some() {
                    return Err(SessionMutationError::InvalidCurrentState(
                        "threadless initialization requires an empty session without a runtime fallback",
                    ));
                }
                current.revision.checked_next()?
            }
        };
        if reader
            .point::<SessionWindowCodec>(&self.window_id, point_limit(SESSION_WINDOW_V1_BYTES))?
            .is_some()
            || claim_by_window(reader, self.window_id)?.is_some()
        {
            return Err(SessionMutationError::WindowExists {
                window_id: self.window_id,
            });
        }
        let window = SessionWindowRecord {
            window_id: self.window_id,
            remembered_target: None,
            selected_thread: None,
            placement: self.placement,
            revision: RecordRevision::INITIAL,
        };
        let header = SessionHeader {
            revision,
            exit_intent: SessionExitIntent::Running,
            fallback: None,
            windows: vec![SessionWindowReference::new(
                window.window_id,
                window.revision,
            )],
        };
        Ok(InitializeThreadlessWindowPrepared { header, window })
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
