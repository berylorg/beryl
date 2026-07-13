use std::{error::Error, fmt, num::NonZeroU32};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{ValueError, VirtualDesktopId, runtime::bounded_text};

const MAX_MONITOR_ID_BYTES: usize = 512;

/// Stable platform monitor hint retained for best-effort restoration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct MonitorId(Box<str>);

impl MonitorId {
    /// Maximum UTF-8 byte length accepted for a persisted monitor identity.
    pub const MAX_BYTES: usize = MAX_MONITOR_ID_BYTES;

    /// Validates a bounded platform-supplied monitor identity.
    pub fn new(value: impl AsRef<str>) -> Result<Self, ValueError> {
        bounded_text("monitor identity", value.as_ref(), MAX_MONITOR_ID_BYTES).map(Self)
    }

    /// Returns the exact platform-supplied identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for MonitorId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// Why stored window geometry could not be represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementError {
    /// Width must be non-zero.
    ZeroWidth,
    /// Height must be non-zero.
    ZeroHeight,
}

impl fmt::Display for PlacementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWidth => formatter.write_str("window width must be non-zero"),
            Self::ZeroHeight => formatter.write_str("window height must be non-zero"),
        }
    }
}

impl Error for PlacementError {}

/// Exact saved outer window rectangle in platform logical coordinates.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct WindowBounds {
    x: i32,
    y: i32,
    width: NonZeroU32,
    height: NonZeroU32,
}

impl WindowBounds {
    /// Constructs a non-empty saved rectangle.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Result<Self, PlacementError> {
        Ok(Self {
            x,
            y,
            width: NonZeroU32::new(width).ok_or(PlacementError::ZeroWidth)?,
            height: NonZeroU32::new(height).ok_or(PlacementError::ZeroHeight)?,
        })
    }

    /// Returns the saved horizontal origin.
    #[must_use]
    pub const fn x(self) -> i32 {
        self.x
    }

    /// Returns the saved vertical origin.
    #[must_use]
    pub const fn y(self) -> i32 {
        self.y
    }

    /// Returns the saved width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width.get()
    }

    /// Returns the saved height.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height.get()
    }
}

/// Saved non-minimized window display state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum WindowDisplayState {
    /// Restore the window at its saved bounds.
    Normal,
    /// Restore the window maximized on its resolved monitor.
    Maximized,
}

/// Platform monitor identity and saved work-area facts used as restoration hints.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MonitorHint {
    id: MonitorId,
    work_area: WindowBounds,
}

impl MonitorHint {
    /// Constructs one best-effort monitor hint.
    #[must_use]
    pub const fn new(id: MonitorId, work_area: WindowBounds) -> Self {
        Self { id, work_area }
    }

    /// Returns the platform monitor identity.
    #[must_use]
    pub const fn id(&self) -> &MonitorId {
        &self.id
    }

    /// Returns the saved monitor work area.
    #[must_use]
    pub const fn work_area(&self) -> WindowBounds {
        self.work_area
    }
}

/// Pure durable placement facts for one restorable main window.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct WindowPlacement {
    bounds: WindowBounds,
    display_state: WindowDisplayState,
    monitor: Option<MonitorHint>,
    virtual_desktop: Option<VirtualDesktopId>,
}

impl WindowPlacement {
    /// Constructs placement facts after the platform boundary has observed them.
    #[must_use]
    pub const fn new(
        bounds: WindowBounds,
        display_state: WindowDisplayState,
        monitor: Option<MonitorHint>,
        virtual_desktop: Option<VirtualDesktopId>,
    ) -> Self {
        Self {
            bounds,
            display_state,
            monitor,
            virtual_desktop,
        }
    }

    /// Returns the saved outer bounds.
    #[must_use]
    pub const fn bounds(&self) -> WindowBounds {
        self.bounds
    }

    /// Returns the saved non-minimized display state.
    #[must_use]
    pub const fn display_state(&self) -> WindowDisplayState {
        self.display_state
    }

    /// Returns the optional monitor restoration hint.
    #[must_use]
    pub const fn monitor(&self) -> Option<&MonitorHint> {
        self.monitor.as_ref()
    }

    /// Returns the optional exact virtual-desktop identity.
    #[must_use]
    pub const fn virtual_desktop(&self) -> Option<VirtualDesktopId> {
        self.virtual_desktop
    }
}
