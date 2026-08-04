use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{
    RecoveryItemSequenceAccumulator, RecoveryItemSequenceRole, SyndicPathDigest, SyndicTurnId,
};

use crate::{
    CanonicalItemKind, ContentEncoding, ContentLifecycle, ConversationParent, ProjectionTextSource,
    RecoveryBudgetKind, RecoveryItemCount, RecoveryProjectionError, SourceEventPayload,
    SourceEventSequence, SyndicPointReadLimit, SyndicStorage, TurnDepth, TurnEndStatus,
    TurnIncompleteReason, TurnItemOrdinal, TurnKind, TurnLifecycle, TurnRecord,
};

use super::{
    INDEX_PAGE_MAX_BYTES, INDEX_PAGE_MAX_ITEMS, RecoveryItemDescriptor, point_limit,
    text::read_recovery_utf8_page,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryTailEligibility {
    Strict,
    PendingParentAuthorityLost,
}

impl RecoveryTailEligibility {
    const fn allows_authority_lost(self) -> bool {
        matches!(self, Self::PendingParentAuthorityLost)
    }
}

pub(super) struct RecoveryTopology {
    pub(super) tail: TurnRecord,
    pub(super) depth: u64,
    pub(super) item_count: u64,
    pub(super) digest: SyndicPathDigest,
    pub(super) tail_eligibility: RecoveryTailEligibility,
}

impl SyndicStorage {
    pub(super) fn inspect_recovery_topology(
        &self,
        store: &HomeStore,
        prefix_tail: SyndicTurnId,
        mut expected_depth: u64,
        tail_eligibility: RecoveryTailEligibility,
    ) -> Result<RecoveryTopology, RecoveryProjectionError> {
        let mut next = prefix_tail;
        let mut item_count = 0_u64;
        let mut tail = None;
        loop {
            let turn = self.load_recovery_turn(store, next)?;
            let is_tail = tail.is_none();
            let turn_item_count = self.validate_recovery_turn(
                store,
                &turn,
                expected_depth,
                if is_tail {
                    tail_eligibility
                } else {
                    RecoveryTailEligibility::Strict
                },
            )?;
            if tail.is_none() {
                tail = Some(turn.clone());
            }
            let required = item_count.checked_add(turn_item_count).ok_or(
                RecoveryProjectionError::BudgetOverflow {
                    kind: RecoveryBudgetKind::ItemCount,
                    maximum: RecoveryItemCount::MAX,
                    actual: u64::MAX,
                },
            )?;
            if required > RecoveryItemCount::MAX {
                return Err(RecoveryProjectionError::BudgetOverflow {
                    kind: RecoveryBudgetKind::ItemCount,
                    maximum: RecoveryItemCount::MAX,
                    actual: required,
                });
            }
            item_count = required;
            #[cfg(feature = "test-faults")]
            crate::test_faults::metrics::record_recovery_resident_state(1, 0);

            match turn.parent() {
                ConversationParent::Root => {
                    if expected_depth != 1 {
                        return Err(RecoveryProjectionError::Invariant(
                            "recovery ancestry reached root at a non-root depth",
                        ));
                    }
                    break;
                }
                ConversationParent::Turn(parent) => {
                    expected_depth =
                        expected_depth
                            .checked_sub(1)
                            .ok_or(RecoveryProjectionError::Invariant(
                                "recovery ancestry parent depth underflowed",
                            ))?;
                    next = parent;
                }
            }
        }
        let tail = tail.expect("a nonempty recovery prefix has one tail");
        Ok(RecoveryTopology {
            depth: tail.depth().get(),
            digest: tail.chain_digest(),
            tail,
            item_count,
            tail_eligibility,
        })
    }

    pub(super) fn recovery_utf8_total(
        &self,
        store: &HomeStore,
        topology: &RecoveryTopology,
        utf8_limit: u64,
    ) -> Result<u64, RecoveryProjectionError> {
        let mut utf8_bytes = 0_u64;
        self.walk_recovery_items(store, topology, |_, descriptor| {
            let logical_bytes = descriptor.source.logical_utf8_bytes();
            if logical_bytes == 0 {
                return Err(RecoveryProjectionError::EmptyHistoryItem);
            }
            let required = utf8_bytes.checked_add(logical_bytes).ok_or(
                RecoveryProjectionError::BudgetOverflow {
                    kind: RecoveryBudgetKind::Utf8Bytes,
                    maximum: utf8_limit,
                    actual: u64::MAX,
                },
            )?;
            if required > utf8_limit {
                return Err(RecoveryProjectionError::BudgetOverflow {
                    kind: RecoveryBudgetKind::Utf8Bytes,
                    maximum: utf8_limit,
                    actual: required,
                });
            }
            utf8_bytes = required;
            Ok(())
        })?;
        Ok(utf8_bytes)
    }

    pub(super) fn hash_recovery_sequence(
        &self,
        store: &HomeStore,
        topology: &RecoveryTopology,
        utf8_bytes: u64,
    ) -> Result<beryl_model::RecoveryItemSequenceDigest, RecoveryProjectionError> {
        let mut accumulator = RecoveryItemSequenceAccumulator::new(topology.item_count, utf8_bytes);
        let mut text_page = [0_u8; super::RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES];
        self.walk_recovery_items(store, topology, |ordinal, descriptor| {
            let declared = descriptor.source.logical_utf8_bytes();
            accumulator
                .begin_item(ordinal, descriptor.role, declared)
                .map_err(sequence_invariant)?;
            let mut offset = 0_u64;
            while offset < declared {
                let (page_len, _) = read_recovery_utf8_page(
                    self,
                    store,
                    descriptor.source,
                    offset,
                    super::RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES,
                    &mut text_page,
                )?;
                accumulator
                    .update_text(&text_page[..page_len])
                    .map_err(sequence_invariant)?;
                offset = offset.checked_add(page_len as u64).ok_or(
                    RecoveryProjectionError::Invariant("recovery text hash offset overflowed"),
                )?;
            }
            accumulator.finish_item().map_err(sequence_invariant)
        })?;
        accumulator.finish().map_err(sequence_invariant)
    }

    fn walk_recovery_items(
        &self,
        store: &HomeStore,
        topology: &RecoveryTopology,
        mut visit: impl FnMut(u64, RecoveryItemDescriptor) -> Result<(), RecoveryProjectionError>,
    ) -> Result<(), RecoveryProjectionError> {
        let mut sequence_ordinal = 0_u64;
        for depth in 1..=topology.depth {
            let turn = self.recovery_turn_at_depth(store, topology, depth)?;
            let item_count = self.validate_recovery_turn(
                store,
                &turn,
                depth,
                if depth == topology.depth {
                    topology.tail_eligibility
                } else {
                    RecoveryTailEligibility::Strict
                },
            )?;
            let mut after = None;
            let mut observed = 0_u64;
            while observed < item_count {
                #[cfg(feature = "test-faults")]
                crate::test_faults::metrics::record_recovery_turn_item_read_attempt();
                let page = self.turn_items(store, turn.id(), after, index_page_limits())?;
                if page.records().is_empty() {
                    return Err(RecoveryProjectionError::MissingHistory {
                        record: "turn-item index",
                    });
                }
                for index in page.records() {
                    if observed >= item_count {
                        return Err(RecoveryProjectionError::Invariant(
                            "turn-item index exceeds its finalized frontier",
                        ));
                    }
                    let expected = TurnItemOrdinal::new(observed + 1).map_err(|_| {
                        RecoveryProjectionError::Invariant(
                            "recovery turn-item ordinal could not be represented",
                        )
                    })?;
                    if index.turn_id() != turn.id() || index.ordinal() != expected {
                        return Err(RecoveryProjectionError::Invariant(
                            "recovery turn-item index order disagrees",
                        ));
                    }
                    let descriptor = self.recovery_item_descriptor(store, turn.id(), index)?;
                    sequence_ordinal = sequence_ordinal.checked_add(1).ok_or(
                        RecoveryProjectionError::Invariant("recovery sequence ordinal overflowed"),
                    )?;
                    visit(sequence_ordinal, descriptor)?;
                    observed += 1;
                    after = Some(index.ordinal());
                    #[cfg(feature = "test-faults")]
                    crate::test_faults::metrics::record_recovery_resident_state(1, 1);
                }
                if observed < item_count && !page.has_more() {
                    return Err(RecoveryProjectionError::MissingHistory {
                        record: "turn-item index",
                    });
                }
                if observed == item_count && page.has_more() {
                    return Err(RecoveryProjectionError::Invariant(
                        "turn-item indexes continue past the finalized frontier",
                    ));
                }
            }
        }
        if sequence_ordinal != topology.item_count {
            return Err(RecoveryProjectionError::Invariant(
                "recovery item scan disagrees with its count-only topology pass",
            ));
        }
        Ok(())
    }

    pub(super) fn recovery_turn_at_depth(
        &self,
        store: &HomeStore,
        topology: &RecoveryTopology,
        depth: u64,
    ) -> Result<TurnRecord, RecoveryProjectionError> {
        let target = TurnDepth::new(depth).map_err(|_| {
            RecoveryProjectionError::Invariant("recovery target turn depth is zero")
        })?;
        crate::selected_path::lift_to_depth(
            topology.tail.clone(),
            target,
            |id| self.load_recovery_turn(store, id),
            RecoveryProjectionError::Invariant,
        )
    }

    pub(super) fn load_recovery_turn(
        &self,
        store: &HomeStore,
        id: SyndicTurnId,
    ) -> Result<TurnRecord, RecoveryProjectionError> {
        let turn = self
            .turn(store, id, point_limit())?
            .ok_or(RecoveryProjectionError::MissingHistory { record: "turn" })?;
        if turn.id() != id {
            return Err(RecoveryProjectionError::Invariant(
                "recovery turn point read returned a different identity",
            ));
        }
        Ok(turn.clone())
    }

    pub(super) fn validate_recovery_turn(
        &self,
        store: &HomeStore,
        turn: &TurnRecord,
        expected_depth: u64,
        tail_eligibility: RecoveryTailEligibility,
    ) -> Result<u64, RecoveryProjectionError> {
        let state = self.turn_state(store, turn.id(), point_limit())?.ok_or(
            RecoveryProjectionError::MissingHistory {
                record: "turn-state",
            },
        )?;
        let state = state;
        if state.turn_id() != turn.id() || turn.depth().get() != expected_depth {
            return Err(RecoveryProjectionError::Invariant(
                "recovery topology identity or depth disagrees",
            ));
        }
        if turn.kind() != TurnKind::OrdinaryUser {
            return Err(RecoveryProjectionError::UnsupportedHistory {
                reason: "provider-operation turn",
            });
        }
        let recovery_complete = matches!(
            state.lifecycle(),
            TurnLifecycle::Complete | TurnLifecycle::Interrupted | TurnLifecycle::Failed
        ) && state.incomplete_reason().is_none();
        let authority_lost_context = tail_eligibility.allows_authority_lost()
            && state.lifecycle() == TurnLifecycle::Incomplete
            && state.incomplete_reason() == Some(TurnIncompleteReason::AuthorityLost)
            && self.has_exact_authority_lost_terminal(store, &state)?;
        if (!recovery_complete && !authority_lost_context)
            || state.provider_observation_issue().is_some()
            || state.finalized_item_count() != state.item_count()
            || state.open_item_count() != 0
            || state.history_blocking_item_count() != 0
        {
            return Err(RecoveryProjectionError::IncompleteHistory {
                reason: "turn is not recovery-eligible; the exact outcome, history-incomplete and provider-observation issue facts, item audit, finalized frontier, and any authority-lost terminal-source exception must all agree",
            });
        }
        if state.item_count() == 0 {
            return Err(RecoveryProjectionError::IncompleteHistory {
                reason: "included turn has no canonical items",
            });
        }
        Ok(state.item_count())
    }

    fn has_exact_authority_lost_terminal(
        &self,
        store: &HomeStore,
        state: &crate::TurnStateRecord,
    ) -> Result<bool, RecoveryProjectionError> {
        let sequence = SourceEventSequence::new(state.source_event_count()).map_err(|_| {
            RecoveryProjectionError::IncompleteHistory {
                reason: "authority-lost tail context has no terminal source event",
            }
        })?;
        let event = self
            .source_event(store, state.turn_id(), sequence, point_limit())?
            .ok_or(RecoveryProjectionError::MissingHistory {
                record: "authority-lost terminal source event",
            })?;
        Ok(event.turn_id() == state.turn_id()
            && event.sequence() == sequence
            && event.source().is_none()
            && event.payload()
                == &SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
                    TurnIncompleteReason::AuthorityLost,
                )))
    }

    pub(super) fn recovery_item_descriptor(
        &self,
        store: &HomeStore,
        turn_id: SyndicTurnId,
        index: &crate::TurnItemIndexRecord,
    ) -> Result<RecoveryItemDescriptor, RecoveryProjectionError> {
        let item = self
            .canonical_item(store, index.item_id(), point_limit())?
            .ok_or(RecoveryProjectionError::MissingHistory {
                record: "canonical-item",
            })?;
        let item = item;
        if item.id() != index.item_id()
            || item.turn_id() != turn_id
            || item.ordinal() != index.ordinal()
            || item.revision() != index.item_revision()
        {
            return Err(RecoveryProjectionError::Invariant(
                "recovery canonical item disagrees with its turn index",
            ));
        }
        self.preflight_recovery_item(store, &item, point_limit())
    }

    fn preflight_recovery_item(
        &self,
        store: &HomeStore,
        item: &crate::CanonicalItemRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<RecoveryItemDescriptor, RecoveryProjectionError> {
        if item.is_history_blocking() {
            return Err(RecoveryProjectionError::IncompleteHistory {
                reason: "canonical item blocks exact history recovery",
            });
        }
        let (role, source) = match item.kind() {
            CanonicalItemKind::UserInput => {
                let content = item.presentation_content().ok_or(
                    RecoveryProjectionError::UnsupportedHistory {
                        reason: "user input omitted composer content",
                    },
                )?;
                if item.presentation().asset_reference_set().is_some()
                    || content.summary().image_marker_count() != 0
                {
                    return Err(RecoveryProjectionError::MediaHistory {
                        reason: "user input contains an image marker",
                    });
                }
                if content.encoding() != ContentEncoding::ComposerV1 {
                    return Err(RecoveryProjectionError::UnsupportedHistory {
                        reason: "user input is not canonical composer content",
                    });
                }
                let manifest = self.content_manifest(store, content.id(), limit)?.ok_or(
                    RecoveryProjectionError::MissingHistory {
                        record: "composer content-manifest",
                    },
                )?;
                let manifest = manifest;
                if manifest.lifecycle() != ContentLifecycle::Sealed
                    || manifest.owner().is_some()
                    || manifest.sealed_reference() != Some(content)
                {
                    return Err(RecoveryProjectionError::UnsupportedHistory {
                        reason: "user item does not reference sealed composer authority",
                    });
                }
                (
                    RecoveryItemSequenceRole::UserInputText,
                    ProjectionTextSource::composer(content),
                )
            }
            CanonicalItemKind::AssistantMessage(_) => {
                let source = item.projection_source().ok_or(
                    RecoveryProjectionError::UnsupportedHistory {
                        reason: "assistant item omitted provider narrative",
                    },
                )?;
                if !matches!(source, ProjectionTextSource::ProviderNarrative(_))
                    || item.narrative_completion()
                        != Some(crate::ProviderNarrativeCompletionDisposition::Equal)
                {
                    return Err(RecoveryProjectionError::UnsupportedHistory {
                        reason: "assistant item has no exact completed narrative",
                    });
                }
                let content =
                    item.provider_content()
                        .ok_or(RecoveryProjectionError::UnsupportedHistory {
                            reason: "assistant item omitted provider content",
                        })?;
                let manifest = self.content_manifest(store, content.id(), limit)?.ok_or(
                    RecoveryProjectionError::MissingHistory {
                        record: "provider content-manifest",
                    },
                )?;
                let manifest = manifest;
                if manifest.lifecycle() != ContentLifecycle::Finalized
                    || manifest.owner() != Some(item.id())
                    || manifest.current_reference() != Some(content)
                {
                    return Err(RecoveryProjectionError::IncompleteHistory {
                        reason: "assistant provider content is not finalized by that item",
                    });
                }
                (RecoveryItemSequenceRole::AssistantOutputText, source)
            }
            CanonicalItemKind::ProviderText(_) => {
                return Err(RecoveryProjectionError::UnsupportedHistory {
                    reason: "typed provider text has no lossless recovery projection",
                });
            }
            CanonicalItemKind::Operational(_) | CanonicalItemKind::Activity(_) => {
                return Err(RecoveryProjectionError::UnsupportedHistory {
                    reason: "operational canonical item",
                });
            }
            CanonicalItemKind::GeneratedMedia => {
                return Err(RecoveryProjectionError::MediaHistory {
                    reason: "generated media has no finalized recovery projection",
                });
            }
        };
        Ok(RecoveryItemDescriptor { role, source })
    }
}

fn index_page_limits() -> CursorReadLimits {
    CursorReadLimits::new(INDEX_PAGE_MAX_ITEMS, INDEX_PAGE_MAX_BYTES)
        .expect("recovery index page bounds are nonzero")
}

fn sequence_invariant(_: beryl_model::RecoveryItemSequenceError) -> RecoveryProjectionError {
    RecoveryProjectionError::Invariant("recovery sequence accumulator rejected preflight replay")
}
