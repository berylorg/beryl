use super::*;

mod authority;
mod basis;
mod scan;
mod validation;

use authority::{
    effective_next_turn_reason, route_head_is_coherent, validate_next_authority, validate_next_leaf,
};
use scan::ScanByteTotals;
use validation::validate_candidate_basis;

#[derive(Clone, Debug, Eq, PartialEq)]
struct NextAuthoritySnapshot {
    source: Option<AcceptedNextSourceRecord>,
    gate: Option<InputGateRecord>,
    head: Option<AcceptedRouteGenerationHeadRecord>,
    generation: Option<AcceptedRouteGenerationRecord>,
}

impl SyndicStorage {
    fn next_authority_snapshot(
        &self,
        store: &HomeStore,
        expected: AcceptedNextSourceRecord,
    ) -> Result<NextAuthoritySnapshot, SyndicReadError> {
        let limit = SyndicPointReadLimit::new(NEXT_POINT_MAX_BYTES)
            .expect("accepted-next point bound is nonzero");
        let key = ThreadRouteKey {
            thread: expected.thread_id(),
            generation: expected.generation(),
        };
        Ok(NextAuthoritySnapshot {
            source: self.point::<AcceptedNextSourcesFamily>(store, key, limit)?,
            gate: self.point::<InputGatesFamily>(store, expected.thread_id(), limit)?,
            head: self.point::<AcceptedRouteGenerationHeadsFamily>(
                store,
                expected.thread_id(),
                limit,
            )?,
            generation: self.point::<AcceptedRouteGenerationsFamily>(store, key, limit)?,
        })
    }

    fn prove_earliest_next_source(
        &self,
        store: &HomeStore,
        expected: AcceptedNextSourceRecord,
    ) -> Result<(), SyndicReadError> {
        let page = store.read_cursor::<crate::domain::SyndicDomain, AcceptedNextSourcesCodec>(
            &self.handle,
            &CursorRange::closed(
                ThreadRouteKey {
                    thread: expected.thread_id(),
                    generation: AcceptedRouteGeneration::FIRST,
                },
                ThreadRouteKey {
                    thread: expected.thread_id(),
                    generation: AcceptedRouteGeneration::new(u64::MAX)
                        .expect("maximum route generation is nonzero"),
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(1, NEXT_POINT_MAX_BYTES)
                .expect("accepted-next earliest-source bound is nonzero"),
        )?;
        let Some(first) = page.records().first() else {
            return Err(SyndicReadError::InvalidAcceptedNextCandidateSource);
        };
        if first.key().thread != expected.thread_id()
            || first.key().generation != expected.generation()
            || first.value() != &expected
        {
            return Err(SyndicReadError::InvalidAcceptedNextCandidateSource);
        }
        Ok(())
    }
}

impl SyndicStorage {
    /// Scans one exact earliest next source and returns at most its first effective candidate.
    ///
    /// The continuation advances over every terminal or otherwise ineligible ordinal. Finding a
    /// candidate or observing a non-idle gate completes this page without a continuation.
    pub fn accepted_next_candidate_page(
        &self,
        store: &HomeStore,
        source: AcceptedNextSource,
        cursor: Option<AcceptedNextCandidateCursor>,
        limits: CursorReadLimits,
    ) -> Result<AcceptedNextCandidatePage, SyndicReadError> {
        if self.revision(store)? != source.source_revision() {
            return Err(SyndicReadError::StaleAcceptedNextCandidateSource);
        }
        let result = self.accepted_next_candidate_page_at_revision(store, source, cursor, limits);
        if self.revision(store)? != source.source_revision() {
            return Err(SyndicReadError::StaleAcceptedNextCandidateSource);
        }
        result
    }

    fn accepted_next_candidate_page_at_revision(
        &self,
        store: &HomeStore,
        source: AcceptedNextSource,
        cursor: Option<AcceptedNextCandidateCursor>,
        limits: CursorReadLimits,
    ) -> Result<AcceptedNextCandidatePage, SyndicReadError> {
        let expected = source.record();
        let snapshot = self.next_authority_snapshot(store, expected)?;
        validate_next_authority(expected, &snapshot)?;
        self.prove_earliest_next_source(store, expected)?;
        let scanned_after = match cursor {
            Some(cursor)
                if cursor.source_revision == source.source_revision()
                    && cursor.thread_id == expected.thread_id()
                    && cursor.gate_revision
                        == snapshot
                            .gate
                            .as_ref()
                            .expect("validated accepted-next gate is present")
                            .revision()
                    && cursor.generation == expected.generation()
                    && cursor.generation_revision == expected.generation_revision()
                    && cursor.scanned_after >= expected.first_ordinal()
                    && cursor.scanned_after < expected.last_ordinal() =>
            {
                Some(cursor.scanned_after)
            }
            Some(_) => return Err(SyndicReadError::InvalidAcceptedNextCandidateCursor),
            None => None,
        };
        if snapshot
            .gate
            .as_ref()
            .is_none_or(|gate| gate.state() != &InputGateState::Idle)
        {
            return Ok(AcceptedNextCandidatePage {
                candidate: None,
                stored_bytes: 0,
                decoded_bytes: 0,
                next_cursor: None,
            });
        }
        let mut ordinal = match scanned_after {
            Some(after) => after.checked_next().map_err(|_| {
                SyndicReadError::Invariant("accepted-next candidate ordinal is exhausted")
            })?,
            None => expected.first_ordinal(),
        };
        let max_bytes = limits.max_bytes().min(ACCEPTED_NEXT_PAGE_MAX_BYTES);
        let max_items = limits.max_items().min(ACCEPTED_NEXT_PAGE_MAX_RECORDS);
        let mut totals = ScanByteTotals::default();
        let mut scanned = 0_usize;
        let mut last_scanned = None;

        while scanned < max_items {
            if scanned != 0 && totals.remaining(max_bytes) < NEXT_CANDIDATE_MAX_ROW_BYTES {
                break;
            }
            let key = ThreadAcceptedKey {
                owner: expected.thread_id(),
                ordinal,
            };
            let order = self.next_scan_record::<AcceptedOrderFamily>(
                store,
                key,
                &mut totals,
                max_bytes,
                "accepted-next source interval is missing accepted-order membership",
            )?;
            if order.thread_id() != expected.thread_id()
                || order.ordinal() != ordinal
                || order.route_generation() != expected.generation()
            {
                return Err(SyndicReadError::Invariant(
                    "accepted-next source interval crosses accepted-order membership",
                ));
            }
            let leaf = self.next_scan_record::<AcceptedRouteLeavesFamily>(
                store,
                order.input_id(),
                &mut totals,
                max_bytes,
                "accepted-next source interval references a missing route leaf",
            )?;
            validate_next_leaf(
                expected,
                snapshot
                    .generation
                    .as_ref()
                    .expect("validated accepted-next generation is present"),
                &order,
                &leaf,
            )?;
            scanned += 1;
            last_scanned = Some(ordinal);

            if let Some(reason) = effective_next_turn_reason(
                snapshot
                    .generation
                    .as_ref()
                    .expect("validated accepted-next generation is present"),
                &leaf,
            ) {
                let basis = self.load_next_candidate_basis(
                    store,
                    source.source_revision(),
                    snapshot,
                    order,
                    leaf,
                )?;
                validate_candidate_basis(&basis, reason)?;
                return Ok(AcceptedNextCandidatePage {
                    candidate: Some(AcceptedNextCandidate { reason, basis }),
                    stored_bytes: totals.stored,
                    decoded_bytes: totals.decoded,
                    next_cursor: None,
                });
            }
            if ordinal == expected.last_ordinal() {
                break;
            }
            ordinal = ordinal.checked_next().map_err(|_| {
                SyndicReadError::Invariant("accepted-next candidate ordinal is exhausted")
            })?;
        }
        let last_scanned = last_scanned.ok_or(SyndicReadError::Invariant(
            "accepted-next source scan made no progress",
        ))?;
        if last_scanned == expected.last_ordinal() {
            return Err(SyndicReadError::Invariant(
                "accepted-next source claims work without an effective candidate",
            ));
        }
        let next_cursor =
            (last_scanned < expected.last_ordinal()).then_some(AcceptedNextCandidateCursor {
                source_revision: source.source_revision(),
                thread_id: expected.thread_id(),
                gate_revision: snapshot
                    .gate
                    .as_ref()
                    .expect("validated accepted-next gate is present")
                    .revision(),
                generation: expected.generation(),
                generation_revision: expected.generation_revision(),
                scanned_after: last_scanned,
            });
        Ok(AcceptedNextCandidatePage {
            candidate: None,
            stored_bytes: totals.stored,
            decoded_bytes: totals.decoded,
            next_cursor,
        })
    }
}
