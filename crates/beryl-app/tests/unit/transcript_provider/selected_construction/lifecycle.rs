use super::*;

#[test]
fn cancellation_capacity_and_closed_service_fence_new_selected_construction() {
    let fixture = Fixture::new();
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    let revision = fixture.home.home_revision().unwrap();
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(true)),
        Err(SelectedTranscriptConstructionError::Cancelled)
    ));
    let release = fixture
        .service
        .saturate_scheduled_ordinary_capacity_for_test();
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)),
        Err(SelectedTranscriptConstructionError::Busy)
    ));
    release();
    assert!(build(&fixture).is_none());
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    quiescent(&fixture);
    assert!(fixture.service.close().is_ok());
    assert!(
        matches!(authority.step(child(), &AtomicBool::new(false)), Err(SelectedTranscriptConstructionError::Admission(error)) if matches!(error.as_ref(), crate::cas_projection::LiveCommandAdmissionError::Closed))
    );
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn cancellation_before_admission_preserves_the_unstarted_build() {
    let fixture = Fixture::new();
    history::create_inherited_discussion(&fixture, 1);
    let authority = fixture.service.selected_transcript_construction_authority();
    let cancellation = AtomicBool::new(false);
    let revision = fixture.home.home_revision().unwrap();
    assert!(matches!(
        authority.step_after_observation(child(), &cancellation, || cancellation
            .store(true, Ordering::Release)),
        Err(SelectedTranscriptConstructionError::Cancelled)
    ));
    assert!(build(&fixture).is_none());
    assert_eq!(retained_count(&authority), 0);
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    quiescent(&fixture);
}

fn published(
    fixture: &Fixture,
    lifetime: &Arc<()>,
) -> crate::app_services::PublishedRunningThreadsReader {
    let (_, sessions) = crate::cas_projection::ProcessScheduledExecutionProvider::new();
    let attention = Arc::new(crate::lifecycle_attention::ProcessLifecycleAttentionPool::new());
    crate::app_services::PublishedRunningThreadsReader::for_test(
        &fixture.service,
        &sessions,
        fixture.state.clone(),
        fixture.home.clone(),
        fixture.storage.clone(),
        &attention,
        lifetime,
    )
}

#[test]
fn published_running_reader_constructs_on_background_refresh_and_fences_retired_delivery() {
    let fixture = Fixture::new();
    history::create_inherited_discussion(&fixture, 1);
    let lifetime = Arc::new(());
    let reader = published(&fixture, &lifetime);
    let worker = reader.clone();
    assert!(matches!(
        std::thread::spawn(
            move || worker.step_selected_transcript(child(), &AtomicBool::new(false))
        )
        .join()
        .unwrap()
        .unwrap(),
        SelectedTranscriptConstructionProgress::Current
    ));
    let prepared = fixture
        .reader
        .prepare_attachment(request(&fixture, 1), &AtomicBool::new(false))
        .unwrap();
    assert!(prepared.activation().view_id.is_some());
    assert_eq!(
        prepared
            .activation()
            .discussion_context
            .as_ref()
            .unwrap()
            .provenance
            .insertion_after,
        2
    );
    let revision = fixture.home.home_revision().unwrap();
    drop(lifetime);
    assert!(matches!(
        reader.step_selected_transcript(child(), &AtomicBool::new(false)),
        Err(SelectedTranscriptConstructionError::Unavailable)
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert!(!fixture.reader.retire());
    let called = AtomicBool::new(false);
    assert!(matches!(
        fixture.reader.publish_if_current(
            prepared,
            &request(&fixture, 1),
            &AtomicBool::new(false),
            |_| called.store(true, Ordering::Release)
        ),
        Err(TranscriptAttachmentError::Retired)
    ));
    assert!(!called.load(Ordering::Acquire));
    assert!(fixture.reader.retire());
    quiescent(&fixture);
}

#[test]
fn published_reader_retirement_during_admitted_write_settles_then_fences_the_next_command() {
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(faults.clone());
    history::create_inherited_discussion(&fixture, 1);
    let lifetime = Arc::new(());
    let reader = published(&fixture, &lifetime);
    let worker = reader.clone();
    let revision = fixture.home.home_revision().unwrap();
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let worker = std::thread::spawn(move || {
        worker.step_selected_transcript(child(), &AtomicBool::new(false))
    });
    let reached = block.wait_until_reached(std::time::Duration::from_secs(10));
    if !reached {
        block.release();
    }
    assert!(reached);
    drop(lifetime);
    block.release();
    assert!(matches!(
        worker.join().unwrap(),
        Err(SelectedTranscriptConstructionError::Unavailable)
    ));
    assert_eq!(
        fixture.home.home_revision().unwrap().get(),
        revision.get() + 1
    );
    assert!(matches!(
        build(&fixture).unwrap().phase(),
        TranscriptBuildPhase::Collecting { .. }
    ));
    assert!(matches!(
        reader.step_selected_transcript(child(), &AtomicBool::new(false)),
        Err(SelectedTranscriptConstructionError::Unavailable)
    ));
    quiescent(&fixture);
}
