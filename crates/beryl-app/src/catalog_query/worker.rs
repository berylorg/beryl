use std::panic::{AssertUnwindSafe, catch_unwind};

use super::*;

pub(super) struct WorkerCustody {
    owner: Option<CatalogQueryOwner>,
    unadmitted: Option<FrozenHomeRead>,
    pub(super) panicked: bool,
    failure: Option<CatalogQueryServiceError>,
}

impl WorkerCustody {
    pub(super) fn new() -> Self {
        Self {
            owner: None,
            unadmitted: None,
            panicked: false,
            failure: None,
        }
    }
    pub(super) fn reads_drained(&self) -> bool {
        self.unadmitted.is_none()
            && self
                .owner
                .as_ref()
                .is_none_or(|owner| owner.pending_collections() == 0)
    }
    fn release_unadmitted(
        &mut self,
        home: &HomeServiceReference,
    ) -> Result<(), CatalogQueryServiceError> {
        if let Some(read) = &self.unadmitted {
            match home.release_frozen_read(read) {
                Ok(())
                | Err(ReadError::FrozenRead(beryl_home_store::FrozenReadAccessError::Released)) => {
                    self.unadmitted = None;
                }
                Err(error) => return Err(CatalogQueryServiceError::SourceRetirement(error)),
            }
        }
        Ok(())
    }
    pub(super) fn retire(
        &mut self,
        home: &HomeServiceReference,
    ) -> Result<(), CatalogQueryServiceError> {
        if let Some(owner) = &mut self.owner {
            owner
                .retire(home)
                .map_err(|error| CatalogQueryServiceError::Retirement(Box::new(error)))?;
        }
        self.release_unadmitted(home)?;
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        Ok(())
    }
}

pub(super) fn run(
    home: Arc<HomeServiceReference>,
    state: BerylState,
    source: CatalogSourceReader,
    signal: Arc<QuerySignal>,
    retained: Arc<Mutex<Box<WorkerCustody>>>,
) {
    let mut custody = retained.lock().unwrap_or_else(|e| e.into_inner());
    if catch_unwind(AssertUnwindSafe(|| {
        drive(&home, &state, &source, &signal, &mut custody)
    }))
    .is_err()
    {
        custody.panicked = true;
    }
    admission::stop(&signal);
    let pending = {
        let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
        state.controls.clear();
        state.collections.clear();
        state.releases.clear();
        std::mem::take(&mut state.queue)
    };
    for request in pending {
        let _ = request.reply.send(Err(if custody.panicked {
            CatalogQueryRequestError::Panicked
        } else {
            CatalogQueryRequestError::Retired
        }));
    }
    // The exact owner stays outside the unwind boundary until explicit retirement.
    match catch_unwind(AssertUnwindSafe(|| custody.retire(&home))) {
        Ok(Err(error)) => custody.failure = Some(error),
        Ok(Ok(())) => {}
        Err(_) => custody.panicked = true,
    }
}

fn drive(
    home: &Arc<HomeServiceReference>,
    state: &BerylState,
    source: &CatalogSourceReader,
    signal: &Arc<QuerySignal>,
    custody: &mut WorkerCustody,
) {
    loop {
        let (release, request) = {
            let mut current = signal.state.lock().unwrap_or_else(|e| e.into_inner());
            while !current.stopped
                && (!current.published || (current.queue.is_empty() && current.releases.is_empty()))
            {
                current = signal
                    .changed
                    .wait(current)
                    .unwrap_or_else(|e| e.into_inner());
            }
            if current.stopped {
                return;
            }
            if let Some(release) = current.releases.pop() {
                (Some(release), None)
            } else {
                (None, current.queue.pop_front())
            }
        };
        if let Some(token) = release {
            if let Some(owner) = &mut custody.owner {
                if let Err(error) = owner.release(home, &token) {
                    custody.failure = Some(CatalogQueryServiceError::Retirement(Box::new(error)));
                    return;
                }
            }
            signal
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .collections
                .retain(|current| current != &token);
            continue;
        }
        let Some(request) = request else { continue };
        #[cfg(all(test, feature = "test-faults"))]
        {
            let pause = {
                let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
                state.active_pause = state.pause.take();
                state.active_pause.clone()
            };
            if let Some(pause) = pause {
                pause.wait();
            }
            signal
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .active_pause = None;
        }
        let mut result = if request.cancellation.is_cancelled() || request.reply.is_canceled() {
            Err(CatalogQueryRequestError::Cancelled)
        } else {
            perform(home, state, source, signal, custody, &request)
        };
        if request.cancellation.is_cancelled() {
            result = Err(CatalogQueryRequestError::Cancelled);
        }
        let stopped = {
            let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
            state
                .controls
                .retain(|control| control.identity != request.identity);
            state.stopped
        };
        if stopped {
            result = Err(CatalogQueryRequestError::Retired);
        }
        let _ = request.reply.send(result);
        if custody.unadmitted.is_some() {
            return;
        }
    }
}

fn perform(
    home: &Arc<HomeServiceReference>,
    state: &BerylState,
    source: &CatalogSourceReader,
    signal: &Arc<QuerySignal>,
    custody: &mut WorkerCustody,
    request: &QueryRequest,
) -> Result<PublishedCatalogQueryResponse, CatalogQueryRequestError> {
    let result = match &request.operation {
        QueryOperation::Open { criteria, limit } => {
            let read = source
                .retain_source(home, &request.cancellation)
                .map_err(|error| CatalogQueryRequestError::Source(Box::new(error)))?;
            custody.unadmitted = Some(read);
            if custody.owner.is_none() {
                match CatalogQueryOwner::new(
                    state.catalog(),
                    state.runtime_roots(),
                    custody.unadmitted.as_ref().unwrap().generation_identity(),
                ) {
                    Ok(owner) => custody.owner = Some(owner),
                    Err(error) => {
                        if let Err(release) = custody.release_unadmitted(home) {
                            custody.failure = Some(release);
                        }
                        return Err(CatalogQueryRequestError::Query(Box::new(error)));
                    }
                }
            }
            let read = custody.unadmitted.as_ref().unwrap().clone();
            let opened = custody.owner.as_mut().unwrap().open(
                home,
                read,
                criteria.clone(),
                *limit,
                &request.cancellation,
            );
            let opened = match opened {
                Ok(opened) => {
                    custody.unadmitted = None;
                    opened
                }
                Err(error) => {
                    let (error, retained) = error.into_parts();
                    custody.unadmitted = retained;
                    if let Err(release) = custody.release_unadmitted(home) {
                        custody.failure = Some(release);
                    }
                    return Err(CatalogQueryRequestError::Query(Box::new(error)));
                }
            };
            let token = opened.token().clone();
            signal
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .collections
                .push(token.clone());
            PublishedCatalogQueryResult::Opened(PublishedCatalogOpened {
                collection: PublishedCatalogCollection {
                    signal: Arc::downgrade(signal),
                    generation: request.identity.generation,
                    token,
                },
                opened,
            })
        }
        QueryOperation::Page {
            token,
            cursor,
            limit,
        } => {
            let owner = custody
                .owner
                .as_mut()
                .ok_or(CatalogQueryRequestError::Foreign)?;
            PublishedCatalogQueryResult::Page(
                owner
                    .page(home, token, cursor.as_ref(), *limit, &request.cancellation)
                    .map_err(|error| CatalogQueryRequestError::Query(Box::new(error)))?,
            )
        }
        QueryOperation::Position { token, thread } => {
            let owner = custody
                .owner
                .as_mut()
                .ok_or(CatalogQueryRequestError::Foreign)?;
            PublishedCatalogQueryResult::Position(
                owner
                    .position(home, token, *thread, &request.cancellation)
                    .map_err(|error| CatalogQueryRequestError::Query(Box::new(error)))?,
            )
        }
    };
    Ok(PublishedCatalogQueryResponse {
        identity: request.identity.clone(),
        result,
    })
}
