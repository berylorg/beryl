use std::io::BufRead;

use gpui::{Image, ImageFormat};
use sha2::{Digest, Sha256};

use super::*;
use crate::atomic_save::{AtomicSaveError, AtomicSaveStage};

const CLIPBOARD_COMPANION_BYTES: usize = 24;

fn source_failure(error: ImageSourceError) -> OriginalImageFailure {
    use beryl_home_store::SidecarError;
    match error {
        ImageSourceError::Cancelled => OriginalImageFailure::Cancelled,
        ImageSourceError::Stale => OriginalImageFailure::Stale,
        ImageSourceError::Identity => OriginalImageFailure::Integrity,
        ImageSourceError::Unsupported => OriginalImageFailure::Unsupported,
        ImageSourceError::Capacity => OriginalImageFailure::Capacity,
        ImageSourceError::Sidecar(
            SidecarError::ContentMismatch | SidecarError::LengthMismatch { .. },
        ) => OriginalImageFailure::Integrity,
        ImageSourceError::Sidecar(SidecarError::Cancelled) => OriginalImageFailure::Cancelled,
        ImageSourceError::Sidecar(SidecarError::BoundExceeded { .. }) => {
            OriginalImageFailure::Capacity
        }
        ImageSourceError::Unavailable | ImageSourceError::Sidecar(_) => {
            OriginalImageFailure::Unavailable
        }
    }
}

fn save_failure(error: AtomicSaveError) -> OriginalImageFailure {
    match error {
        AtomicSaveError::Destination | AtomicSaveError::Create => OriginalImageFailure::Destination,
        AtomicSaveError::Write | AtomicSaveError::Prepare => OriginalImageFailure::Write,
        AtomicSaveError::Publish => OriginalImageFailure::Publish,
        AtomicSaveError::Cleanup => OriginalImageFailure::Cleanup,
    }
}

fn check(operation: &OriginalCommandState) -> Result<(), OriginalImageFailure> {
    if operation.live() {
        Ok(())
    } else {
        Err(OriginalImageFailure::Cancelled)
    }
}

pub(super) fn copy_format(media_type: &str) -> Result<ImageFormat, OriginalImageFailure> {
    match media_type {
        "image/png" => Ok(ImageFormat::Png),
        "image/gif" => Ok(ImageFormat::Gif),
        "image/jpeg" => Ok(ImageFormat::Jpeg),
        "image/svg+xml" => Ok(ImageFormat::Svg),
        _ => Err(OriginalImageFailure::Unsupported),
    }
}

pub(super) fn copy_capacity(
    length: u64,
    capacity: usize,
    config: crate::image_processing::MediaResourceConfiguration,
) -> Result<usize, OriginalImageFailure> {
    let length = usize::try_from(length).map_err(|_| OriginalImageFailure::Capacity)?;
    let total = capacity
        .checked_add(length)
        .and_then(|bytes| bytes.checked_add(CLIPBOARD_COMPANION_BYTES))
        .ok_or(OriginalImageFailure::Capacity)?;
    if length == 0
        || capacity < length
        || total > config.original_clipboard_total_bytes
        || CLIPBOARD_COMPANION_BYTES > config.original_clipboard_metadata_bytes
    {
        return Err(OriginalImageFailure::Capacity);
    }
    Ok(length)
}

pub(super) fn run(
    core: &Arc<ProcessCore>,
    operation: &Arc<OriginalCommandState>,
    attachment: &Arc<AttachmentState>,
) -> Result<(), OriginalImageFailure> {
    let stage_lease = worker::SourceStageLease::acquire(core).map_err(|error| match error.0 {
        ImageMediaFailure::Capacity => OriginalImageFailure::Capacity,
        _ => OriginalImageFailure::Cancelled,
    })?;
    check(operation)?;
    let source = lock(&attachment.source)
        .clone()
        .ok_or(OriginalImageFailure::Cancelled)?;
    let resolved = source
        .resolve_original(&operation.demand, &operation.cancellation)
        .map_err(source_failure)?;
    let copy = copy_format(resolved.metadata().media_type().as_str()).and_then(|format| {
        let length = usize::try_from(resolved.asset_id().length().get())
            .map_err(|_| OriginalImageFailure::Capacity)?;
        copy_capacity(
            resolved.asset_id().length().get(),
            length,
            core.limits.configuration(),
        )?;
        Ok(format)
    });
    if operation.request == OriginalImageRequest::Copy {
        copy?;
    }
    let mut reader = source
        .open_original_verified(&resolved, operation.cancellation.clone())
        .map_err(source_failure)?;
    if operation.request == OriginalImageRequest::Availability {
        drop(reader);
        drop(resolved);
        drop(source);
        drop(stage_lease);
        let _state = lock(&core.state);
        check(operation)?;
        lock(&operation.data).state = OriginalImageState::Available(OriginalImageAvailability {
            copy: copy.map(|_| ()),
            save: true,
        });
        return Ok(());
    }
    let asset = resolved.asset_id();
    if operation.request == OriginalImageRequest::Copy {
        let length =
            usize::try_from(asset.length().get()).map_err(|_| OriginalImageFailure::Capacity)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| OriginalImageFailure::Capacity)?;
        copy_capacity(
            asset.length().get(),
            bytes.capacity(),
            core.limits.configuration(),
        )?;
        let mut digest = Sha256::new();
        while bytes.len() < length {
            check(operation)?;
            let page = reader.fill_buf().map_err(|_| {
                if operation.cancellation.is_cancelled() {
                    OriginalImageFailure::Cancelled
                } else {
                    OriginalImageFailure::Unavailable
                }
            })?;
            if page.is_empty() || page.len() > length - bytes.len() {
                return Err(OriginalImageFailure::Integrity);
            }
            digest.update(page);
            bytes.extend_from_slice(page);
            let count = page.len();
            reader.consume(count);
        }
        if !reader
            .fill_buf()
            .map_err(|_| OriginalImageFailure::Unavailable)?
            .is_empty()
            || <[u8; 32]>::from(digest.finalize()) != asset.digest()
        {
            return Err(OriginalImageFailure::Integrity);
        }
        drop(reader);
        drop(resolved);
        drop(source);
        drop(stage_lease);
        let capacity = bytes.capacity();
        let item = ClipboardItem::from(Image::from_bytes(copy?, bytes));
        let _state = lock(&core.state);
        check(operation)?;
        let mut data = lock(&operation.data);
        data.copy = Some(item);
        data.encoded_bytes = capacity;
        data.encoded_length = length;
        data.state = OriginalImageState::PreparedCopy;
        return Ok(());
    }
    let choice = lock(&operation.data)
        .choice
        .take()
        .ok_or(OriginalImageFailure::Cancelled)?;
    let mut stage = AtomicSaveStage::create(choice, "beryl-image").map_err(save_failure)?;
    let prepared = (|| {
        let mut remaining = asset.length().get();
        let mut digest = Sha256::new();
        while remaining != 0 {
            check(operation)?;
            let page = reader.fill_buf().map_err(|_| {
                if operation.cancellation.is_cancelled() {
                    OriginalImageFailure::Cancelled
                } else {
                    OriginalImageFailure::Unavailable
                }
            })?;
            if page.is_empty() || page.len() as u64 > remaining {
                return Err(OriginalImageFailure::Integrity);
            }
            digest.update(page);
            stage.write(page).map_err(save_failure)?;
            let count = page.len();
            remaining -= count as u64;
            reader.consume(count);
        }
        if !reader
            .fill_buf()
            .map_err(|_| OriginalImageFailure::Unavailable)?
            .is_empty()
            || <[u8; 32]>::from(digest.finalize()) != asset.digest()
        {
            return Err(OriginalImageFailure::Integrity);
        }
        check(operation)?;
        stage.prepare().map_err(save_failure)
    })();
    drop(reader);
    drop(resolved);
    drop(source);
    drop(stage_lease);
    if let Err(error) = prepared {
        return match stage.cleanup() {
            Ok(()) => Err(error),
            Err(_) => Err(OriginalImageFailure::Cleanup),
        };
    }
    let elected = {
        let mut state = lock(&core.state);
        if check(operation).is_err() {
            false
        } else {
            lock(&operation.data).state = OriginalImageState::PreparedSave;
            operation.event.notify();
            loop {
                let command_state = lock(&operation.data).state;
                if command_state == OriginalImageState::Publishing {
                    break true;
                }
                if command_state != OriginalImageState::PreparedSave
                    || check(operation).is_err()
                    || state.stopping
                {
                    break false;
                }
                state = core
                    .work
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
    };
    if !elected {
        return match stage.cleanup() {
            Ok(()) => Err(OriginalImageFailure::Cancelled),
            Err(_) => Err(OriginalImageFailure::Cleanup),
        };
    }
    let published = stage.publish().map_err(save_failure);
    let result = match published {
        Ok(()) => Ok(()),
        Err(error) => match stage.cleanup() {
            Ok(()) => Err(error),
            Err(_) => Err(OriginalImageFailure::Cleanup),
        },
    };
    lock(&operation.data).state = OriginalImageState::Settled(result);
    Ok(())
}
