use beryl_model::{
    MonitorHint, MonitorId, VirtualDesktopId, WindowBounds, WindowDisplayState, WindowPlacement,
};
use gpui::{Bounds, DevicePixels, WindowsCapturedWindowPlacement};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum WindowPlacementCaptureError {
    #[error("captured placement scale must be finite and positive")]
    InvalidScale,
    #[error("captured placement dimensions must be positive")]
    InvalidDimensions,
    #[error("captured placement cannot fit the saved logical coordinate range")]
    LogicalBoundsOutOfRange,
    #[error("captured monitor identity is invalid: {0}")]
    MonitorIdentity(String),
}

pub fn physical_window_bounds_to_saved(
    bounds: Bounds<DevicePixels>,
    scale: f32,
) -> Result<WindowBounds, WindowPlacementCaptureError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(WindowPlacementCaptureError::InvalidScale);
    }
    if bounds.size.width.0 <= 0 || bounds.size.height.0 <= 0 {
        return Err(WindowPlacementCaptureError::InvalidDimensions);
    }
    let [x, y, width, height] = [
        bounds.origin.x.0,
        bounds.origin.y.0,
        bounds.size.width.0,
        bounds.size.height.0,
    ]
    .map(|value| (f64::from(value) / f64::from(scale)).round());
    if ![x, y, width, height].into_iter().all(f64::is_finite)
        || x < f64::from(i32::MIN)
        || x > f64::from(i32::MAX)
        || y < f64::from(i32::MIN)
        || y > f64::from(i32::MAX)
        || width < 1.0
        || width > f64::from(u32::MAX)
        || height < 1.0
        || height > f64::from(u32::MAX)
    {
        return Err(WindowPlacementCaptureError::LogicalBoundsOutOfRange);
    }
    WindowBounds::new(x as i32, y as i32, width as u32, height as u32)
        .map_err(|_| WindowPlacementCaptureError::InvalidDimensions)
}

pub fn windows_window_placement_from_capture(
    captured: WindowsCapturedWindowPlacement,
    virtual_desktop: Option<VirtualDesktopId>,
) -> Result<WindowPlacement, WindowPlacementCaptureError> {
    let monitor = captured.monitor();
    let bounds =
        physical_window_bounds_to_saved(captured.normal_outer_bounds(), monitor.scale_factor())?;
    let work_area = physical_window_bounds_to_saved(monitor.work_area(), monitor.scale_factor())?;
    let id = MonitorId::new(monitor.uuid().to_string())
        .map_err(|error| WindowPlacementCaptureError::MonitorIdentity(error.to_string()))?;
    Ok(WindowPlacement::new(
        bounds,
        if captured.maximized() {
            WindowDisplayState::Maximized
        } else {
            WindowDisplayState::Normal
        },
        Some(MonitorHint::new(id, work_area)),
        virtual_desktop,
    ))
}
