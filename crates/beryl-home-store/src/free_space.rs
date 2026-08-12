//! Synchronous physical free-space observation for one opened home.

use std::path::Path;

/// Result of one free-space observation against an opened Beryl home.
///
/// This result is an admission input, not a capacity reservation. A sufficient
/// observation does not prevent a later filesystem write from returning
/// `ENOSPC`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreeSpaceOutcome {
    /// The observed capacity available to the current caller meets the validated requirement.
    Sufficient {
        /// Bytes available to the current caller in the observed filesystem.
        available_bytes: u64,
        /// The validated requirement evaluated against this observation.
        reserve_bytes: u64,
    },
    /// The observed capacity available to the current caller is below the validated requirement.
    BelowReserve {
        /// Bytes available to the current caller in the observed filesystem.
        available_bytes: u64,
        /// The validated requirement evaluated against this observation.
        reserve_bytes: u64,
    },
    /// The platform supplied no filesystem availability observation.
    Unavailable,
    /// The platform supplied an observation that is inconsistent for this home.
    Indeterminate,
}

pub(crate) fn query(
    canonical_home_path: &Path,
    reserve_bytes: u64,
    faults: &crate::fault::FaultController,
) -> FreeSpaceOutcome {
    #[cfg(feature = "test-faults")]
    if let Some(observation) = faults.free_space_observation() {
        return classify_test_observation(observation, reserve_bytes);
    }

    observe(canonical_home_path, reserve_bytes)
}

fn classify(
    available_bytes: u64,
    total_free_bytes: u64,
    total_bytes: u64,
    reserve_bytes: u64,
) -> FreeSpaceOutcome {
    if available_bytes > total_free_bytes || total_free_bytes > total_bytes {
        return FreeSpaceOutcome::Indeterminate;
    }
    if available_bytes >= reserve_bytes {
        FreeSpaceOutcome::Sufficient {
            available_bytes,
            reserve_bytes,
        }
    } else {
        FreeSpaceOutcome::BelowReserve {
            available_bytes,
            reserve_bytes,
        }
    }
}

#[cfg(feature = "test-faults")]
fn classify_test_observation(
    observation: crate::fault::FreeSpaceTestObservation,
    reserve_bytes: u64,
) -> FreeSpaceOutcome {
    match observation {
        crate::fault::FreeSpaceTestObservation::Observed {
            available_bytes,
            total_free_bytes,
            total_bytes,
        } => classify(
            available_bytes,
            total_free_bytes,
            total_bytes,
            reserve_bytes,
        ),
        crate::fault::FreeSpaceTestObservation::Unavailable => FreeSpaceOutcome::Unavailable,
    }
}

#[cfg(target_os = "windows")]
fn observe(canonical_home_path: &Path, reserve_bytes: u64) -> FreeSpaceOutcome {
    use std::os::windows::ffi::OsStrExt;

    use windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::PCWSTR};

    let path: Vec<u16> = canonical_home_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut available_bytes = 0;
    let mut total_bytes = 0;
    let mut total_free_bytes = 0;
    let result = unsafe {
        // SAFETY: `path` is null-terminated and all output pointers refer to
        // initialized stack values that remain live for this synchronous call.
        GetDiskFreeSpaceExW(
            PCWSTR(path.as_ptr()),
            Some(&mut available_bytes),
            Some(&mut total_bytes),
            Some(&mut total_free_bytes),
        )
    };
    if result.is_err() {
        return FreeSpaceOutcome::Unavailable;
    }

    classify(
        available_bytes,
        total_free_bytes,
        total_bytes,
        reserve_bytes,
    )
}

#[cfg(not(target_os = "windows"))]
fn observe(_canonical_home_path: &Path, _reserve_bytes: u64) -> FreeSpaceOutcome {
    FreeSpaceOutcome::Unavailable
}
