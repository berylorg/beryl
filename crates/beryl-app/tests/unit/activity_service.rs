use super::*;

#[path = "activity_service/preparation.rs"]
mod candidate;
#[path = "activity_service/fixture.rs"]
mod fixture;
#[path = "activity_service/lifetime.rs"]
mod lifetime;
#[path = "activity_service/recovery.rs"]
mod recovery;
use crate::support;

use fixture::*;
use support::id;

#[test]
fn eligible_empty_and_bounded_pages_preserve_counts_and_order() {
    let f = Fixture::new(true);
    let service = f.service(limits(2, 2, 2));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let empty = request.open().unwrap();
    empty
        .with_current_head(|head| assert_eq!(head.logical_row_count(), 0))
        .unwrap();
    let page = empty.read_page(None).unwrap();
    page.with_current(|_, page| assert!(page.records().is_empty()))
        .unwrap();
    assert!(page.continuation().unwrap().is_none());
    drop(page);
    drop(empty);
    assert!(matches!(
        request.open(),
        Err(ActivityReadError::RequestCompleted)
    ));
    drop(request);

    let source = f.activate();
    for index in 230..233 {
        f.add_command(&source, index);
    }
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let collection = request.open().unwrap();
    collection
        .with_current_head(|head| {
            assert_eq!(head.logical_row_count(), 3);
            assert_eq!(head.running_row_count(), 3);
            assert_eq!(head.completed_row_count(), 0);
        })
        .unwrap();
    let first = collection.read_page(None).unwrap();
    let first_orders = first
        .with_current(|_, page| {
            assert_eq!(page.records().len(), 2);
            assert!(page.stored_bytes() <= 65_536);
            page.records()
                .iter()
                .map(|row| row.order())
                .collect::<Vec<_>>()
        })
        .unwrap();
    let cursor = first.continuation().unwrap().unwrap();
    drop(first);
    let last = collection.read_page(Some(&cursor)).unwrap();
    let final_order = last
        .with_current(|_, page| {
            assert_eq!(page.records().len(), 1);
            page.records()[0].order()
        })
        .unwrap();
    assert!(first_orders[0] < first_orders[1]);
    assert!(first_orders[1] < final_order);
    assert!(last.continuation().unwrap().is_none());
}

#[test]
fn explicit_retry_replaces_the_head_and_excludes_prior_or_foreign_continuations() {
    let f = Fixture::new(true);
    let source = f.activate();
    for index in 230..233 {
        f.add_command(&source, index);
    }
    let service = f.service(limits(3, 2, 1));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let original = request.open().unwrap();
    let first = original.read_page(None).unwrap();
    let cursor = first.continuation().unwrap().unwrap();
    drop(first);
    f.add_command(&source, 233);
    assert!(matches!(
        original.read_page(Some(&cursor)),
        Err(ActivityReadError::Read(SyndicReadError::StaleActivityQuery))
    ));
    let retry_request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let retry = retry_request.open().unwrap();
    retry
        .with_current_head(|head| assert_eq!(head.logical_row_count(), 4))
        .unwrap();
    assert!(matches!(
        retry.read_page(Some(&cursor)),
        Err(ActivityReadError::InvalidContinuation)
    ));
    let other_request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let other = other_request.open().unwrap();
    assert!(matches!(
        other.read_page(Some(&cursor)),
        Err(ActivityReadError::InvalidContinuation)
    ));
    let page = retry.read_page(None).unwrap();
    assert!(page.continuation().unwrap().is_some());
}

#[test]
fn retained_pages_bound_capacity_and_can_be_replaced_inside_publication() {
    let f = Fixture::new(true);
    let service = f.service(limits(1, 2, 1));
    let request = service
        .prepare_collection(id(30), f.binding.runtime_id())
        .unwrap();
    let collection = request.open().unwrap();
    assert!(matches!(
        service.prepare_collection(id(30), f.binding.runtime_id()),
        Err(ActivityReadError::CollectionCapacity)
    ));
    let old = collection.read_page(None).unwrap();
    let replacement = collection.read_page(None).unwrap();
    assert!(matches!(
        collection.read_page(None),
        Err(ActivityReadError::PageCapacity)
    ));
    replacement.with_current(|_, _| drop(old)).unwrap();
    drop(collection.read_page(None).unwrap());
    drop(collection);
    drop(request);
    assert!(matches!(
        service.prepare_collection(id(30), f.binding.runtime_id()),
        Err(ActivityReadError::CollectionCapacity)
    ));
    drop(replacement);
    drop(
        service
            .prepare_collection(id(30), f.binding.runtime_id())
            .unwrap(),
    );
}
