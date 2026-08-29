use super::*;

impl SyndicStorage {
    pub(super) fn owner_page<F: OwnerPageFamily>(
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

    pub(crate) fn page<F: Family>(
        &self,
        store: &HomeStore,
        range: CursorRange<F::Key>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<F::Value>, SyndicReadError> {
        let page = store.read_cursor::<crate::domain::SyndicDomain, ExactCodec<F>>(
            &self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        Ok(SyndicPage {
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
            stored_bytes,
            decoded_bytes,
            has_more,
        })
    }
}

pub(super) trait OwnerPageFamily: Family {
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
    AcceptedOrderFamily,
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
