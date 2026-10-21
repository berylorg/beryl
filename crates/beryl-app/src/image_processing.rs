mod config;
mod generated;
mod png_rows;
mod png_source;
mod process;
mod source;
mod surfaces;

#[cfg(test)]
mod test_input {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/paged_image_input.rs"
    ));
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/image_processing.rs"
    ));
}

pub(crate) use config::{MediaResourceConfiguration, MediaResourceLimits};
pub(crate) use generated::{GeneratedMediaCoordinator, GeneratedMediaCoordinatorError};
#[cfg(feature = "test-faults")]
pub use generated::qualification as generated_media_qualification;
pub(crate) use png_rows::decode_png_into;
pub(crate) use png_source::{PngPreflight, preflight_png};
pub(crate) use process::{
    ImageMediaAttachment, ImageMediaChanged, ImageMediaDiagnostics, ImageMediaError,
    ImageMediaFailure, ImageMediaInterest, ImageMediaProcess, ImageMediaWindow,
    OriginalCapacityChanged, OriginalImageAvailability, OriginalImageCommand,
    OriginalImageCopyEffect, OriginalImageFailure, OriginalImageRequest, OriginalImageState,
};
pub(crate) use source::{
    ImageSourceDemand, ImageSourceError, ImageSourceProvider, ResolvedImageSource,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ImagePreparationError {
    #[error("image presentation limits are inconsistent")]
    Configuration,
    #[error("image rendition exceeds the presentation capacity")]
    Capacity,
    #[error("image rendition was cancelled")]
    Cancelled,
    #[error("image original does not match its admitted identity")]
    Identity,
    #[error("image rendition format is unsupported")]
    Unsupported,
    #[error("image rendition source is malformed")]
    Malformed,
    #[error("image rendition source is unavailable")]
    Io(#[from] std::io::Error),
    #[error("image rendition could not be decoded")]
    Decode(#[from] png::DecodingError),
}

pub(crate) fn check_cancelled(
    cancelled: &beryl_home_store::CommandCancellation,
) -> Result<(), ImagePreparationError> {
    if cancelled.is_cancelled() {
        Err(ImagePreparationError::Cancelled)
    } else {
        Ok(())
    }
}

pub(crate) fn zeroed_bytes(length: usize) -> Result<Vec<u8>, ImagePreparationError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| ImagePreparationError::Capacity)?;
    bytes.resize(length, 0);
    Ok(bytes)
}
