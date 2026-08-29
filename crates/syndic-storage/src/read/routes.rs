use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::{SyndicAcceptedInputId, SyndicThreadId};

use crate::{
    AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord, AcceptedRouteGeneration,
    AcceptedRouteGenerationRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    AcceptedRouteRevision, AcceptedRouteTarget, NextTurnReason, SyndicReadError, codec::*,
    domain::SyndicStorage,
};

pub const ACCEPTED_ROUTE_PAGE_MAX_RECORDS: usize = 64;
pub const ACCEPTED_ROUTE_PAGE_MAX_STORED_BYTES: usize = 2 * 1024 * 1024;
const ROUTE_POINT_MAX_BYTES: usize = 512 * 1024;

/// Revision-bound continuation for one accepted-input route generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedRouteCursor {
    thread_id: SyndicThreadId,
    generation: AcceptedRouteGeneration,
    revision: AcceptedRouteRevision,
    after: AcceptedInputOrdinal,
}

/// Effective delivery state resolved from one leaf and its generation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedRouteEffectiveState {
    Ready,
    Delivering,
    NextTurn(NextTurnReason),
    Promoted,
    Delivered,
    Failed,
    DeliveryUnknown,
}

/// One bounded route-page row with immutable content and current leaf evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedRouteEntry {
    input: AcceptedInputRecord,
    leaf: AcceptedRouteLeafRecord,
    effective_state: AcceptedRouteEffectiveState,
}

impl AcceptedRouteEntry {
    #[must_use]
    pub const fn input(&self) -> &AcceptedInputRecord {
        &self.input
    }
    #[must_use]
    pub const fn leaf(&self) -> &AcceptedRouteLeafRecord {
        &self.leaf
    }
    #[must_use]
    pub const fn effective_state(&self) -> AcceptedRouteEffectiveState {
        self.effective_state
    }
}

/// One fixed-cap route page and its exact optional continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedRoutePage {
    records: Vec<AcceptedRouteEntry>,
    stored_bytes: usize,
    decoded_bytes: usize,
    next_cursor: Option<AcceptedRouteCursor>,
}

impl AcceptedRoutePage {
    #[must_use]
    pub fn records(&self) -> &[AcceptedRouteEntry] {
        &self.records
    }
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<AcceptedRouteCursor> {
        self.next_cursor
    }
}

impl SyndicStorage {
    /// Reads one fixed-work page from an exact route-generation revision.
    pub fn accepted_route_page(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        generation: AcceptedRouteGeneration,
        revision: AcceptedRouteRevision,
        cursor: Option<AcceptedRouteCursor>,
    ) -> Result<AcceptedRoutePage, SyndicReadError> {
        let route_key = ThreadRouteKey {
            thread: thread_id,
            generation,
        };
        let before = self.route_generation(store, route_key)?;
        if before.revision() != revision {
            return Err(SyndicReadError::StaleAcceptedRoute);
        }
        let after = match cursor {
            Some(cursor)
                if cursor.thread_id == thread_id
                    && cursor.generation == generation
                    && cursor.revision == revision =>
            {
                Some(cursor.after)
            }
            Some(_) => return Err(SyndicReadError::InvalidAcceptedRouteCursor),
            None => None,
        };
        let Some(first) = before.first_ordinal() else {
            return Ok(AcceptedRoutePage {
                records: Vec::new(),
                stored_bytes: 0,
                decoded_bytes: 0,
                next_cursor: None,
            });
        };
        let last = before.last_ordinal().ok_or(SyndicReadError::Invariant(
            "accepted-route generation has partial interval",
        ))?;
        if after.is_some_and(|after| after < first || after > last) {
            return Err(SyndicReadError::InvalidAcceptedRouteCursor);
        }
        let range = match after {
            Some(after) if after == last => {
                return Ok(AcceptedRoutePage {
                    records: Vec::new(),
                    stored_bytes: 0,
                    decoded_bytes: 0,
                    next_cursor: None,
                });
            }
            Some(after) => CursorRange::after(
                ThreadAcceptedKey {
                    owner: thread_id,
                    ordinal: after,
                },
                ThreadAcceptedKey {
                    owner: thread_id,
                    ordinal: last,
                },
            ),
            None => CursorRange::closed(
                ThreadAcceptedKey {
                    owner: thread_id,
                    ordinal: first,
                },
                ThreadAcceptedKey {
                    owner: thread_id,
                    ordinal: last,
                },
            ),
        };
        self.read_route_page(store, &before, range)
    }

    pub(crate) fn route_generation(
        &self,
        store: &HomeStore,
        key: ThreadRouteKey,
    ) -> Result<AcceptedRouteGenerationRecord, SyndicReadError> {
        self.point::<AcceptedRouteGenerationsFamily>(
            store,
            key,
            super::SyndicPointReadLimit::new(ROUTE_POINT_MAX_BYTES)
                .expect("route point bound is nonzero"),
        )?
        .ok_or(SyndicReadError::StaleAcceptedRoute)
    }

    pub(crate) fn route_generation_head(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<Option<crate::AcceptedRouteGenerationHeadRecord>, SyndicReadError> {
        self.point::<AcceptedRouteGenerationHeadsFamily>(
            store,
            thread_id,
            super::SyndicPointReadLimit::new(ROUTE_POINT_MAX_BYTES)
                .expect("route point bound is nonzero"),
        )
    }

    fn read_route_page(
        &self,
        store: &HomeStore,
        expected: &AcceptedRouteGenerationRecord,
        range: CursorRange<ThreadAcceptedKey>,
    ) -> Result<AcceptedRoutePage, SyndicReadError> {
        let page = store.read_cursor::<crate::domain::SyndicDomain, AcceptedOrderCodec>(
            &self.handle,
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(ACCEPTED_ROUTE_PAGE_MAX_RECORDS, 64 * 1024)
                .expect("route page bounds are nonzero"),
        )?;
        let mut has_more = page.has_more();
        let mut stored_bytes = page.stored_bytes();
        let mut decoded_bytes = page.decoded_bytes();
        let mut records = Vec::with_capacity(page.records().len());
        for record in page.into_records() {
            let order = record.into_parts().1;
            if order.route_generation() != expected.generation() {
                return Err(SyndicReadError::Invariant(
                    "route page crossed generation membership",
                ));
            }
            let (input, input_stored_bytes, input_decoded_bytes) =
                self.route_point::<AcceptedInputsFamily>(store, order.input_id())?;
            let (leaf, leaf_stored_bytes, leaf_decoded_bytes) =
                self.route_point::<AcceptedRouteLeavesFamily>(store, order.input_id())?;
            let next_stored_bytes = stored_bytes
                .checked_add(input_stored_bytes)
                .and_then(|value| value.checked_add(leaf_stored_bytes))
                .ok_or(SyndicReadError::Invariant(
                    "route page byte accounting overflowed",
                ))?;
            let next_decoded_bytes = decoded_bytes
                .checked_add(input_decoded_bytes)
                .and_then(|value| value.checked_add(leaf_decoded_bytes))
                .ok_or(SyndicReadError::Invariant(
                    "route page decoded-byte accounting overflowed",
                ))?;
            if next_stored_bytes > ACCEPTED_ROUTE_PAGE_MAX_STORED_BYTES
                || next_decoded_bytes > ACCEPTED_ROUTE_PAGE_MAX_STORED_BYTES
            {
                if records.is_empty() {
                    return Err(SyndicReadError::Invariant(
                        "one accepted-route row exceeds the fixed page byte bound",
                    ));
                }
                has_more = true;
                break;
            }
            stored_bytes = next_stored_bytes;
            decoded_bytes = next_decoded_bytes;
            if input.route_generation() != expected.generation()
                || leaf.generation() != expected.generation()
                || input.ordinal() != order.ordinal()
                || leaf.ordinal() != order.ordinal()
            {
                return Err(SyndicReadError::Invariant("route page records disagree"));
            }
            records.push(AcceptedRouteEntry {
                effective_state: effective_state(expected.target(), &leaf)?,
                input,
                leaf,
            });
        }
        let confirmed = self.route_generation(
            store,
            ThreadRouteKey {
                thread: expected.thread_id(),
                generation: expected.generation(),
            },
        )?;
        if &confirmed != expected {
            return Err(SyndicReadError::StaleAcceptedRoute);
        }
        let next_cursor = has_more.then(|| AcceptedRouteCursor {
            thread_id: expected.thread_id(),
            generation: expected.generation(),
            revision: expected.revision(),
            after: records
                .last()
                .expect("more requires one route record")
                .input
                .ordinal(),
        });
        Ok(AcceptedRoutePage {
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }

    fn route_point<F>(
        &self,
        store: &HomeStore,
        key: SyndicAcceptedInputId,
    ) -> Result<(F::Value, usize, usize), SyndicReadError>
    where
        F: Family<Key = SyndicAcceptedInputId>,
    {
        let page = self.page::<F>(
            store,
            CursorRange::closed(key, key),
            CursorReadLimits::new(1, ROUTE_POINT_MAX_BYTES)
                .expect("route page point bounds are nonzero"),
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let record = page
            .into_records()
            .into_iter()
            .next()
            .ok_or(SyndicReadError::Invariant(
                "route page references a missing record",
            ))?;
        Ok((record, stored_bytes, decoded_bytes))
    }
}

fn effective_state(
    target: &AcceptedRouteTarget,
    leaf: &AcceptedRouteLeafRecord,
) -> Result<AcceptedRouteEffectiveState, SyndicReadError> {
    if let AcceptedRouteLeafState::NextTurn(reason) = leaf.state() {
        return Ok(AcceptedRouteEffectiveState::NextTurn(reason));
    }
    if let AcceptedRouteTarget::NextTurn(reason) = target
        && matches!(
            leaf.lifecycle(),
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable
        )
    {
        return Ok(AcceptedRouteEffectiveState::NextTurn(*reason));
    }
    if matches!(target, AcceptedRouteTarget::AwaitingTerminal(_))
        && matches!(
            leaf.lifecycle(),
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable
        )
    {
        return Ok(AcceptedRouteEffectiveState::NextTurn(
            NextTurnReason::UnknownTerminal,
        ));
    }
    if matches!(target, AcceptedRouteTarget::ProjectionLost(_)) {
        return match leaf.lifecycle() {
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable => Ok(
                AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost),
            ),
            AcceptedInputLifecycle::Delivering => Ok(AcceptedRouteEffectiveState::DeliveryUnknown),
            AcceptedInputLifecycle::Delivered => Ok(AcceptedRouteEffectiveState::Delivered),
            AcceptedInputLifecycle::Failed => Ok(AcceptedRouteEffectiveState::Failed),
            AcceptedInputLifecycle::DeliveryUnknown => {
                Ok(AcceptedRouteEffectiveState::DeliveryUnknown)
            }
            AcceptedInputLifecycle::Promoted => Ok(AcceptedRouteEffectiveState::Promoted),
        };
    }
    match leaf.lifecycle() {
        AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable => {
            Ok(AcceptedRouteEffectiveState::Ready)
        }
        AcceptedInputLifecycle::Delivering => Ok(AcceptedRouteEffectiveState::Delivering),
        AcceptedInputLifecycle::Delivered => Ok(AcceptedRouteEffectiveState::Delivered),
        AcceptedInputLifecycle::Failed => Ok(AcceptedRouteEffectiveState::Failed),
        AcceptedInputLifecycle::DeliveryUnknown => Ok(AcceptedRouteEffectiveState::DeliveryUnknown),
        AcceptedInputLifecycle::Promoted => Ok(AcceptedRouteEffectiveState::Promoted),
    }
}
