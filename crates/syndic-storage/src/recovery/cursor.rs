use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{RecoveryItemSequenceAccumulator, SyndicTurnId};
use beryl_stream::PageLease;

pub use beryl_model::RecoveryItemSequenceRole;

use crate::{RecoveryProjectionError, SyndicStorage, TurnItemOrdinal, read::ReadByteTotals};

use super::{
    INDEX_PAGE_MAX_BYTES, RecoveryItemDescriptor, RecoveryProjection, RecoveryTailEligibility,
    RecoveryTopology, text::read_recovery_utf8_page,
};

/// One bounded, nonempty, valid UTF-8 page from a sequential recovery replay.
pub struct RecoveryCursorPage {
    sequence_ordinal: u64,
    role: RecoveryItemSequenceRole,
    declared_item_utf8_bytes: u64,
    item_offset: u64,
    page_lease: PageLease,
    stored_bytes: usize,
    decoded_bytes: usize,
    item_terminal: bool,
    sequence_terminal: bool,
}

impl RecoveryCursorPage {
    /// One-based ordinal of the logical recovery item containing this page.
    #[must_use]
    pub const fn sequence_ordinal(&self) -> u64 {
        self.sequence_ordinal
    }

    #[must_use]
    pub const fn role(&self) -> RecoveryItemSequenceRole {
        self.role
    }

    #[must_use]
    pub const fn declared_item_utf8_bytes(&self) -> u64 {
        self.declared_item_utf8_bytes
    }

    /// Zero-based item-local byte offset of `text`.
    #[must_use]
    pub const fn item_offset(&self) -> u64 {
        self.item_offset
    }

    #[must_use]
    pub fn text(&self) -> &str {
        std::str::from_utf8(self.page_lease.as_slice())
            .expect("recovery cursor pages contain validated UTF-8")
    }

    /// Consumes the page and transfers its exact leased storage without copying bytes.
    #[must_use]
    pub fn into_page_lease(self) -> PageLease {
        self.page_lease
    }

    /// Actual aggregate stored bytes acquired by cursor reads for this page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Actual aggregate practical decoded bytes published by cursor reads for this page.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    #[must_use]
    pub const fn item_terminal(&self) -> bool {
        self.item_terminal
    }

    /// True only after exact EOF totals and digest were re-proven for this final page.
    #[must_use]
    pub const fn sequence_terminal(&self) -> bool {
        self.sequence_terminal
    }
}

/// Opaque non-cloneable state for one revision-bound sequential recovery replay.
pub struct RecoveryCursor {
    proof: RecoveryProjection,
    topology: RecoveryTopology,
    next_turn_depth: u64,
    turn: Option<CursorTurn>,
    item: Option<CursorItem>,
    completed_items: u64,
    accumulator: Option<RecoveryItemSequenceAccumulator>,
    pending_read_bytes: ReadByteTotals,
    state: CursorState,
    poisoned: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CursorState {
    Active,
    TerminalPendingEof,
    EofReturned,
}

#[derive(Clone, Copy)]
struct CursorTurn {
    id: SyndicTurnId,
    item_count: u64,
    next_ordinal: u64,
}

#[derive(Clone, Copy)]
struct CursorItem {
    descriptor: RecoveryItemDescriptor,
    declared_utf8_bytes: u64,
    offset: u64,
}

impl SyndicStorage {
    /// Opens a compact replay cursor after rechecking the proof's exact source revision and path.
    pub fn open_recovery_cursor(
        &self,
        store: &HomeStore,
        proof: RecoveryProjection,
    ) -> Result<RecoveryCursor, RecoveryProjectionError> {
        let topology = self.validate_recovery_cursor_proof(store, proof)?;
        Ok(RecoveryCursor {
            proof,
            accumulator: Some(RecoveryItemSequenceAccumulator::new(
                proof.item_count().get().into(),
                proof.utf8_bytes().get(),
            )),
            topology,
            next_turn_depth: 1,
            turn: None,
            item: None,
            completed_items: 0,
            pending_read_bytes: ReadByteTotals::default(),
            state: CursorState::Active,
            poisoned: false,
        })
    }

    /// Fills the caller's exact lease with the next bounded UTF-8 page or observes exact EOF.
    ///
    /// `max_utf8_bytes` must be nonzero. The effective read bound is the minimum of that request,
    /// the supplied lease capacity, 65,536 bytes, and the current item's remaining bytes.
    /// The final page has both its applicable item terminal and sequence terminal set. The next
    /// call rechecks the source binding and returns `None`; any later call is rejected.
    pub fn read_recovery_cursor_page(
        &self,
        store: &HomeStore,
        cursor: &mut RecoveryCursor,
        mut page_lease: PageLease,
        max_utf8_bytes: usize,
    ) -> Result<Option<RecoveryCursorPage>, RecoveryProjectionError> {
        if max_utf8_bytes == 0 {
            return Err(RecoveryProjectionError::InvalidCursorPageLimit {
                actual: max_utf8_bytes,
            });
        }
        match cursor.state {
            CursorState::EofReturned => return Err(RecoveryProjectionError::CursorTerminal),
            CursorState::TerminalPendingEof => {
                self.ensure_recovery_cursor_bound(store, cursor)?;
                cursor.state = CursorState::EofReturned;
                return Ok(None);
            }
            CursorState::Active => {}
        }
        if cursor.poisoned {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "cursor was invalidated by an earlier replay mismatch",
            });
        }
        self.ensure_recovery_cursor_bound(store, cursor)?;
        if cursor.item.is_none()
            && let Err(error) = self.load_next_cursor_item(store, cursor)
        {
            cursor.poisoned = matches!(error, RecoveryProjectionError::CursorMismatch { .. });
            return Err(error);
        }
        let item = cursor
            .item
            .expect("a nonterminal cursor has one current item");
        page_lease.clear();
        let (text_len, text_totals) = match read_recovery_utf8_page(
            self,
            store,
            item.descriptor.source,
            item.offset,
            max_utf8_bytes,
            page_lease.buffer_mut(),
        ) {
            Ok(text_len) => text_len,
            Err(error) => {
                cursor.poisoned = matches!(error, RecoveryProjectionError::CursorMismatch { .. });
                return Err(error);
            }
        };
        page_lease.set_len(text_len).map_err(|_| {
            RecoveryProjectionError::Invariant("recovery page length exceeds lease")
        })?;
        self.ensure_recovery_cursor_bound(store, cursor)?;
        let mut page_totals = cursor.pending_read_bytes;
        page_totals.add(
            text_totals.stored,
            text_totals.decoded,
            "recovery cursor page byte accounting overflowed",
        )?;
        self.accept_cursor_page(cursor, item, page_lease, page_totals)
            .map(Some)
    }

    fn validate_recovery_cursor_proof(
        &self,
        store: &HomeStore,
        proof: RecoveryProjection,
    ) -> Result<RecoveryTopology, RecoveryProjectionError> {
        let tail_eligibility = self.ensure_recovery_proof_bound(store, proof)?;
        let prefix_tail =
            proof
                .represented_prefix()
                .tail()
                .ok_or(RecoveryProjectionError::CursorMismatch {
                    reason: "a ready recovery proof has no represented prefix tail",
                })?;
        let tail = self.load_recovery_turn(store, prefix_tail)?;
        let topology = self.inspect_recovery_topology(
            store,
            prefix_tail,
            tail.depth().get(),
            tail_eligibility,
        )?;
        if topology.digest != proof.represented_prefix().digest()
            || topology.item_count != u64::from(proof.item_count().get())
        {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "recovery topology disagrees with the compact proof",
            });
        }
        if self.ensure_recovery_proof_bound(store, proof)? != tail_eligibility {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "recovery eligibility changed while opening its cursor",
            });
        }
        Ok(topology)
    }

    fn ensure_recovery_cursor_bound(
        &self,
        store: &HomeStore,
        cursor: &RecoveryCursor,
    ) -> Result<(), RecoveryProjectionError> {
        self.ensure_recovery_proof_bound(store, cursor.proof)
            .and_then(|eligibility| {
                if eligibility == cursor.topology.tail_eligibility {
                    Ok(())
                } else {
                    Err(RecoveryProjectionError::CursorMismatch {
                        reason: "recovery cursor eligibility no longer matches its proof",
                    })
                }
            })
    }

    fn load_next_cursor_item(
        &self,
        store: &HomeStore,
        cursor: &mut RecoveryCursor,
    ) -> Result<(), RecoveryProjectionError> {
        loop {
            if cursor.turn.is_none() {
                if cursor.next_turn_depth > cursor.topology.depth {
                    return Err(RecoveryProjectionError::CursorMismatch {
                        reason: "recovery topology ended before its declared item total",
                    });
                }
                let turn =
                    self.recovery_turn_at_depth(store, &cursor.topology, cursor.next_turn_depth)?;
                let item_count = self.validate_recovery_turn(
                    store,
                    &turn,
                    cursor.next_turn_depth,
                    if cursor.next_turn_depth == cursor.topology.depth {
                        cursor.topology.tail_eligibility
                    } else {
                        RecoveryTailEligibility::Strict
                    },
                )?;
                cursor.turn = Some(CursorTurn {
                    id: turn.id(),
                    item_count,
                    next_ordinal: 1,
                });
            }
            let turn = cursor.turn.expect("cursor just loaded one turn");
            if turn.next_ordinal > turn.item_count {
                cursor.turn = None;
                cursor.next_turn_depth = cursor.next_turn_depth.checked_add(1).ok_or(
                    RecoveryProjectionError::CursorMismatch {
                        reason: "recovery cursor turn depth overflowed",
                    },
                )?;
                continue;
            }

            let after = if turn.next_ordinal == 1 {
                None
            } else {
                Some(TurnItemOrdinal::new(turn.next_ordinal - 1).map_err(|_| {
                    RecoveryProjectionError::CursorMismatch {
                        reason: "recovery cursor item predecessor is invalid",
                    }
                })?)
            };
            #[cfg(feature = "test-faults")]
            crate::test_faults::metrics::record_recovery_turn_item_read_attempt();
            let page = self.turn_items(store, turn.id, after, cursor_index_page_limits())?;
            let [index] = page.records() else {
                return Err(RecoveryProjectionError::MissingHistory {
                    record: "turn-item index",
                });
            };
            let expected = TurnItemOrdinal::new(turn.next_ordinal).map_err(|_| {
                RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor item ordinal is invalid",
                }
            })?;
            if index.turn_id() != turn.id
                || index.ordinal() != expected
                || page.has_more() != (turn.next_ordinal < turn.item_count)
            {
                return Err(RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor item page disagrees with its finalized frontier",
                });
            }
            let descriptor = self.recovery_item_descriptor(store, turn.id, index)?;
            let declared_utf8_bytes = descriptor.source.logical_utf8_bytes();
            if declared_utf8_bytes == 0 {
                return Err(RecoveryProjectionError::EmptyHistoryItem);
            }
            let sequence_ordinal = cursor.completed_items.checked_add(1).ok_or(
                RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor sequence ordinal overflowed",
                },
            )?;
            cursor
                .accumulator
                .as_mut()
                .ok_or(RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor lost its sequence accumulator before EOF",
                })?
                .begin_item(sequence_ordinal, descriptor.role, declared_utf8_bytes)
                .map_err(cursor_sequence_mismatch)?;
            cursor.item = Some(CursorItem {
                descriptor,
                declared_utf8_bytes,
                offset: 0,
            });
            cursor.pending_read_bytes.add(
                page.stored_bytes(),
                page.decoded_bytes(),
                "recovery cursor index-page byte accounting overflowed",
            )?;
            #[cfg(feature = "test-faults")]
            crate::test_faults::metrics::record_recovery_resident_state(1, 1);
            return Ok(());
        }
    }

    fn accept_cursor_page(
        &self,
        cursor: &mut RecoveryCursor,
        item: CursorItem,
        page_lease: PageLease,
        page_totals: ReadByteTotals,
    ) -> Result<RecoveryCursorPage, RecoveryProjectionError> {
        let text_bytes = page_lease.len() as u64;
        let next_offset =
            item.offset
                .checked_add(text_bytes)
                .ok_or(RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor item-local offset overflowed",
                })?;
        let accumulator =
            cursor
                .accumulator
                .as_mut()
                .ok_or(RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor lost its sequence accumulator before EOF",
                })?;
        if let Err(error) = accumulator.update_text(page_lease.as_slice()) {
            cursor.poisoned = true;
            return Err(cursor_sequence_mismatch(error));
        }
        let item_terminal = next_offset == item.declared_utf8_bytes;
        if next_offset > item.declared_utf8_bytes {
            cursor.poisoned = true;
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "recovery cursor text crossed the declared item frontier",
            });
        }

        let sequence_ordinal = cursor.completed_items + 1;
        cursor.item = Some(CursorItem {
            offset: next_offset,
            ..item
        });
        if item_terminal {
            if let Err(error) = accumulator.finish_item() {
                cursor.poisoned = true;
                return Err(cursor_sequence_mismatch(error));
            }
            cursor.item = None;
            cursor.completed_items = sequence_ordinal;
            let turn = cursor
                .turn
                .as_mut()
                .expect("a cursor item belongs to one turn");
            turn.next_ordinal = turn.next_ordinal.checked_add(1).ok_or(
                RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor item ordinal overflowed",
                },
            )?;
        }

        let sequence_terminal =
            item_terminal && cursor.completed_items == u64::from(cursor.proof.item_count().get());
        if sequence_terminal {
            let turn = cursor
                .turn
                .expect("the final cursor item belongs to one turn");
            if cursor.next_turn_depth != cursor.topology.depth
                || turn.next_ordinal <= turn.item_count
            {
                cursor.poisoned = true;
                return Err(RecoveryProjectionError::CursorMismatch {
                    reason: "declared recovery item total ended before topology EOF",
                });
            }
            let digest = cursor
                .accumulator
                .take()
                .ok_or(RecoveryProjectionError::CursorMismatch {
                    reason: "recovery cursor lost its EOF sequence accumulator",
                })?
                .finish()
                .map_err(cursor_sequence_mismatch)?;
            if digest != cursor.proof.sequence_digest() {
                cursor.poisoned = true;
                return Err(RecoveryProjectionError::CursorMismatch {
                    reason: "replayed recovery sequence digest disagrees with preflight",
                });
            }
            cursor.state = CursorState::TerminalPendingEof;
        }
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_recovery_cursor_page(page_lease.len());
        cursor.pending_read_bytes = ReadByteTotals::default();
        Ok(RecoveryCursorPage {
            sequence_ordinal,
            role: item.descriptor.role,
            declared_item_utf8_bytes: item.declared_utf8_bytes,
            item_offset: item.offset,
            page_lease,
            stored_bytes: page_totals.stored,
            decoded_bytes: page_totals.decoded,
            item_terminal,
            sequence_terminal,
        })
    }
}

fn cursor_index_page_limits() -> CursorReadLimits {
    CursorReadLimits::new(1, INDEX_PAGE_MAX_BYTES)
        .expect("recovery cursor index-page bounds are nonzero")
}

fn cursor_sequence_mismatch(_: beryl_model::RecoveryItemSequenceError) -> RecoveryProjectionError {
    RecoveryProjectionError::CursorMismatch {
        reason: "shared recovery sequence accumulator rejected cursor replay",
    }
}
