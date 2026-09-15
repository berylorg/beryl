use beryl_home_store::{CursorReadLimits, HomeStore};

use crate::{InputGateState, SyndicPointReadLimit, SyndicReadError, domain::SyndicStorage};

use super::*;
use crate::read::access::ReadAccess;

impl SyndicStorage {
    pub fn delivery_recovery_startup_page(
        &self,
        store: &HomeStore,
        cursor: Option<DeliveryRecoveryStartupCursor>,
        limits: CursorReadLimits,
    ) -> Result<DeliveryRecoveryStartupPage, SyndicReadError> {
        self.delivery_recovery_startup_page_with_access(ReadAccess::Ordinary(store), cursor, limits)
    }

    pub fn delivery_recovery_startup_page_candidate(
        &self,
        store: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        cursor: Option<DeliveryRecoveryStartupCursor>,
        limits: CursorReadLimits,
    ) -> Result<DeliveryRecoveryStartupPage, SyndicReadError> {
        self.delivery_recovery_startup_page_with_access(
            ReadAccess::Candidate(store),
            cursor,
            limits,
        )
    }

    fn delivery_recovery_startup_page_with_access(
        &self,
        store: ReadAccess<'_>,
        cursor: Option<DeliveryRecoveryStartupCursor>,
        limits: CursorReadLimits,
    ) -> Result<DeliveryRecoveryStartupPage, SyndicReadError> {
        let revision = match cursor {
            Some(cursor) => cursor.source.source_revision(),
            None => self.revision_with_access(store)?,
        };
        let page = self
            .non_idle_gate_source_page_with_access(
                store,
                revision,
                cursor.map(|c| c.source),
                limits,
            )
            .map_err(startup_error)?;
        let point_limit = SyndicPointReadLimit::new(DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES)
            .expect("fixed startup point limit is nonzero");
        let mut records = Vec::with_capacity(page.records().len());
        for source in page.records() {
            let gate = self.resolve_non_idle_gate_source_with_access(
                store,
                revision,
                *source,
                point_limit,
            )?;
            records.push(DeliveryRecoverySource {
                home_id: store.home_id(),
                home_generation: self.home_generation,
                gate,
            });
        }
        if self.revision_with_access(store)? != revision {
            return Err(SyndicReadError::StaleNonIdleGateSourceScan);
        }
        Ok(DeliveryRecoveryStartupPage {
            records,
            stored_bytes: page.stored_bytes(),
            decoded_bytes: page.decoded_bytes(),
            next_cursor: page
                .next_cursor()
                .map(|source| DeliveryRecoveryStartupCursor { source }),
        })
    }

    pub fn rebase_delivery_recovery_startup_cursor(
        &self,
        store: &HomeStore,
        cursor: DeliveryRecoveryStartupCursor,
    ) -> Result<DeliveryRecoveryStartupCursor, SyndicReadError> {
        self.rebase_non_idle_gate_source_cursor(store, cursor.source)
            .map(|source| DeliveryRecoveryStartupCursor { source })
            .map_err(startup_error)
    }

    pub fn rebase_delivery_recovery_startup_cursor_candidate(
        &self,
        store: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        cursor: DeliveryRecoveryStartupCursor,
    ) -> Result<DeliveryRecoveryStartupCursor, SyndicReadError> {
        self.rebase_non_idle_gate_source_cursor_with_access(
            ReadAccess::Candidate(store),
            cursor.source,
        )
        .map(|source| DeliveryRecoveryStartupCursor { source })
        .map_err(startup_error)
    }

    pub fn recovered_pending_page(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        cursor: Option<RecoveredPendingCursor>,
        limits: CursorReadLimits,
        point_limit: SyndicPointReadLimit,
    ) -> Result<RecoveredPendingPage, SyndicReadError> {
        if self.revision(store)? != expected_revision {
            return Err(SyndicReadError::StaleRecoveredPendingScan);
        }
        let page = self
            .non_idle_gate_source_page(store, expected_revision, cursor.map(|c| c.source), limits)
            .map_err(pending_error)?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let mut records = Vec::with_capacity(page.records().len());
        for source in page.records() {
            let gate = self
                .resolve_non_idle_gate_source(store, expected_revision, *source, point_limit)
                .map_err(pending_error)?;
            let InputGateState::PendingTurn(turn_id) = gate.state() else {
                continue;
            };
            if gate.selected_route().is_some() {
                continue;
            }
            let source = match self.prove_recovered_pending(
                store,
                expected_revision,
                &gate,
                *turn_id,
                point_limit,
            ) {
                Ok(source) => source,
                Err(error) => {
                    return self.pending_error_or_stale(store, expected_revision, error);
                }
            };
            records.push(source);
        }
        if self.revision(store)? != expected_revision {
            return Err(SyndicReadError::StaleRecoveredPendingScan);
        }
        let next_cursor = page
            .next_cursor()
            .map(|source| RecoveredPendingCursor { source });
        Ok(RecoveredPendingPage {
            source_revision: expected_revision,
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }

    pub fn rebase_recovered_pending_cursor(
        &self,
        store: &HomeStore,
        cursor: RecoveredPendingCursor,
    ) -> Result<RecoveredPendingCursor, SyndicReadError> {
        self.rebase_non_idle_gate_source_cursor(store, cursor.source)
            .map(|source| RecoveredPendingCursor { source })
            .map_err(pending_error)
    }
}

impl SyndicStorage {
    fn prove_recovered_pending(
        &self,
        store: &HomeStore,
        source_revision: DomainRevision,
        gate: &InputGateRecord,
        turn_id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<RecoveredPendingSource, SyndicReadError> {
        let pending = self
            .pending_dispatch_evidence(store, gate.thread_id(), limit)?
            .ok_or(SyndicReadError::Invariant(
                "recovered-pending dispatch evidence is absent",
            ))?;
        if pending.source_revision() != source_revision {
            return Err(SyndicReadError::StaleRecoveredPendingScan);
        }
        if pending.turn_id() != turn_id
            || pending.thread_id() != gate.thread_id()
            || pending.gate_revision() != gate.revision()
        {
            return Err(SyndicReadError::Invariant(
                "recovered-pending source is not safe undispatched work",
            ));
        }
        Ok(RecoveredPendingSource {
            source_revision,
            thread_id: gate.thread_id(),
            turn_id,
            gate_revision: gate.revision(),
            state_revision: pending.state_revision(),
            minimum_timestamp: pending.minimum_timestamp(),
        })
    }

    fn pending_error_or_stale<T>(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        error: SyndicReadError,
    ) -> Result<T, SyndicReadError> {
        if self.revision(store)? != expected_revision {
            Err(SyndicReadError::StaleRecoveredPendingScan)
        } else {
            Err(error)
        }
    }
}

fn startup_error(error: SyndicReadError) -> SyndicReadError {
    match error {
        SyndicReadError::InvalidNonIdleGateSourceCursor => {
            SyndicReadError::InvalidDeliveryRecoveryStartupCursor
        }
        error => error,
    }
}

fn pending_error(error: SyndicReadError) -> SyndicReadError {
    match error {
        SyndicReadError::InvalidNonIdleGateSourceCursor => {
            SyndicReadError::InvalidRecoveredPendingCursor
        }
        SyndicReadError::StaleNonIdleGateSourceScan => SyndicReadError::StaleRecoveredPendingScan,
        error => error,
    }
}
