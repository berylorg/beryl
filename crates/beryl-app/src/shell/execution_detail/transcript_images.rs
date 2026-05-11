use std::{collections::HashMap, ops::Range};

use beryl_backend::UserInput;
use gpui::ImageFormat;

#[path = "transcript_images/generated_labels.rs"]
mod generated_labels;
use generated_labels::{
    GeneratedImageAnchorKey, GeneratedImageBindings, generated_image_bindings_for_records,
    generated_image_label_anchors_in_text,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptImagePathResolver {
    local_paths: HashMap<String, TranscriptImageSourceResolution>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptImageSourceResolution {
    asset_id: Option<String>,
    asset_format: Option<ImageFormat>,
    preview_state: TranscriptImagePreviewState,
    runtime_readable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptImageMarker {
    occurrence_id: String,
    label: String,
    display_range: Range<usize>,
    copy_text: String,
    source: TranscriptImageSource,
    label_source: TranscriptImageLabelSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptImageMarkerSpec {
    label: String,
    display_range: Range<usize>,
    source: TranscriptImageSource,
    label_source: TranscriptImageLabelSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptImageSource {
    input: TranscriptImageInputSource,
    asset_id: Option<String>,
    asset_format: Option<ImageFormat>,
    preview_state: TranscriptImagePreviewState,
    runtime_readable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptImageInputSource {
    LocalImage { path: String },
    RemoteImage { url: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptImagePreviewState {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptImageLabelSource {
    Generated,
    Fallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptImageFragmentParts {
    display_text: String,
    image_markers: Vec<TranscriptImageMarkerSpec>,
}

impl TranscriptImagePathResolver {
    pub(crate) fn insert_local_path_resolution(
        &mut self,
        path: impl Into<String>,
        resolution: TranscriptImageSourceResolution,
    ) {
        self.local_paths.insert(path_key(path.into()), resolution);
    }

    pub(crate) fn resolve_local_path(
        &self,
        path: &str,
    ) -> Option<&TranscriptImageSourceResolution> {
        self.local_paths.get(&path_key(path))
    }
}

impl TranscriptImageSourceResolution {
    pub(crate) fn new(
        asset_id: Option<String>,
        preview_state: TranscriptImagePreviewState,
    ) -> Self {
        let runtime_readable =
            asset_id.is_some() && preview_state == TranscriptImagePreviewState::Available;
        Self {
            asset_id,
            asset_format: None,
            preview_state,
            runtime_readable,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn available_asset(asset_id: impl Into<String>) -> Self {
        Self::new(
            Some(asset_id.into()),
            TranscriptImagePreviewState::Available,
        )
    }

    pub(crate) fn available_asset_with_format(
        asset_id: impl Into<String>,
        asset_format: ImageFormat,
        runtime_readable: bool,
    ) -> Self {
        Self {
            asset_id: Some(asset_id.into()),
            asset_format: Some(asset_format),
            preview_state: TranscriptImagePreviewState::Available,
            runtime_readable,
        }
    }

    pub(crate) fn unavailable_asset(asset_id: impl Into<String>) -> Self {
        Self {
            asset_id: Some(asset_id.into()),
            asset_format: None,
            preview_state: TranscriptImagePreviewState::Unavailable,
            runtime_readable: false,
        }
    }

    fn unavailable() -> Self {
        Self::new(None, TranscriptImagePreviewState::Unavailable)
    }

    #[allow(dead_code)]
    pub(crate) fn asset_id(&self) -> Option<&str> {
        self.asset_id.as_deref()
    }

    #[allow(dead_code)]
    pub(crate) fn preview_state(&self) -> TranscriptImagePreviewState {
        self.preview_state
    }

    #[allow(dead_code)]
    pub(crate) fn asset_format(&self) -> Option<ImageFormat> {
        self.asset_format
    }

    #[allow(dead_code)]
    pub(crate) fn runtime_readable(&self) -> bool {
        self.runtime_readable
    }
}

impl TranscriptImageMarker {
    pub(crate) fn from_spec(
        fragment_id: u64,
        occurrence_index: usize,
        spec: TranscriptImageMarkerSpec,
    ) -> Self {
        Self {
            occurrence_id: format!("fragment:{fragment_id}:image:{occurrence_index}"),
            copy_text: image_copy_text(&spec.label),
            label: spec.label,
            display_range: spec.display_range,
            source: spec.source,
            label_source: spec.label_source,
        }
    }

    pub(crate) fn to_spec(&self) -> TranscriptImageMarkerSpec {
        TranscriptImageMarkerSpec {
            label: self.label.clone(),
            display_range: self.display_range.clone(),
            source: self.source.clone(),
            label_source: self.label_source,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn occurrence_id(&self) -> &str {
        &self.occurrence_id
    }

    #[allow(dead_code)]
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    #[allow(dead_code)]
    pub(crate) fn display_range(&self) -> Range<usize> {
        self.display_range.clone()
    }

    #[allow(dead_code)]
    pub(crate) fn copy_text(&self) -> &str {
        &self.copy_text
    }

    #[allow(dead_code)]
    pub(crate) fn source(&self) -> &TranscriptImageSource {
        &self.source
    }

    #[allow(dead_code)]
    pub(crate) fn label_source(&self) -> TranscriptImageLabelSource {
        self.label_source
    }
}

impl TranscriptImageMarkerSpec {
    pub(crate) fn new(
        label: impl Into<String>,
        display_range: Range<usize>,
        source: TranscriptImageSource,
    ) -> Self {
        Self::with_label_source(
            label,
            display_range,
            source,
            TranscriptImageLabelSource::Generated,
        )
    }

    pub(crate) fn with_label_source(
        label: impl Into<String>,
        display_range: Range<usize>,
        source: TranscriptImageSource,
        label_source: TranscriptImageLabelSource,
    ) -> Self {
        Self {
            label: label.into(),
            display_range,
            source,
            label_source,
        }
    }
}

impl TranscriptImageSource {
    fn new(input: TranscriptImageInputSource, resolution: TranscriptImageSourceResolution) -> Self {
        Self {
            input,
            asset_id: resolution.asset_id,
            asset_format: resolution.asset_format,
            preview_state: resolution.preview_state,
            runtime_readable: resolution.runtime_readable,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn input(&self) -> &TranscriptImageInputSource {
        &self.input
    }

    #[allow(dead_code)]
    pub(crate) fn asset_id(&self) -> Option<&str> {
        self.asset_id.as_deref()
    }

    #[allow(dead_code)]
    pub(crate) fn preview_state(&self) -> TranscriptImagePreviewState {
        self.preview_state
    }

    #[allow(dead_code)]
    pub(crate) fn asset_format(&self) -> Option<ImageFormat> {
        self.asset_format
    }

    #[allow(dead_code)]
    pub(crate) fn runtime_readable(&self) -> bool {
        self.runtime_readable
    }
}

impl TranscriptImageFragmentParts {
    pub(crate) fn new(
        display_text: impl Into<String>,
        image_markers: Vec<TranscriptImageMarkerSpec>,
    ) -> Self {
        Self {
            display_text: display_text.into(),
            image_markers,
        }
    }

    pub(crate) fn display_text(&self) -> &str {
        &self.display_text
    }

    pub(crate) fn into_image_markers(self) -> Vec<TranscriptImageMarkerSpec> {
        self.image_markers
    }
}

pub(crate) fn transcript_image_parts_for_backend_records(
    records: &[UserInput],
    resolver: &TranscriptImagePathResolver,
) -> TranscriptImageFragmentParts {
    let generated_bindings = generated_image_bindings_for_records(records, resolver);
    let mut text = String::new();
    let mut markers = Vec::new();
    let mut next_fallback_image_index = 0usize;
    let mut image_sources_by_label = HashMap::new();

    for (index, input) in records.iter().enumerate() {
        if let UserInput::Text { text: input_text } = input {
            push_text_with_bound_generated_image_anchors(
                &mut text,
                &mut markers,
                input_text,
                index,
                &generated_bindings,
                &mut image_sources_by_label,
                &mut next_fallback_image_index,
            );
        } else if generated_bindings.image_record_indexes.contains(&index) {
            continue;
        } else if let Some(source) = transcript_image_source_for_input(input, resolver) {
            let label =
                next_fallback_image_label(&mut next_fallback_image_index, &image_sources_by_label);
            push_marker(
                &mut text,
                &mut markers,
                &label,
                source.clone(),
                TranscriptImageLabelSource::Fallback,
            );
            image_sources_by_label.insert(label, source);
        } else if let Some(input_text) = display_text_for_user_input(input) {
            text.push_str(&input_text);
        }
    }

    TranscriptImageFragmentParts::new(text, markers)
}

pub(crate) fn transcript_image_markers_from_specs(
    fragment_id: u64,
    specs: Vec<TranscriptImageMarkerSpec>,
) -> Vec<TranscriptImageMarker> {
    specs
        .into_iter()
        .enumerate()
        .map(|(index, spec)| TranscriptImageMarker::from_spec(fragment_id, index, spec))
        .collect()
}

pub(crate) fn transcript_image_marker_specs_from_markers(
    markers: &[TranscriptImageMarker],
) -> Vec<TranscriptImageMarkerSpec> {
    markers.iter().map(TranscriptImageMarker::to_spec).collect()
}

pub(crate) fn transcript_image_source_from_local_image(
    path: impl Into<String>,
    asset_id: Option<String>,
    preview_state: TranscriptImagePreviewState,
) -> TranscriptImageSource {
    TranscriptImageSource::new(
        TranscriptImageInputSource::LocalImage { path: path.into() },
        TranscriptImageSourceResolution::new(asset_id, preview_state),
    )
}

pub(crate) fn transcript_image_source_from_local_image_with_format(
    path: impl Into<String>,
    asset_id: String,
    asset_format: ImageFormat,
) -> TranscriptImageSource {
    TranscriptImageSource::new(
        TranscriptImageInputSource::LocalImage { path: path.into() },
        TranscriptImageSourceResolution::available_asset_with_format(asset_id, asset_format, true),
    )
}

fn transcript_image_source_for_input(
    input: &UserInput,
    resolver: &TranscriptImagePathResolver,
) -> Option<TranscriptImageSource> {
    match input {
        UserInput::LocalImage { path } => {
            let resolution = resolver
                .resolve_local_path(path)
                .cloned()
                .unwrap_or_else(TranscriptImageSourceResolution::unavailable);
            Some(TranscriptImageSource::new(
                TranscriptImageInputSource::LocalImage { path: path.clone() },
                resolution,
            ))
        }
        UserInput::Image { url } => Some(TranscriptImageSource::new(
            TranscriptImageInputSource::RemoteImage { url: url.clone() },
            TranscriptImageSourceResolution::unavailable(),
        )),
        UserInput::Text { .. } | UserInput::Skill { .. } | UserInput::Mention { .. } => None,
    }
}

fn push_marker(
    text: &mut String,
    markers: &mut Vec<TranscriptImageMarkerSpec>,
    label: &str,
    source: TranscriptImageSource,
    label_source: TranscriptImageLabelSource,
) {
    let marker = image_marker_text(label);
    let start = text.len();
    text.push_str(&marker);
    markers.push(TranscriptImageMarkerSpec::with_label_source(
        label.to_string(),
        start..text.len(),
        source,
        label_source,
    ));
}

fn display_text_for_user_input(input: &UserInput) -> Option<String> {
    match input {
        UserInput::Text { text } => Some(text.clone()),
        UserInput::Image { url } => Some(format!("Image: {url}")),
        UserInput::LocalImage { path } => Some(format!("Local image: {path}")),
        UserInput::Skill { name, path } => Some(format!("Skill: {name} ({path})")),
        UserInput::Mention { name, path } => Some(format!("Mention: {name} ({path})")),
    }
}

fn push_text_with_bound_generated_image_anchors(
    text: &mut String,
    markers: &mut Vec<TranscriptImageMarkerSpec>,
    input_text: &str,
    record_index: usize,
    generated_bindings: &GeneratedImageBindings,
    image_sources_by_label: &mut HashMap<String, TranscriptImageSource>,
    next_fallback_image_index: &mut usize,
) {
    let mut cursor = 0usize;
    for anchor in generated_image_label_anchors_in_text(input_text) {
        let key = GeneratedImageAnchorKey {
            record_index,
            start: anchor.range.start,
        };
        let Some(binding) = generated_bindings.anchors.get(&key) else {
            continue;
        };

        push_text_with_generated_image_references(
            text,
            markers,
            &input_text[cursor..anchor.range.start],
            image_sources_by_label,
        );
        push_marker(
            text,
            markers,
            &binding.label,
            binding.source.clone(),
            TranscriptImageLabelSource::Generated,
        );
        image_sources_by_label.insert(binding.label.clone(), binding.source.clone());
        observe_fallback_image_label(next_fallback_image_index, &binding.label);
        cursor = anchor.range.end;
    }

    push_text_with_generated_image_references(
        text,
        markers,
        &input_text[cursor..],
        image_sources_by_label,
    );
}

fn push_text_with_generated_image_references(
    text: &mut String,
    markers: &mut Vec<TranscriptImageMarkerSpec>,
    input_text: &str,
    image_sources_by_label: &HashMap<String, TranscriptImageSource>,
) {
    let mut cursor = 0usize;
    while let Some(relative_start) = input_text[cursor..].find("[Image ") {
        let start = cursor + relative_start;
        let label_start = start + "[Image ".len();
        let Some(relative_end) = input_text[label_start..].find(']') else {
            break;
        };
        let end = label_start + relative_end;
        let label = &input_text[label_start..end];
        if valid_generated_image_label(label)
            && let Some(source) = image_sources_by_label.get(label)
        {
            text.push_str(&input_text[cursor..start]);
            push_marker(
                text,
                markers,
                label,
                source.clone(),
                TranscriptImageLabelSource::Generated,
            );
            cursor = end + ']'.len_utf8();
            continue;
        }

        text.push_str(&input_text[cursor..start + '['.len_utf8()]);
        cursor = start + '['.len_utf8();
    }
    text.push_str(&input_text[cursor..]);
}

fn valid_generated_image_label(label: &str) -> bool {
    !label.is_empty() && label.chars().all(|ch| ch.is_ascii_uppercase())
}

fn next_fallback_image_label(
    next_fallback_image_index: &mut usize,
    image_sources_by_label: &HashMap<String, TranscriptImageSource>,
) -> String {
    loop {
        let label = image_label_for_index(*next_fallback_image_index);
        *next_fallback_image_index = (*next_fallback_image_index).saturating_add(1);
        if !image_sources_by_label.contains_key(&label) {
            return label;
        }
    }
}

fn observe_fallback_image_label(next_fallback_image_index: &mut usize, label: &str) {
    if let Some(index) = image_label_index(label) {
        *next_fallback_image_index = (*next_fallback_image_index).max(index.saturating_add(1));
    }
}

fn image_marker_text(label: &str) -> String {
    format!("[{label}]")
}

fn image_copy_text(label: &str) -> String {
    format!("[Image {label}]")
}

fn image_label_for_index(mut index: usize) -> String {
    let mut label = Vec::new();
    loop {
        let remainder = index % 26;
        label.push((b'A' + remainder as u8) as char);
        if index < 26 {
            break;
        }
        index = (index / 26) - 1;
    }
    label.iter().rev().collect()
}

fn image_label_index(label: &str) -> Option<usize> {
    if label.is_empty() || !label.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return None;
    }

    let mut one_based = 0usize;
    for byte in label.bytes() {
        let value = usize::from(byte - b'A' + 1);
        one_based = one_based.checked_mul(26)?.checked_add(value)?;
    }
    one_based.checked_sub(1)
}

fn path_key(path: impl AsRef<str>) -> String {
    let mut key = path.as_ref().replace('\\', "/");
    let bytes = key.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/' {
        let drive = key[..1].to_ascii_lowercase();
        key.replace_range(0..1, &drive);
    }
    key
}
