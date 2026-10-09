use super::*;

#[test]
fn actual_untitled_source_is_retained_with_the_fenced_transcript_response() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    assert_eq!(
        prepared.resolved_title().source(),
        beryl_state::CatalogTitleSource::Absent
    );
    assert_eq!(prepared.resolved_title().text(), None);
    let identity = prepared.source_identity();
    let called = AtomicBool::new(false);
    fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |_| {
            called.store(true, Ordering::Release)
        })
        .unwrap();
    assert!(called.load(Ordering::Acquire));
    let prepared = fixture
        .reader
        .prepare_attachment(fixture.request(2), &cancel)
        .unwrap();
    assert_eq!(prepared.resolved_title(), &identity.title);
}

#[test]
fn selected_title_completion_does_not_publish_after_a_source_revision_change() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    fixture.seed_head(2);
    let called = AtomicBool::new(false);
    assert_eq!(
        fixture
            .reader
            .publish_if_current(prepared, &request, &cancel, |_| called
                .store(true, Ordering::Release)),
        Err(TranscriptAttachmentError::Stale)
    );
    assert!(!called.load(Ordering::Acquire));
    assert!(
        fixture
            .reader
            .prepare_attachment(fixture.request(2), &cancel)
            .is_ok()
    );
}

#[test]
fn cancelled_selected_title_response_releases_the_single_prepared_slot() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    cancel.store(true, Ordering::Release);
    let called = AtomicBool::new(false);
    assert_eq!(
        fixture
            .reader
            .publish_if_current(prepared, &request, &cancel, |_| called
                .store(true, Ordering::Release)),
        Err(TranscriptAttachmentError::Cancelled)
    );
    assert!(!called.load(Ordering::Acquire));
    assert!(
        fixture
            .reader
            .prepare_attachment(fixture.request(2), &AtomicBool::new(false))
            .is_ok()
    );
}
