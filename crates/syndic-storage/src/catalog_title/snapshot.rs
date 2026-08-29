use super::*;

impl TitleSnapshot for StoreTitleSnapshot<'_, '_> {
    fn turn(&self, id: SyndicTurnId) -> Result<Option<TurnRecord>, HistoryTitleReadError> {
        self.point::<TurnsFamily>(&id)
    }

    fn first_item(
        &self,
        turn: SyndicTurnId,
    ) -> Result<Option<TurnItemIndexRecord>, HistoryTitleReadError> {
        self.point::<TurnItemsFamily>(&TurnItemKey {
            owner: turn,
            ordinal: TurnItemOrdinal::FIRST,
        })
    }

    fn item(&self, id: SyndicItemId) -> Result<Option<CanonicalItemRecord>, HistoryTitleReadError> {
        self.point::<CanonicalItemsFamily>(&id)
    }

    fn manifest(
        &self,
        content: SyndicContentId,
    ) -> Result<Option<ContentManifestRecord>, HistoryTitleReadError> {
        self.point::<crate::codec::ContentManifestsFamily>(&content)
    }

    fn chunk(
        &self,
        key: ContentChunkKey,
    ) -> Result<Option<ContentChunkRecord>, HistoryTitleReadError> {
        self.point::<ContentChunksFamily>(&key)
    }

    fn text_spans(
        &self,
        content: SyndicContentId,
        after: Option<u64>,
        through: u64,
    ) -> Result<TextSpanPage, HistoryTitleReadError> {
        let page = self
            .store
            .read_cursor::<SyndicDomain, ExactCodec<ContentTextSpansFamily>>(
                &self.storage.handle,
                &text_span_range(content, after, through),
                CursorDirection::Forward,
                text_span_limits(),
            )?;
        Ok(TextSpanPage {
            has_more: page.has_more(),
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
        })
    }
}

impl StoreTitleSnapshot<'_, '_> {
    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, HistoryTitleReadError> {
        self.store
            .read_point::<SyndicDomain, ExactCodec<F>>(
                &self.storage.handle,
                key,
                family_point_limit::<F>(),
            )
            .map_err(Into::into)
    }
}

impl TitleSnapshot for DomainTitleSnapshot<'_, '_> {
    fn turn(&self, id: SyndicTurnId) -> Result<Option<TurnRecord>, HistoryTitleReadError> {
        self.point::<TurnsFamily>(&id)
    }

    fn first_item(
        &self,
        turn: SyndicTurnId,
    ) -> Result<Option<TurnItemIndexRecord>, HistoryTitleReadError> {
        self.point::<TurnItemsFamily>(&TurnItemKey {
            owner: turn,
            ordinal: TurnItemOrdinal::FIRST,
        })
    }

    fn item(&self, id: SyndicItemId) -> Result<Option<CanonicalItemRecord>, HistoryTitleReadError> {
        self.point::<CanonicalItemsFamily>(&id)
    }

    fn manifest(
        &self,
        content: SyndicContentId,
    ) -> Result<Option<ContentManifestRecord>, HistoryTitleReadError> {
        self.point::<crate::codec::ContentManifestsFamily>(&content)
    }

    fn chunk(
        &self,
        key: ContentChunkKey,
    ) -> Result<Option<ContentChunkRecord>, HistoryTitleReadError> {
        self.point::<ContentChunksFamily>(&key)
    }

    fn text_spans(
        &self,
        content: SyndicContentId,
        after: Option<u64>,
        through: u64,
    ) -> Result<TextSpanPage, HistoryTitleReadError> {
        let page = self.reader.cursor::<ExactCodec<ContentTextSpansFamily>>(
            &text_span_range(content, after, through),
            CursorDirection::Forward,
            text_span_limits(),
        )?;
        Ok(TextSpanPage {
            has_more: page.has_more(),
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
        })
    }
}

impl DomainTitleSnapshot<'_, '_> {
    fn point<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, HistoryTitleReadError> {
        self.reader
            .point::<ExactCodec<F>>(key, family_point_limit::<F>())
            .map_err(Into::into)
    }
}

fn text_span_range(
    content: SyndicContentId,
    after: Option<u64>,
    through: u64,
) -> CursorRange<ContentTextSpanKey> {
    let last = ContentTextSpanKey {
        owner: content,
        logical_start: through,
    };
    match after {
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
    }
}

fn text_span_limits() -> CursorReadLimits {
    CursorReadLimits::new(TEXT_SPAN_PAGE_MAX_ITEMS, TEXT_SPAN_PAGE_MAX_BYTES)
        .expect("history-title text-span page limits are nonzero")
}
