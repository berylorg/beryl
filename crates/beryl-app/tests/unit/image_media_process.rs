use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Wake, Waker},
};

use super::*;

#[derive(Default)]
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

#[test]
fn interest_change_wakes_once_and_rearms_without_lost_notifications() {
    let event = Arc::new(signal::ChangeEvent::default());
    let wake = Arc::new(WakeCount::default());
    let waker = Waker::from(wake.clone());
    let mut context = Context::from_waker(&waker);
    let mut changed = event.changed(false);
    assert!(matches!(
        Pin::new(&mut changed).poll(&mut context),
        Poll::Pending
    ));
    event.notify();
    assert_eq!(wake.0.load(Ordering::Acquire), 1);
    assert!(matches!(
        Pin::new(&mut changed).poll(&mut context),
        Poll::Ready(())
    ));
    let mut changed = event.changed(false);
    event.notify();
    assert!(matches!(
        Pin::new(&mut changed).poll(&mut context),
        Poll::Ready(())
    ));
    let mut changed = event.changed(false);
    assert!(matches!(
        Pin::new(&mut changed).poll(&mut context),
        Poll::Pending
    ));
    event.notify();
    assert_eq!(wake.0.load(Ordering::Acquire), 2);
}

#[test]
fn completion_notification_does_not_retain_interest_or_source_custody() {
    let event = Arc::new(signal::ChangeEvent::default());
    let weak = Arc::downgrade(&event);
    let mut changed = event.changed(true);
    drop(event);
    let waker = Waker::from(Arc::new(WakeCount::default()));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(
        Pin::new(&mut changed).poll(&mut context),
        Poll::Ready(())
    ));
    drop(changed);
    assert!(weak.upgrade().is_none());
}

#[test]
fn native_retry_preserves_progress_before_failure_and_ignores_unrelated_failure_wakes() {
    let event = Arc::new(signal::ChangeEvent::default());
    let progress = Arc::new(signal::NativeProgress::default());
    let requested = progress.revision();
    let cancellation = CommandCancellation::new();
    let waker = Waker::from(Arc::new(WakeCount::default()));
    let mut context = Context::from_waker(&waker);
    let mut pending = event.native_retry_changed(progress.clone(), requested, cancellation.clone());
    assert!(matches!(
        Pin::new(&mut pending).poll(&mut context),
        Poll::Pending
    ));
    event.notify();
    assert!(matches!(
        Pin::new(&mut pending).poll(&mut context),
        Poll::Pending
    ));
    progress.advance().unwrap();
    event.notify();
    assert!(matches!(
        Pin::new(&mut pending).poll(&mut context),
        Poll::Ready(())
    ));
    let mut observed_after_failure =
        event.native_retry_changed(progress.clone(), requested, cancellation);
    assert!(matches!(
        Pin::new(&mut observed_after_failure).poll(&mut context),
        Poll::Ready(())
    ));
    let mut fresh_request = event.native_retry_changed(
        progress.clone(),
        progress.revision(),
        CommandCancellation::new(),
    );
    assert!(matches!(
        Pin::new(&mut fresh_request).poll(&mut context),
        Poll::Pending
    ));
}

#[test]
fn native_progress_exhaustion_never_wraps_and_releases_the_existing_retry_waiter() {
    let event = Arc::new(signal::ChangeEvent::default());
    let progress = Arc::new(signal::NativeProgress::at_revision_for_tests(u64::MAX));
    let mut changed =
        event.native_retry_changed(progress.clone(), u64::MAX, CommandCancellation::new());
    let waker = Waker::from(Arc::new(WakeCount::default()));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(
        Pin::new(&mut changed).poll(&mut context),
        Poll::Pending
    ));
    assert!(matches!(
        progress.advance(),
        Err(ImageMediaError(ImageMediaFailure::Capacity))
    ));
    event.notify();
    assert!(progress.is_exhausted());
    assert_eq!(progress.revision(), u64::MAX);
    assert!(matches!(
        Pin::new(&mut changed).poll(&mut context),
        Poll::Ready(())
    ));
    assert!(progress.advance().is_err());
    assert_eq!(progress.revision(), u64::MAX);
}

#[cfg(target_os = "windows")]
fn with_native_core(run: impl FnOnce(Arc<ProcessCore>) + 'static) {
    use std::{cell::Cell, rc::Rc};
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    gpui::Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let limits =
                MediaResourceLimits::new(super::super::MediaResourceConfiguration::default())
                    .unwrap();
            let config = limits.configuration();
            let domain = app
                .image_resource_domain(gpui::ImageResourceDomainConfig {
                    staging_buffers: config.upload_staging_buffers,
                    staging_bytes: config.upload_staging_bytes,
                    texture_entries: config.gpu_texture_entries,
                    texture_bytes: config.gpu_texture_bytes as u64,
                    window_pin_entries: config.window_pin_entries,
                    window_pin_bytes: config.window_pin_gpu_bytes as u64,
                })
                .unwrap();
            let core = Arc::new(ProcessCore {
                generated_capacity: Arc::new(signal::ChangeEvent::default()),
                original_capacity: Arc::new(original::OriginalCapacityEvent::new(limits)),
                native_progress: Arc::new(signal::NativeProgress::default()),
                domain,
                limits,
                cpu: CpuSurfaceCache::new(limits),
                state: Mutex::new(ProcessState::default()),
                work: Condvar::new(),
                event: Arc::new(signal::ChangeEvent::default()),
            });
            run(core);
            done.set(true);
            app.quit();
        });
    assert!(completed.get());
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_shared_flight_keeps_staging_until_the_last_live_interest_cancels() {
    with_native_core(|core| {
        let consumer = core.domain.begin_consumer().unwrap();
        let identity = RenditionIdentity {
            attachment: 1,
            asset: beryl_model::AssetId::sha256_v1([17; 32], std::num::NonZeroU64::MIN),
            extent: [2, 2],
        };
        let layout = gpui::Rgba8ImageLayout::new(2, 2, 8).unwrap();
        let gpui::ImageUploadAdmission::Upload(ticket) = consumer
            .reserve_upload(identity.native_key(), layout, 16)
            .unwrap()
        else {
            panic!("new flight needs one ticket")
        };
        let native_cancellation = ticket.cancellation();
        let cancellation = CommandCancellation::new();
        let interest = |id| {
            Arc::new(InterestState {
                id,
                native_revision: core.native_progress.revision(),
                window: Weak::new(),
                core: Arc::downgrade(&core),
                cancellation: CommandCancellation::new(),
                status: Mutex::new(InterestStatus::Prepared(identity)),
                event: Arc::new(signal::ChangeEvent::default()),
            })
        };
        let first = interest(1);
        let second = interest(2);
        lock(&core.state).flights.insert(
            identity,
            Flight {
                worker: 1,
                cancellation: cancellation.clone(),
                upload_cancellation: Some(native_cancellation.clone()),
                interests: BTreeMap::from([
                    (1, Arc::downgrade(&first)),
                    (2, Arc::downgrade(&second)),
                ]),
                phase: FlightPhase::Prepared(ticket),
            },
        );
        presentation::cancel_interest(&first, ImageMediaFailure::Cancelled);
        assert!(!cancellation.is_cancelled());
        assert!(!native_cancellation.is_cancelled());
        assert_eq!(core.domain.diagnostics().staging_buffers, 1);
        assert_eq!(lock(&core.state).flights.len(), 1);
        presentation::cancel_interest(&second, ImageMediaFailure::Cancelled);
        assert!(cancellation.is_cancelled());
        assert!(native_cancellation.is_cancelled());
        assert_eq!(core.domain.diagnostics().staging_buffers, 0);
        assert_eq!(core.domain.diagnostics().reserved_textures, 0);
        assert!(lock(&core.state).flights.is_empty());
        assert_eq!(consumer.retire(), ImageResourceDrain::Drained);
    });
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires an announced native GPUI qualification run"]
fn native_source_stage_leases_refuse_excess_and_release_both_configured_slots() {
    with_native_core(|core| {
        let first = worker::SourceStageLease::acquire(&core).unwrap();
        let second = worker::SourceStageLease::acquire(&core).unwrap();
        assert!(matches!(
            worker::SourceStageLease::acquire(&core),
            Err(ImageMediaError(ImageMediaFailure::Capacity))
        ));
        assert_eq!(lock(&core.state).path_slots, 2);
        assert_eq!(lock(&core.state).verification_slots, 2);
        drop(first);
        let third = worker::SourceStageLease::acquire(&core).unwrap();
        drop(second);
        drop(third);
        assert_eq!(lock(&core.state).path_slots, 0);
        assert_eq!(lock(&core.state).verification_slots, 0);
    });
}
