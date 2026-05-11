use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use beryl_model::workspace::WorkspaceId;
use gpui::{Image, ImageFormat};
use tracing::debug;

use super::{
    path_policy::{RuntimePathResolution, resolve_markdown_runtime_path},
    sizing::TranscriptMediaNaturalDimensions,
    types::{
        TranscriptMediaFileReader, TranscriptMediaLoadOutcome, TranscriptMediaLoadedImage,
        TranscriptMediaSource, fallback_alt,
    },
};

pub(super) fn load_transcript_media<R>(
    source: &TranscriptMediaSource,
    execution_target: &WorkspaceId,
    reader: &mut R,
    timeout: Duration,
) -> TranscriptMediaLoadOutcome
where
    R: TranscriptMediaFileReader,
{
    match source {
        TranscriptMediaSource::MarkdownImage {
            alt, destination, ..
        } => load_markdown_image(
            alt.trim().to_string(),
            destination,
            execution_target,
            reader,
            timeout,
        ),
        TranscriptMediaSource::NativeImageGeneration {
            revised_prompt,
            result,
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
            result.as_deref().map(String::as_str),
            saved_path.as_deref(),
            *complete,
            execution_target,
            reader,
            timeout,
        ),
    }
}

fn load_markdown_image<R>(
    alt: String,
    destination: &str,
    execution_target: &WorkspaceId,
    reader: &mut R,
    timeout: Duration,
) -> TranscriptMediaLoadOutcome
where
    R: TranscriptMediaFileReader,
{
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
    let read_started = Instant::now();
    let bytes = match reader.read_file_bytes(path.as_str(), timeout) {
        Ok(bytes) => bytes,
        Err(_) => return TranscriptMediaLoadOutcome::FileUnavailable { alt },
    };
    let read_elapsed = read_started.elapsed();
    let bytes_len = bytes.len();
    loaded_image(
        fallback_alt(&alt),
        format,
        bytes,
        Some(path),
        LoadedImageTimingContext {
            source: "markdown_image",
            branch: "file_path",
            complete: true,
            load_started,
            saved_path_read: Some(read_elapsed),
            inline_base64_decode: None,
            bytes_len,
        },
    )
}

fn load_native_generated_image<R>(
    alt: String,
    result: Option<&str>,
    saved_path: Option<&str>,
    complete: bool,
    _execution_target: &WorkspaceId,
    reader: &mut R,
    timeout: Duration,
) -> TranscriptMediaLoadOutcome
where
    R: TranscriptMediaFileReader,
{
    let load_started = Instant::now();
    if let Some(saved_path) = saved_path.filter(|path| !path.trim().is_empty()) {
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
        let read_started = Instant::now();
        return match reader.read_file_bytes(saved_path, timeout) {
            Ok(bytes) => {
                let read_elapsed = read_started.elapsed();
                let bytes_len = bytes.len();
                loaded_image(
                    alt,
                    format,
                    bytes,
                    Some(saved_path.to_string()),
                    LoadedImageTimingContext {
                        source: "native_generated_image",
                        branch: "saved_path",
                        complete,
                        load_started,
                        saved_path_read: Some(read_elapsed),
                        inline_base64_decode: None,
                        bytes_len,
                    },
                )
            }
            Err(_) => {
                debug!(
                    source = "native_generated_image",
                    branch = "saved_path",
                    complete,
                    outcome = "file_unavailable",
                    saved_path_read_ms = elapsed_ms(read_started.elapsed()),
                    total_ms = elapsed_ms(load_started.elapsed()),
                    "generated-image media load finished"
                );
                TranscriptMediaLoadOutcome::FileUnavailable { alt }
            }
        };
    }

    let Some(result) = result.filter(|result| !result.trim().is_empty()) else {
        return if complete {
            debug!(
                source = "native_generated_image",
                branch = "inline_result_missing",
                complete,
                outcome = "file_unavailable",
                total_ms = elapsed_ms(load_started.elapsed()),
                "generated-image media load finished"
            );
            TranscriptMediaLoadOutcome::FileUnavailable { alt }
        } else {
            debug!(
                source = "native_generated_image",
                branch = "inline_result_missing",
                complete,
                outcome = "pending",
                total_ms = elapsed_ms(load_started.elapsed()),
                "generated-image media load finished"
            );
            TranscriptMediaLoadOutcome::Pending { alt }
        };
    };
    let decode_started = Instant::now();
    let Ok(bytes) = BASE64_STANDARD.decode(result.trim()) else {
        debug!(
            source = "native_generated_image",
            branch = "inline_result",
            complete,
            outcome = "file_unavailable",
            inline_base64_decode_ms = elapsed_ms(decode_started.elapsed()),
            total_ms = elapsed_ms(load_started.elapsed()),
            "generated-image media load finished"
        );
        return TranscriptMediaLoadOutcome::FileUnavailable { alt };
    };
    let decode_elapsed = decode_started.elapsed();
    let bytes_len = bytes.len();
    loaded_image(
        alt,
        ImageFormat::Png,
        bytes,
        None,
        LoadedImageTimingContext {
            source: "native_generated_image",
            branch: "inline_result",
            complete,
            load_started,
            saved_path_read: None,
            inline_base64_decode: Some(decode_elapsed),
            bytes_len,
        },
    )
}

struct LoadedImageTimingContext {
    source: &'static str,
    branch: &'static str,
    complete: bool,
    load_started: Instant,
    saved_path_read: Option<Duration>,
    inline_base64_decode: Option<Duration>,
    bytes_len: usize,
}

fn loaded_image(
    alt: String,
    format: ImageFormat,
    bytes: Vec<u8>,
    source_path: Option<String>,
    timing: LoadedImageTimingContext,
) -> TranscriptMediaLoadOutcome {
    let dimensions_started = Instant::now();
    let Some(natural_dimensions) = decoded_raster_dimensions(format, bytes.as_slice()) else {
        debug!(
            source = timing.source,
            branch = timing.branch,
            complete = timing.complete,
            outcome = "render_not_supported",
            bytes = timing.bytes_len,
            saved_path_read_ms = timing.saved_path_read.map(elapsed_ms),
            inline_base64_decode_ms = timing.inline_base64_decode.map(elapsed_ms),
            raster_dimensions_decode_ms = elapsed_ms(dimensions_started.elapsed()),
            total_ms = elapsed_ms(timing.load_started.elapsed()),
            "transcript media load finished"
        );
        return TranscriptMediaLoadOutcome::RenderNotSupported { alt };
    };
    let dimensions_elapsed = dimensions_started.elapsed();
    let image_started = Instant::now();
    let image = Arc::new(Image::from_bytes(format, bytes.clone()));
    let image_elapsed = image_started.elapsed();
    debug!(
        source = timing.source,
        branch = timing.branch,
        complete = timing.complete,
        outcome = "loaded",
        bytes = timing.bytes_len,
        width = natural_dimensions.width(),
        height = natural_dimensions.height(),
        saved_path_read_ms = timing.saved_path_read.map(elapsed_ms),
        inline_base64_decode_ms = timing.inline_base64_decode.map(elapsed_ms),
        raster_dimensions_decode_ms = elapsed_ms(dimensions_elapsed),
        gpui_image_from_bytes_ms = elapsed_ms(image_elapsed),
        total_ms = elapsed_ms(timing.load_started.elapsed()),
        "transcript media load finished"
    );

    TranscriptMediaLoadOutcome::Loaded(TranscriptMediaLoadedImage::new(
        alt,
        format,
        bytes,
        image,
        natural_dimensions,
        source_path,
    ))
}

fn decoded_raster_dimensions(
    format: ImageFormat,
    bytes: &[u8],
) -> Option<TranscriptMediaNaturalDimensions> {
    let image = image::load_from_memory_with_format(bytes, image_format(format)).ok()?;
    TranscriptMediaNaturalDimensions::new(image.width(), image.height())
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
