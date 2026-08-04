use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::SyndicThreadId;

use crate::{
    BindingState, InputGateState, SyndicPointReadLimit, SyndicReadError,
    codec::{ExactCodec, InputGatesFamily},
    domain::{SyndicDomain, SyndicStorage},
};

use super::*;

impl SyndicStorage {
    /// Scans one bounded physical input-gate page and returns only non-idle startup sources.
    ///
    /// The cursor is bound to the home identity but deliberately not to its domain revision.
    /// Limits are clamped to the public delivery-recovery maxima.
    pub fn delivery_recovery_startup_page(
        &self,
        store: &HomeStore,
        cursor: Option<DeliveryRecoveryStartupCursor>,
        limits: CursorReadLimits,
    ) -> Result<DeliveryRecoveryStartupPage, SyndicReadError> {
        let home_id = store.home_id();
        let range = match cursor {
            Some(cursor) if cursor.home_id == home_id => CursorRange::after(
                cursor.after_thread_id,
                SyndicThreadId::from_bytes([u8::MAX; 16]),
            ),
            Some(_) => return Err(SyndicReadError::InvalidDeliveryRecoveryStartupCursor),
            None => CursorRange::closed(
                SyndicThreadId::from_bytes([0; 16]),
                SyndicThreadId::from_bytes([u8::MAX; 16]),
            ),
        };
        let page = store.read_cursor::<SyndicDomain, ExactCodec<InputGatesFamily>>(
            self.handle,
            &range,
            CursorDirection::Forward,
            recovery_gate_limits(limits),
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        let mut last_scanned = None;
        let mut records = Vec::with_capacity(page.records().len());
        for record in page.into_records() {
            let (thread_id, gate) = record.into_parts();
            last_scanned = Some(thread_id);
            if gate.thread_id() != thread_id {
                return Err(SyndicReadError::Invariant(
                    "delivery-recovery input-gate key and identity disagree",
                ));
            }
            if !matches!(gate.state(), InputGateState::Idle) {
                records.push(DeliveryRecoverySource { home_id, gate });
            }
        }
        let next_cursor = continuation(home_id, last_scanned, has_more)?;
        Ok(DeliveryRecoveryStartupPage {
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }

    /// Scans one revision-bound physical input-gate page for proven safe pending turns.
    ///
    /// Filtering requires `PendingTurn`, no selected route, a pending source-free turn state, and
    /// a non-active current binding. The continuation advances by the last physical gate row even
    /// when every row was filtered out.
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
        let range = match cursor {
            Some(cursor)
                if cursor.home_id == store.home_id()
                    && cursor.source_revision == expected_revision =>
            {
                CursorRange::after(
                    cursor.after_thread_id,
                    SyndicThreadId::from_bytes([u8::MAX; 16]),
                )
            }
            Some(_) => return Err(SyndicReadError::InvalidRecoveredPendingCursor),
            None => CursorRange::closed(
                SyndicThreadId::from_bytes([0; 16]),
                SyndicThreadId::from_bytes([u8::MAX; 16]),
            ),
        };
        let page = store.read_cursor::<SyndicDomain, ExactCodec<InputGatesFamily>>(
            self.handle,
            &range,
            CursorDirection::Forward,
            recovery_gate_limits(limits),
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        let mut last_scanned = None;
        let mut records = Vec::with_capacity(page.records().len());
        for record in page.into_records() {
            let (thread_id, gate) = record.into_parts();
            last_scanned = Some(thread_id);
            if gate.thread_id() != thread_id {
                return self.pending_error_or_stale(
                    store,
                    expected_revision,
                    SyndicReadError::Invariant(
                        "recovered-pending input-gate key and identity disagree",
                    ),
                );
            }
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
        let next_cursor =
            pending_continuation(store.home_id(), expected_revision, last_scanned, has_more)?;
        Ok(RecoveredPendingPage {
            source_revision: expected_revision,
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }

    /// Rebinds an existing physical pending-scan continuation to the home's current revision.
    ///
    /// This preserves the caller-owned forward scan floor after an already-scanned candidate
    /// mutates Syndic. The caller must separately guarantee that any mutation capable of making
    /// work eligible behind that floor opens a fresh scan from the beginning. This method does not
    /// reauthenticate or carry authority for any returned source and is not a generic response to
    /// arbitrary concurrent drift.
    pub fn rebase_recovered_pending_cursor(
        &self,
        store: &HomeStore,
        cursor: RecoveredPendingCursor,
    ) -> Result<RecoveredPendingCursor, SyndicReadError> {
        if cursor.home_id != store.home_id() {
            return Err(SyndicReadError::InvalidRecoveredPendingCursor);
        }
        Ok(RecoveredPendingCursor {
            home_id: cursor.home_id,
            source_revision: self.revision(store)?,
            after_thread_id: cursor.after_thread_id,
        })
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
        let state = self
            .turn_state(store, turn_id, limit)?
            .ok_or(SyndicReadError::Invariant(
                "recovered-pending gate references a missing turn state",
            ))?;
        let summary = self
            .history_summary(store, gate.thread_id(), limit)?
            .ok_or(SyndicReadError::Invariant(
                "recovered-pending gate references a missing history summary",
            ))?;
        let binding = self
            .current_binding(store, gate.thread_id(), limit)?
            .ok_or(SyndicReadError::Invariant(
                "recovered-pending thread has no current binding",
            ))?;
        if state.turn_id() != turn_id
            || state.lifecycle() != crate::TurnLifecycle::Pending
            || state.source_event_count() != 0
            || summary.thread_id() != gate.thread_id()
            || summary.committed_tail() != Some(turn_id)
            || summary.complete()
            || gate.live_steering_count() != 0
            || binding.binding().thread_id() != gate.thread_id()
            || matches!(binding.binding().state(), BindingState::Active(_))
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
            state_revision: state.revision(),
            minimum_timestamp: state.updated_at().max(summary.last_activity_at()),
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

fn recovery_gate_limits(limits: CursorReadLimits) -> CursorReadLimits {
    CursorReadLimits::new(
        limits
            .max_items()
            .min(DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS),
        limits
            .max_bytes()
            .min(DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES),
    )
    .expect("clamped nonzero delivery-recovery gate limits remain nonzero")
}

fn continuation(
    home_id: BerylHomeId,
    last_scanned: Option<SyndicThreadId>,
    has_more: bool,
) -> Result<Option<DeliveryRecoveryStartupCursor>, SyndicReadError> {
    if !has_more {
        return Ok(None);
    }
    let after_thread_id = last_scanned.ok_or(SyndicReadError::Invariant(
        "delivery-recovery startup page reported more without scanning a gate",
    ))?;
    Ok(Some(DeliveryRecoveryStartupCursor {
        home_id,
        after_thread_id,
    }))
}

fn pending_continuation(
    home_id: BerylHomeId,
    source_revision: DomainRevision,
    last_scanned: Option<SyndicThreadId>,
    has_more: bool,
) -> Result<Option<RecoveredPendingCursor>, SyndicReadError> {
    if !has_more {
        return Ok(None);
    }
    let after_thread_id = last_scanned.ok_or(SyndicReadError::Invariant(
        "recovered-pending page reported more without scanning a gate",
    ))?;
    Ok(Some(RecoveredPendingCursor {
        home_id,
        source_revision,
        after_thread_id,
    }))
}
