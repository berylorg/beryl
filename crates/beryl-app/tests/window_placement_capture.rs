#![cfg(target_os = "windows")]

use beryl_app::main_window::{WindowPlacementCaptureError, physical_window_bounds_to_saved};
use gpui::{Bounds, DevicePixels, point, size};

fn bounds(x: i32, y: i32, width: i32, height: i32) -> Bounds<DevicePixels> {
    Bounds::new(
        point(DevicePixels(x), DevicePixels(y)),
        size(DevicePixels(width), DevicePixels(height)),
    )
}

#[test]
fn physical_geometry_uses_each_captured_scale_and_preserves_negative_origins() {
    for (scale, expected) in [
        (1.0, (-1500, 45, 1200, 900)),
        (1.25, (-1200, 36, 960, 720)),
        (1.5, (-1000, 30, 800, 600)),
        (2.0, (-750, 23, 600, 450)),
    ] {
        let saved = physical_window_bounds_to_saved(bounds(-1500, 45, 1200, 900), scale).unwrap();
        assert_eq!(
            (saved.x(), saved.y(), saved.width(), saved.height()),
            expected
        );
    }
}

#[test]
fn rounds_components_independently_with_halfway_values_away_from_zero() {
    let saved = physical_window_bounds_to_saved(bounds(-3, 3, 3, 5), 2.0).unwrap();
    assert_eq!(
        (saved.x(), saved.y(), saved.width(), saved.height()),
        (-2, 2, 2, 3)
    );
    let saved = physical_window_bounds_to_saved(bounds(-1, 1, 1, 1), 1.5).unwrap();
    assert_eq!(
        (saved.x(), saved.y(), saved.width(), saved.height()),
        (-1, 1, 1, 1)
    );
}

#[test]
fn rejects_invalid_scale_and_dimensions_without_saturating() {
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            physical_window_bounds_to_saved(bounds(0, 0, 800, 600), scale),
            Err(WindowPlacementCaptureError::InvalidScale)
        );
    }
    for rect in [
        bounds(0, 0, 0, 1),
        bounds(0, 0, 1, 0),
        bounds(0, 0, -1, 1),
        bounds(0, 0, 1, -1),
    ] {
        assert_eq!(
            physical_window_bounds_to_saved(rect, 1.0),
            Err(WindowPlacementCaptureError::InvalidDimensions)
        );
    }
    for (rect, scale) in [
        (bounds(i32::MAX, 0, 1, 1), 0.5),
        (bounds(0, i32::MIN, 1, 1), 0.5),
        (bounds(0, 0, i32::MAX, 1), 0.25),
        (bounds(0, 0, 1, i32::MAX), 0.25),
        (bounds(0, 0, 1, 800), 3.0),
        (bounds(0, 0, 800, 1), 3.0),
        (bounds(0, 0, 800, 600), f32::MAX),
        (bounds(0, 0, 800, 600), f32::MIN_POSITIVE),
    ] {
        assert_eq!(
            physical_window_bounds_to_saved(rect, scale),
            Err(WindowPlacementCaptureError::LogicalBoundsOutOfRange)
        );
    }
}

#[test]
fn accepts_durable_signed_origins_and_unsigned_dimensions_at_their_limits() {
    let saved =
        physical_window_bounds_to_saved(bounds(i32::MIN, i32::MAX, i32::MAX, 1), 1.0).unwrap();
    assert_eq!((saved.x(), saved.y()), (i32::MIN, i32::MAX));
    assert_eq!(saved.width(), i32::MAX as u32);
    let saved = physical_window_bounds_to_saved(bounds(0, 0, i32::MAX, i32::MAX), 0.5).unwrap();
    assert_eq!(
        (saved.width(), saved.height()),
        (u32::MAX - 1, u32::MAX - 1)
    );
}
