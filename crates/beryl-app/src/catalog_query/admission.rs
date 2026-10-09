use super::*;

pub(super) fn current(identity: &CatalogQueryRequestIdentity) -> bool {
    let Some(signal) = identity.service.upgrade() else {
        return false;
    };
    let state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
    state.published
        && !state.stopped
        && state.generation == identity.generation
        && identity.query.as_ref().is_none_or(|query| {
            state.collections.contains(query) && !state.releases.contains(query)
        })
}

fn admit(
    signal: &Weak<QuerySignal>,
    query: Option<CatalogQueryToken>,
    operation: QueryOperation,
    cancellation: CommandCancellation,
) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
    let signal = signal.upgrade().ok_or(CatalogQueryRequestError::Retired)?;
    let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
    if state.stopped {
        return Err(CatalogQueryRequestError::Retired);
    }
    if !state.published {
        return Err(CatalogQueryRequestError::NotPublished);
    }
    if cancellation.is_cancelled() {
        return Err(CatalogQueryRequestError::Cancelled);
    }
    if query
        .as_ref()
        .is_some_and(|query| !state.collections.contains(query) || state.releases.contains(query))
    {
        return Err(CatalogQueryRequestError::Foreign);
    }
    if state.controls.len() >= CATALOG_QUERY_PENDING_REQUEST_LIMIT {
        return Err(CatalogQueryRequestError::RequestLimit);
    }
    let next = state
        .next_request
        .checked_add(1)
        .ok_or(CatalogQueryRequestError::IdentityExhausted)?;
    let identity = CatalogQueryRequestIdentity {
        service: Arc::downgrade(&signal),
        id: state.next_request,
        generation: state.generation,
        query,
    };
    state.next_request = next;
    let (reply, receiver) = oneshot::channel();
    state.controls.push(RequestControl {
        identity: identity.clone(),
        cancellation: cancellation.clone(),
    });
    state.queue.push_back(QueryRequest {
        identity: identity.clone(),
        operation,
        cancellation: cancellation.clone(),
        reply,
    });
    signal.changed.notify_one();
    Ok(PublishedCatalogQueryRequest {
        identity,
        cancellation,
        receiver: Some(receiver),
    })
}

impl PublishedCatalogQueryReader {
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_pause_next_request(
        &self,
    ) -> Result<CatalogQueryTestPause, CatalogQueryRequestError> {
        let signal = self
            .signal
            .upgrade()
            .ok_or(CatalogQueryRequestError::Retired)?;
        let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            return Err(CatalogQueryRequestError::Retired);
        }
        if !state.published {
            return Err(CatalogQueryRequestError::NotPublished);
        }
        if state.pause.is_some() || state.active_pause.is_some() {
            return Err(CatalogQueryRequestError::RequestLimit);
        }
        let pause = tests::RequestPause::new();
        state.pause = Some(Arc::clone(&pause));
        Ok(CatalogQueryTestPause {
            signal: Arc::downgrade(&signal),
            pause,
        })
    }
    pub fn is_ready(&self) -> bool {
        let Some(signal) = self.signal.upgrade() else {
            return false;
        };
        let current = {
            let state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
            state.published && !state.stopped
        };
        current && signal.source.certified_threads().is_ok()
    }
    pub fn open(
        &self,
        criteria: CatalogQueryCriteria,
        limit: CatalogQueryPageLimit,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            None,
            QueryOperation::Open { criteria, limit },
            cancellation,
        )
    }
}

impl PublishedCatalogCollection {
    pub fn refine(
        &self,
        criteria: CatalogQueryCriteria,
        limit: CatalogQueryPageLimit,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::Refine {
                token: self.token.clone(),
                criteria,
                limit,
            },
            cancellation,
        )
    }
    pub fn page_at(
        &self,
        start: u64,
        limit: CatalogQueryPageLimit,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::PageAt {
                token: self.token.clone(),
                start,
                limit,
            },
            cancellation,
        )
    }
    pub fn runtime_page(
        &self,
        search: CatalogNormalizedQuery,
        start: u64,
        limit: CatalogQueryPageLimit,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::Runtimes {
                token: self.token.clone(),
                search,
                start,
                limit,
            },
            cancellation,
        )
    }
    pub fn root_page(
        &self,
        runtime: RuntimeId,
        search: CatalogNormalizedQuery,
        start: u64,
        limit: CatalogQueryPageLimit,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::Roots {
                token: self.token.clone(),
                runtime,
                search,
                start,
                limit,
            },
            cancellation,
        )
    }
    pub fn runtime_position(
        &self,
        search: CatalogNormalizedQuery,
        runtime: RuntimeId,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::RuntimePosition {
                token: self.token.clone(),
                search,
                runtime,
            },
            cancellation,
        )
    }
    pub fn root_position(
        &self,
        runtime: RuntimeId,
        search: CatalogNormalizedQuery,
        root: RootId,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::RootPosition {
                token: self.token.clone(),
                runtime,
                search,
                root,
            },
            cancellation,
        )
    }
    pub fn page(
        &self,
        cursor: Option<CatalogQueryCursor>,
        limit: CatalogQueryPageLimit,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::Page {
                token: self.token.clone(),
                cursor,
                limit,
            },
            cancellation,
        )
    }
    pub fn position(
        &self,
        thread: SyndicThreadId,
        cancellation: CommandCancellation,
    ) -> Result<PublishedCatalogQueryRequest, CatalogQueryRequestError> {
        admit(
            &self.signal,
            Some(self.token.clone()),
            QueryOperation::Position {
                token: self.token.clone(),
                thread,
            },
            cancellation,
        )
    }
    pub fn is_current(&self) -> bool {
        current(&CatalogQueryRequestIdentity {
            service: self.signal.clone(),
            id: 0,
            generation: self.generation,
            query: Some(self.token.clone()),
        })
    }
    pub fn release(self) {
        drop(self);
    }
}

impl Drop for PublishedCatalogCollection {
    fn drop(&mut self) {
        let Some(signal) = self.signal.upgrade() else {
            return;
        };
        let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.stopped
            && state.collections.contains(&self.token)
            && !state.releases.contains(&self.token)
        {
            for control in &state.controls {
                if control.identity.query.as_ref() == Some(&self.token) {
                    control.cancellation.cancel();
                }
            }
            state.releases.push(self.token.clone());
            signal.changed.notify_one();
        }
    }
}

pub(super) fn stop(signal: &QuerySignal) {
    let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
    state.stopped = true;
    state.published = false;
    for control in &state.controls {
        control.cancellation.cancel();
    }
    #[cfg(all(test, feature = "test-faults"))]
    if let Some(pause) = state.active_pause.as_ref().or(state.pause.as_ref()) {
        pause.release();
    }
    signal.changed.notify_all();
}
