use super::*;
use crate::{
    cas_projection::initial_start::InitialStartOwner, catalog_readiness::CatalogSourceCoordinator,
};
use beryl_state::{CatalogNormalizedQuery, CatalogQueryScope};
use std::{
    future::Future,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

#[path = "../catalog_projection_support/mod.rs"]
mod support;

#[path = "catalog_query/reply_delivery.rs"]
mod reply_delivery;

pub(super) struct RequestPause {
    flags: Mutex<(bool, bool)>,
    changed: Condvar,
    panic_after_wait: bool,
}

impl RequestPause {
    fn new() -> Arc<Self> {
        Self::with_panic(false)
    }
    fn with_panic(panic_after_wait: bool) -> Arc<Self> {
        Arc::new(Self {
            flags: Mutex::new((false, false)),
            changed: Condvar::new(),
            panic_after_wait,
        })
    }
    pub(super) fn wait(&self) {
        let mut flags = self.flags.lock().unwrap();
        flags.0 = true;
        self.changed.notify_all();
        while !flags.1 {
            flags = self.changed.wait(flags).unwrap();
        }
        drop(flags);
        assert!(!self.panic_after_wait, "query worker injected panic");
    }
    pub(super) fn release(&self) {
        self.flags.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
    fn entered(&self) {
        until(|| self.flags.lock().unwrap().0);
    }
}

struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn receive(
    request: PublishedCatalogQueryRequest,
) -> Result<PublishedCatalogQueryResponse, CatalogQueryRequestError> {
    let mut future = Box::pin(request.receive());
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
        assert!(Instant::now() < deadline, "query response deadline");
        std::thread::park_timeout(Duration::from_millis(2));
    }
}

fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !predicate() {
        assert!(Instant::now() < deadline, "query fixture deadline");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn criteria() -> CatalogQueryCriteria {
    CatalogQueryCriteria::new(
        CatalogQueryScope::All,
        CatalogNormalizedQuery::new("").unwrap(),
    )
}

fn service(fixture: &support::Fixture) -> (CatalogQueryService, CatalogSourceCoordinator) {
    let start = InitialStartOwner::new();
    let source = CatalogSourceCoordinator::prepare(
        Arc::new(fixture.store.service_reference()),
        fixture.syndic.clone(),
        fixture.state.clone(),
        start.gate(),
    )
    .unwrap();
    let reader = source.reader();
    start.release();
    until(|| matches!(reader.certified_threads(), Ok(1)));
    let query = CatalogQueryService::prepare(
        Arc::new(fixture.store.service_reference()),
        fixture.state.clone(),
        reader,
    )
    .unwrap();
    (query, source)
}

fn open(reader: &PublishedCatalogQueryReader) -> PublishedCatalogOpened {
    let request = reader
        .open(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    let identity = request.identity().clone();
    let response = receive(request).unwrap();
    assert!(response.qualifies(&identity));
    let PublishedCatalogQueryResult::Opened(opened) = response.into_result() else {
        panic!("opened result")
    };
    opened
}

fn pause(service: &CatalogQueryService) -> Arc<RequestPause> {
    let pause = RequestPause::new();
    service.signal.state.lock().unwrap().pause = Some(Arc::clone(&pause));
    pause
}

#[test]
fn publication_gates_requests_and_first_page_position_keep_exact_frozen_presentation() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    let reader = query.reader();
    assert!(matches!(
        reader.open(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::NotPublished)
    ));
    query.publish();
    let opened = open(&reader);
    assert_eq!(opened.metadata().count(), 1);
    let row = &opened.metadata().first_page().rows()[0];
    assert_eq!(row.catalog().thread_id(), fixture.thread_id);
    assert_eq!(
        row.runtime_root().runtime().runtime_id(),
        fixture.runtime_id
    );
    assert_eq!(row.runtime_root().root().root_id(), fixture.root_id);
    let response = receive(
        opened
            .collection()
            .position(fixture.thread_id, CommandCancellation::new())
            .unwrap(),
    )
    .unwrap();
    let PublishedCatalogQueryResult::Position(Some(position)) = response.into_result() else {
        panic!("exact position")
    };
    assert_eq!(position.index(), 0);
    let response = receive(
        opened
            .collection()
            .page(
                position.cursor_before().cloned(),
                CatalogQueryPageLimit::maximum(),
                CommandCancellation::new(),
            )
            .unwrap(),
    )
    .unwrap();
    let PublishedCatalogQueryResult::Page(page) = response.into_result() else {
        panic!("page")
    };
    assert_eq!(page.rows(), opened.metadata().first_page().rows());
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    drop(opened);
    until(|| query.signal.state.lock().unwrap().collections.is_empty());
    query.stop_and_join().unwrap();
    assert!(query.reads_drained());
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    assert!(matches!(
        reader.open(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::Retired)
    ));
}

#[test]
fn cancellation_and_saturated_queue_leave_release_and_stop_controls_available() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    let opened = open(&query.reader());
    let pause = pause(&query);
    let mut requests = Vec::new();
    requests.push(
        opened
            .collection()
            .page(
                None,
                CatalogQueryPageLimit::maximum(),
                CommandCancellation::new(),
            )
            .unwrap(),
    );
    pause.entered();
    for _ in 1..CATALOG_QUERY_PENDING_REQUEST_LIMIT {
        requests.push(
            opened
                .collection()
                .page(
                    None,
                    CatalogQueryPageLimit::maximum(),
                    CommandCancellation::new(),
                )
                .unwrap(),
        );
    }
    assert!(matches!(
        opened.collection().page(
            None,
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::RequestLimit)
    ));
    drop(opened);
    assert_eq!(query.signal.state.lock().unwrap().releases.len(), 1);
    pause.release();
    for request in requests {
        assert!(matches!(
            receive(request),
            Err(CatalogQueryRequestError::Cancelled)
        ));
    }
    until(|| query.signal.state.lock().unwrap().collections.is_empty());
    query.stop_and_join().unwrap();
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn dropped_first_response_releases_collection_and_join_cancels_an_active_request() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    let request = query
        .reader()
        .open(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    until(|| query.signal.state.lock().unwrap().controls.is_empty());
    assert_eq!(query.signal.state.lock().unwrap().collections.len(), 1);
    drop(request);
    until(|| query.signal.state.lock().unwrap().collections.is_empty());
    let pause = pause(&query);
    let request = query
        .reader()
        .open(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    pause.entered();
    query.stop_and_join().unwrap();
    assert!(matches!(
        receive(request),
        Err(CatalogQueryRequestError::Cancelled | CatalogQueryRequestError::Retired)
    ));
    assert!(query.reads_drained());
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn identity_exhaustion_refuses_admission_without_retaining_another_source() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    query.signal.state.lock().unwrap().next_request = u64::MAX;
    assert!(matches!(
        query.reader().open(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::IdentityExhausted)
    ));
    assert!(query.signal.state.lock().unwrap().controls.is_empty());
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    query.stop_and_join().unwrap();
    source.stop_and_join().unwrap();
}

#[test]
fn cancelled_page_preserves_collection_until_explicit_termination() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    let opened = open(&query.reader());
    let pause = pause(&query);
    let request = opened
        .collection()
        .page(
            None,
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    pause.entered();
    request.cancel();
    pause.release();
    assert!(matches!(
        receive(request),
        Err(CatalogQueryRequestError::Cancelled)
    ));
    assert!(opened.collection().is_current());
    assert!(
        receive(
            opened
                .collection()
                .page(
                    None,
                    CatalogQueryPageLimit::maximum(),
                    CommandCancellation::new()
                )
                .unwrap()
        )
        .is_ok()
    );
    drop(opened);
    query.stop_and_join().unwrap();
    source.stop_and_join().unwrap();
}

#[test]
fn existing_collection_survives_live_source_replacement_without_recapture() {
    use beryl_model::{ExecutionBinding, RuntimeMode, SyndicDraftId};
    use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicTimestamp};
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    let old = open(&query.reader());
    let revision = old.metadata().home_revision();
    let second = SyndicThreadId::from_bytes([80; 16]);
    support::execute_contribution(
        &fixture.store,
        fixture.syndic.create_thread(
            fixture.syndic.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                second,
                SyndicDraftId::from_bytes([81; 16]),
                ExecutionBinding::new(
                    fixture.runtime_id,
                    fixture.root_id,
                    support::native_path(RuntimeMode::host(), r"C:\Work\Beryl"),
                ),
                SyndicTimestamp::from_unix_millis(90),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    source.waker().wake_by_ref();
    until(|| matches!(source.reader().certified_threads(), Ok(2)));
    let fresh = open(&query.reader());
    assert_eq!(fresh.metadata().count(), 2);
    assert_ne!(fresh.metadata().home_revision(), revision);
    let response = receive(
        old.collection()
            .page(
                None,
                CatalogQueryPageLimit::maximum(),
                CommandCancellation::new(),
            )
            .unwrap(),
    )
    .unwrap();
    let PublishedCatalogQueryResult::Page(page) = response.into_result() else {
        panic!("page")
    };
    assert_eq!(page.rows(), old.metadata().first_page().rows());
    let response = receive(
        old.collection()
            .position(second, CommandCancellation::new())
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        response.into_result(),
        PublishedCatalogQueryResult::Position(None)
    ));
    drop(old);
    drop(fresh);
    query.stop_and_join().unwrap();
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn worker_panic_preserves_original_owner_until_joined_exact_read_retirement() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    let opened = open(&query.reader());
    let pause = RequestPause::with_panic(true);
    query.signal.state.lock().unwrap().pause = Some(Arc::clone(&pause));
    let request = opened
        .collection()
        .page(
            None,
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    pause.entered();
    pause.release();
    assert!(matches!(
        receive(request),
        Err(CatalogQueryRequestError::Retired | CatalogQueryRequestError::Panicked)
    ));
    assert!(matches!(
        query.stop_and_join(),
        Err(CatalogQueryServiceError::Panicked)
    ));
    assert!(query.reads_drained());
    assert!(!opened.collection().is_current());
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}
