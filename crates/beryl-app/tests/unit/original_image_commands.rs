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

fn limits() -> MediaResourceLimits {
    MediaResourceLimits::new(crate::image_processing::MediaResourceConfiguration::default())
        .unwrap()
}

#[test]
fn copy_preflight_charges_owned_capacity_native_backing_and_companion() {
    let mut config = limits().configuration();
    config.original_clipboard_total_bytes = 64;
    assert_eq!(preparation::copy_capacity(20, 20, config), Ok(20));
    assert_eq!(
        preparation::copy_capacity(20, 21, config),
        Err(OriginalImageFailure::Capacity)
    );
    assert_eq!(
        preparation::copy_capacity(21, 21, config),
        Err(OriginalImageFailure::Capacity)
    );
    assert_eq!(
        preparation::copy_capacity(u64::MAX, usize::MAX, config),
        Err(OriginalImageFailure::Capacity)
    );
    config.original_clipboard_metadata_bytes = 23;
    assert_eq!(
        preparation::copy_capacity(1, 1, config),
        Err(OriginalImageFailure::Capacity)
    );
}

#[test]
fn clipboard_formats_are_independent_of_png_rendition_support() {
    for (media_type, format) in [
        ("image/png", gpui::ImageFormat::Png),
        ("image/gif", gpui::ImageFormat::Gif),
        ("image/jpeg", gpui::ImageFormat::Jpeg),
        ("image/svg+xml", gpui::ImageFormat::Svg),
    ] {
        assert_eq!(preparation::copy_format(media_type), Ok(format));
    }
    for media_type in ["image/webp", "image/bmp", "image/tiff", "text/plain"] {
        assert_eq!(
            preparation::copy_format(media_type),
            Err(OriginalImageFailure::Unsupported)
        );
    }
}

#[test]
fn capacity_broadcast_preserves_all_live_waiters_and_progress_before_poll() {
    let event = Arc::new(OriginalCapacityEvent::new(limits()));
    let mut first = event.unbound_changed().unwrap();
    let mut second = event.unbound_changed().unwrap();
    let first_count = Arc::new(WakeCount::default());
    let second_count = Arc::new(WakeCount::default());
    let first_waker = Waker::from(first_count.clone());
    let second_waker = Waker::from(second_count.clone());
    assert_eq!(
        Pin::new(&mut first).poll(&mut Context::from_waker(&first_waker)),
        Poll::Pending
    );
    assert_eq!(
        Pin::new(&mut second).poll(&mut Context::from_waker(&second_waker)),
        Poll::Pending
    );
    event.notify();
    assert_eq!(first_count.0.load(Ordering::Acquire), 1);
    assert_eq!(second_count.0.load(Ordering::Acquire), 1);
    drop(first);
    assert_eq!(
        Pin::new(&mut second).poll(&mut Context::from_waker(&second_waker)),
        Poll::Ready(())
    );
    let mut before_poll = event.unbound_changed().unwrap();
    event.notify();
    assert_eq!(
        Pin::new(&mut before_poll).poll(&mut Context::from_waker(&first_waker)),
        Poll::Ready(())
    );
    let mut rearmed = event.unbound_changed().unwrap();
    assert_eq!(
        Pin::new(&mut rearmed).poll(&mut Context::from_waker(&first_waker)),
        Poll::Pending
    );
    drop(second);
    event.notify();
    assert_eq!(first_count.0.load(Ordering::Acquire), 2);
}

#[test]
fn capacity_waiters_refuse_excess_and_dropping_one_releases_only_its_admission() {
    let mut config = limits().configuration();
    config.window_pin_entries = 1;
    let event = Arc::new(OriginalCapacityEvent::new(
        MediaResourceLimits::new(config).unwrap(),
    ));
    let mut waiters = (0..beryl_state::MAX_RESTORABLE_WINDOWS)
        .map(|_| event.unbound_changed().unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        event.unbound_changed(),
        Err(ImageMediaError(ImageMediaFailure::Capacity))
    ));
    drop(waiters.pop());
    let replacement = event.unbound_changed().unwrap();
    assert!(matches!(
        event.unbound_changed(),
        Err(ImageMediaError(ImageMediaFailure::Capacity))
    ));
    drop(replacement);
    drop(waiters);
    assert!(event.unbound_changed().is_ok());
}

#[cfg(all(target_os = "windows", feature = "test-faults"))]
#[path = "original_image_commands/native.rs"]
mod native;
