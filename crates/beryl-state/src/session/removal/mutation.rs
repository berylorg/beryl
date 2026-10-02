use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use super::{
    CLAIM_V1_BYTES, RecoveredWindow, RemovalSource, SESSION_HEADER_V1_BYTES,
    SESSION_WINDOW_V1_BYTES, SessionDomain, SessionMutationError, SessionWindowRemovalEvidence,
    SessionWindowRemovalState, classify, invalid, limit,
};
use crate::session::codec::{
    ClaimByThreadCodec, ClaimByWindowCodec, HEADER_KEY, SessionHeaderCodec, SessionWindowCodec,
};

pub(super) struct RemoveCapturedWindow(pub(super) SessionWindowRemovalEvidence);
pub(super) struct RecoverRemovedWindow(pub(super) SessionWindowRemovalEvidence);

impl DomainMutation<SessionDomain> for RemoveCapturedWindow {
    type Error = SessionMutationError;
    type Prepared = SessionWindowRemovalEvidence;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if classify(&source(reader, &self.0)?, &self.0)? != SessionWindowRemovalState::Original {
            return Err(invalid("captured removal source changed"));
        }
        self.0.recovered()?;
        Ok(self.0)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reserve(reservation, &self.0)
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<SessionWindowCodec>(&prepared.window.window_id)?;
        if let Some(claim) = prepared.claim {
            mutations.delete::<ClaimByWindowCodec>(&claim.window_id)?;
            mutations.delete::<ClaimByThreadCodec>(&claim.thread_id)?;
        }
        mutations.put::<SessionHeaderCodec>(&HEADER_KEY, &prepared.removed_header()?)?;
        Ok(())
    }
}

impl DomainMutation<SessionDomain> for RecoverRemovedWindow {
    type Error = SessionMutationError;
    type Prepared = RecoveredWindow;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if classify(&source(reader, &self.0)?, &self.0)? != SessionWindowRemovalState::Removed {
            return Err(invalid("removed window recovery source changed"));
        }
        self.0.recovered()
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reserve(reservation, &self.0)
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<SessionWindowCodec>(&prepared.window.window_id, &prepared.window)?;
        if let Some(claim) = prepared.claim {
            mutations.put::<ClaimByWindowCodec>(&claim.window_id, &claim)?;
            mutations.put::<ClaimByThreadCodec>(&claim.thread_id, &claim)?;
        }
        mutations.put::<SessionHeaderCodec>(&HEADER_KEY, &prepared.header)?;
        Ok(())
    }
}

fn source(
    reader: &DomainReader<'_, SessionDomain>,
    evidence: &SessionWindowRemovalEvidence,
) -> Result<RemovalSource, SessionMutationError> {
    Ok(RemovalSource {
        header: reader.point::<SessionHeaderCodec>(&HEADER_KEY, limit(SESSION_HEADER_V1_BYTES))?,
        window: reader.point::<SessionWindowCodec>(
            &evidence.window.window_id,
            limit(SESSION_WINDOW_V1_BYTES),
        )?,
        by_window: reader
            .point::<ClaimByWindowCodec>(&evidence.window.window_id, limit(CLAIM_V1_BYTES))?,
        by_thread: evidence
            .claim
            .map(|claim| {
                reader.point::<ClaimByThreadCodec>(&claim.thread_id, limit(CLAIM_V1_BYTES))
            })
            .transpose()?
            .flatten(),
    })
}

fn reserve(
    reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    evidence: &SessionWindowRemovalEvidence,
) -> Result<(), SessionMutationError> {
    reservation.reserve_records::<SessionHeaderCodec>(1)?;
    reservation.reserve_records::<SessionWindowCodec>(1)?;
    if evidence.claim.is_some() {
        reservation.reserve_records::<ClaimByWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByThreadCodec>(1)?;
    }
    Ok(())
}
