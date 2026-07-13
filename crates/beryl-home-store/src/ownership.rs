#[cfg(target_os = "windows")]
#[path = "ownership/windows.rs"]
mod platform;
#[cfg(not(target_os = "windows"))]
#[path = "ownership/unsupported.rs"]
mod platform;

pub(crate) use platform::{HomeOwnership, OpenedHomeDirectory};

/// Stable identity of an opened directory object for the lifetime of a process.
///
/// This value is deliberately not the durable [`beryl_model::BerylHomeId`]. A
/// filesystem may reuse an opened-object identifier after deletion, so Beryl
/// uses it only to collapse live path aliases.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CanonicalHomeIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

impl CanonicalHomeIdentity {
    pub(crate) const fn new(volume_serial_number: u64, file_id: [u8; 16]) -> Self {
        Self {
            volume_serial_number,
            file_id,
        }
    }

    /// Returns the volume serial observed from the retained directory handle.
    #[must_use]
    pub const fn volume_serial_number(self) -> u64 {
        self.volume_serial_number
    }

    /// Returns the 128-bit file identifier observed from the directory handle.
    #[must_use]
    pub const fn file_id(self) -> [u8; 16] {
        self.file_id
    }
}
