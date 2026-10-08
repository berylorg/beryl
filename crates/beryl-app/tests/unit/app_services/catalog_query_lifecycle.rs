use super::*;
use crate::{catalog_query::*, catalog_readiness::CatalogSourceReadError};
use beryl_state::{
    CatalogNormalizedQuery, CatalogQueryCriteria, CatalogQueryPageLimit, CatalogQueryScope,
};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

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
        assert!(Instant::now() < deadline, "graph query deadline");
        std::thread::park_timeout(Duration::from_millis(2));
    }
}

fn request(reader: &PublishedCatalogQueryReader) -> PublishedCatalogQueryRequest {
    reader
        .open(
            CatalogQueryCriteria::new(
                CatalogQueryScope::All,
                CatalogNormalizedQuery::new("").unwrap(),
            ),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap()
}

fn certify(owner: &ProcessServiceOwner) {
    let reader = owner.graph().unwrap().catalog_source_reader();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match reader.certified_threads() {
            Ok(0) => return,
            Err(CatalogSourceReadError::NotReady) => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
            other => panic!("empty graph certification: {other:?}"),
        }
    }
}

#[test]
fn published_graph_query_reader_does_not_pin_home_after_shutdown() {
    let (directory, mut owner, _) = recovery_support::installed();
    certify(&owner);
    let reader = owner.catalog_query_reader().unwrap();
    let response = receive(request(&reader)).unwrap();
    let PublishedCatalogQueryResult::Opened(opened) = response.into_result() else {
        panic!("open")
    };
    assert_eq!(opened.metadata().count(), 0);
    close(&mut owner);
    assert!(!opened.collection().is_current());
    assert!(matches!(
        reader.open(
            CatalogQueryCriteria::new(
                CatalogQueryScope::All,
                CatalogNormalizedQuery::new("").unwrap()
            ),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::Retired)
    ));
    assert_reopens(&directory);
    drop(opened);
}

#[test]
fn held_startup_source_refuses_first_query_and_cancellation_retires_reader() {
    let (directory, candidate, state, syndic, _) = fixture();
    let mut owner = owner(&candidate);
    let start = owner
        .open_initial_for_startup(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let reader = owner.catalog_query_reader().unwrap();
    assert!(
        matches!(receive(request(&reader)), Err(CatalogQueryRequestError::Source(error)) if matches!(*error, CatalogSourceReadError::NotReady))
    );
    assert_eq!(
        owner
            .graph()
            .unwrap()
            .home()
            .retained_frozen_read_count()
            .unwrap(),
        0
    );
    drop(start);
    owner.drain_initial_catalog_source();
    assert!(matches!(
        reader.open(
            CatalogQueryCriteria::new(
                CatalogQueryScope::All,
                CatalogNormalizedQuery::new("").unwrap()
            ),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::Retired)
    ));
    close(&mut owner);
    assert_reopens(&directory);
}

#[test]
fn failed_graph_query_custody_settles_after_original_home_generation_retirement() {
    let (directory, mut owner, faults) = recovery_support::installed();
    certify(&owner);
    let reader = owner.catalog_query_reader().unwrap();
    let response = receive(request(&reader)).unwrap();
    let PublishedCatalogQueryResult::Opened(opened) = response.into_result() else {
        panic!("open")
    };
    let expected = owner.graph().unwrap().home().health().generation().unwrap();
    owner
        .graph
        .as_mut()
        .unwrap()
        .catalog_source
        .as_mut()
        .unwrap()
        .stop_and_join()
        .unwrap();
    owner
        .graph
        .as_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
    recovery_support::fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    assert!(!opened.collection().is_current());
    let mut candidate = owner.recover_retired_service_home(expected).unwrap();
    assert_ne!(candidate.generation(), expected);
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    owner
        .settle_retired_process_work(
            &candidate.recovery_access().unwrap(),
            &state,
            &syndic,
            &CommandCancellation::new(),
        )
        .unwrap();
    owner.require_catalog_recovery_settlement(expected).unwrap();
    let generation = candidate.generation();
    let mut candidate = Some(candidate);
    let prepared = owner
        .prepare_recovery_service_graph(
            expected,
            &mut candidate,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(candidate.is_none());
    let mut prepared = Some(prepared);
    let start = owner
        .publish_recovery_service_graph(
            expected,
            generation,
            &mut prepared,
            &CommandCancellation::new(),
        )
        .unwrap();
    let fresh = owner.catalog_query_reader().unwrap();
    assert!(
        matches!(receive(request(&fresh)), Err(CatalogQueryRequestError::Source(error)) if matches!(*error, CatalogSourceReadError::NotReady))
    );
    assert!(start.release());
    certify(&owner);
    let response = receive(request(&fresh)).unwrap();
    let PublishedCatalogQueryResult::Opened(fresh_opened) = response.into_result() else {
        panic!("fresh open")
    };
    assert_eq!(fresh_opened.metadata().count(), 0);
    assert_ne!(
        fresh_opened.metadata().token().generation_identity(),
        opened.metadata().token().generation_identity()
    );
    owner.graph.take().unwrap().dispose_unstarted().unwrap();
    assert!(!fresh_opened.collection().is_current());
    assert_reopens(&directory);
    drop(opened);
}

#[test]
fn private_graph_query_reader_refuses_work_and_cannot_pin_cancelled_preparation() {
    let (directory, candidate, state, syndic, _) = fixture();
    let owner = owner(&candidate);
    let prepared = preparation::PreparedAppServices::prepare(
        &owner,
        candidate,
        state,
        syndic,
        configuration(),
        SyndicTimestamp::from_unix_millis(1),
        &CommandCancellation::new(),
    )
    .unwrap();
    let reader = prepared.catalog_query_reader();
    assert!(matches!(
        reader.open(
            CatalogQueryCriteria::new(
                CatalogQueryScope::All,
                CatalogNormalizedQuery::new("").unwrap()
            ),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::NotPublished)
    ));
    prepared.dispose().unwrap();
    assert!(matches!(
        reader.open(
            CatalogQueryCriteria::new(
                CatalogQueryScope::All,
                CatalogNormalizedQuery::new("").unwrap()
            ),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new()
        ),
        Err(CatalogQueryRequestError::Retired)
    ));
    assert_reopens(&directory);
}
