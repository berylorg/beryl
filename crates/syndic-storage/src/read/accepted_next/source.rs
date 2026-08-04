use super::*;

impl SyndicStorage {
    /// Reads one global `(thread, generation)`-ordered next-turn source page.
    ///
    /// `expected_revision` fences the complete multi-page scan. Limits are clamped to the public
    /// accepted-next maxima, and the domain revision is checked before and after every page.
    pub fn accepted_next_source_page(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        cursor: Option<AcceptedNextSourceCursor>,
        limits: CursorReadLimits,
    ) -> Result<AcceptedNextSourcePage, SyndicReadError> {
        if self.revision(store)? != expected_revision {
            return Err(SyndicReadError::StaleAcceptedNextSourceScan);
        }
        let first = ThreadRouteKey {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: AcceptedRouteGeneration::FIRST,
        };
        let last = ThreadRouteKey {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: AcceptedRouteGeneration::new(u64::MAX)
                .expect("maximum route generation is nonzero"),
        };
        let range = match cursor {
            Some(cursor) if cursor.source_revision == expected_revision => CursorRange::after(
                ThreadRouteKey {
                    thread: cursor.after_thread_id,
                    generation: cursor.after_generation,
                },
                last,
            ),
            Some(_) => return Err(SyndicReadError::InvalidAcceptedNextSourceCursor),
            None => CursorRange::closed(first, last),
        };
        let page = store.read_cursor::<crate::domain::SyndicDomain, AcceptedNextSourcesCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            accepted_next_limits(limits),
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        let mut records = Vec::with_capacity(page.records().len());
        for record in page.into_records() {
            let (key, source) = record.into_parts();
            if key.thread != source.thread_id() || key.generation != source.generation() {
                return Err(SyndicReadError::Invariant(
                    "accepted-next source key and identity disagree",
                ));
            }
            records.push(AcceptedNextSource {
                source_revision: expected_revision,
                record: source,
            });
        }
        if self.revision(store)? != expected_revision {
            return Err(SyndicReadError::StaleAcceptedNextSourceScan);
        }
        let next_cursor = if has_more {
            let source = records.last().ok_or(SyndicReadError::Invariant(
                "accepted-next source page reported more without a record",
            ))?;
            Some(AcceptedNextSourceCursor {
                source_revision: expected_revision,
                after_thread_id: source.thread_id(),
                after_generation: source.generation(),
            })
        } else {
            None
        };
        Ok(AcceptedNextSourcePage {
            source_revision: expected_revision,
            records,
            stored_bytes,
            decoded_bytes,
            next_cursor,
        })
    }
}

fn accepted_next_limits(limits: CursorReadLimits) -> CursorReadLimits {
    CursorReadLimits::new(
        limits.max_items().min(ACCEPTED_NEXT_PAGE_MAX_RECORDS),
        limits.max_bytes().min(ACCEPTED_NEXT_PAGE_MAX_BYTES),
    )
    .expect("clamped nonzero accepted-next limits remain nonzero")
}
