use super::super::*;

fn runtime_page(request: PickerPageRequest, total: usize) -> PickerRuntimePageOutcome {
    let rows = (request.range.start..request.range.end.min(total))
        .map(|position| PickerRuntimeRow {
            row: row(position),
            browse_roots: PickerCommandState::enabled("Browse roots"),
            add_root: PickerCommandState::enabled("Add root"),
            active_scope: false,
        })
        .collect();
    PickerRuntimePageOutcome::Success(PickerRuntimePage {
        request,
        total_count: total,
        rows,
    })
}

#[test]
fn independent_runtime_pages_preserve_exact_end_focus_and_bounded_residency() {
    let mut runtimes =
        PickerRuntimeCollection::new(PickerCollectionKey("registry".into()), 1, 1_000_000);
    let first = runtimes.request(0).unwrap();
    runtimes.settle(runtime_page(first, 1_000_000), false);
    assert!(runtimes.focus_position(3));
    let (_, request) = runtimes.navigate(PickerNavigation::End, 2);
    let request = request.unwrap();
    assert_eq!(request.range, 999_999..1_000_031);
    assert_eq!(runtimes.focused_key(), Some(&PickerRowKey("row-3".into())));
    assert_eq!(
        runtimes.settle(runtime_page(request, 1_000_000), true),
        Some(999_999)
    );
    for position in (0..1_000_000).step_by(7919) {
        runtimes.retain_window(position..position.saturating_add(12));
        if let Some(request) = runtimes.request(position) {
            runtimes.settle(runtime_page(request, 1_000_000), false);
        }
        assert!(runtimes.resident_row_count() <= PICKER_MAX_RESIDENT_PAGES * PICKER_PAGE_ROWS);
        assert!(runtimes.pending_requests().len() <= 2);
    }
}

#[test]
fn runtime_failure_and_obsolete_success_preserve_coherent_rows_and_focus() {
    let mut runtimes = PickerRuntimeCollection::new(PickerCollectionKey("registry".into()), 1, 100);
    let request = runtimes.request(0).unwrap();
    runtimes.settle(runtime_page(request, 100), false);
    runtimes.focus_position(3);
    let (_, request) = runtimes.navigate(PickerNavigation::End, 2);
    let request = request.unwrap();
    runtimes.settle(
        PickerRuntimePageOutcome::Failed {
            request: request.clone(),
            message: "Retry available".into(),
        },
        true,
    );
    assert_eq!(runtimes.focused_key(), Some(&PickerRowKey("row-3".into())));
    assert!(runtimes.row(3).is_some());
    runtimes.replace(PickerCollectionKey("registry".into()), 2, 100);
    runtimes.settle(runtime_page(request, 100), true);
    assert_eq!(runtimes.focused_key(), Some(&PickerRowKey("row-3".into())));
    assert!(runtimes.row(99).is_none());
}

#[test]
fn exact_failed_range_retry_is_visible_until_success_and_suppresses_duplicates() {
    let mut collection = ready(10_000);
    collection.focus_position(3);
    let (_, failed) = collection.navigate(PickerNavigation::End, 8);
    let failed = failed.unwrap();
    collection.settle(
        PickerPageOutcome::Failed {
            request: failed.clone(),
            message: "Retry".into(),
        },
        true,
    );
    assert_eq!(collection.failed_request(), Some(&failed));
    let retry = collection.retry().unwrap();
    assert_eq!(retry.range, failed.range);
    assert_ne!(retry.request_id, failed.request_id);
    assert!(collection.retry_pending());
    assert!(collection.retry().is_none());
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-3".into()))
    );
    collection.settle(page(retry, 10_000), true);
    assert!(collection.failed_request().is_none());
    assert!(!collection.retry_pending());
    assert_eq!(
        collection.focused_key(),
        Some(&PickerRowKey("row-3".into()))
    );
}

#[test]
fn query_change_obsoletes_failure_and_cancelled_retry_does_not_duplicate_pending_work() {
    let mut collection = ready(100);
    let failed = collection.request(80).unwrap();
    collection.settle(
        PickerPageOutcome::Failed {
            request: failed,
            message: "Retry".into(),
        },
        false,
    );
    let retry = collection.retry().unwrap();
    collection.settle(PickerPageOutcome::Cancelled(retry), false);
    assert!(!collection.retry_pending());
    assert!(collection.retry().is_some());
    collection.replace(PickerCollectionKey("other".into()), 2, 100);
    assert!(collection.failed_request().is_none());
    assert!(collection.retry().is_none());
}

#[test]
fn unrelated_page_success_cannot_clear_named_failure_or_retry_eligibility() {
    let mut collection = ready(10_000);
    let failed = collection.request(96).unwrap();
    let unrelated = collection.request(192).unwrap();
    collection.settle(
        PickerPageOutcome::Failed {
            request: failed.clone(),
            message: "Retry the missing range".into(),
        },
        false,
    );
    collection.settle(page(unrelated, 10_000), false);
    assert_eq!(collection.failed_request(), Some(&failed));
    let retry = collection.retry().unwrap();
    assert_eq!(retry.range, failed.range);
    collection.settle(page(retry, 10_000), false);
    assert!(collection.failed_request().is_none());
}
