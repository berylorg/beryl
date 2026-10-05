use std::{
    io::Cursor,
    num::{NonZeroU64, NonZeroUsize},
};

use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeStore, SidecarByteLimit, SidecarError,
    SidecarNamespace,
};
use beryl_model::AssetId;
use beryl_state::{AssetDimensions, AssetMediaType, AssetState, PublishAssetMetadata};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClipboardImageAdmissionError {
    #[error("clipboard image is unsupported or has an invalid bounded header")]
    Unsupported,
    #[error("clipboard image exceeds the configured byte allowance")]
    TooLarge,
    #[error("clipboard image admission was cancelled")]
    Cancelled,
    #[error("clipboard image storage is unavailable")]
    Storage,
    #[error("clipboard image sidecar admission failed")]
    Sidecar(#[source] SidecarError),
    #[error("clipboard image metadata admission did not complete")]
    Metadata(Box<CommandOutcome>),
}

pub(crate) fn admit_clipboard_image(
    store: &HomeStore,
    assets: AssetState,
    bytes: &[u8],
    byte_limit: NonZeroU64,
    page_bytes: NonZeroUsize,
    cancellation: &CommandCancellation,
) -> Result<AssetId, ClipboardImageAdmissionError> {
    use ClipboardImageAdmissionError as Error;
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let length = NonZeroU64::new(u64::try_from(bytes.len()).map_err(|_| Error::TooLarge)?)
        .ok_or(Error::Unsupported)?;
    if length.get() > byte_limit.get() {
        return Err(Error::TooLarge);
    }
    let (media_type, dimensions) = image_header(bytes).ok_or(Error::Unsupported)?;
    let sidecar = store
        .admit_sidecar_stream(
            SidecarNamespace::new("images").map_err(|_| Error::Storage)?,
            length.get(),
            SidecarByteLimit::new(byte_limit),
            page_bytes,
            || Ok(Cursor::new(bytes)),
            || cancellation.is_cancelled(),
        )
        .map_err(|error| match error {
            SidecarError::Cancelled => Error::Cancelled,
            error => Error::Sidecar(error),
        })?;
    let asset_id = AssetId::sha256_v1(sidecar.address().digest().as_bytes(), length);
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if let Some(metadata) = assets
        .metadata(store, asset_id)
        .map_err(|_| Error::Storage)?
    {
        if metadata.media_type().as_str() != media_type || metadata.dimensions() != Some(dimensions)
        {
            return Err(Error::Storage);
        }
        return Ok(asset_id);
    }
    let revision = assets.revision(store).map_err(|_| Error::Storage)?;
    let contribution = assets
        .publish_metadata(
            revision,
            sidecar,
            PublishAssetMetadata::new(
                asset_id,
                AssetMediaType::new(media_type).map_err(|_| Error::Unsupported)?,
                Some(dimensions),
                revision.checked_next().map_err(|_| Error::Storage)?,
            ),
        )
        .map_err(|_| Error::Storage)?;
    let mut command = HomeCommand::new(store.home_revision().map_err(|_| Error::Storage)?)
        .with_cancellation(cancellation.clone());
    contribution
        .add_to(&mut command)
        .map_err(|_| Error::Storage)?;
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => Ok(asset_id),
        outcome => Err(Error::Metadata(Box::new(outcome))),
    }
}

fn image_header(bytes: &[u8]) -> Option<(&'static str, AssetDimensions)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.get(8..12)? != 13u32.to_be_bytes() || bytes.get(12..16)? != b"IHDR" {
            return None;
        }
        let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
        let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
        let depth = *bytes.get(24)?;
        let color = *bytes.get(25)?;
        let valid_depth = match color {
            0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
            2 | 4 | 6 => matches!(depth, 8 | 16),
            3 => matches!(depth, 1 | 2 | 4 | 8),
            _ => false,
        };
        if !valid_depth || bytes.get(26..28)? != [0, 0] || *bytes.get(28)? > 1 {
            return None;
        }
        // IHDR is fixed-size; the encoded source remains opaque and is never decoded here.
        let expected_crc = u32::from_be_bytes(bytes.get(29..33)?.try_into().ok()?);
        if png_header_crc(bytes.get(12..29)?) != expected_crc {
            return None;
        }
        return Some(("image/png", dimensions(width, height)?));
    }
    if bytes.get(..2)? != [0xff, 0xd8] {
        return None;
    }
    let header = &bytes[..bytes.len().min(64 * 1024)];
    let mut position = 2usize;
    while position < header.len() {
        if *header.get(position)? != 0xff {
            return None;
        }
        while *header.get(position)? == 0xff {
            position = position.checked_add(1)?;
        }
        let marker = *header.get(position)?;
        position = position.checked_add(1)?;
        if matches!(marker, 0 | 0xd8 | 0xd9 | 0xda) {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes(
            header
                .get(position..position.checked_add(2)?)?
                .try_into()
                .ok()?,
        ));
        if length < 2 {
            return None;
        }
        let end = position.checked_add(length)?;
        let segment = header.get(position..end)?;
        if matches!(marker, 0xc0 | 0xc1 | 0xc2) {
            let precision = *segment.get(2)?;
            let components = usize::from(*segment.get(7)?);
            if !matches!(precision, 8 | 12) || components == 0 || length != 8 + 3 * components {
                return None;
            }
            let height = u32::from(u16::from_be_bytes(segment.get(3..5)?.try_into().ok()?));
            let width = u32::from(u16::from_be_bytes(segment.get(5..7)?.try_into().ok()?));
            return Some(("image/jpeg", dimensions(width, height)?));
        }
        position = end;
    }
    None
}

fn dimensions(width: u32, height: u32) -> Option<AssetDimensions> {
    Some(AssetDimensions::new(
        NonZeroU64::new(u64::from(width))?,
        NonZeroU64::new(u64::from(height))?,
    ))
}

fn png_header_crc(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../tests/unit/clipboard_image_admission.rs"]
mod tests;
