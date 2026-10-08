use super::*;
use crate::StatePage;
use beryl_home_store::FrozenHomeRead;

impl CatalogState {
    pub fn frozen_revision(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
    ) -> Result<DomainRevision, ReadError> {
        store.frozen_domain_revision(frozen, &self.handle)
    }

    pub fn frozen_row(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        thread_id: SyndicThreadId,
        limit: CatalogPointReadLimit,
    ) -> Result<Option<CatalogRow>, CatalogReadError> {
        let row = store.read_frozen_point::<CatalogDomain, CatalogRowCodec>(
            frozen,
            &self.handle,
            &thread_id,
            PointReadLimit::new(limit.max_bytes()).expect("catalog point limit is nonzero"),
        )?;
        let Some(row) = row else { return Ok(None) };
        if row.thread_id() != thread_id {
            return Err(CatalogReadError::Invariant(
                "catalog primary key and row identity disagree",
            ));
        }
        let reverse = store.read_frozen_point::<CatalogDomain, CatalogRecencyCodec>(
            frozen,
            &self.handle,
            &row.recency_cursor(),
            PointReadLimit::new(limit.max_bytes()).expect("catalog point limit is nonzero"),
        )?;
        if reverse.as_ref() != Some(&row) {
            return Err(CatalogReadError::Invariant(
                "catalog frozen primary and recency copies disagree",
            ));
        }
        Ok(Some(row))
    }

    pub fn frozen_primary_page(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        after: Option<SyndicThreadId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<CatalogRow>, CatalogReadError> {
        let end = SyndicThreadId::from_bytes([u8::MAX; 16]);
        let range = match after {
            Some(after) => CursorRange::after(after, end),
            None => CursorRange::closed(SyndicThreadId::from_bytes([0; 16]), end),
        };
        let page = store.read_frozen_cursor::<CatalogDomain, CatalogRowCodec>(
            frozen,
            &self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        for record in page.records() {
            if *record.key() != record.value().thread_id() {
                return Err(CatalogReadError::Invariant(
                    "catalog primary key and row identity disagree",
                ));
            }
            if self
                .frozen_row(
                    store,
                    frozen,
                    *record.key(),
                    CatalogPointReadLimit::schema_maximum(),
                )?
                .as_ref()
                != Some(record.value())
            {
                return Err(CatalogReadError::Invariant(
                    "catalog frozen primary row changed within one read",
                ));
            }
        }
        Ok(StatePage {
            stored_bytes: page.stored_bytes(),
            decoded_bytes: page.decoded_bytes(),
            has_more: page.has_more(),
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
        })
    }

    pub fn frozen_recency_page(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        after: Option<CatalogRecencyCursor>,
        limits: CursorReadLimits,
    ) -> Result<CatalogPage, CatalogReadError> {
        let range = match after {
            Some(after) => CursorRange::after(after, CatalogRecencyCursor::last()),
            None => {
                CursorRange::closed(CatalogRecencyCursor::first(), CatalogRecencyCursor::last())
            }
        };
        let page = store.read_frozen_cursor::<CatalogDomain, CatalogRecencyCodec>(
            frozen,
            &self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        for record in page.records() {
            if *record.key() != record.value().recency_cursor() {
                return Err(CatalogReadError::Invariant(
                    "catalog recency key and row identity disagree",
                ));
            }
            if self
                .frozen_row(
                    store,
                    frozen,
                    record.value().thread_id(),
                    CatalogPointReadLimit::schema_maximum(),
                )?
                .as_ref()
                != Some(record.value())
            {
                return Err(CatalogReadError::Invariant(
                    "catalog frozen recency and primary copies disagree",
                ));
            }
        }
        Ok(CatalogPage {
            stored_bytes: page.stored_bytes(),
            decoded_bytes: page.decoded_bytes(),
            has_more: page.has_more(),
            rows: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
        })
    }
}
