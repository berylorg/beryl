use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use beryl_backend::ManagedBackendSession;
use beryl_model::workspace::WorkspaceId;
use gpui::{Image, ImageFormat, hash as gpui_hash};

use super::{load::load_transcript_media, sizing::TranscriptMediaNaturalDimensions};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptMediaCacheKey(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMediaSource {
    MarkdownImage {
        alt: String,
        destination: String,
        title: Option<String>,
    },
    NativeImageGeneration {
        id: String,
        revised_prompt: Option<String>,
        result: Option<Arc<String>>,
        saved_path: Option<String>,
        complete: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMediaLoadOutcome {
    Pending { alt: String },
    Loaded(TranscriptMediaLoadedImage),
    RenderNotSupported { alt: String },
    TooLarge { alt: String },
    FileUnavailable { alt: String },
    PathNotAllowed { alt: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptMediaLoadedImage {
    alt: String,
    format: ImageFormat,
    bytes: Arc<[u8]>,
    image: Arc<Image>,
    natural_dimensions: TranscriptMediaNaturalDimensions,
    source_path: Option<String>,
}

pub(crate) trait TranscriptMediaFileReader {
    type Error: fmt::Display;

    fn read_file_bytes(&mut self, path: &str, timeout: Duration) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Debug)]
pub(crate) struct TranscriptMediaLookup {
    pub(crate) outcome: Arc<TranscriptMediaLoadOutcome>,
    pub(crate) load_request: Option<TranscriptMediaLoadRequest>,
    pub(crate) evicted_images: Vec<Arc<Image>>,
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptMediaLoadRequest {
    pub(super) key: TranscriptMediaCacheKey,
    pub(super) fingerprint: TranscriptMediaSourceFingerprint,
    pub(super) scope_generation: u64,
    pub(super) source: TranscriptMediaSource,
    pub(super) execution_target: WorkspaceId,
    pub(super) timeout: Duration,
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptMediaLoadCompletion {
    pub(super) key: TranscriptMediaCacheKey,
    pub(super) fingerprint: TranscriptMediaSourceFingerprint,
    pub(super) scope_generation: u64,
    pub(super) outcome: TranscriptMediaLoadOutcome,
    pub(super) elapsed: Duration,
}

#[derive(Debug, Default)]
pub(crate) struct TranscriptMediaLoadCompletionResult {
    pub(crate) display_changed: bool,
    pub(crate) follow_up_request: Option<TranscriptMediaLoadRequest>,
    pub(crate) stale: bool,
    pub(crate) evicted_images: Vec<Arc<Image>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TranscriptMediaSourceFingerprint {
    hash: u64,
}

impl TranscriptMediaCacheKey {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TranscriptMediaSource {
    pub(crate) fn markdown_image(
        alt: impl Into<String>,
        destination: impl Into<String>,
        title: Option<String>,
    ) -> Self {
        Self::MarkdownImage {
            alt: normalize_alt(alt.into()),
            destination: destination.into(),
            title,
        }
    }

    pub(crate) fn native_image_generation(
        id: impl Into<String>,
        revised_prompt: Option<String>,
        result: Option<Arc<String>>,
        saved_path: Option<String>,
        complete: bool,
    ) -> Self {
        Self::NativeImageGeneration {
            id: id.into(),
            revised_prompt,
            result,
            saved_path,
            complete,
        }
    }

    pub(super) fn alt(&self) -> String {
        match self {
            Self::MarkdownImage { alt, .. } => fallback_alt(alt),
            Self::NativeImageGeneration { revised_prompt, .. } => revised_prompt
                .as_deref()
                .map(str::trim)
                .filter(|alt| !alt.is_empty())
                .unwrap_or("generated image")
                .to_string(),
        }
    }
}

impl TranscriptMediaLoadOutcome {
    pub(crate) fn fallback_text(&self) -> Option<String> {
        match self {
            Self::Pending { .. } | Self::Loaded(_) => None,
            Self::RenderNotSupported { alt } => {
                Some(status_fallback_text(alt, "render not supported"))
            }
            Self::TooLarge { alt } => Some(status_fallback_text(alt, "image too large")),
            Self::FileUnavailable { alt } => Some(status_fallback_text(alt, "file unavailable")),
            Self::PathNotAllowed { alt } => Some(status_fallback_text(alt, "path not allowed")),
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    pub(crate) fn loaded(&self) -> Option<&TranscriptMediaLoadedImage> {
        match self {
            Self::Loaded(image) => Some(image),
            _ => None,
        }
    }
}

impl TranscriptMediaLoadedImage {
    pub(super) fn new(
        alt: String,
        format: ImageFormat,
        bytes: Vec<u8>,
        image: Arc<Image>,
        natural_dimensions: TranscriptMediaNaturalDimensions,
        source_path: Option<String>,
    ) -> Self {
        Self {
            alt,
            format,
            bytes: Arc::from(bytes),
            image,
            natural_dimensions,
            source_path,
        }
    }

    pub(crate) fn alt(&self) -> &str {
        &self.alt
    }

    pub(crate) fn format(&self) -> ImageFormat {
        self.format
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    pub(crate) fn bytes_arc(&self) -> Arc<[u8]> {
        self.bytes.clone()
    }

    pub(crate) fn image(&self) -> Arc<Image> {
        self.image.clone()
    }

    pub(crate) fn image_id(&self) -> u64 {
        self.image.id()
    }

    pub(crate) fn image_asset_key_hash(&self) -> u64 {
        gpui_hash(&self.image)
    }

    pub(crate) fn natural_dimensions(&self) -> TranscriptMediaNaturalDimensions {
        self.natural_dimensions
    }

    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
}

impl TranscriptMediaFileReader for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn read_file_bytes(&mut self, path: &str, timeout: Duration) -> Result<Vec<u8>, Self::Error> {
        ManagedBackendSession::read_file_bytes(self, path, timeout)
    }
}

impl TranscriptMediaLoadRequest {
    pub(crate) fn load<R>(self, reader: &mut R) -> TranscriptMediaLoadCompletion
    where
        R: TranscriptMediaFileReader,
    {
        let started_at = Instant::now();
        let outcome =
            load_transcript_media(&self.source, &self.execution_target, reader, self.timeout);
        TranscriptMediaLoadCompletion {
            key: self.key,
            fingerprint: self.fingerprint,
            scope_generation: self.scope_generation,
            outcome,
            elapsed: started_at.elapsed(),
        }
    }
}

impl TranscriptMediaLoadCompletion {
    pub(crate) fn loaded_image(&self) -> Option<&TranscriptMediaLoadedImage> {
        self.outcome.loaded()
    }
}

impl TranscriptMediaSourceFingerprint {
    pub(super) fn new(source: &TranscriptMediaSource, execution_target: &WorkspaceId) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        match source {
            TranscriptMediaSource::MarkdownImage {
                alt,
                destination,
                title,
            } => {
                0_u8.hash(&mut hasher);
                alt.hash(&mut hasher);
                destination.hash(&mut hasher);
                title.hash(&mut hasher);
            }
            TranscriptMediaSource::NativeImageGeneration {
                id,
                revised_prompt,
                result,
                saved_path,
                complete,
            } => {
                1_u8.hash(&mut hasher);
                id.hash(&mut hasher);
                revised_prompt.hash(&mut hasher);
                saved_path.hash(&mut hasher);
                complete.hash(&mut hasher);
                if saved_path
                    .as_deref()
                    .is_none_or(|path| path.trim().is_empty())
                {
                    match result {
                        Some(result) => {
                            true.hash(&mut hasher);
                            result.len().hash(&mut hasher);
                            Arc::as_ptr(result).hash(&mut hasher);
                        }
                        None => false.hash(&mut hasher),
                    }
                } else {
                    false.hash(&mut hasher);
                }
            }
        }
        execution_target.hash(&mut hasher);
        Self {
            hash: hasher.finish(),
        }
    }
}

pub(super) fn media_load_request(
    key: TranscriptMediaCacheKey,
    fingerprint: TranscriptMediaSourceFingerprint,
    scope_generation: u64,
    source: TranscriptMediaSource,
    execution_target: WorkspaceId,
    timeout: Duration,
) -> TranscriptMediaLoadRequest {
    TranscriptMediaLoadRequest {
        key,
        fingerprint,
        scope_generation,
        source,
        execution_target,
        timeout,
    }
}

pub(super) fn fallback_alt(alt: &str) -> String {
    let alt = alt.trim();
    if alt.is_empty() {
        "image".to_string()
    } else {
        alt.to_string()
    }
}

fn normalize_alt(alt: String) -> String {
    alt.trim().to_string()
}

fn status_fallback_text(alt: &str, status: &str) -> String {
    let alt = alt.trim();
    if alt.is_empty() {
        format!("({status})")
    } else {
        format!("{alt} ({status})")
    }
}
