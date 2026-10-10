use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use crate::cas_projection::{ModelReadObservation, ModelReadSource, ModelSourceError};
use beryl_backend::{ModelListOptions, ModelPageCursor, ModelPageLimit};
use beryl_home_store::HomeMutationObservation;
use beryl_model::{ExecutionBinding, WindowId};
use beryl_state::{SessionState, WindowClaimSelection};

mod facts;
#[cfg(all(test, feature = "test-faults"))]
pub(crate) mod qualification_support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/model_reader/qualification_support.rs"
    ));
}
pub(crate) use facts::{
    ModelDefaults, ModelOptionRecord, ModelReasoningEffort, ModelSupportedEfforts,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ModelReadError {
    #[error("the selected model source is unavailable")]
    Unavailable,
    #[error("the selected model source could not complete the request")]
    Source(#[from] ModelSourceError),
    #[error("model reader limits are invalid")]
    Limits,
    #[error("model reader capacity is full")]
    Capacity,
    #[error("model query is closed")]
    Closed,
    #[error("model query is pending")]
    Pending,
    #[error("model query publication is busy")]
    Busy,
    #[error("model continuation belongs to another query")]
    Continuation,
    #[error("retry must repeat the original failed model page")]
    Retry,
}

impl ModelReadError {
    pub(crate) fn runtime_unavailable(&self) -> bool {
        matches!(
            self,
            Self::Unavailable
                | Self::Source(
                    ModelSourceError::Unavailable
                        | ModelSourceError::Runtime(
                            crate::cas_projection::RuntimeModelReadError::Unavailable
                        )
                )
        )
    }

    pub(crate) fn scope_retired(&self) -> bool {
        self.runtime_unavailable()
            || matches!(
                self,
                Self::Closed | Self::Source(ModelSourceError::Selection | ModelSourceError::Active)
            )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ModelReaderLimits {
    queries: NonZeroUsize,
    pages: NonZeroUsize,
    records: ModelPageLimit,
    timeout: Duration,
}

impl ModelReaderLimits {
    pub(crate) fn new(
        queries: NonZeroUsize,
        pages: NonZeroUsize,
        records: u32,
        timeout: Duration,
    ) -> Result<Self, ModelReadError> {
        if timeout.is_zero() {
            return Err(ModelReadError::Limits);
        }
        Ok(Self {
            queries,
            pages,
            records: ModelPageLimit::try_new(records).map_err(|_| ModelReadError::Limits)?,
            timeout,
        })
    }
}

#[derive(Clone)]
pub(crate) struct PublishedModelReader {
    source: ModelReadSource,
    session: SessionState,
    lifetime: Weak<()>,
    capacity: Arc<CapacityOwner>,
    limits: ModelReaderLimits,
}

struct CapacityOwner {
    queries: AtomicUsize,
    pages: AtomicUsize,
}
enum CapacityKind {
    Query,
    Page,
}
struct Capacity {
    owner: Arc<CapacityOwner>,
    kind: CapacityKind,
}

impl CapacityOwner {
    fn reserve(
        self: &Arc<Self>,
        kind: CapacityKind,
        maximum: usize,
    ) -> Result<Capacity, ModelReadError> {
        let count = match kind {
            CapacityKind::Query => &self.queries,
            CapacityKind::Page => &self.pages,
        };
        count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < maximum).then(|| count + 1)
            })
            .map_err(|_| ModelReadError::Capacity)?;
        Ok(Capacity {
            owner: Arc::clone(self),
            kind,
        })
    }
}
impl Drop for Capacity {
    fn drop(&mut self) {
        let count = match self.kind {
            CapacityKind::Query => &self.owner.queries,
            CapacityKind::Page => &self.owner.pages,
        };
        count.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) struct ModelQuery {
    observation: Arc<ModelReadObservation>,
    lifetime: Weak<()>,
    closed: Mutex<bool>,
    idle: bool,
    busy: AtomicBool,
    boundary: Mutex<HomeMutationObservation>,
    failed: Mutex<Option<Option<Arc<ModelPageCursor>>>>,
    capacity: Arc<CapacityOwner>,
    limits: ModelReaderLimits,
    _slot: Capacity,
}

pub(crate) struct ModelPreparationFence {
    observation: Arc<ModelReadObservation>,
    lifetime: Weak<()>,
    capacity: Weak<CapacityOwner>,
    boundary: Mutex<HomeMutationObservation>,
    execution: ExecutionBinding,
    draft_only: bool,
    window: WindowId,
    claim: WindowClaimSelection,
}

impl ModelPreparationFence {
    pub(crate) fn revalidate(&self) -> Result<(), ModelReadError> {
        let _publication = self.lifetime.upgrade().ok_or(ModelReadError::Closed)?;
        let boundary = self.observation.validate_idle()?;
        self.observation.with_current(&boundary, || ())?;
        *self.boundary.lock().map_err(|_| ModelReadError::Closed)? = boundary;
        Ok(())
    }

    pub(crate) fn with_current<T>(&self, publish: impl FnOnce() -> T) -> Result<T, ModelReadError> {
        let _publication = self.lifetime.upgrade().ok_or(ModelReadError::Closed)?;
        let boundary = self.boundary.try_lock().map_err(|_| ModelReadError::Busy)?;
        Ok(self.observation.with_current(&boundary, publish)?)
    }

    pub(crate) fn belongs_to_query(&self, query: &Arc<ModelQuery>) -> bool {
        Arc::ptr_eq(&self.observation, &query.observation)
    }

    pub(crate) fn matches_selection(&self, window: WindowId, claim: WindowClaimSelection) -> bool {
        self.window == window && self.claim == claim
    }
}

struct Request<'a>(&'a AtomicBool);
impl<'a> Request<'a> {
    fn begin(busy: &'a AtomicBool) -> Result<Self, ModelReadError> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ModelReadError::Pending)?;
        Ok(Self(busy))
    }
}
impl Drop for Request<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
pub(crate) struct ModelContinuation {
    query: Weak<ModelQuery>,
    cursor: Arc<ModelPageCursor>,
}

pub(crate) struct ModelPage {
    query: Weak<ModelQuery>,
    records: Box<[ModelOptionRecord]>,
    continuation: Option<ModelContinuation>,
    _slot: Capacity,
}

impl PublishedModelReader {
    pub(crate) fn new(
        source: ModelReadSource,
        session: SessionState,
        lifetime: Weak<()>,
        limits: ModelReaderLimits,
    ) -> Self {
        Self {
            source,
            session,
            lifetime,
            capacity: Arc::new(CapacityOwner {
                queries: AtomicUsize::new(0),
                pages: AtomicUsize::new(0),
            }),
            limits,
        }
    }
    pub(crate) fn identity(
        &self,
    ) -> (
        beryl_model::BerylHomeId,
        beryl_home_store::HomeGeneration,
        crate::cas_projection::ProjectionServiceGeneration,
    ) {
        self.source.identity()
    }
    pub(crate) fn prepare_selected(
        &self,
        window: WindowId,
        claim: WindowClaimSelection,
    ) -> Result<(Arc<ModelQuery>, ExecutionBinding, bool), ModelReadError> {
        let (execution, draft_only, facts_boundary) =
            self.source.selection_facts(&self.session, window, claim)?;
        let query = self.prepare(window, claim, execution.clone())?;
        query.elect(&facts_boundary, || ())?;
        Ok((query, execution, draft_only))
    }

    pub(crate) fn preparation_fence(
        &self,
        window: WindowId,
        claim: WindowClaimSelection,
    ) -> Result<Arc<ModelPreparationFence>, ModelReadError> {
        let _publication = self.lifetime.upgrade().ok_or(ModelReadError::Closed)?;
        let (execution, draft_only, facts_boundary) =
            self.source.selection_facts(&self.session, window, claim)?;
        let observation = Arc::new(self.source.prepare(
            self.session.clone(),
            window,
            claim,
            execution.clone(),
        )?);
        let boundary = observation.validate_idle()?;
        observation.with_current(&facts_boundary, || ())?;
        observation.with_current(&boundary, || ())?;
        Ok(Arc::new(ModelPreparationFence {
            observation,
            lifetime: self.lifetime.clone(),
            capacity: Arc::downgrade(&self.capacity),
            boundary: Mutex::new(boundary),
            execution,
            draft_only,
            window,
            claim,
        }))
    }

    pub(crate) fn prepare_fenced(
        &self,
        fence: &Arc<ModelPreparationFence>,
    ) -> Result<(Arc<ModelQuery>, ExecutionBinding, bool), ModelReadError> {
        if !Weak::ptr_eq(&fence.capacity, &Arc::downgrade(&self.capacity))
            || !Weak::ptr_eq(&fence.lifetime, &self.lifetime)
        {
            return Err(ModelReadError::Continuation);
        }
        fence.revalidate()?;
        let slot = self
            .capacity
            .reserve(CapacityKind::Query, self.limits.queries.get())?;
        let boundary = fence
            .boundary
            .lock()
            .map_err(|_| ModelReadError::Closed)?
            .clone();
        let query = Arc::new(ModelQuery {
            observation: Arc::clone(&fence.observation),
            lifetime: self.lifetime.clone(),
            closed: Mutex::new(false),
            idle: true,
            busy: AtomicBool::new(false),
            boundary: Mutex::new(boundary),
            failed: Mutex::new(None),
            capacity: Arc::clone(&self.capacity),
            limits: self.limits,
            _slot: slot,
        });
        query.with_current(|| ())?;
        Ok((query, fence.execution.clone(), fence.draft_only))
    }
    pub(crate) fn prepare_selected_status(
        &self,
        window: WindowId,
        claim: WindowClaimSelection,
    ) -> Result<(Arc<ModelQuery>, ExecutionBinding, bool), ModelReadError> {
        let (execution, draft_only, facts_boundary) =
            self.source.selection_facts(&self.session, window, claim)?;
        let query = self.prepare_status(window, claim, execution.clone())?;
        query.elect(&facts_boundary, || ())?;
        Ok((query, execution, draft_only))
    }
    pub(crate) fn prepare(
        &self,
        window: WindowId,
        claim: WindowClaimSelection,
        execution: ExecutionBinding,
    ) -> Result<Arc<ModelQuery>, ModelReadError> {
        self.prepare_scoped(window, claim, execution, true)
    }
    pub(crate) fn prepare_status(
        &self,
        window: WindowId,
        claim: WindowClaimSelection,
        execution: ExecutionBinding,
    ) -> Result<Arc<ModelQuery>, ModelReadError> {
        self.prepare_scoped(window, claim, execution, false)
    }
    fn prepare_scoped(
        &self,
        window: WindowId,
        claim: WindowClaimSelection,
        execution: ExecutionBinding,
        idle: bool,
    ) -> Result<Arc<ModelQuery>, ModelReadError> {
        let _publication = self.lifetime.upgrade().ok_or(ModelReadError::Closed)?;
        let slot = self
            .capacity
            .reserve(CapacityKind::Query, self.limits.queries.get())?;
        let observation = self
            .source
            .prepare(self.session.clone(), window, claim, execution)?;
        let boundary = if idle {
            observation.validate_idle()?
        } else {
            observation.validate()?
        };
        let query = Arc::new(ModelQuery {
            observation: Arc::new(observation),
            lifetime: self.lifetime.clone(),
            closed: Mutex::new(false),
            idle,
            busy: AtomicBool::new(false),
            boundary: Mutex::new(boundary),
            failed: Mutex::new(None),
            capacity: Arc::clone(&self.capacity),
            limits: self.limits,
            _slot: slot,
        });
        query.with_current_status(|| ())?;
        Ok(query)
    }
}

impl ModelQuery {
    pub(crate) fn close(&self) {
        *self
            .closed
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
    }
    fn ensure_open(&self) -> Result<(), ModelReadError> {
        if *self.closed.try_lock().map_err(|_| ModelReadError::Busy)?
            || self.lifetime.strong_count() == 0
        {
            Err(ModelReadError::Closed)
        } else {
            Ok(())
        }
    }
    fn begin(&self) -> Result<Request<'_>, ModelReadError> {
        self.ensure_open()?;
        Request::begin(&self.busy)
    }
    fn elect<T>(
        &self,
        boundary: &HomeMutationObservation,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ModelReadError> {
        let closed = self.closed.try_lock().map_err(|_| ModelReadError::Busy)?;
        let _publication = self.lifetime.upgrade().ok_or(ModelReadError::Closed)?;
        if *closed {
            return Err(ModelReadError::Closed);
        }
        Ok(self.observation.with_current(boundary, publish)?)
    }
    pub(crate) fn with_current<T>(&self, publish: impl FnOnce() -> T) -> Result<T, ModelReadError> {
        if !self.idle {
            return Err(ModelSourceError::Active.into());
        }
        self.with_current_status(publish)
    }
    pub(crate) fn with_current_status<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ModelReadError> {
        let boundary = self.boundary.try_lock().map_err(|_| ModelReadError::Busy)?;
        self.elect(&boundary, publish)
    }
    pub(crate) fn revalidate(&self) -> Result<(), ModelReadError> {
        let _request = self.begin()?;
        let boundary = if self.idle {
            self.observation.validate_idle()?
        } else {
            self.observation.validate()?
        };
        self.elect(&boundary, || ())?;
        *self.boundary.lock().map_err(|_| ModelReadError::Closed)? = boundary;
        Ok(())
    }
    pub(crate) fn read_defaults(&self) -> Result<ModelDefaults, ModelReadError> {
        let _request = self.begin()?;
        if self.idle {
            self.observation.validate_idle()?;
        } else {
            self.observation.validate()?;
        }
        let (defaults, _) = self.observation.read_defaults(self.limits.timeout)?;
        let boundary = if self.idle {
            self.observation.validate_idle()?
        } else {
            self.observation.validate()?
        };
        self.elect(&boundary, || ())?;
        *self.boundary.lock().map_err(|_| ModelReadError::Closed)? = boundary;
        Ok(ModelDefaults::from_backend(defaults))
    }
    pub(crate) fn read_page(
        self: &Arc<Self>,
        continuation: Option<&ModelContinuation>,
    ) -> Result<ModelPage, ModelReadError> {
        let cursor = match continuation {
            Some(continuation) if continuation.query.ptr_eq(&Arc::downgrade(self)) => {
                Some(Arc::clone(&continuation.cursor))
            }
            Some(_) => return Err(ModelReadError::Continuation),
            None => None,
        };
        self.read_cursor(cursor)
    }
    pub(crate) fn retry_page(self: &Arc<Self>) -> Result<ModelPage, ModelReadError> {
        let cursor = self
            .failed
            .lock()
            .map_err(|_| ModelReadError::Closed)?
            .clone()
            .ok_or(ModelReadError::Retry)?;
        self.read_cursor(cursor)
    }
    pub(crate) fn has_failed_page(&self) -> bool {
        self.failed.lock().is_ok_and(|failed| failed.is_some())
    }
    fn read_cursor(
        self: &Arc<Self>,
        cursor: Option<Arc<ModelPageCursor>>,
    ) -> Result<ModelPage, ModelReadError> {
        if !self.idle {
            return Err(ModelSourceError::Active.into());
        }
        let _request = self.begin()?;
        {
            let failed = self.failed.lock().map_err(|_| ModelReadError::Closed)?;
            if let Some(expected) = failed.as_ref() {
                if expected.as_ref().map(|cursor| cursor.as_str())
                    != cursor.as_ref().map(|cursor| cursor.as_str())
                {
                    return Err(ModelReadError::Retry);
                }
            }
        }
        self.observation.validate_idle()?;
        let slot = self
            .capacity
            .reserve(CapacityKind::Page, self.limits.pages.get())?;
        let mut options = ModelListOptions::page(u32::from(self.limits.records.get()))
            .map_err(|_| ModelReadError::Limits)?;
        if let Some(cursor) = &cursor {
            options = options.with_cursor(
                ModelPageCursor::try_new(cursor.as_str())
                    .map_err(|_| ModelReadError::Continuation)?,
            );
        }
        let result = self.observation.read_page(&options, self.limits.timeout);
        let (mut page, _) = match result {
            Ok(result) => result,
            Err(error) => {
                *self.failed.lock().map_err(|_| ModelReadError::Closed)? = Some(cursor);
                return Err(error.into());
            }
        };
        let boundary = self.observation.validate_idle()?;
        if page.len() > usize::from(self.limits.records.get()) {
            return Err(ModelReadError::Limits);
        }
        let records = page
            .records()
            .filter(|record| !record.hidden())
            .map(ModelOptionRecord::from_backend)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let continuation = page.take_next_cursor().map(|cursor| ModelContinuation {
            query: Arc::downgrade(self),
            cursor: Arc::new(cursor),
        });
        self.elect(&boundary, || ())?;
        *self.boundary.lock().map_err(|_| ModelReadError::Closed)? = boundary.clone();
        *self.failed.lock().map_err(|_| ModelReadError::Closed)? = None;
        Ok(ModelPage {
            query: Arc::downgrade(self),
            records,
            continuation,
            _slot: slot,
        })
    }
}

impl ModelPage {
    pub(crate) fn belongs_to(&self, query: &Arc<ModelQuery>) -> bool {
        self.query.ptr_eq(&Arc::downgrade(query))
    }
    pub(crate) fn records(&self) -> &[ModelOptionRecord] {
        &self.records
    }
    pub(crate) fn continuation(&self) -> Option<&ModelContinuation> {
        self.continuation.as_ref()
    }
    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce(&[ModelOptionRecord]) -> T,
    ) -> Result<T, ModelReadError> {
        let query = self.query.upgrade().ok_or(ModelReadError::Closed)?;
        query.with_current(|| publish(&self.records))
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/model_reader.rs"
    ));
}
