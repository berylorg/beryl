use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::{
    BindingRevision, SyndicContentId, SyndicItemId, SyndicProjectionId, SyndicThreadId,
    SyndicTurnId,
};

use crate::{BindingRecord, SourceEventRecord, SyndicReadError, codec::*, domain::SyndicStorage};

use super::SyndicPage;

const PHASE_7_PAGE_MAX_ITEMS: usize = 256;
const PHASE_7_PAGE_MAX_STORED_BYTES: usize = crate::TRANSCRIPT_PAGE_MAX_BYTES;

impl SyndicStorage {
    pub fn content_chunks(
        &self,
        store: &HomeStore,
        content: SyndicContentId,
        after: Option<crate::ContentChunkOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::ContentChunkRecord>, SyndicReadError> {
        self.owner_page::<ContentChunksFamily>(store, content, after, limits)
    }

    pub fn content_byte_spans(
        &self,
        store: &HomeStore,
        content: SyndicContentId,
        after_start: Option<u64>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::ContentByteSpanRecord>, SyndicReadError> {
        let first = ContentByteSpanKey {
            owner: content,
            start: 0,
        };
        let last = ContentByteSpanKey {
            owner: content,
            start: u64::MAX,
        };
        let range = match after_start {
            Some(start) => CursorRange::after(
                ContentByteSpanKey {
                    owner: content,
                    start,
                },
                last,
            ),
            None => CursorRange::closed(first, last),
        };
        self.page::<ContentByteSpansFamily>(store, range, limits)
    }

    pub fn content_text_spans(
        &self,
        store: &HomeStore,
        content: SyndicContentId,
        after_logical_start: Option<u64>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::ContentTextSpanRecord>, SyndicReadError> {
        let last = ContentTextSpanKey {
            owner: content,
            logical_start: u64::MAX,
        };
        let range = match after_logical_start {
            Some(logical_start) => CursorRange::after(
                ContentTextSpanKey {
                    owner: content,
                    logical_start,
                },
                last,
            ),
            None => CursorRange::closed(
                ContentTextSpanKey {
                    owner: content,
                    logical_start: 0,
                },
                last,
            ),
        };
        self.page::<ContentTextSpansFamily>(store, range, limits)
    }

    pub fn content_pieces(
        &self,
        store: &HomeStore,
        content: SyndicContentId,
        after: Option<crate::ContentPieceOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::ContentPieceRecord>, SyndicReadError> {
        self.owner_page::<ContentPiecesFamily>(store, content, after, limits)
    }

    pub fn source_events(
        &self,
        store: &HomeStore,
        turn: SyndicTurnId,
        after: Option<crate::SourceEventSequence>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<SourceEventRecord>, SyndicReadError> {
        self.owner_page::<SourceEventsFamily>(store, turn, after, limits)
    }

    pub fn input_marker_resolutions(
        &self,
        store: &HomeStore,
        owner: crate::InputMarkerOwner,
        after: Option<crate::InputMarkerOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::InputMarkerResolutionRecord>, SyndicReadError> {
        self.owner_page::<InputMarkerResolutionsFamily>(store, owner, after, limits)
    }

    pub fn accepted_order(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        after: Option<crate::AcceptedInputOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::AcceptedOrderIndexRecord>, SyndicReadError> {
        self.owner_page::<AcceptedOrderFamily>(store, thread, after, limits)
    }

    pub fn accepted_next_turn(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        after: Option<crate::AcceptedInputOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::AcceptedNextTurnIndexRecord>, SyndicReadError> {
        self.owner_page::<AcceptedNextFamily>(store, thread, after, limits)
    }

    pub fn accepted_steering(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        after: Option<crate::AcceptedInputOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::AcceptedSteeringIndexRecord>, SyndicReadError> {
        let first = SteeringKey {
            thread,
            turn,
            ordinal: crate::AcceptedInputOrdinal::FIRST,
        };
        let last = SteeringKey {
            thread,
            turn,
            ordinal: crate::AcceptedInputOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        };
        let range = match after {
            Some(ordinal) => CursorRange::after(
                SteeringKey {
                    thread,
                    turn,
                    ordinal,
                },
                last,
            ),
            None => CursorRange::closed(first, last),
        };
        self.page::<AcceptedSteeringFamily>(store, range, limits)
    }

    pub fn turn_items(
        &self,
        store: &HomeStore,
        turn: SyndicTurnId,
        after: Option<crate::TurnItemOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::TurnItemIndexRecord>, SyndicReadError> {
        self.owner_page::<TurnItemsFamily>(store, turn, after, limits)
    }

    pub fn item_source_events(
        &self,
        store: &HomeStore,
        item: SyndicItemId,
        after: Option<crate::ItemSourceEventOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::ItemSourceEventIndexRecord>, SyndicReadError> {
        self.owner_page::<ItemSourceEventsFamily>(store, item, after, limits)
    }

    pub fn transcript_entries(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        generation: crate::TranscriptGeneration,
        after: Option<crate::TranscriptPosition>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::TranscriptViewEntryRecord>, SyndicReadError> {
        let limits = phase_7_page_limits(limits);
        let first = ThreadTranscriptKey {
            thread,
            generation,
            position: crate::TranscriptPosition::FIRST,
        };
        let last = ThreadTranscriptKey {
            thread,
            generation,
            position: crate::TranscriptPosition::new(u64::MAX).expect("maximum is nonzero"),
        };
        let range = match after {
            Some(position) => CursorRange::after(
                ThreadTranscriptKey {
                    thread,
                    generation,
                    position,
                },
                last,
            ),
            None => CursorRange::closed(first, last),
        };
        self.page::<TranscriptEntriesFamily>(store, range, limits)
    }

    pub fn transcript_path_turns(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        generation: crate::TranscriptGeneration,
        after: Option<crate::TurnDepth>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::TranscriptPathTurnRecord>, SyndicReadError> {
        let limits = phase_7_page_limits(limits);
        let last = ThreadTranscriptPathKey {
            thread,
            generation,
            depth: crate::TurnDepth::new(u64::MAX).expect("maximum is nonzero"),
        };
        let range = match after {
            Some(depth) => CursorRange::after(
                ThreadTranscriptPathKey {
                    thread,
                    generation,
                    depth,
                },
                last,
            ),
            None => CursorRange::closed(
                ThreadTranscriptPathKey {
                    thread,
                    generation,
                    depth: crate::TurnDepth::FIRST,
                },
                last,
            ),
        };
        self.page::<TranscriptPathTurnsFamily>(store, range, limits)
    }

    pub fn item_projections(
        &self,
        store: &HomeStore,
        item: SyndicItemId,
        generation: crate::ItemProjectionGeneration,
        after: Option<crate::ProjectionOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::ItemProjectionIndexRecord>, SyndicReadError> {
        let limits = phase_7_page_limits(limits);
        let set = self
            .item_projection_set(
                store,
                item,
                generation,
                super::SyndicPointReadLimit::new(crate::codec::SMALL_MAX + 64)
                    .expect("projection-set point bound is nonzero"),
            )?
            .ok_or(SyndicReadError::Invariant(
                "item-projection generation set is missing",
            ))?;
        let set = set.record();
        if set.item_id() != item || set.generation() != generation {
            return Err(SyndicReadError::Invariant(
                "item-projection generation set identity disagrees",
            ));
        }
        let Some(mut next) = after
            .map(crate::ProjectionOrdinal::get)
            .unwrap_or(0)
            .checked_add(1)
        else {
            return Ok(SyndicPage {
                records: Vec::new(),
                stored_bytes: 0,
                has_more: false,
            });
        };
        if next > set.projection_count() {
            return Ok(SyndicPage {
                records: Vec::new(),
                stored_bytes: 0,
                has_more: false,
            });
        }

        let mut records = Vec::with_capacity(limits.max_items());
        let mut stored_bytes = 0_usize;
        if next <= set.stable_projection_count() {
            let first_ordinal = crate::ProjectionOrdinal::new(next)
                .map_err(|_| SyndicReadError::Invariant("stable projection ordinal is invalid"))?;
            let last_ordinal = crate::ProjectionOrdinal::new(set.stable_projection_count())
                .map_err(|_| SyndicReadError::Invariant("stable projection frontier is invalid"))?;
            let page = store.read_cursor::<
                crate::domain::SyndicDomain,
                ExactCodec<StableItemProjectionsFamily>,
            >(
                self.handle,
                &CursorRange::closed(
                    StableItemProjectionKey {
                        item,
                        ordinal: first_ordinal,
                    },
                    StableItemProjectionKey {
                        item,
                        ordinal: last_ordinal,
                    },
                ),
                CursorDirection::Forward,
                limits,
            )?;
            stored_bytes = page.stored_bytes();
            let stable_has_more = page.has_more();
            let mut ordinal_space_exhausted = false;
            for record in page.into_records() {
                let index = record.into_parts().1;
                match index.ordinal().get().checked_add(1) {
                    Some(ordinal) => next = ordinal,
                    None => ordinal_space_exhausted = true,
                }
                records.push(crate::ItemProjectionIndexRecord::new(
                    index.item_id(),
                    generation,
                    index.ordinal(),
                    index.projection_id(),
                    index.projection_revision(),
                ));
            }
            if stable_has_more {
                return Ok(SyndicPage {
                    records,
                    stored_bytes,
                    has_more: true,
                });
            }
            if ordinal_space_exhausted {
                return Ok(SyndicPage {
                    records,
                    stored_bytes,
                    has_more: false,
                });
            }
            let Some(after_stable) = set.stable_projection_count().checked_add(1) else {
                return Ok(SyndicPage {
                    records,
                    stored_bytes,
                    has_more: false,
                });
            };
            next = next.max(after_stable);
        }

        if next <= set.projection_count()
            && records.len() < limits.max_items()
            && stored_bytes < limits.max_bytes()
        {
            let remaining = CursorReadLimits::new(
                limits.max_items() - records.len(),
                limits.max_bytes() - stored_bytes,
            )
            .expect("nonzero merged item-projection page remainder");
            let first_ordinal = crate::ProjectionOrdinal::new(next)
                .map_err(|_| SyndicReadError::Invariant("projection ordinal is invalid"))?;
            let last_ordinal = crate::ProjectionOrdinal::new(set.projection_count())
                .map_err(|_| SyndicReadError::Invariant("projection frontier is invalid"))?;
            let page = store
                .read_cursor::<crate::domain::SyndicDomain, ExactCodec<ItemProjectionsFamily>>(
                    self.handle,
                    &CursorRange::closed(
                        ItemProjectionKey {
                            item,
                            generation,
                            ordinal: first_ordinal,
                        },
                        ItemProjectionKey {
                            item,
                            generation,
                            ordinal: last_ordinal,
                        },
                    ),
                    CursorDirection::Forward,
                    remaining,
                )?;
            stored_bytes =
                stored_bytes
                    .checked_add(page.stored_bytes())
                    .ok_or(SyndicReadError::Invariant(
                        "item-projection page byte count overflowed",
                    ))?;
            let has_more = page.has_more();
            records.extend(
                page.into_records()
                    .into_iter()
                    .map(|record| record.into_parts().1),
            );
            return Ok(SyndicPage {
                records,
                stored_bytes,
                has_more,
            });
        }
        Ok(SyndicPage {
            has_more: next <= set.projection_count(),
            records,
            stored_bytes,
        })
    }

    pub fn projection_resources(
        &self,
        store: &HomeStore,
        projection: SyndicProjectionId,
        after: Option<crate::ResourceOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::ProjectionResourceIndexRecord>, SyndicReadError> {
        self.owner_page::<ProjectionResourcesFamily>(
            store,
            projection,
            after,
            phase_7_page_limits(limits),
        )
    }

    pub fn binding_history(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        after: Option<BindingRevision>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<BindingRecord>, SyndicReadError> {
        let first = BindingKey {
            thread,
            revision: BindingRevision::new(1).expect("one is nonzero"),
        };
        let last = BindingKey {
            thread,
            revision: BindingRevision::new(u64::MAX).expect("maximum is nonzero"),
        };
        let range = match after {
            Some(revision) => CursorRange::after(BindingKey { thread, revision }, last),
            None => CursorRange::closed(first, last),
        };
        self.page::<BindingsFamily>(store, range, limits)
    }

    pub fn turn_children(
        &self,
        store: &HomeStore,
        parent: SyndicTurnId,
        after: Option<SyndicTurnId>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<crate::TurnChildIndexRecord>, SyndicReadError> {
        let first = TurnPairKey {
            parent,
            child: SyndicTurnId::from_bytes([0; 16]),
        };
        let last = TurnPairKey {
            parent,
            child: SyndicTurnId::from_bytes([u8::MAX; 16]),
        };
        let range = match after {
            Some(child) => CursorRange::after(TurnPairKey { parent, child }, last),
            None => CursorRange::closed(first, last),
        };
        self.page::<TurnChildrenFamily>(store, range, limits)
    }

    fn owner_page<F: OwnerPageFamily>(
        &self,
        store: &HomeStore,
        owner: F::Owner,
        after: Option<F::Ordinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<F::Value>, SyndicReadError> {
        let first = F::first(owner);
        let last = F::last(owner);
        let range = match after {
            Some(after) => CursorRange::after(F::key(owner, after), last),
            None => CursorRange::closed(first, last),
        };
        self.page::<F>(store, range, limits)
    }

    fn page<F: Family>(
        &self,
        store: &HomeStore,
        range: CursorRange<F::Key>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<F::Value>, SyndicReadError> {
        let page = store.read_cursor::<crate::domain::SyndicDomain, ExactCodec<F>>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let has_more = page.has_more();
        Ok(SyndicPage {
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
            stored_bytes,
            has_more,
        })
    }
}

fn phase_7_page_limits(limits: CursorReadLimits) -> CursorReadLimits {
    CursorReadLimits::new(
        limits.max_items().min(PHASE_7_PAGE_MAX_ITEMS),
        limits.max_bytes().min(PHASE_7_PAGE_MAX_STORED_BYTES),
    )
    .expect("clamped nonzero Phase 7 page bounds remain nonzero")
}

trait OwnerPageFamily: Family {
    type Owner: Copy;
    type Ordinal: Copy;
    fn key(owner: Self::Owner, ordinal: Self::Ordinal) -> Self::Key;
    fn first(owner: Self::Owner) -> Self::Key;
    fn last(owner: Self::Owner) -> Self::Key;
}

macro_rules! owner_page_family {
    ($family:ty,$owner:ty,$ordinal:ty,$key:ident) => {
        impl OwnerPageFamily for $family {
            type Owner = $owner;
            type Ordinal = $ordinal;
            fn key(owner: Self::Owner, ordinal: Self::Ordinal) -> Self::Key {
                $key { owner, ordinal }
            }
            fn first(owner: Self::Owner) -> Self::Key {
                $key {
                    owner,
                    ordinal: <$ordinal>::FIRST,
                }
            }
            fn last(owner: Self::Owner) -> Self::Key {
                $key {
                    owner,
                    ordinal: <$ordinal>::new(u64::MAX).expect("maximum is nonzero"),
                }
            }
        }
    };
}
owner_page_family!(
    ContentChunksFamily,
    SyndicContentId,
    crate::ContentChunkOrdinal,
    ContentChunkKey
);
owner_page_family!(
    ContentPiecesFamily,
    SyndicContentId,
    crate::ContentPieceOrdinal,
    ContentPieceKey
);
owner_page_family!(
    SourceEventsFamily,
    SyndicTurnId,
    crate::SourceEventSequence,
    TurnEventKey
);
owner_page_family!(
    InputMarkerResolutionsFamily,
    crate::InputMarkerOwner,
    crate::InputMarkerOrdinal,
    InputMarkerKey
);
owner_page_family!(
    AcceptedOrderFamily,
    SyndicThreadId,
    crate::AcceptedInputOrdinal,
    ThreadAcceptedKey
);
owner_page_family!(
    AcceptedNextFamily,
    SyndicThreadId,
    crate::AcceptedInputOrdinal,
    ThreadAcceptedKey
);
owner_page_family!(
    TurnItemsFamily,
    SyndicTurnId,
    crate::TurnItemOrdinal,
    TurnItemKey
);
owner_page_family!(
    ItemSourceEventsFamily,
    SyndicItemId,
    crate::ItemSourceEventOrdinal,
    ItemEventKey
);
owner_page_family!(
    ProjectionResourcesFamily,
    SyndicProjectionId,
    crate::ResourceOrdinal,
    ProjectionResourceKey
);
