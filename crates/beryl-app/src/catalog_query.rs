use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, Weak},
    thread::JoinHandle,
};

use beryl_home_store::{
    CommandCancellation, FrozenHomeRead, HomeGeneration, HomeServiceReference, ReadError,
};
use beryl_model::{RootId, RuntimeId, SyndicThreadId};
use beryl_state::{
    BerylState, CatalogNormalizedQuery, CatalogOptionPage, CatalogQueryCriteria,
    CatalogQueryCursor, CatalogQueryError, CatalogQueryOpened, CatalogQueryOwner, CatalogQueryPage,
    CatalogQueryPageLimit, CatalogQueryPosition, CatalogQueryToken, CatalogRootRow,
    CatalogRuntimeRow,
};
use futures_channel::oneshot;

use crate::catalog_readiness::{CatalogSourceReadError, CatalogSourceReader};

mod admission;
mod worker;

pub const CATALOG_QUERY_PENDING_REQUEST_LIMIT: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum CatalogQueryRequestError {
    #[error("catalog query graph is not published")]
    NotPublished,
    #[error("catalog query graph is retired")]
    Retired,
    #[error("catalog query request was cancelled")]
    Cancelled,
    #[error("catalog query request queue is full")]
    RequestLimit,
    #[error("catalog query request identities are exhausted")]
    IdentityExhausted,
    #[error("catalog query response belongs to another request or collection")]
    Foreign,
    #[error("catalog query worker panicked")]
    Panicked,
    #[error(transparent)]
    Source(#[from] Box<CatalogSourceReadError>),
    #[error(transparent)]
    Query(#[from] Box<CatalogQueryError>),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CatalogQueryServiceError {
    #[error("catalog query worker could not start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("catalog query worker panicked after its work was joined")]
    Panicked,
    #[error("catalog query read retirement failed: {0}")]
    Retirement(#[source] Box<CatalogQueryError>),
    #[error("unadmitted catalog source retirement failed: {0}")]
    SourceRetirement(#[source] ReadError),
}

#[derive(Clone, Debug)]
pub struct CatalogQueryRequestIdentity {
    service: Weak<QuerySignal>,
    id: u64,
    generation: HomeGeneration,
    query: Option<CatalogQueryToken>,
}

impl PartialEq for CatalogQueryRequestIdentity {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.service, &other.service)
            && self.id == other.id
            && self.generation == other.generation
            && self.query == other.query
    }
}
impl Eq for CatalogQueryRequestIdentity {}

#[derive(Clone)]
pub struct PublishedCatalogQueryReader {
    signal: Weak<QuerySignal>,
}

#[cfg(all(test, feature = "test-faults"))]
pub(crate) struct CatalogQueryTestPause {
    signal: Weak<QuerySignal>,
    pause: Arc<tests::RequestPause>,
}

#[cfg(all(test, feature = "test-faults"))]
impl CatalogQueryTestPause {
    pub(crate) fn has_entered(&self) -> bool {
        self.pause.has_entered()
    }
    pub(crate) fn release(&self) {
        self.pause.release();
    }
}

#[cfg(all(test, feature = "test-faults"))]
impl Drop for CatalogQueryTestPause {
    fn drop(&mut self) {
        self.pause.release();
        if let Some(signal) = self.signal.upgrade() {
            let mut state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
            if state
                .pause
                .as_ref()
                .is_some_and(|pause| Arc::ptr_eq(pause, &self.pause))
            {
                state.pause = None;
            }
        }
    }
}

pub struct PublishedCatalogCollection {
    signal: Weak<QuerySignal>,
    generation: HomeGeneration,
    token: CatalogQueryToken,
}

pub struct PublishedCatalogOpened {
    collection: PublishedCatalogCollection,
    opened: CatalogQueryOpened,
}

impl PublishedCatalogOpened {
    pub fn collection(&self) -> &PublishedCatalogCollection {
        &self.collection
    }
    pub fn metadata(&self) -> &CatalogQueryOpened {
        &self.opened
    }
    pub fn into_parts(self) -> (PublishedCatalogCollection, CatalogQueryOpened) {
        (self.collection, self.opened)
    }
}

pub enum PublishedCatalogQueryResult {
    Opened(PublishedCatalogOpened),
    Page(CatalogQueryPage),
    Position(Option<CatalogQueryPosition>),
    Runtimes(CatalogOptionPage<CatalogRuntimeRow>),
    Roots(CatalogOptionPage<CatalogRootRow>),
    OptionPosition(Option<u64>),
}

pub struct PublishedCatalogQueryResponse {
    identity: CatalogQueryRequestIdentity,
    result: PublishedCatalogQueryResult,
}

impl PublishedCatalogQueryResponse {
    pub fn identity(&self) -> &CatalogQueryRequestIdentity {
        &self.identity
    }
    pub fn qualifies(&self, expected: &CatalogQueryRequestIdentity) -> bool {
        self.identity == *expected && admission::current(&self.identity)
    }
    pub fn into_result(self) -> PublishedCatalogQueryResult {
        self.result
    }
}

pub struct PublishedCatalogQueryRequest {
    identity: CatalogQueryRequestIdentity,
    cancellation: CommandCancellation,
    receiver:
        Option<oneshot::Receiver<Result<PublishedCatalogQueryResponse, CatalogQueryRequestError>>>,
}

impl PublishedCatalogQueryRequest {
    pub fn identity(&self) -> &CatalogQueryRequestIdentity {
        &self.identity
    }
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    pub async fn receive(
        mut self,
    ) -> Result<PublishedCatalogQueryResponse, CatalogQueryRequestError> {
        let response = self
            .receiver
            .take()
            .expect("one response receiver")
            .await
            .map_err(|_| CatalogQueryRequestError::Retired)??;
        if self.cancellation.is_cancelled() {
            return Err(CatalogQueryRequestError::Cancelled);
        }
        if !response.qualifies(&self.identity) {
            return Err(CatalogQueryRequestError::Retired);
        }
        Ok(response)
    }
}

impl Drop for PublishedCatalogQueryRequest {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

enum QueryOperation {
    Open {
        criteria: CatalogQueryCriteria,
        limit: CatalogQueryPageLimit,
    },
    Page {
        token: CatalogQueryToken,
        cursor: Option<CatalogQueryCursor>,
        limit: CatalogQueryPageLimit,
    },
    Position {
        token: CatalogQueryToken,
        thread: SyndicThreadId,
    },
    Refine {
        token: CatalogQueryToken,
        criteria: CatalogQueryCriteria,
        limit: CatalogQueryPageLimit,
    },
    PageAt {
        token: CatalogQueryToken,
        start: u64,
        limit: CatalogQueryPageLimit,
    },
    Runtimes {
        token: CatalogQueryToken,
        search: CatalogNormalizedQuery,
        start: u64,
        limit: CatalogQueryPageLimit,
    },
    Roots {
        token: CatalogQueryToken,
        runtime: RuntimeId,
        search: CatalogNormalizedQuery,
        start: u64,
        limit: CatalogQueryPageLimit,
    },
    RuntimePosition {
        token: CatalogQueryToken,
        search: CatalogNormalizedQuery,
        runtime: RuntimeId,
    },
    RootPosition {
        token: CatalogQueryToken,
        runtime: RuntimeId,
        search: CatalogNormalizedQuery,
        root: RootId,
    },
}

struct QueryRequest {
    identity: CatalogQueryRequestIdentity,
    operation: QueryOperation,
    cancellation: CommandCancellation,
    reply: oneshot::Sender<Result<PublishedCatalogQueryResponse, CatalogQueryRequestError>>,
}

struct RequestControl {
    identity: CatalogQueryRequestIdentity,
    cancellation: CommandCancellation,
}

struct QueryState {
    published: bool,
    stopped: bool,
    generation: HomeGeneration,
    next_request: u64,
    queue: VecDeque<QueryRequest>,
    controls: Vec<RequestControl>,
    collections: Vec<CatalogQueryToken>,
    releases: Vec<CatalogQueryToken>,
    #[cfg(all(test, feature = "test-faults"))]
    pause: Option<Arc<tests::RequestPause>>,
    #[cfg(all(test, feature = "test-faults"))]
    active_pause: Option<Arc<tests::RequestPause>>,
}

struct QuerySignal {
    source: CatalogSourceReader,
    state: Mutex<QueryState>,
    changed: Condvar,
}

impl std::fmt::Debug for QuerySignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogQuerySignal").finish_non_exhaustive()
    }
}

pub(crate) struct CatalogQueryService {
    home: Arc<HomeServiceReference>,
    signal: Arc<QuerySignal>,
    worker: Option<JoinHandle<()>>,
    custody: Arc<Mutex<Box<worker::WorkerCustody>>>,
}

impl CatalogQueryService {
    pub(crate) fn prepare(
        home: Arc<HomeServiceReference>,
        state: BerylState,
        source: CatalogSourceReader,
    ) -> Result<Self, CatalogQueryServiceError> {
        let signal = Arc::new(QuerySignal {
            source: source.clone(),
            state: Mutex::new(QueryState {
                published: false,
                stopped: false,
                generation: home
                    .health()
                    .generation()
                    .expect("validated graph generation"),
                next_request: 1,
                queue: VecDeque::new(),
                controls: Vec::new(),
                collections: Vec::new(),
                releases: Vec::new(),
                #[cfg(all(test, feature = "test-faults"))]
                pause: None,
                #[cfg(all(test, feature = "test-faults"))]
                active_pause: None,
            }),
            changed: Condvar::new(),
        });
        let worker_home = Arc::clone(&home);
        let worker_signal = Arc::clone(&signal);
        let custody = Arc::new(Mutex::new(Box::new(worker::WorkerCustody::new())));
        let worker_custody = Arc::clone(&custody);
        let worker = std::thread::Builder::new()
            .name("catalog-query".into())
            .spawn(move || {
                worker::run(worker_home, state, source, worker_signal, worker_custody)
            })?;
        Ok(Self {
            home,
            signal,
            worker: Some(worker),
            custody,
        })
    }
    pub(crate) fn publish(&self) {
        let mut state = self.signal.state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!state.stopped && !state.published);
        state.published = true;
        self.signal.changed.notify_all();
    }
    pub(crate) fn reader(&self) -> PublishedCatalogQueryReader {
        PublishedCatalogQueryReader {
            signal: Arc::downgrade(&self.signal),
        }
    }
    pub(crate) fn work_drained(&self) -> bool {
        self.worker.is_none()
    }
    pub(crate) fn reads_drained(&self) -> bool {
        self.work_drained()
            && self
                .custody
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reads_drained()
    }
    pub(crate) fn stop_and_join(&mut self) -> Result<(), CatalogQueryServiceError> {
        admission::stop(&self.signal);
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                self.custody
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .panicked = true;
            }
        }
        let mut custody = self.custody.lock().unwrap_or_else(|e| e.into_inner());
        custody.retire(&self.home)?;
        if std::mem::take(&mut custody.panicked) {
            return Err(CatalogQueryServiceError::Panicked);
        }
        Ok(())
    }
}

impl Drop for CatalogQueryService {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../tests/unit/catalog_query.rs"]
mod tests;
