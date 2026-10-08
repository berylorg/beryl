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
