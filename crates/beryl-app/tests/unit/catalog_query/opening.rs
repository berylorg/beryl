use super::*;

#[test]
fn refined_and_option_responses_share_opening_and_released_anchor_cancels_pending_refinement() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    let reader = query.reader();
    assert!(!reader.is_ready());
    let retained = fixture.store.retained_frozen_read_count().unwrap();
    query.publish();
    assert!(reader.is_ready());
    assert_eq!(
        fixture.store.retained_frozen_read_count().unwrap(),
        retained
    );
    let opening = open(&query.reader());
    let revision = opening.metadata().home_revision();
    let response = receive(
        opening
            .collection()
            .runtime_page(
                CatalogNormalizedQuery::new("CODEX.EXE").unwrap(),
                0,
                CatalogQueryPageLimit::maximum(),
                CommandCancellation::new(),
            )
            .unwrap(),
    )
    .unwrap();
    let PublishedCatalogQueryResult::Runtimes(page) = response.into_result() else {
        panic!("runtimes")
    };
    assert_eq!(page.home_revision(), revision);
    assert_eq!(page.count(), 1);
    assert_eq!(page.rows()[0].root_count(), 1);
    let response = receive(
        opening
            .collection()
            .root_page(
                fixture.runtime_id,
                CatalogNormalizedQuery::new("WORK\\BERYL").unwrap(),
                0,
                CatalogQueryPageLimit::maximum(),
                CommandCancellation::new(),
            )
            .unwrap(),
    )
    .unwrap();
    let PublishedCatalogQueryResult::Roots(page) = response.into_result() else {
        panic!("roots")
    };
    assert_eq!(page.home_revision(), revision);
    assert_eq!(page.rows()[0].thread_count(), 1);
    let request = opening
        .collection()
        .refine(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    let identity = request.identity().clone();
    let response = receive(request).unwrap();
    assert!(response.qualifies(&identity));
    let PublishedCatalogQueryResult::Opened(refined) = response.into_result() else {
        panic!("refined")
    };
    assert_eq!(refined.metadata().home_revision(), revision);
    assert_eq!(
        refined.metadata().first_page().rows(),
        opening.metadata().first_page().rows()
    );
    let response = receive(
        refined
            .collection()
            .page_at(
                0,
                CatalogQueryPageLimit::maximum(),
                CommandCancellation::new(),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(
        matches!(response.into_result(), PublishedCatalogQueryResult::Page(page) if page.rows().len() == 1)
    );
    let response = receive(
        opening
            .collection()
            .root_position(
                fixture.runtime_id,
                CatalogNormalizedQuery::new("").unwrap(),
                fixture.root_id,
                CommandCancellation::new(),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        response.into_result(),
        PublishedCatalogQueryResult::OptionPosition(Some(0))
    ));
    let paused = pause(&query);
    let pending = opening
        .collection()
        .refine(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    paused.entered();
    drop(opening);
    paused.release();
    assert!(matches!(
        receive(pending),
        Err(CatalogQueryRequestError::Cancelled)
    ));
    assert!(refined.collection().is_current());
    drop(refined);
    query.stop_and_join().unwrap();
    assert!(!reader.is_ready());
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn cancelled_option_request_preserves_anchor_and_dropped_refinement_response_releases_new_read() {
    let fixture = support::Fixture::new(r"C:\Work\Beryl");
    let (mut query, mut source) = service(&fixture);
    query.publish();
    let opening = open(&query.reader());
    let paused = pause(&query);
    let request = opening
        .collection()
        .runtime_page(
            CatalogNormalizedQuery::new("").unwrap(),
            0,
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    paused.entered();
    request.cancel();
    paused.release();
    assert!(matches!(
        receive(request),
        Err(CatalogQueryRequestError::Cancelled)
    ));
    assert!(opening.collection().is_current());
    let request = opening
        .collection()
        .refine(
            criteria(),
            CatalogQueryPageLimit::maximum(),
            CommandCancellation::new(),
        )
        .unwrap();
    let response = receive(request).unwrap();
    drop(response);
    until(|| query.signal.state.lock().unwrap().collections.len() == 1);
    drop(opening);
    query.stop_and_join().unwrap();
    source.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}
