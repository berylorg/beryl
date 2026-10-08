use super::*;
use beryl_home_store::FrozenHomeRead;

impl RuntimeRootState {
    pub fn frozen_revision(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
    ) -> Result<beryl_model::DomainRevision, ReadError> {
        store.frozen_domain_revision(frozen, &self.handle)
    }

    pub fn frozen_runtime(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        runtime_id: RuntimeId,
    ) -> Result<Option<RuntimeRecord>, RuntimeRootCatalogSourceError> {
        let record = store.read_frozen_point::<RuntimeRootDomain, RuntimeRecordCodec>(
            frozen,
            &self.handle,
            &runtime_id,
            point_limit(RUNTIME_RECORD_LIMIT),
        )?;
        if record
            .as_ref()
            .is_some_and(|record| record.runtime_id() != runtime_id)
        {
            return Err(RuntimeRootCatalogSourceError::SourceChanged(
                "runtime key/value identity",
            ));
        }
        Ok(record)
    }

    pub fn frozen_root(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        root_id: RootId,
    ) -> Result<Option<RootRecord>, RuntimeRootCatalogSourceError> {
        let Some(runtime_id) = store.read_frozen_point::<RuntimeRootDomain, RootIdIndexCodec>(
            frozen,
            &self.handle,
            &root_id,
            point_limit(32),
        )?
        else {
            return Ok(None);
        };
        let record = store.read_frozen_point::<RuntimeRootDomain, RootRecordCodec>(
            frozen,
            &self.handle,
            &RuntimeRootKey::new(runtime_id, root_id),
            point_limit(ROOT_RECORD_LIMIT),
        )?;
        if !record
            .as_ref()
            .is_some_and(|record| record.root_id() == root_id && record.runtime_id() == runtime_id)
        {
            return Err(RuntimeRootCatalogSourceError::SourceChanged(
                "root index/key/value identity",
            ));
        }
        Ok(record)
    }

    pub fn frozen_catalog_source(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        runtime_id: RuntimeId,
        root_id: RootId,
    ) -> Result<RuntimeRootCatalogSource, RuntimeRootCatalogSourceError> {
        let runtime = self
            .frozen_runtime(store, frozen, runtime_id)?
            .ok_or(RuntimeRootCatalogSourceError::RuntimeMissing { runtime_id })?;
        let root = self
            .frozen_root(store, frozen, root_id)?
            .ok_or(RuntimeRootCatalogSourceError::RootMissing { root_id })?;
        RuntimeRootCatalogSource::new(runtime, root)
    }

    pub fn frozen_runtimes_page(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        after: Option<RuntimeId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<RuntimeRecord>, RuntimeRootCatalogSourceError> {
        let end = RuntimeId::from_bytes([u8::MAX; 16]);
        let range = match after {
            Some(after) => CursorRange::after(after, end),
            None => CursorRange::closed(RuntimeId::from_bytes([0; 16]), end),
        };
        let page = store.read_frozen_cursor::<RuntimeRootDomain, RuntimeRecordCodec>(
            frozen,
            &self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        for record in page.records() {
            if *record.key() != record.value().runtime_id() {
                return Err(RuntimeRootCatalogSourceError::SourceChanged(
                    "runtime key/value identity",
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

    pub fn frozen_roots_page(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        runtime_id: RuntimeId,
        after: Option<RootId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<RootRecord>, RuntimeRootCatalogSourceError> {
        let start = RuntimeRootKey::new(
            runtime_id,
            after.unwrap_or_else(|| RootId::from_bytes([0; 16])),
        );
        let end = RuntimeRootKey::new(runtime_id, RootId::from_bytes([u8::MAX; 16]));
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = store.read_frozen_cursor::<RuntimeRootDomain, RootRecordCodec>(
            frozen,
            &self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        for record in page.records() {
            let row = record.value();
            if record.key().runtime_id() != row.runtime_id()
                || record.key().root_id() != row.root_id()
                || self.frozen_root(store, frozen, row.root_id())?.as_ref() != Some(row)
            {
                return Err(RuntimeRootCatalogSourceError::SourceChanged(
                    "root index/key/value identity",
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
}
