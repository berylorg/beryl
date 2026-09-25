use beryl_model::{MonitorId, WindowBounds, WindowId, WindowPlacement};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPlacementRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl WindowPlacementRect {
    pub fn gpui_bounds(
        self,
        scale_factor: f32,
    ) -> Result<gpui::Bounds<gpui::Pixels>, WindowPlacementPreparationError> {
        if !self.valid() || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err(WindowPlacementPreparationError::NativeBounds(
                "logical bounds and scale must be finite with positive dimensions".to_owned(),
            ));
        }
        let scale = f64::from(scale_factor);
        let (x, width) = gpui_axis(self.x, self.width, scale)?;
        let (y, height) = gpui_axis(self.y, self.height, scale)?;
        Ok(gpui::Bounds::new(
            gpui::point(gpui::px(x), gpui::px(y)),
            gpui::size(gpui::px(width), gpui::px(height)),
        ))
    }

    fn from_saved(bounds: WindowBounds) -> Self {
        Self {
            x: f64::from(bounds.x()),
            y: f64::from(bounds.y()),
            width: f64::from(bounds.width()),
            height: f64::from(bounds.height()),
        }
    }

    fn right(self) -> f64 {
        self.x + self.width
    }

    fn bottom(self) -> f64 {
        self.y + self.height
    }

    fn valid(self) -> bool {
        [
            self.x,
            self.y,
            self.width,
            self.height,
            self.right(),
            self.bottom(),
        ]
        .into_iter()
        .all(f64::is_finite)
            && self.width > 0.0
            && self.height > 0.0
            && self.right() > self.x
            && self.bottom() > self.y
    }
}

fn gpui_axis(
    origin: f64,
    extent: f64,
    scale: f64,
) -> Result<(f32, f32), WindowPlacementPreparationError> {
    let physical_start = (origin * scale).round();
    let physical_end = ((origin + extent) * scale).round();
    if !physical_start.is_finite()
        || !physical_end.is_finite()
        || physical_start <= f64::from(i32::MIN)
        || physical_end > f64::from(i32::MAX)
        || physical_end <= physical_start
        || physical_end - physical_start > f64::from(i32::MAX)
    {
        return Err(WindowPlacementPreparationError::NativeBounds(
            "rounded physical coordinates exceed the native position or size range".to_owned(),
        ));
    }
    let nearest_origin = origin as f32;
    let nearest_extent = extent as f32;
    for origin in [
        nearest_origin,
        nearest_origin.next_down(),
        nearest_origin.next_up(),
    ] {
        if !origin.is_finite() || (f64::from(origin) * scale).round() != physical_start {
            continue;
        }
        for extent in [
            nearest_extent,
            nearest_extent.next_down(),
            nearest_extent.next_up(),
        ] {
            if extent.is_finite()
                && extent > 0.0
                && ((f64::from(origin) + f64::from(extent)) * scale).round() == physical_end
            {
                return Ok((origin, extent));
            }
        }
    }
    Err(WindowPlacementPreparationError::NativeBounds(
        "adjacent GPUI coordinates cannot preserve the resolved physical rectangle".to_owned(),
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPlacementMonitor {
    pub id: MonitorId,
    pub work_area: WindowPlacementRect,
}

#[derive(Debug, thiserror::Error)]
pub enum WindowPlacementPreparationError {
    #[error("no usable monitor is available for window placement")]
    NoUsableMonitor,
    #[error("invalid placement facts for monitor {monitor:?}: {reason}")]
    InvalidMonitorFacts {
        monitor: MonitorId,
        reason: &'static str,
    },
    #[error("prepared placement belongs to another window")]
    WindowIdentityChanged,
    #[error("saved placement changed after preparation")]
    SavedPlacementChanged,
    #[error("native monitor discovery failed: {0}")]
    MonitorDiscovery(String),
    #[error("resolved placement cannot be used as native outer bounds: {0}")]
    NativeBounds(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedWindowPlacement {
    window_id: WindowId,
    saved: WindowPlacement,
    monitor_id: MonitorId,
    bounds: WindowPlacementRect,
}

impl ResolvedWindowPlacement {
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub fn saved_placement(&self) -> &WindowPlacement {
        &self.saved
    }

    pub fn monitor_id(&self) -> &MonitorId {
        &self.monitor_id
    }

    pub fn bounds(&self) -> WindowPlacementRect {
        self.bounds
    }

    pub fn validate_binding(
        &self,
        window_id: WindowId,
        saved: &WindowPlacement,
    ) -> Result<(), WindowPlacementPreparationError> {
        if window_id != self.window_id {
            return Err(WindowPlacementPreparationError::WindowIdentityChanged);
        }
        if saved != &self.saved {
            return Err(WindowPlacementPreparationError::SavedPlacementChanged);
        }
        Ok(())
    }
}

pub fn resolve_window_placement(
    window_id: WindowId,
    saved: WindowPlacement,
    monitors: impl IntoIterator<Item = WindowPlacementMonitor>,
) -> Result<ResolvedWindowPlacement, WindowPlacementPreparationError> {
    let mut selection = PlacementSelection::new(window_id, saved);
    for monitor in monitors {
        selection.consider(monitor, ())?;
    }
    selection.finish().map(|(resolved, ())| resolved)
}

struct PlacementSelection<T> {
    window_id: WindowId,
    saved: WindowPlacement,
    winner: Option<PlacementCandidate<T>>,
}

struct PlacementCandidate<T> {
    monitor: WindowPlacementMonitor,
    payload: T,
    saved_identity: bool,
    intersection: f64,
    distance: f64,
}

impl<T> PlacementCandidate<T> {
    fn preference(&self, other: &Self) -> Ordering {
        self.saved_identity
            .cmp(&other.saved_identity)
            .then_with(|| self.intersection.total_cmp(&other.intersection))
            .then_with(|| other.distance.total_cmp(&self.distance))
            .then_with(|| other.monitor.id.cmp(&self.monitor.id))
            .then_with(|| {
                let own = self.monitor.work_area;
                let other = other.monitor.work_area;
                other
                    .x
                    .total_cmp(&own.x)
                    .then_with(|| other.y.total_cmp(&own.y))
                    .then_with(|| other.width.total_cmp(&own.width))
                    .then_with(|| other.height.total_cmp(&own.height))
            })
    }
}

impl<T> PlacementSelection<T> {
    fn new(window_id: WindowId, saved: WindowPlacement) -> Self {
        Self {
            window_id,
            saved,
            winner: None,
        }
    }

    fn consider(
        &mut self,
        monitor: WindowPlacementMonitor,
        payload: T,
    ) -> Result<(), WindowPlacementPreparationError> {
        let invalid = |reason| WindowPlacementPreparationError::InvalidMonitorFacts {
            monitor: monitor.id.clone(),
            reason,
        };
        let work = monitor.work_area;
        if !work.valid() {
            return Err(invalid(
                "work area must have finite coordinates and positive representable dimensions",
            ));
        }
        let saved = WindowPlacementRect::from_saved(self.saved.bounds());
        let intersection_width = (saved.right().min(work.right()) - saved.x.max(work.x)).max(0.0);
        let intersection_height =
            (saved.bottom().min(work.bottom()) - saved.y.max(work.y)).max(0.0);
        let intersection = intersection_width * intersection_height;
        let center_x = (saved.x + saved.width / 2.0) - (work.x + work.width / 2.0);
        let center_y = (saved.y + saved.height / 2.0) - (work.y + work.height / 2.0);
        let distance = center_x * center_x + center_y * center_y;
        if !intersection.is_finite() || !distance.is_finite() {
            return Err(invalid("monitor comparison exceeds finite geometry"));
        }
        let saved_identity = self
            .saved
            .monitor()
            .is_some_and(|hint| hint.id() == &monitor.id);
        let candidate = PlacementCandidate {
            monitor,
            payload,
            saved_identity,
            intersection,
            distance,
        };
        if self
            .winner
            .as_ref()
            .is_none_or(|winner| candidate.preference(winner).is_gt())
        {
            self.winner = Some(candidate);
        }
        Ok(())
    }

    fn finish(self) -> Result<(ResolvedWindowPlacement, T), WindowPlacementPreparationError> {
        let winner = self
            .winner
            .ok_or(WindowPlacementPreparationError::NoUsableMonitor)?;
        let mut bounds = WindowPlacementRect::from_saved(self.saved.bounds());
        let work = winner.monitor.work_area;
        if let Some(hint) = self
            .saved
            .monitor()
            .filter(|hint| hint.id() == &winner.monitor.id)
        {
            bounds.x = work.x + (bounds.x - f64::from(hint.work_area().x()));
            bounds.y = work.y + (bounds.y - f64::from(hint.work_area().y()));
        }
        bounds.width = bounds.width.min(work.width);
        bounds.height = bounds.height.min(work.height);
        let invalid = |reason| WindowPlacementPreparationError::InvalidMonitorFacts {
            monitor: winner.monitor.id.clone(),
            reason,
        };
        if !bounds.x.is_finite() || !bounds.y.is_finite() {
            return Err(invalid("translated saved origin exceeds finite geometry"));
        }
        bounds.x = if bounds.width == work.width {
            work.x
        } else {
            clamp_origin(bounds.x, work.x, work.right(), bounds.width)
        };
        bounds.y = if bounds.height == work.height {
            work.y
        } else {
            clamp_origin(bounds.y, work.y, work.bottom(), bounds.height)
        };
        if !bounds.valid() || bounds.right() > work.right() || bounds.bottom() > work.bottom() {
            return Err(invalid(
                "resolved bounds cannot be represented inside the work area",
            ));
        }
        Ok((
            ResolvedWindowPlacement {
                window_id: self.window_id,
                saved: self.saved,
                monitor_id: winner.monitor.id,
                bounds,
            },
            winner.payload,
        ))
    }
}

fn clamp_origin(preferred: f64, start: f64, end: f64, size: f64) -> f64 {
    let mut maximum = end - size;
    if maximum + size > end {
        maximum = maximum.next_down().min(end.next_down() - size);
    }
    preferred.clamp(start, maximum.max(start))
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
pub struct PreparedWindowsWindowPlacement {
    resolved: ResolvedWindowPlacement,
    monitor: gpui::WindowsWindowPlacementMonitor,
}

#[cfg(target_os = "windows")]
impl PreparedWindowsWindowPlacement {
    pub fn resolved(&self) -> &ResolvedWindowPlacement {
        &self.resolved
    }

    pub fn monitor(&self) -> gpui::WindowsWindowPlacementMonitor {
        self.monitor
    }

    pub fn gpui_window_bounds(
        &self,
    ) -> Result<gpui::WindowBounds, WindowPlacementPreparationError> {
        let resolved = self.resolved.bounds();
        let bounds = resolved.gpui_bounds(self.monitor.scale_factor())?;
        let physical = self
            .monitor
            .outer_placement(bounds, false)
            .map_err(|error| WindowPlacementPreparationError::NativeBounds(error.to_string()))?
            .screen_bounds();
        let scale = f64::from(self.monitor.scale_factor());
        let expected = [resolved.x, resolved.y, resolved.right(), resolved.bottom()]
            .map(|value| (value * scale).round());
        let actual = [
            i64::from(physical.origin.x.0),
            i64::from(physical.origin.y.0),
            i64::from(physical.origin.x.0) + i64::from(physical.size.width.0),
            i64::from(physical.origin.y.0) + i64::from(physical.size.height.0),
        ];
        if actual
            .into_iter()
            .zip(expected)
            .any(|(actual, expected)| actual as f64 != expected)
        {
            return Err(WindowPlacementPreparationError::NativeBounds(
                "GPUI coordinate precision changes the resolved physical rectangle".to_owned(),
            ));
        }
        let work = self.monitor.work_area();
        if actual[0] < i64::from(work.origin.x.0)
            || actual[1] < i64::from(work.origin.y.0)
            || actual[2] > i64::from(work.origin.x.0) + i64::from(work.size.width.0)
            || actual[3] > i64::from(work.origin.y.0) + i64::from(work.size.height.0)
            || actual[0] == i64::from(i32::MIN)
            || actual[1] == i64::from(i32::MIN)
        {
            return Err(WindowPlacementPreparationError::NativeBounds(
                "physical outer bounds do not fit the prepared work area or native position range"
                    .to_owned(),
            ));
        }
        Ok(match self.resolved.saved_placement().display_state() {
            beryl_model::WindowDisplayState::Normal => gpui::WindowBounds::Windowed(bounds),
            beryl_model::WindowDisplayState::Maximized => gpui::WindowBounds::Maximized(bounds),
        })
    }

    pub fn validate_binding(
        &self,
        window_id: WindowId,
        saved: &WindowPlacement,
    ) -> Result<(), WindowPlacementPreparationError> {
        self.resolved.validate_binding(window_id, saved)
    }
}

#[cfg(target_os = "windows")]
pub fn prepare_windows_window_placement(
    window_id: WindowId,
    saved: WindowPlacement,
) -> Result<PreparedWindowsWindowPlacement, WindowPlacementPreparationError> {
    let mut selection = PlacementSelection::new(window_id, saved);
    gpui::WindowsWindowPlacementMonitor::visit(|monitor| {
        let scale = f64::from(monitor.scale_factor());
        let work = monitor.work_area();
        let id = MonitorId::new(monitor.uuid().to_string()).map_err(|error| {
            WindowPlacementPreparationError::MonitorDiscovery(error.to_string())
        })?;
        selection.consider(
            WindowPlacementMonitor {
                id,
                work_area: WindowPlacementRect {
                    x: f64::from(work.origin.x.0) / scale,
                    y: f64::from(work.origin.y.0) / scale,
                    width: f64::from(work.size.width.0) / scale,
                    height: f64::from(work.size.height.0) / scale,
                },
            },
            monitor,
        )?;
        Ok(std::ops::ControlFlow::Continue(()))
    })
    .map_err(
        |error| match error.downcast::<WindowPlacementPreparationError>() {
            Ok(error) => error,
            Err(error) => WindowPlacementPreparationError::MonitorDiscovery(error.to_string()),
        },
    )?;
    let (resolved, monitor) = selection.finish()?;
    Ok(PreparedWindowsWindowPlacement { resolved, monitor })
}
