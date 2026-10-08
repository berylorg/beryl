use super::*;

impl CatalogQueryOwner {
    pub fn new(
        catalog: CatalogState,
        runtime_roots: RuntimeRootState,
        generation: HomeGenerationIdentity,
    ) -> Result<Self, CatalogQueryError> {
        let id = NEXT_OWNER
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| CatalogQueryError::IdentityExhausted)?;
        Ok(Self {
            id,
            generation,
            catalog,
            runtime_roots,
            next_query: 1,
            requests: 0,
            entries: Vec::new(),
            retired: false,
        })
    }

    pub fn open(
        &mut self,
        store: &HomeStore,
        retained: FrozenHomeRead,
        criteria: CatalogQueryCriteria,
        limit: CatalogQueryPageLimit,
        cancel: &CommandCancellation,
    ) -> Result<CatalogQueryOpened, CatalogQueryOpenError> {
        let admission = (|| {
            if self.retired {
                return Err(CatalogQueryError::Retired);
            }
            if retained.generation_identity() != self.generation {
                return Err(CatalogQueryError::GenerationChanged);
            }
            if self.entries.len() >= CATALOG_QUERY_COLLECTION_LIMIT {
                return Err(CatalogQueryError::CollectionLimit);
            }
            let next = self
                .next_query
                .checked_add(1)
                .ok_or(CatalogQueryError::IdentityExhausted)?;
            self.request(store)?;
            Ok(next)
        })();
        let next = match admission {
            Ok(next) => next,
            Err(error) => return Err(CatalogQueryOpenError::new(error, Some(retained))),
        };
        let token = CatalogQueryToken {
            owner: self.id,
            generation: self.generation,
            id: self.next_query,
        };
        self.next_query = next;
        self.entries.push(Entry {
            token: token.clone(),
            frozen: retained,
            criteria,
            revision: None,
            count: 0,
            retiring: false,
        });
        let index = self.entries.len() - 1;
        let result = self.prepare(store, index, limit, cancel);
        match result {
            Ok(opened) => Ok(opened),
            Err(error) => {
                self.entries[index].retiring = true;
                let _ = self.release_index(store, index);
                Err(CatalogQueryOpenError::new(error, None))
            }
        }
    }

    pub fn page(
        &mut self,
        store: &HomeStore,
        token: &CatalogQueryToken,
        cursor: Option<&CatalogQueryCursor>,
        limit: CatalogQueryPageLimit,
        cancel: &CommandCancellation,
    ) -> Result<CatalogQueryPage, CatalogQueryError> {
        self.request(store)?;
        let index = self.entry_index(token)?;
        if let Some(cursor) = cursor {
            if cursor.token != *token {
                return Err(CatalogQueryError::Foreign);
            }
            if cursor.offset > self.entries[index].count {
                return Err(CatalogQueryError::Structural(
                    "query cursor exceeds exact count",
                ));
            }
        }
        self.collect_page(store, &self.entries[index], cursor, limit, cancel)
    }

    pub fn position(
        &mut self,
        store: &HomeStore,
        token: &CatalogQueryToken,
        thread_id: SyndicThreadId,
        cancel: &CommandCancellation,
    ) -> Result<Option<CatalogQueryPosition>, CatalogQueryError> {
        self.request(store)?;
        let entry = &self.entries[self.entry_index(token)?];
        let mut index = 0u64;
        let mut previous = None;
        let mut result = None;
        self.walk_recency(store, entry, None, cancel, |row| {
            if !entry.criteria.matches(&row) {
                return Ok(true);
            }
            if row.thread_id() == thread_id {
                result = Some(CatalogQueryPosition {
                    index,
                    before: previous.clone(),
                });
                return Ok(false);
            }
            index = index
                .checked_add(1)
                .ok_or(CatalogQueryError::CountExhausted)?;
            previous = Some(CatalogQueryCursor {
                token: token.clone(),
                after: row.recency_cursor(),
                offset: index,
            });
            Ok(true)
        })?;
        Ok(result)
    }

    pub fn release(
        &mut self,
        store: &HomeStore,
        token: &CatalogQueryToken,
    ) -> Result<(), CatalogQueryError> {
        let index = self.owned_index(token)?;
        self.entries[index].retiring = true;
        self.release_index(store, index)
    }

    pub fn retire(&mut self, store: &HomeStore) -> Result<(), CatalogQueryError> {
        self.retired = true;
        for entry in &mut self.entries {
            entry.retiring = true;
        }
        while !self.entries.is_empty() {
            self.release_index(store, 0)?;
        }
        Ok(())
    }

    pub fn pending_collections(&self) -> usize {
        self.entries.len()
    }

    fn request(&mut self, store: &HomeStore) -> Result<(), CatalogQueryError> {
        if self.retired {
            return Err(CatalogQueryError::Retired);
        }
        if store.generation_identity()? != self.generation {
            return Err(CatalogQueryError::GenerationChanged);
        }
        self.requests = self
            .requests
            .checked_add(1)
            .ok_or(CatalogQueryError::RequestExhausted)?;
        Ok(())
    }

    fn owned_index(&self, token: &CatalogQueryToken) -> Result<usize, CatalogQueryError> {
        if token.owner != self.id {
            return Err(CatalogQueryError::Foreign);
        }
        if token.generation != self.generation {
            return Err(CatalogQueryError::GenerationChanged);
        }
        self.entries
            .iter()
            .position(|entry| entry.token == *token)
            .ok_or(CatalogQueryError::Released)
    }

    fn entry_index(&self, token: &CatalogQueryToken) -> Result<usize, CatalogQueryError> {
        let index = self.owned_index(token)?;
        if self.entries[index].retiring {
            return Err(CatalogQueryError::Released);
        }
        Ok(index)
    }

    fn release_index(&mut self, store: &HomeStore, index: usize) -> Result<(), CatalogQueryError> {
        match store.release_frozen_read(&self.entries[index].frozen) {
            Ok(()) | Err(ReadError::FrozenRead(FrozenReadAccessError::Released)) => {
                self.entries.remove(index);
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn exhaust_query_identities_for_test(&mut self) {
        self.next_query = u64::MAX;
    }

    #[cfg(feature = "test-faults")]
    pub fn exhaust_requests_for_test(&mut self) {
        self.requests = u64::MAX;
    }
}
