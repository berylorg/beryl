use super::*;

#[test]
fn progressive_count_exposes_one_continuation_row_without_inventing_an_inventory_total() {
    let mut collection = ModelMenuCollection::new();
    assert_eq!(collection.logical_count(), 1);
    collection.observed = 64;
    assert_eq!(collection.logical_count(), 65);
    collection.observed = 10_000_000;
    assert_eq!(collection.logical_count(), 10_000_001);
    collection.complete = true;
    assert_eq!(collection.logical_count(), 10_000_000);
    assert!(collection.pages.is_empty());
}

#[test]
fn keyboard_target_for_an_evicted_range_remains_logical_until_matching_data_arrives() {
    let mut collection = ModelMenuCollection::new();
    collection.observed = 10_000_000;
    collection.reveal_focus(5_500_021);
    assert_eq!(collection.requested_focus, Some(5_500_021));
    assert!(collection.focused.is_none());
    let (start, continuation) = collection.request_for(5_500_021);
    assert_eq!(start, 0);
    assert!(continuation.is_none());
    collection.reveal_focus(0);
    assert_eq!(collection.requested_focus, Some(0));
    assert!(collection.row(5_500_021).is_none());
}

#[test]
fn menu_bound_is_independent_of_total_inventory_and_has_explicit_fixed_row_realization() {
    assert_eq!(RESIDENT_MODEL_PAGES, 2);
    assert_eq!(MODEL_ROW_HEIGHT, 30.);
    assert_eq!(MODEL_OVERSCAN, 0);
    let viewport = 320_f32;
    let maximum_realized = (viewport / MODEL_ROW_HEIGHT).ceil() as usize + MODEL_OVERSCAN * 2;
    assert_eq!(maximum_realized, 11);
    assert!(maximum_realized < 64);
}

#[cfg(feature = "test-faults")]
#[test]
fn genuine_pages_keep_two_residents_and_pin_selection_and_focus_across_eviction() {
    use crate::model_selection::test_support::{Fixture, ModelResponse};
    let fixture = Fixture::with_page_capacity(3);
    let query = fixture.query();
    let mut collection = ModelMenuCollection::new();
    collection.range = 0..11;
    fixture
        .server
        .enqueue(ModelResponse::page_range(0, 64, Some("second")));
    let first = Arc::new(query.read_page(None).unwrap());
    let second_cursor = first.continuation().cloned().unwrap();
    collection.install(0, first, Some("model-70"));
    collection.reveal_focus(5);
    fixture
        .server
        .enqueue(ModelResponse::page_range(64, 64, Some("third")));
    let second = Arc::new(query.read_page(Some(&second_cursor)).unwrap());
    let third_cursor = second.continuation().cloned().unwrap();
    collection.install(64, second, Some("model-70"));
    assert_eq!(collection.selected.as_ref().unwrap().0, 70);
    assert_eq!(collection.focused.as_ref().unwrap().1.id, "id-5");
    fixture
        .server
        .enqueue(ModelResponse::page_range(128, 64, None));
    let third = Arc::new(query.read_page(Some(&third_cursor)).unwrap());
    collection.install(128, third, Some("model-70"));
    assert_eq!(collection.pages.len(), 2);
    assert!(collection.row(5).is_some());
    assert!(collection.row(70).is_none());
    assert_eq!(collection.selected.as_ref().unwrap().1.id, "id-70");
    assert_eq!(collection.logical_count(), 192);
    collection.range = 128..139;
    let (start, cursor) = collection.request_for(70);
    assert_eq!(start, 64);
    fixture
        .server
        .enqueue(ModelResponse::page_range(64, 64, Some("third")));
    let replay = read_model_range(&query, start, cursor, 70, false, Some("model-70"))
        .unwrap_or_else(|_| panic!("bounded replay failed"));
    collection.install(replay.start, replay.page, Some("model-70"));
    assert_eq!(collection.row(70).unwrap().1.id, "id-70");
    assert_eq!(collection.pages.len(), 2);
    assert!(collection.row(130).is_some());
    query.close();
    assert!(collection.row(70).unwrap().0.with_current(|_| ()).is_err());
}

#[cfg(feature = "test-faults")]
#[test]
fn later_page_failure_preserves_genuine_presented_rows_and_retry_repeats_exact_cursor() {
    use crate::model_selection::test_support::{Fixture, ModelResponse};
    let fixture = Fixture::with_page_capacity(3);
    let query = fixture.query();
    fixture
        .server
        .enqueue(ModelResponse::page_range(0, 64, Some("exact-second")));
    let first = Arc::new(query.read_page(None).unwrap());
    let mut collection = ModelMenuCollection::new();
    collection.install(0, first, Some("model-5"));
    collection.reveal_focus(5);
    let (start, cursor) = collection.request_for(64);
    fixture.server.enqueue(ModelResponse::Failure);
    let failure = match read_model_range(&query, start, cursor, 64, false, Some("model-5")) {
        Err(failure) => failure,
        Ok(_) => panic!("failed page unexpectedly published"),
    };
    assert!(failure.retry_page);
    assert_eq!(failure.start, 64);
    assert_eq!(collection.row(5).unwrap().1.id, "id-5");
    assert_eq!(collection.selected.as_ref().unwrap().0, 5);
    assert_eq!(collection.focused.as_ref().unwrap().0, 5);
    fixture
        .server
        .enqueue(ModelResponse::page_range(64, 8, None));
    let retry = read_model_range(&query, failure.start, None, 64, true, Some("model-5"))
        .unwrap_or_else(|_| panic!("exact page retry failed"));
    collection.install(retry.start, retry.page, Some("model-5"));
    let requests = fixture.server.requests("model/list");
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1]["params"]["cursor"],
        requests[2]["params"]["cursor"]
    );
    assert_eq!(requests[2]["params"]["cursor"], "exact-second");
    assert_eq!(collection.logical_count(), 72);
    assert_eq!(collection.row(64).unwrap().1.id, "id-64");
}

#[cfg(feature = "test-faults")]
#[test]
fn range_replay_refreshes_unrelated_home_writes_off_gui_and_cannot_publish_after_close() {
    use crate::model_selection::test_support::{Fixture, ModelResponse};
    let fixture = Fixture::with_page_capacity(3);
    let query = fixture.query();
    fixture.unrelated_write();
    fixture
        .server
        .enqueue(ModelResponse::page_range(0, 16, None));
    let page = read_model_range(&query, 0, None, 5, false, Some("model-5"))
        .unwrap_or_else(|_| panic!("unrelated write was not refreshed"));
    assert_eq!(page.selected.as_ref().unwrap().0, 5);
    query.close();
    let failure = match read_model_range(&query, 0, None, 0, false, None) {
        Err(failure) => failure,
        Ok(_) => panic!("closed query replay unexpectedly published"),
    };
    assert!(failure.error.scope_retired());
    assert!(!failure.retry_page);
    assert_eq!(fixture.server.requests("model/list").len(), 1);
}

#[cfg(feature = "test-faults")]
#[test]
fn manual_capacity_retry_retains_the_original_sealed_continuation_without_failed_request() {
    use crate::model_selection::test_support::{Fixture, ModelResponse};
    let fixture = Fixture::with_page_capacity(2);
    let query = fixture.query();
    fixture
        .server
        .enqueue(ModelResponse::page_range(0, 64, Some("second-page")));
    let first = query.read_page(None).unwrap();
    let second_cursor = first.continuation().unwrap().clone();
    fixture
        .server
        .enqueue(ModelResponse::page_range(64, 64, Some("third-page")));
    let second = query.read_page(Some(&second_cursor)).unwrap();
    let third_cursor = second.continuation().unwrap().clone();
    let failure = match read_model_range(&query, 128, Some(third_cursor), 128, false, None) {
        Err(failure) => failure,
        Ok(_) => panic!("full page capacity unexpectedly admitted a request"),
    };
    assert_eq!(failure.start, 128);
    assert!(matches!(failure.error, ModelReadError::Capacity));
    assert!(!failure.retry_page);
    assert!(!query.has_failed_page());
    assert_eq!(fixture.server.requests("model/list").len(), 2);
    drop(first);
    fixture
        .server
        .enqueue(ModelResponse::page_range(128, 8, None));
    let retry = read_model_range(
        &query,
        failure.start,
        failure.continuation,
        128,
        failure.retry_page,
        None,
    )
    .unwrap_or_else(|_| panic!("manual same-continuation retry failed"));
    assert_eq!(retry.start, 128);
    assert_eq!(retry.page.records()[0].id, "id-128");
    assert_eq!(
        fixture.server.requests("model/list")[2]["params"]["cursor"],
        "third-page"
    );
    drop(second);
}
