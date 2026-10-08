use beryl_app::thread_root_picker::*;

#[path = "thread_root_picker/mod.rs"]
mod picker_support;

fn row(position: usize) -> PickerRow {
    PickerRow {
        key: PickerRowKey(format!("row-{position}")),
        primary: format!("Title {position}"),
        secondary: "metadata".into(),
        status: "RUNNING".into(),
        tooltip: None,
        unavailable_reason: None,
        current: false,
        activation_pending: false,
    }
}

fn page(request: PickerPageRequest, total: usize) -> PickerPageOutcome {
    let rows = (request.range.start..request.range.end.min(total))
        .map(row)
        .collect();
    PickerPageOutcome::Success(PickerPage {
        request,
        total_count: total,
        rows,
    })
}

fn ready(total: usize) -> PickerCollection {
    let mut collection =
        PickerCollection::new(PickerCollectionKey("opaque-collection".into()), 1, total);
    let request = collection.request(0).expect("first request");
    collection.settle(page(request, total), false);
    collection
}

#[test]
fn an_admitted_empty_collection_stops_discovery_until_the_query_revision_changes() {
    let mut collection = ready(0);
    for _ in 0..64 {
        assert!(collection.request(0).is_none());
        assert!(collection.request(32).is_none());
    }
    assert!(collection.pending_requests().is_empty());
    assert_eq!(collection.resident_page_count(), 1);
    collection.replace(PickerCollectionKey("opaque-collection".into()), 2, 0);
    let request = collection.request(0).expect("new query discovery");
    collection.settle(page(request, 0), false);
    assert!(collection.request(0).is_none());
    assert!(collection.pending_requests().is_empty());
}

#[test]
fn exact_end_loads_only_target_page_and_preserves_actual_focus_until_settlement() {
    let mut collection = ready(1_000_000);
    assert!(collection.focus_position(3));
    let (reveal, request) = collection.navigate(PickerNavigation::End, 8);
    assert_eq!(reveal, None);
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-3".into()))
    );
    let request = request.expect("target page request");
    assert_eq!(request.range, 999_999..1_000_031);
    assert_eq!(collection.pending_requests().len(), 1);
    assert_eq!(
        collection.settle(page(request, 1_000_000), true),
        Some(999_999)
    );
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-999999".into()))
    );
}

#[test]
fn later_navigation_and_focus_departure_retire_end_intent_without_late_focus() {
    let mut collection = ready(1_000_000);
    collection.focus_position(2);
    let (_, end_request) = collection.navigate(PickerNavigation::End, 8);
    let end_request = end_request.unwrap();
    let (reveal, _) = collection.navigate(PickerNavigation::Home, 8);
    assert_eq!(reveal, Some(0));
    assert_eq!(collection.settle(page(end_request, 1_000_000), true), None);
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-0".into()))
    );
    let (_, target) = collection.navigate(PickerNavigation::PageDown, 800);
    collection.cancel_navigation();
    assert_eq!(
        collection.settle(page(target.unwrap(), 1_000_000), true),
        None
    );
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-0".into()))
    );
}

#[test]
fn revision_and_exact_request_identity_reject_success_and_failure_replay() {
    let mut collection = ready(1000);
    collection.focus_position(4);
    let old = collection.request(96).unwrap();
    collection.replace(PickerCollectionKey("opaque-collection".into()), 2, 1000);
    let current = collection.request(96).unwrap();
    assert_ne!(old.request_id, current.request_id);
    assert_eq!(collection.settle(page(old.clone(), 1000), true), None);
    collection.settle(
        PickerPageOutcome::Failed {
            request: old,
            message: "obsolete".into(),
        },
        true,
    );
    assert_eq!(collection.pending_requests(), &[current.clone()]);
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-4".into()))
    );
    collection.settle(page(current.clone(), 1000), false);
    collection.settle(
        PickerPageOutcome::Failed {
            request: current,
            message: "replayed".into(),
        },
        false,
    );
    assert!(collection.is_current());
}

#[test]
fn failure_and_malformed_page_preserve_count_and_focus() {
    let mut collection = ready(10_000);
    collection.focus_position(5);
    let (_, request) = collection.navigate(PickerNavigation::End, 8);
    collection.settle(
        PickerPageOutcome::Failed {
            request: request.unwrap(),
            message: "Page unavailable".into(),
        },
        true,
    );
    assert!(collection.pending_requests().is_empty());
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-5".into()))
    );
    let request = collection.request(64).unwrap();
    collection.settle(
        PickerPageOutcome::Success(PickerPage {
            request,
            total_count: 1,
            rows: vec![row(0)],
        }),
        true,
    );
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-5".into()))
    );
    let (_, request) = collection.navigate(PickerNavigation::End, 8);
    assert_eq!(request.unwrap().range, 9999..10031);
}

#[test]
fn disabled_and_pending_rows_remain_focusable_without_activation() {
    let mut collection = PickerCollection::new(PickerCollectionKey("opaque".into()), 1, 2);
    let request = collection.request(0).unwrap();
    let mut unavailable = row(0);
    unavailable.unavailable_reason = Some("Unavailable".into());
    let mut pending = row(1);
    pending.activation_pending = true;
    collection.settle(
        PickerPageOutcome::Success(PickerPage {
            request,
            total_count: 2,
            rows: vec![unavailable.clone(), pending.clone()],
        }),
        true,
    );
    assert!(collection.focus_position(0));
    assert_eq!(collection.activation(&unavailable.key), None);
    assert!(collection.focus_position(1));
    assert_eq!(collection.activation(&pending.key), None);
}

#[test]
fn arbitrary_scroll_positions_keep_residency_and_pending_demands_bounded() {
    let mut collection = ready(1_000_000);
    for position in (0..1_000_000).step_by(8191) {
        if let Some(request) = collection.request(position) {
            collection.settle(page(request, 1_000_000), false);
        }
        assert!(collection.pending_requests().len() <= 2);
        assert!(collection.resident_page_count() <= PICKER_MAX_RESIDENT_PAGES);
        assert!(collection.resident_row_count() <= PICKER_MAX_RESIDENT_PAGES * PICKER_PAGE_ROWS);
        assert!(collection.row(position).is_some());
    }
    let first = collection.request(0).unwrap();
    let _ = collection.request(96);
    let _ = collection.request(192);
    assert_eq!(collection.settle(page(first, 1_000_000), true), None);
    assert!(collection.pending_requests().len() <= 2);
}

#[test]
fn byte_capped_partial_page_requests_missing_rank_without_repeating_aligned_prefix() {
    let mut collection = PickerCollection::new(PickerCollectionKey("opaque".into()), 1, 100);
    let request = collection.request(0).unwrap();
    collection.settle(
        PickerPageOutcome::Success(PickerPage {
            request,
            total_count: 100,
            rows: vec![row(0), row(1)],
        }),
        false,
    );
    assert!(collection.row(2).is_none());
    let request = collection.request(2).unwrap();
    assert_eq!(request.range, 2..34);
    collection.settle(page(request, 100), false);
    assert_eq!(collection.total_count(), 100);
    assert_eq!(collection.resident_page_count(), 2);
    assert_eq!(collection.resident_row_count(), 34);
}

#[test]
fn reorder_restores_exact_stable_identity_at_owner_supplied_rank() {
    let mut collection = ready(1000);
    collection.focus_position(5);
    collection.replace(PickerCollectionKey("opaque-collection".into()), 2, 1000);
    let (_, request) = collection.restore_focus_position(994);
    let request = request.unwrap();
    let mut rows: Vec<_> = (request.range.start..1000).map(row).collect();
    rows[994 - request.range.start] = row(5);
    assert_eq!(
        collection.settle(
            PickerPageOutcome::Success(PickerPage {
                request,
                total_count: 1000,
                rows
            }),
            true
        ),
        Some(994)
    );
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-5".into()))
    );
}

#[test]
fn cancelled_navigation_and_empty_collection_never_create_activation_or_focus() {
    let mut collection = ready(1000);
    collection.focus_position(1);
    let (_, request) = collection.navigate(PickerNavigation::End, 8);
    collection.settle(PickerPageOutcome::Cancelled(request.unwrap()), true);
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-1".into()))
    );
    let mut empty = ready(0);
    assert_eq!(empty.navigate(PickerNavigation::End, 8), (None, None));
}
