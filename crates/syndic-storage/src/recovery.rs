use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{DomainRevision, SyndicTurnId};

use crate::{
    CanonicalItemKind, CasRepresentedPrefixProof, ContentEncoding, ContentLifecycle,
    ContentReference, ConversationParent, RecoveryBudgetKind, RecoveryItemCount,
    RecoveryProjectionError, RecoveryProjectionVersion, RecoveryUtf8ByteCount,
    SyndicPointReadLimit, SyndicStorage, TurnItemOrdinal, TurnKind, TurnLifecycle,
};

const POINT_READ_MAX_BYTES: usize = 16_384;
const INDEX_PAGE_MAX_ITEMS: usize = 256;
const INDEX_PAGE_MAX_BYTES: usize = 65_536;

mod text;
mod types;

use text::recovery_sequence_digest;
pub use types::{
    RecoveryAssembly, RecoveryItem, RecoveryItemRole, RecoveryItemTextKind, RecoveryProjection,
    RecoveryProjectionRequest, RecoveryProjectionScope,
};

#[derive(Clone, Copy)]
struct TurnFrontier {
    id: SyndicTurnId,
    item_count: u64,
}

#[derive(Clone, Copy)]
struct ItemFrontier {
    role: RecoveryItemRole,
    content: ContentReference,
}

impl SyndicStorage {
    /// Prepares one exact recovery prefix without mutating Syndic state.
    ///
    /// Current-selected-path scope assembles through that path's tail for pre-admission validation.
    /// Pending-selected-turn-parent scope never reads the pending turn's canonical items and
    /// retains the restart behavior of assembling only its parent prefix. Both scopes follow
    /// immutable parents, canonical turn-item indexes, logical text spans, and bounded physical
    /// chunk point reads.
    pub fn prepare_recovery_projection(
        &self,
        store: &HomeStore,
        request: RecoveryProjectionRequest,
    ) -> Result<RecoveryAssembly, RecoveryProjectionError> {
        let source_revision = self.revision(store)?;
        let limit = point_limit();
        let thread = self
            .thread(store, request.thread_id(), limit)?
            .ok_or(RecoveryProjectionError::StaleSelectedPath)?;
        let thread = thread.record();
        if thread.id() != request.thread_id()
            || thread.revision() != request.selected_path().thread_revision()
            || thread.committed_tail() != request.selected_path().tail()
            || thread.selected_path_digest() != request.selected_path().digest()
        {
            return Err(RecoveryProjectionError::StaleSelectedPath);
        }

        let Some(tail_id) = request.selected_path().tail() else {
            return match request.scope() {
                RecoveryProjectionScope::CurrentSelectedPath => {
                    self.native_empty_recovery(store, request, source_revision)
                }
                RecoveryProjectionScope::PendingSelectedTurnParent => {
                    Err(RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser)
                }
            };
        };
        let tail = self
            .turn(store, tail_id, limit)?
            .ok_or(RecoveryProjectionError::MissingHistory { record: "turn" })?;
        let tail = tail.record();
        let tail_state = self.turn_state(store, tail_id, limit)?.ok_or(
            RecoveryProjectionError::MissingHistory {
                record: "turn-state",
            },
        )?;
        let tail_state = tail_state.record();
        if tail.id() != tail_id
            || tail_state.turn_id() != tail_id
            || tail.chain_digest() != request.selected_path().digest()
        {
            return Err(RecoveryProjectionError::StaleSelectedPath);
        }

        let (prefix_tail, expected_depth) = match request.scope() {
            RecoveryProjectionScope::CurrentSelectedPath => (tail_id, tail.depth().get()),
            RecoveryProjectionScope::PendingSelectedTurnParent => {
                if tail.kind() != TurnKind::OrdinaryUser
                    || tail_state.lifecycle() != TurnLifecycle::Pending
                {
                    return Err(RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser);
                }
                match tail.parent() {
                    ConversationParent::Root => {
                        if tail.depth().get() != 1 {
                            return Err(RecoveryProjectionError::Invariant(
                                "a root pending tail does not have depth one",
                            ));
                        }
                        return self.native_empty_recovery(store, request, source_revision);
                    }
                    ConversationParent::Turn(parent) => {
                        let depth = tail.depth().get().checked_sub(1).ok_or(
                            RecoveryProjectionError::Invariant(
                                "a non-root pending tail has no parent depth",
                            ),
                        )?;
                        (parent, depth)
                    }
                }
            }
        };

        let model_tokens = request
            .model_context_window_tokens()
            .ok_or(RecoveryProjectionError::MissingModelContextWindow)?;
        if model_tokens == 0 {
            return Err(RecoveryProjectionError::ZeroModelContextWindow);
        }
        let utf8_limit = RecoveryUtf8ByteCount::MAX.min(model_tokens / 2);

        let (turns, prefix_digest, expected_item_count) =
            self.collect_turn_frontier(store, prefix_tail, expected_depth, limit)?;
        let represented_prefix = CasRepresentedPrefixProof::new(
            Some(prefix_tail),
            request.selected_path().thread_revision(),
            prefix_digest,
        );
        let (item_frontier, utf8_bytes) =
            self.collect_item_frontier(store, &turns, expected_item_count, utf8_limit, limit)?;
        let items = self.materialize_items(store, &item_frontier)?;
        let item_count = RecoveryItemCount::new(expected_item_count).map_err(|_| {
            RecoveryProjectionError::Invariant("accepted recovery item count became invalid")
        })?;
        let utf8_bytes = RecoveryUtf8ByteCount::new(utf8_bytes).map_err(|_| {
            RecoveryProjectionError::Invariant("accepted recovery byte count became invalid")
        })?;
        let sequence_digest = recovery_sequence_digest(&items, utf8_bytes.get());

        if self.revision(store)? != source_revision {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        Ok(RecoveryAssembly::Ready(RecoveryProjection {
            version: RecoveryProjectionVersion::V1,
            thread_id: request.thread_id(),
            selected_path: request.selected_path(),
            represented_prefix,
            items: items.into_boxed_slice(),
            item_count,
            utf8_bytes,
            sequence_digest,
            source_revision,
        }))
    }

    fn native_empty_recovery(
        &self,
        store: &HomeStore,
        request: RecoveryProjectionRequest,
        source_revision: DomainRevision,
    ) -> Result<RecoveryAssembly, RecoveryProjectionError> {
        if self.revision(store)? != source_revision {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        Ok(RecoveryAssembly::NativeEmptyPrefix {
            thread_id: request.thread_id(),
            selected_path: request.selected_path(),
            source_revision,
        })
    }

    fn collect_turn_frontier(
        &self,
        store: &HomeStore,
        prefix_tail: SyndicTurnId,
        mut expected_depth: u64,
        limit: SyndicPointReadLimit,
    ) -> Result<(Vec<TurnFrontier>, beryl_model::SyndicPathDigest, u64), RecoveryProjectionError>
    {
        let mut turns = Vec::new();
        let mut next = prefix_tail;
        let mut item_count = 0_u64;
        let mut prefix_digest = None;
        loop {
            let turn = self
                .turn(store, next, limit)?
                .ok_or(RecoveryProjectionError::MissingHistory { record: "turn" })?;
            let turn = turn.record();
            let state = self.turn_state(store, next, limit)?.ok_or(
                RecoveryProjectionError::MissingHistory {
                    record: "turn-state",
                },
            )?;
            let state = state.record();
            if turn.id() != next || state.turn_id() != next || turn.depth().get() != expected_depth
            {
                return Err(RecoveryProjectionError::Invariant(
                    "recovery parent topology identity or depth disagrees",
                ));
            }
            prefix_digest.get_or_insert(turn.chain_digest());
            if turn.kind() != TurnKind::OrdinaryUser {
                return Err(RecoveryProjectionError::UnsupportedHistory {
                    reason: "provider-operation turn",
                });
            }
            if !matches!(
                state.lifecycle(),
                TurnLifecycle::Complete | TurnLifecycle::Interrupted | TurnLifecycle::Failed
            ) || state.incomplete_reason().is_some()
                || state.finalized_item_count() != state.item_count()
                || state.open_item_count() != 0
                || state.history_blocking_item_count() != 0
            {
                return Err(RecoveryProjectionError::IncompleteHistory {
                    reason: "turn is not recovery-complete; the exact outcome, history-incomplete reason, item audit, and finalized frontier must all be complete",
                });
            }
            if state.item_count() == 0 {
                return Err(RecoveryProjectionError::IncompleteHistory {
                    reason: "included turn has no canonical items",
                });
            }
            let required = item_count.checked_add(state.item_count()).ok_or(
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
            turns.push(TurnFrontier {
                id: next,
                item_count: state.item_count(),
            });

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
        Ok((
            turns,
            prefix_digest.expect("nonempty parent prefix has one digest"),
            item_count,
        ))
    }

    fn collect_item_frontier(
        &self,
        store: &HomeStore,
        turns_tail_to_root: &[TurnFrontier],
        expected_item_count: u64,
        utf8_limit: u64,
        limit: SyndicPointReadLimit,
    ) -> Result<(Vec<ItemFrontier>, u64), RecoveryProjectionError> {
        let capacity = usize::try_from(expected_item_count).map_err(|_| {
            RecoveryProjectionError::Invariant("recovery item allocation length overflowed")
        })?;
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_recovery_frontier_allocation_attempt(capacity);
        let mut items = Vec::with_capacity(capacity);
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_recovery_frontier_allocation_completion(
            items.capacity(),
        );
        let mut utf8_bytes = 0_u64;
        let page_limits = || {
            CursorReadLimits::new(INDEX_PAGE_MAX_ITEMS, INDEX_PAGE_MAX_BYTES)
                .expect("recovery index page bounds are nonzero")
        };

        for turn in turns_tail_to_root.iter().rev() {
            let mut after = None;
            let mut observed = 0_u64;
            while observed < turn.item_count {
                #[cfg(feature = "test-faults")]
                crate::test_faults::metrics::record_recovery_turn_item_read_attempt();
                let page = self.turn_items(store, turn.id, after, page_limits())?;
                if page.records().is_empty() {
                    return Err(RecoveryProjectionError::MissingHistory {
                        record: "turn-item index",
                    });
                }
                for index in page.records() {
                    if observed >= turn.item_count {
                        return Err(RecoveryProjectionError::Invariant(
                            "turn-item index exceeds its finalized frontier",
                        ));
                    }
                    let expected_ordinal = TurnItemOrdinal::new(observed + 1).map_err(|_| {
                        RecoveryProjectionError::Invariant(
                            "recovery turn-item ordinal could not be represented",
                        )
                    })?;
                    if index.turn_id() != turn.id || index.ordinal() != expected_ordinal {
                        return Err(RecoveryProjectionError::Invariant(
                            "recovery turn-item index order disagrees",
                        ));
                    }
                    let item = self.canonical_item(store, index.item_id(), limit)?.ok_or(
                        RecoveryProjectionError::MissingHistory {
                            record: "canonical-item",
                        },
                    )?;
                    let item = item.record();
                    if item.id() != index.item_id()
                        || item.turn_id() != turn.id
                        || item.ordinal() != index.ordinal()
                        || item.revision() != index.item_revision()
                    {
                        return Err(RecoveryProjectionError::Invariant(
                            "recovery canonical item disagrees with its turn index",
                        ));
                    }
                    let frontier = self.preflight_item(store, item, limit)?;
                    let logical_bytes = frontier.content.summary().logical_utf8_bytes();
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
                    items.push(frontier);
                    observed += 1;
                    after = Some(index.ordinal());
                }
                if observed < turn.item_count && !page.has_more() {
                    return Err(RecoveryProjectionError::MissingHistory {
                        record: "turn-item index",
                    });
                }
                if observed == turn.item_count && page.has_more() {
                    return Err(RecoveryProjectionError::Invariant(
                        "turn-item indexes continue past the finalized frontier",
                    ));
                }
            }
        }
        if u64::try_from(items.len()).ok() != Some(expected_item_count) {
            return Err(RecoveryProjectionError::Invariant(
                "recovery item frontier count disagrees",
            ));
        }
        Ok((items, utf8_bytes))
    }

    fn preflight_item(
        &self,
        store: &HomeStore,
        item: &crate::CanonicalItemRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<ItemFrontier, RecoveryProjectionError> {
        let (role, content) = match item.kind() {
            CanonicalItemKind::UserInput => {
                let content = item.payload().content().ok_or(
                    RecoveryProjectionError::UnsupportedHistory {
                        reason: "user input omitted canonical content",
                    },
                )?;
                if item.payload().marker_count() != 0 || content.summary().image_marker_count() != 0
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
                (RecoveryItemRole::User, content)
            }
            CanonicalItemKind::AssistantMessage(_) => {
                let content = item.payload().content().ok_or(
                    RecoveryProjectionError::UnsupportedHistory {
                        reason: "assistant item omitted canonical content",
                    },
                )?;
                if content.summary().image_marker_count() != 0 {
                    return Err(RecoveryProjectionError::MediaHistory {
                        reason: "assistant item contains media",
                    });
                }
                if content.encoding() != ContentEncoding::Utf8V1 {
                    return Err(RecoveryProjectionError::UnsupportedHistory {
                        reason: "assistant output is not canonical UTF-8 content",
                    });
                }
                (RecoveryItemRole::Assistant, content)
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
            CanonicalItemKind::Unsupported(_) => {
                return Err(RecoveryProjectionError::UnsupportedHistory {
                    reason: "canonical item has an explicit unsupported-history disposition",
                });
            }
        };

        let manifest = self.content_manifest(store, content.id(), limit)?.ok_or(
            RecoveryProjectionError::MissingHistory {
                record: "content-manifest",
            },
        )?;
        let manifest = manifest.record();
        if manifest.id() != content.id()
            || manifest.current_reference() != Some(content)
            || !manifest.lifecycle().is_immutable()
        {
            return Err(RecoveryProjectionError::IncompleteHistory {
                reason: "canonical content is missing its exact immutable manifest frontier",
            });
        }
        match role {
            RecoveryItemRole::User
                if manifest.lifecycle() != ContentLifecycle::Sealed
                    || manifest.owner().is_some() =>
            {
                return Err(RecoveryProjectionError::UnsupportedHistory {
                    reason: "user item does not reference sealed composer authority",
                });
            }
            RecoveryItemRole::Assistant
                if manifest.lifecycle() != ContentLifecycle::Finalized
                    || manifest.owner() != Some(item.id()) =>
            {
                return Err(RecoveryProjectionError::IncompleteHistory {
                    reason: "assistant item content is not finalized by that item",
                });
            }
            RecoveryItemRole::User | RecoveryItemRole::Assistant => {}
        }
        Ok(ItemFrontier { role, content })
    }
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_MAX_BYTES).expect("recovery point-read bound is nonzero")
}
