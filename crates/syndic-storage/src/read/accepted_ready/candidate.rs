use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReadyAuthoritySnapshot {
    source: Option<AcceptedReadySourceRecord>,
    gate: Option<InputGateRecord>,
    head: Option<AcceptedRouteGenerationHeadRecord>,
    generation: Option<AcceptedRouteGenerationRecord>,
}

impl ReadyAuthoritySnapshot {
    fn agrees(&self, expected: &AcceptedReadySourceRecord) -> bool {
        let (Some(source), Some(gate), Some(head), Some(generation)) =
            (&self.source, &self.gate, &self.head, &self.generation)
        else {
            return false;
        };
        let proof =
            AcceptedRouteHeadProof::new(expected.generation(), expected.generation_revision());
        let InputGateState::Steerable(gate_turn) = gate.state() else {
            return false;
        };
        let AcceptedRouteTarget::Steering(target) = generation.target() else {
            return false;
        };
        source == expected
            && gate.thread_id() == expected.thread_id()
            && gate.revision() == expected.gate_revision()
            && gate.selected_route() == Some(proof)
            && head.thread_id() == expected.thread_id()
            && head.proof() == proof
            && generation.thread_id() == expected.thread_id()
            && generation.generation() == expected.generation()
            && generation.revision() == expected.generation_revision()
            && generation.first_ordinal() == Some(expected.first_ordinal())
            && generation.last_ordinal() == Some(expected.last_ordinal())
            && generation.ready_retryable_count() > 0
            && target.pending().active_turn_id() == *gate_turn
    }
}

impl SyndicStorage {
    fn ready_authority_snapshot(
        &self,
        store: &HomeStore,
        expected: AcceptedReadySourceRecord,
    ) -> Result<ReadyAuthoritySnapshot, SyndicReadError> {
        let limit = SyndicPointReadLimit::new(READY_POINT_MAX_BYTES)
            .expect("accepted-ready point bound is nonzero");
        let key = ThreadRouteKey {
            thread: expected.thread_id(),
            generation: expected.generation(),
        };
        Ok(ReadyAuthoritySnapshot {
            source: self.point::<AcceptedReadySourcesFamily>(store, key, limit)?,
            gate: self.point::<InputGatesFamily>(store, expected.thread_id(), limit)?,
            head: self.point::<AcceptedRouteGenerationHeadsFamily>(
                store,
                expected.thread_id(),
                limit,
            )?,
            generation: self.point::<AcceptedRouteGenerationsFamily>(store, key, limit)?,
        })
    }

    fn classify_ready_authority_disagreement(
        &self,
        store: &HomeStore,
        expected: AcceptedReadySourceRecord,
        observed: ReadyAuthoritySnapshot,
    ) -> Result<AcceptedReadyCandidatePage, SyndicReadError> {
        let confirmed = self.ready_authority_snapshot(store, expected)?;
        if confirmed != observed {
            Err(SyndicReadError::StaleAcceptedReadyCandidateSource)
        } else {
            Err(SyndicReadError::Invariant(
                "accepted-ready source and current route authority disagree",
            ))
        }
    }

    fn ready_leaf_page(
        &self,
        store: &HomeStore,
        input_id: SyndicAcceptedInputId,
        stored_remaining: usize,
        decoded_remaining: usize,
    ) -> Result<(AcceptedRouteLeafRecord, usize, usize), SyndicReadError> {
        let remaining = stored_remaining.min(decoded_remaining).max(1);
        let page = self.page::<AcceptedRouteLeavesFamily>(
            store,
            CursorRange::closed(input_id, input_id),
            CursorReadLimits::new(1, remaining)
                .expect("accepted-ready leaf point bound is nonzero"),
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let leaf = page
            .into_records()
            .into_iter()
            .next()
            .ok_or(SyndicReadError::Invariant(
                "accepted-ready candidate references a missing route leaf",
            ))?;
        Ok((leaf, stored_bytes, decoded_bytes))
    }
}

fn validate_candidate_leaf(
    source: &AcceptedReadySourceRecord,
    order: &crate::AcceptedOrderIndexRecord,
    leaf: &AcceptedRouteLeafRecord,
) -> Result<(), SyndicReadError> {
    if leaf.input_id() != order.input_id()
        || leaf.thread_id() != source.thread_id()
        || leaf.generation() != source.generation()
        || leaf.ordinal() != order.ordinal()
        || (matches!(leaf.state(), AcceptedRouteLeafState::NextTurn(_))
            && leaf.lifecycle().is_terminal())
    {
        return Err(SyndicReadError::Invariant(
            "accepted-ready candidate route leaf disagrees",
        ));
    }
    Ok(())
}

const fn is_ready_lifecycle(lifecycle: AcceptedInputLifecycle) -> bool {
    matches!(
        lifecycle,
        AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable
    )
}

impl SyndicStorage {
    /// Scans one exact ready source in accepted order and returns only ready or retryable leaves.
    ///
    /// The cursor advances after every scanned ordinal, including delivering, next-turn, and
    /// terminal rows. The source, gate, route head, and generation are stabilized around the page.
    pub fn accepted_ready_candidate_page(
        &self,
        store: &HomeStore,
        source: AcceptedReadySourceRecord,
        cursor: Option<AcceptedReadyCandidateCursor>,
        limits: CursorReadLimits,
    ) -> Result<AcceptedReadyCandidatePage, SyndicReadError> {
        let before = self.ready_authority_snapshot(store, source)?;
        if before.source.as_ref() != Some(&source) {
            return Err(SyndicReadError::StaleAcceptedReadyCandidateSource);
        }
        if !before.agrees(&source) {
            return self.classify_ready_authority_disagreement(store, source, before);
        }
        let scanned_after = match cursor {
            Some(cursor)
                if cursor.thread_id == source.thread_id()
                    && cursor.gate_revision == source.gate_revision()
                    && cursor.generation == source.generation()
                    && cursor.generation_revision == source.generation_revision()
                    && cursor.scanned_after >= source.first_ordinal()
                    && cursor.scanned_after < source.last_ordinal() =>
            {
                Some(cursor.scanned_after)
            }
            Some(_) => return Err(SyndicReadError::InvalidAcceptedReadyCandidateCursor),
            None => None,
        };
        let first = match scanned_after {
            Some(after) => after.checked_next().map_err(|_| {
                SyndicReadError::Invariant("accepted-ready candidate ordinal is exhausted")
            })?,
            None => source.first_ordinal(),
        };
        let max_bytes = limits.max_bytes().min(ACCEPTED_READY_PAGE_MAX_BYTES);
        let rows_for_bytes = (max_bytes / READY_CANDIDATE_MAX_ROW_BYTES).max(1);
        let max_items = limits
            .max_items()
            .min(ACCEPTED_READY_PAGE_MAX_RECORDS)
            .min(rows_for_bytes);
        let page = store.read_cursor::<crate::domain::SyndicDomain, AcceptedOrderCodec>(
            self.handle,
            &CursorRange::closed(
                ThreadAcceptedKey {
                    owner: source.thread_id(),
                    ordinal: first,
                },
                ThreadAcceptedKey {
                    owner: source.thread_id(),
                    ordinal: source.last_ordinal(),
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(max_items, max_bytes)
                .expect("accepted-ready candidate scan bounds are nonzero"),
        )?;
        let order_has_more = page.has_more();
        let mut stored_bytes = page.stored_bytes();
        let mut decoded_bytes = page.decoded_bytes();
        let mut expected_ordinal = first;
        let mut last_scanned = None;
        let mut records = Vec::with_capacity(page.records().len());
        for record in page.into_records() {
            let (key, order) = record.into_parts();
            if key.owner != source.thread_id()
                || key.ordinal != expected_ordinal
                || order.thread_id() != source.thread_id()
                || order.ordinal() != expected_ordinal
                || order.route_generation() != source.generation()
            {
                return Err(SyndicReadError::Invariant(
                    "accepted-ready candidate order membership disagrees",
                ));
            }
            let (leaf, leaf_stored, leaf_decoded) = self.ready_leaf_page(
                store,
                order.input_id(),
                max_bytes.saturating_sub(stored_bytes.min(max_bytes)),
                max_bytes.saturating_sub(decoded_bytes.min(max_bytes)),
            )?;
            stored_bytes =
                stored_bytes
                    .checked_add(leaf_stored)
                    .ok_or(SyndicReadError::Invariant(
                        "accepted-ready stored-byte total overflowed",
                    ))?;
            decoded_bytes =
                decoded_bytes
                    .checked_add(leaf_decoded)
                    .ok_or(SyndicReadError::Invariant(
                        "accepted-ready decoded-byte total overflowed",
                    ))?;
            if stored_bytes > max_bytes || decoded_bytes > max_bytes {
                return Err(SyndicReadError::Invariant(
                    "accepted-ready candidate page exceeded its clamped byte bound",
                ));
            }
            validate_candidate_leaf(&source, &order, &leaf)?;
            if leaf.state() == AcceptedRouteLeafState::Routed
                && is_ready_lifecycle(leaf.lifecycle())
            {
                records.push(AcceptedReadyCandidate {
                    input_id: order.input_id(),
                    ordinal: order.ordinal(),
                    lifecycle: leaf.lifecycle(),
                    leaf_revision: leaf.revision(),
                });
            }
            last_scanned = Some(order.ordinal());
            if order.ordinal() < source.last_ordinal() {
                expected_ordinal = order.ordinal().checked_next().map_err(|_| {
                    SyndicReadError::Invariant("accepted-ready candidate ordinal is exhausted")
                })?;
            }
        }
        let last_scanned = last_scanned.ok_or(SyndicReadError::Invariant(
            "accepted-ready source interval is missing accepted-order membership",
        ))?;
        if !order_has_more && last_scanned < source.last_ordinal() {
            return Err(SyndicReadError::Invariant(
                "accepted-ready source interval has an accepted-order gap",
            ));
        }
        let confirmed = self.ready_authority_snapshot(store, source)?;
        if confirmed != before {
            return Err(SyndicReadError::StaleAcceptedReadyCandidateSource);
        }
        let next_cursor =
            (last_scanned < source.last_ordinal()).then_some(AcceptedReadyCandidateCursor {
                thread_id: source.thread_id(),
                gate_revision: source.gate_revision(),
                generation: source.generation(),
                generation_revision: source.generation_revision(),
                scanned_after: last_scanned,
            });
        Ok(AcceptedReadyCandidatePage {
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }
}
