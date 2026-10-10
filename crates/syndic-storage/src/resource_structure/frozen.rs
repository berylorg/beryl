use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainHandle, FrozenHomeRead, HomeStore,
    PointReadLimit, RECORD_VERSION_BYTES, ReadError,
};

use super::{
    AnchorKey, AnchorLocator,
    codec::SourceAnchorsFamily,
    construction::{AuthenticationRecords, ConstructionError},
    query::QueryRecords,
    query_anchor::AnchorRecords,
    source::SourceRecords,
};
use crate::{
    ContentChunkRecord, ProviderNarrativeReference, ProviderNarrativeSpanRecord,
    codec::{
        ContentChunkKey, ContentChunksFamily, ExactCodec, Family, ProviderNarrativeSpanKey,
        ProviderNarrativeSpansFamily,
    },
    domain::SyndicDomain,
};

pub(super) struct FrozenRecords<'a> {
    pub(super) store: &'a HomeStore,
    pub(super) handle: &'a DomainHandle<SyndicDomain>,
    pub(super) read: &'a FrozenHomeRead,
}

fn bounded<T>(result: Result<T, ReadError>) -> Result<T, ConstructionError> {
    match result {
        Err(ReadError::BoundExceeded { .. }) => Err(ConstructionError::Limit),
        result => Ok(result?),
    }
}

impl FrozenRecords<'_> {
    fn seek<F: Family>(
        &self,
        first: F::Key,
        last: F::Key,
        direction: CursorDirection,
    ) -> Result<Option<(F::Key, F::Value)>, ConstructionError> {
        let page = bounded(
            self.store
                .read_frozen_cursor::<SyndicDomain, ExactCodec<F>>(
                    self.read,
                    self.handle,
                    &CursorRange::closed(first, last),
                    direction,
                    CursorReadLimits::new(1, 512).map_err(|_| ConstructionError::Limit)?,
                ),
        )?;
        Ok(page
            .into_records()
            .into_iter()
            .next()
            .map(|record| record.into_parts()))
    }
}

impl QueryRecords for FrozenRecords<'_> {
    fn point<F: Family>(
        &self,
        key: &F::Key,
        payload_limit: usize,
    ) -> Result<Option<F::Value>, ConstructionError> {
        let limit = payload_limit
            .checked_add(RECORD_VERSION_BYTES)
            .and_then(|bytes| PointReadLimit::new(bytes).ok())
            .ok_or(ConstructionError::Limit)?;
        bounded(self.store.read_frozen_point::<SyndicDomain, ExactCodec<F>>(
            self.read,
            self.handle,
            key,
            limit,
        ))
    }
}

impl AuthenticationRecords for FrozenRecords<'_> {
    fn narrative_frontier(
        &self,
        narrative: ProviderNarrativeReference,
    ) -> Result<Option<(ProviderNarrativeSpanKey, ProviderNarrativeSpanRecord)>, ConstructionError>
    {
        self.seek::<ProviderNarrativeSpansFamily>(
            ProviderNarrativeSpanKey::first_for_generation(
                narrative.content_id(),
                narrative.generation(),
            ),
            ProviderNarrativeSpanKey::new(
                narrative.content_id(),
                narrative.generation(),
                narrative.logical_utf8_bytes() - 1,
            ),
            CursorDirection::Reverse,
        )
    }
}

impl SourceRecords for FrozenRecords<'_> {
    fn predecessor<F: Family>(
        &self,
        first: F::Key,
        last: F::Key,
    ) -> Result<(F::Key, F::Value), ConstructionError> {
        self.seek::<F>(first, last, CursorDirection::Reverse)?
            .ok_or(ConstructionError::Corrupt)
    }

    fn chunk(&self, key: &ContentChunkKey) -> Result<ContentChunkRecord, ConstructionError> {
        self.point::<ContentChunksFamily>(key, ContentChunksFamily::MAX_VALUE_BYTES)?
            .ok_or(ConstructionError::Corrupt)
    }
}

impl AnchorRecords for FrozenRecords<'_> {
    fn seek_anchor(
        &self,
        first: AnchorKey,
        last: AnchorKey,
        reverse: bool,
    ) -> Result<Option<(AnchorKey, AnchorLocator)>, ConstructionError> {
        self.seek::<SourceAnchorsFamily>(
            first,
            last,
            if reverse {
                CursorDirection::Reverse
            } else {
                CursorDirection::Forward
            },
        )
    }
}
