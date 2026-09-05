use std::{
    fs::{self, File},
    io::{BufReader, Cursor, Read},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use beryl_model::workspace::WorkspaceId;
use gpui::{Image, ImageFormat};
use tracing::debug;

use super::{
    path_policy::{RuntimePathResolution, resolve_markdown_runtime_path},
    sizing::TranscriptMediaNaturalDimensions,
    types::{TranscriptMediaLoadOutcome, TranscriptMediaLoadedImage, TranscriptMediaSource},
};

pub(crate) const TRANSCRIPT_MEDIA_MAX_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;
pub(crate) const TRANSCRIPT_MEDIA_MAX_DECODED_IMAGE_BYTES: usize = 128 * 1024 * 1024;
pub(crate) const TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn load_transcript_media(
    source: &TranscriptMediaSource,
    execution_target: &WorkspaceId,
) -> TranscriptMediaLoadOutcome {
    match source {
        TranscriptMediaSource::MarkdownImage {
            alt, destination, ..
        } => load_markdown_image(alt.trim().to_string(), destination, execution_target),
        TranscriptMediaSource::NativeImageGeneration {
            revised_prompt,
            saved_path,
            complete,
            ..
        } => load_native_generated_image(
            revised_prompt
                .as_deref()
                .map(str::trim)
                .filter(|alt| !alt.is_empty())
                .unwrap_or("generated image")
                .to_string(),
            saved_path.as_deref(),
            *complete,
            execution_target,
        ),
    }
}

fn load_markdown_image(
    alt: String,
    destination: &str,
    execution_target: &WorkspaceId,
) -> TranscriptMediaLoadOutcome {
    let load_started = Instant::now();
    let path = match resolve_markdown_runtime_path(destination, execution_target) {
        RuntimePathResolution::Allowed { backend_path } => backend_path,
        RuntimePathResolution::PathNotAllowed => {
            return TranscriptMediaLoadOutcome::PathNotAllowed { alt };
        }
        RuntimePathResolution::RenderNotSupported => {
            return TranscriptMediaLoadOutcome::RenderNotSupported { alt };
        }
    };
    let Some(format) = raster_image_format_from_path(path.as_str()) else {
        return TranscriptMediaLoadOutcome::RenderNotSupported { alt };
    };
    // Markdown remains byte-backed so a revalidation creates a fresh GPUI
    // image even when the file changes without changing its path or dimensions.
    // Acquisition is still from the directly readable host file; no backend
    // image-byte RPC is involved.
    load_byte_backed_file(
        alt,
        format,
        execution_target.host_openable_path(Path::new(&path)),
        Some(path),
        "markdown_image",
        true,
        load_started,
    )
}

fn load_byte_backed_file(
    alt: String,
    format: ImageFormat,
    host_path: std::path::PathBuf,
    source_path: Option<String>,
    source: &'static str,
    complete: bool,
    load_started: Instant,
) -> TranscriptMediaLoadOutcome {
    let compressed_bytes = match fs::metadata(&host_path) {
        Ok(metadata) => metadata.len(),
        Err(_) => return TranscriptMediaLoadOutcome::FileUnavailable { alt },
    };
    if compressed_bytes > TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES as u64 {
        debug!(
            source,
            branch = "direct_file_bytes",
            complete,
            outcome = "too_large",
            host_path = %host_path.display(),
            bytes = compressed_bytes,
            max_bytes = TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES,
            total_ms = elapsed_ms(load_started.elapsed()),
            "transcript media load rejected before decode"
        );
        return TranscriptMediaLoadOutcome::TooLarge { alt };
    }
    let mut file = match File::open(&host_path) {
        Ok(file) => file,
        Err(_) => return TranscriptMediaLoadOutcome::FileUnavailable { alt },
    };
    let mut bytes = Vec::new();
    if file
        .by_ref()
        .take((TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return TranscriptMediaLoadOutcome::FileUnavailable { alt };
    }
    if bytes.len() > TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES {
        debug!(
            source,
            branch = "direct_file_bytes",
            complete,
            outcome = "too_large",
            host_path = %host_path.display(),
            bytes = bytes.len(),
            max_bytes = TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES,
            total_ms = elapsed_ms(load_started.elapsed()),
            "transcript media load rejected after bounded source read"
        );
        return TranscriptMediaLoadOutcome::TooLarge { alt };
    }
    let dimensions_started = Instant::now();
    let natural_dimensions = match decoded_raster_dimensions(format, &bytes) {
        Ok(dimensions) => dimensions,
        Err(RasterAdmissionError::Unsupported) => {
            return TranscriptMediaLoadOutcome::RenderNotSupported { alt };
        }
        Err(RasterAdmissionError::TooLarge {
            pixels,
            decoded_bytes,
        }) => {
            debug!(
                source,
                branch = "direct_file_bytes",
                complete,
                outcome = "too_large",
                host_path = %host_path.display(),
                pixels,
                decoded_bytes,
                max_pixels = TRANSCRIPT_MEDIA_MAX_IMAGE_PIXELS,
                max_decoded_bytes = TRANSCRIPT_MEDIA_MAX_DECODED_IMAGE_BYTES,
                raster_dimensions_decode_ms = elapsed_ms(dimensions_started.elapsed()),
                total_ms = elapsed_ms(load_started.elapsed()),
                "transcript media load rejected after decode"
            );
            return TranscriptMediaLoadOutcome::TooLarge { alt };
        }
    };
    let image = Arc::new(Image::from_bytes(format, bytes.clone()));
    debug!(
        source,
        branch = "direct_file_bytes",
        complete,
        outcome = "loaded",
        host_path = %host_path.display(),
        width = natural_dimensions.width(),
        height = natural_dimensions.height(),
        raster_dimensions_decode_ms = elapsed_ms(dimensions_started.elapsed()),
        total_ms = elapsed_ms(load_started.elapsed()),
        "transcript media load finished"
    );
    TranscriptMediaLoadOutcome::Loaded(TranscriptMediaLoadedImage::new(
        alt,
        format,
        bytes,
        image,
        natural_dimensions,
        source_path,
        Some(host_path),
    ))
}

fn load_native_generated_image(
    alt: String,
    saved_path: Option<&str>,
    complete: bool,
    execution_target: &WorkspaceId,
) -> TranscriptMediaLoadOutcome {
    let load_started = Instant::now();
    if let Some(saved_path) = saved_path.filter(|path| !path.trim().is_empty()) {
        let saved_path = saved_path.trim();
        let Some(format) = raster_image_format_from_path(saved_path) else {
            debug!(
                source = "native_generated_image",
                branch = "saved_path",
                complete,
                outcome = "render_not_supported",
                total_ms = elapsed_ms(load_started.elapsed()),
                "generated-image media load finished"
            );
            return TranscriptMediaLoadOutcome::RenderNotSupported { alt };
        };
        let host_path = execution_target.host_openable_path(Path::new(saved_path));
        return load_source_backed_file(
            alt,
            format,
            host_path,
            Some(saved_path.to_string()),
            "native_generated_image",
            complete,
            load_started,
        );
    }

    if complete {
        TranscriptMediaLoadOutcome::FileUnavailable { alt }
    } else {
        TranscriptMediaLoadOutcome::Pending { alt }
    }
}

fn load_source_backed_file(
    alt: String,
    format: ImageFormat,
    host_path: std::path::PathBuf,
    source_path: Option<String>,
    source: &'static str,
    complete: bool,
    load_started: Instant,
) -> TranscriptMediaLoadOutcome {
    let compressed_bytes = match fs::metadata(&host_path) {
        Ok(metadata) => metadata.len(),
        Err(_) => return TranscriptMediaLoadOutcome::FileUnavailable { alt },
    };
    if compressed_bytes > TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES as u64 {
        debug!(
            source,
            branch = "file_path",
            complete,
            outcome = "too_large",
            host_path = %host_path.display(),
            bytes = compressed_bytes,
            max_bytes = TRANSCRIPT_MEDIA_MAX_COMPRESSED_IMAGE_BYTES,
            total_ms = elapsed_ms(load_started.elapsed()),
            "transcript media load rejected before decode"
        );
        return TranscriptMediaLoadOutcome::TooLarge { alt };
    }
    let dimensions_started = Instant::now();
    let natural_dimensions = match decoded_raster_file_dimensions(format, &host_path) {
        Ok(dimensions) => dimensions,
        Err(RasterFileAdmissionError::Unavailable) => {
            debug!(
                source,
                branch = "file_path",
                complete,
                outcome = "file_unavailable",
                host_path = %host_path.display(),
                raster_dimensions_decode_ms = elapsed_ms(dimensions_started.elapsed()),
                total_ms = elapsed_ms(load_started.elapsed()),
                "transcript media load finished"
            );
            return TranscriptMediaLoadOutcome::FileUnavailable { alt };
        }
        Err(RasterFileAdmissionError::Unsupported) => {
            return TranscriptMediaLoadOutcome::RenderNotSupported { alt };
        }
        Err(RasterFileAdmissionError::TooLarge {
            pixels,
            decoded_bytes,
        }) => {
            debug!(
                source,
                branch = "file_path",
                complete,
                outcome = "too_large",
                host_path = %host_path.display(),
                pixels,
                decoded_bytes,
                max_pixels = TRANSCRIPT_MEDIA_MAX_IMAGE_PIXELS,
                max_decoded_bytes = TRANSCRIPT_MEDIA_MAX_DECODED_IMAGE_BYTES,
                raster_dimensions_decode_ms = elapsed_ms(dimensions_started.elapsed()),
                total_ms = elapsed_ms(load_started.elapsed()),
                "transcript media load rejected after dimension decode"
            );
            return TranscriptMediaLoadOutcome::TooLarge { alt };
        }
    };
    debug!(
        source,
        branch = "file_path",
        complete,
        outcome = "loaded",
        host_path = %host_path.display(),
        width = natural_dimensions.width(),
        height = natural_dimensions.height(),
        raster_dimensions_decode_ms = elapsed_ms(dimensions_started.elapsed()),
        total_ms = elapsed_ms(load_started.elapsed()),
        "transcript media load finished"
    );

    TranscriptMediaLoadOutcome::Loaded(TranscriptMediaLoadedImage::new_source_backed_file(
        alt,
        format,
        host_path,
        natural_dimensions,
        source_path,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RasterAdmissionError {
    Unsupported,
    TooLarge { pixels: u64, decoded_bytes: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RasterFileAdmissionError {
    Unavailable,
    Unsupported,
    TooLarge { pixels: u64, decoded_bytes: usize },
}

fn decoded_raster_dimensions(
    format: ImageFormat,
    bytes: &[u8],
) -> Result<TranscriptMediaNaturalDimensions, RasterAdmissionError> {
    let dimensions = image::ImageReader::with_format(Cursor::new(bytes), image_format(format))
        .into_dimensions()
        .map_err(|_| RasterAdmissionError::Unsupported)?;
    let natural_dimensions = admit_raster_dimensions(dimensions)?;

    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image_format(format));
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(TRANSCRIPT_MEDIA_MAX_DECODED_IMAGE_BYTES as u64);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| RasterAdmissionError::Unsupported)?;
    Ok(natural_dimensions)
}

fn decoded_raster_file_dimensions(
    format: ImageFormat,
    path: &Path,
) -> Result<TranscriptMediaNaturalDimensions, RasterFileAdmissionError> {
    let file = File::open(path).map_err(|_| RasterFileAdmissionError::Unavailable)?;
    let dimensions = image::ImageReader::with_format(BufReader::new(file), image_format(format))
        .into_dimensions()
        .map_err(|_| RasterFileAdmissionError::Unsupported)?;
    let natural_dimensions = admit_raster_dimensions(dimensions).map_err(|error| match error {
        RasterAdmissionError::Unsupported => RasterFileAdmissionError::Unsupported,
        RasterAdmissionError::TooLarge {
            pixels,
            decoded_bytes,
        } => RasterFileAdmissionError::TooLarge {
            pixels,
            decoded_bytes,
        },
    })?;

    let file = File::open(path).map_err(|_| RasterFileAdmissionError::Unavailable)?;
    let mut reader = image::ImageReader::with_format(BufReader::new(file), image_format(format));
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(TRANSCRIPT_MEDIA_MAX_DECODED_IMAGE_BYTES as u64);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| RasterFileAdmissionError::Unsupported)?;
    Ok(natural_dimensions)
}

fn admit_raster_dimensions(
    dimensions: (u32, u32),
) -> Result<TranscriptMediaNaturalDimensions, RasterAdmissionError> {
    let decoded_bytes =
        decoded_rgba_bytes(dimensions.0, dimensions.1).ok_or(RasterAdmissionError::TooLarge {
            pixels: u64::MAX,
            decoded_bytes: usize::MAX,
        })?;
    let pixels = u64::from(dimensions.0).saturating_mul(u64::from(dimensions.1));
    if pixels > TRANSCRIPT_MEDIA_MAX_IMAGE_PIXELS
        || decoded_bytes > TRANSCRIPT_MEDIA_MAX_DECODED_IMAGE_BYTES
    {
        return Err(RasterAdmissionError::TooLarge {
            pixels,
            decoded_bytes,
        });
    }

    TranscriptMediaNaturalDimensions::new(dimensions.0, dimensions.1)
        .ok_or(RasterAdmissionError::Unsupported)
}

fn decoded_rgba_bytes(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
}

fn image_format(format: ImageFormat) -> image::ImageFormat {
    match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Webp => image::ImageFormat::WebP,
        ImageFormat::Gif => image::ImageFormat::Gif,
        ImageFormat::Bmp => image::ImageFormat::Bmp,
        ImageFormat::Tiff => image::ImageFormat::Tiff,
        ImageFormat::Svg => unreachable!("SVG is not a supported raster transcript media format"),
    }
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn raster_image_format_from_path(path: &str) -> Option<ImageFormat> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        "gif" => Some(ImageFormat::Gif),
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}
