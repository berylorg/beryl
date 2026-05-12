use std::{
    collections::{HashMap, HashSet, VecDeque},
    ops::Range,
};

use gpui::{ClipboardEntry, ClipboardItem};
use serde::{Deserialize, Serialize};

use super::composer_draft::{
    ComposerDraftImageData, composer_image_copy_text, composer_image_marker,
};
use crate::text_input::TextInputSelectionAtom;

const COMPOSER_CLIPBOARD_METADATA_MARKER: &str = "beryl.composer.image-selection";
const COMPOSER_CLIPBOARD_METADATA_VERSION: u32 = 1;
const DEFAULT_MAX_COMPOSER_CLIPBOARD_PAYLOADS: usize = 32;
pub(super) const DEFAULT_MAX_COMPOSER_CLIPBOARD_IMAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct ComposerClipboardStore {
    next_token: u64,
    payloads: HashMap<String, ComposerClipboardPayload>,
    token_order: VecDeque<String>,
    max_payloads: usize,
    max_image_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerClipboardPayload {
    selected_display_text: String,
    fallback_text: String,
    source_label_scope: ComposerClipboardLabelScope,
    atoms: Vec<ComposerClipboardAtom>,
    images: Vec<ComposerClipboardImage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ComposerClipboardLabelScope {
    PendingNewThread(u64),
    Thread(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerClipboardAtom {
    label: String,
    range: Range<usize>,
    display_text: String,
    fallback_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerClipboardImage {
    label: String,
    data: ComposerDraftImageData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ComposerClipboardPayloadError {
    EmptyAtomSelection,
    InvalidAtomId,
    InvalidAtomRange,
    InvalidAtomMarker,
    DuplicateImageLabel,
    MissingImageData,
    ExtraImageData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerClipboardPastePlan {
    display_text: String,
    atoms: Vec<TextInputSelectionAtom>,
    images: Vec<ComposerClipboardPastedImage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerClipboardPastedImage {
    label: String,
    data: ComposerDraftImageData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ComposerClipboardPastePlanError {
    MissingLabelMapping,
    DuplicateTargetLabel,
    InvalidTargetLabel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ComposerClipboardRetainedCounts {
    pub(super) payloads: usize,
    pub(super) tokens: usize,
    pub(super) token_bytes: usize,
    pub(super) selected_text_bytes: usize,
    pub(super) fallback_text_bytes: usize,
    pub(super) label_scope_bytes: usize,
    pub(super) atom_count: usize,
    pub(super) atom_bytes: usize,
    pub(super) image_count: usize,
    pub(super) image_bytes: usize,
    pub(super) image_label_bytes: usize,
    pub(super) image_asset_id_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ComposerClipboardMetadata {
    marker: String,
    version: u32,
    token: String,
}

impl Default for ComposerClipboardStore {
    fn default() -> Self {
        Self {
            next_token: 0,
            payloads: HashMap::new(),
            token_order: VecDeque::new(),
            max_payloads: DEFAULT_MAX_COMPOSER_CLIPBOARD_PAYLOADS,
            max_image_bytes: DEFAULT_MAX_COMPOSER_CLIPBOARD_IMAGE_BYTES,
        }
    }
}

impl ComposerClipboardStore {
    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn with_capacity(max_payloads: usize) -> Self {
        Self {
            max_payloads,
            ..Self::default()
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn with_limits(max_payloads: usize, max_image_bytes: usize) -> Self {
        Self {
            max_payloads: max_payloads.max(1),
            max_image_bytes: max_image_bytes.max(1),
            ..Self::default()
        }
    }

    pub(super) fn store_payload(&mut self, payload: ComposerClipboardPayload) -> ClipboardItem {
        let token = self.allocate_token();
        let item = ClipboardItem::new_string_with_json_metadata(
            payload.fallback_text().to_string(),
            ComposerClipboardMetadata::new(token.clone()),
        );
        self.payloads.insert(token.clone(), payload);
        self.token_order.push_back(token);
        self.enforce_capacity();
        item
    }

    pub(super) fn resolve_payload(&self, item: &ClipboardItem) -> Option<ComposerClipboardPayload> {
        let metadata = composer_clipboard_metadata(item)?;
        let payload = self.payloads.get(&metadata.token)?;
        (metadata.marker == COMPOSER_CLIPBOARD_METADATA_MARKER
            && metadata.version == COMPOSER_CLIPBOARD_METADATA_VERSION
            && item.text().as_deref() == Some(payload.fallback_text()))
        .then(|| payload.clone())
    }

    pub(super) fn retained_counts(&self) -> ComposerClipboardRetainedCounts {
        let mut counts = ComposerClipboardRetainedCounts {
            payloads: self.payloads.len(),
            tokens: self.token_order.len(),
            token_bytes: self
                .payloads
                .keys()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(self.token_order.iter().map(String::len).sum::<usize>()),
            ..ComposerClipboardRetainedCounts::default()
        };

        for payload in self.payloads.values() {
            counts.add_payload(payload);
        }

        counts
    }

    fn allocate_token(&mut self) -> String {
        let token = format!("composer-clipboard-{token:016x}", token = self.next_token);
        self.next_token = self.next_token.wrapping_add(1);
        token
    }

    fn enforce_capacity(&mut self) {
        while self.token_order.len() > self.max_payloads
            || self.retained_image_bytes() > self.max_image_bytes
        {
            if let Some(token) = self.token_order.pop_front() {
                self.payloads.remove(&token);
            } else {
                break;
            }
        }
    }

    fn retained_image_bytes(&self) -> usize {
        self.payloads
            .values()
            .map(ComposerClipboardPayload::image_bytes)
            .sum()
    }
}

impl ComposerClipboardRetainedCounts {
    fn add_payload(&mut self, payload: &ComposerClipboardPayload) {
        self.selected_text_bytes = self
            .selected_text_bytes
            .saturating_add(payload.selected_display_text.len());
        self.fallback_text_bytes = self
            .fallback_text_bytes
            .saturating_add(payload.fallback_text.len());
        self.label_scope_bytes = self
            .label_scope_bytes
            .saturating_add(label_scope_retained_bytes(&payload.source_label_scope));
        self.atom_count = self.atom_count.saturating_add(payload.atoms.len());
        self.atom_bytes = self.atom_bytes.saturating_add(
            payload
                .atoms
                .iter()
                .map(composer_clipboard_atom_retained_bytes)
                .sum::<usize>(),
        );
        self.image_count = self.image_count.saturating_add(payload.images.len());
        self.image_bytes = self.image_bytes.saturating_add(
            payload
                .images
                .iter()
                .map(|image| image.data.bytes().len())
                .sum::<usize>(),
        );
        self.image_label_bytes = self.image_label_bytes.saturating_add(
            payload
                .images
                .iter()
                .map(|image| image.label.len())
                .sum::<usize>(),
        );
        self.image_asset_id_bytes = self.image_asset_id_bytes.saturating_add(
            payload
                .images
                .iter()
                .map(|image| image.data.asset_id().map_or(0, str::len))
                .sum::<usize>(),
        );
    }
}

impl ComposerClipboardPayload {
    pub(super) fn new(
        selected_display_text: impl Into<String>,
        fallback_text: impl Into<String>,
        source_label_scope: ComposerClipboardLabelScope,
        atoms: Vec<ComposerClipboardAtom>,
        images: Vec<ComposerClipboardImage>,
    ) -> Result<Self, ComposerClipboardPayloadError> {
        let payload = Self {
            selected_display_text: selected_display_text.into(),
            fallback_text: fallback_text.into(),
            source_label_scope,
            atoms,
            images,
        };
        payload.validate()?;
        Ok(payload)
    }

    #[allow(dead_code)]
    pub(super) fn selected_display_text(&self) -> &str {
        &self.selected_display_text
    }

    pub(super) fn fallback_text(&self) -> &str {
        &self.fallback_text
    }

    #[allow(dead_code)]
    pub(super) fn source_label_scope(&self) -> &ComposerClipboardLabelScope {
        &self.source_label_scope
    }

    #[allow(dead_code)]
    pub(super) fn atoms(&self) -> &[ComposerClipboardAtom] {
        &self.atoms
    }

    #[allow(dead_code)]
    pub(super) fn images(&self) -> &[ComposerClipboardImage] {
        &self.images
    }

    #[allow(dead_code)]
    pub(super) fn image_data_for_label(&self, label: &str) -> Option<&ComposerDraftImageData> {
        self.images
            .iter()
            .find(|image| image.label == label)
            .map(|image| &image.data)
    }

    fn validate(&self) -> Result<(), ComposerClipboardPayloadError> {
        if self.atoms.is_empty() {
            return Err(ComposerClipboardPayloadError::EmptyAtomSelection);
        }

        validate_atoms(&self.selected_display_text, &self.atoms)?;
        validate_images(&self.atoms, &self.images)?;
        Ok(())
    }

    fn image_bytes(&self) -> usize {
        self.images
            .iter()
            .map(|image| image.data.bytes().len())
            .sum()
    }
}

impl ComposerClipboardLabelScope {
    pub(super) fn for_selected_thread(
        selected_thread_id: Option<&str>,
        pending_new_thread_scope_id: u64,
    ) -> Self {
        match selected_thread_id {
            Some(thread_id) => Self::Thread(thread_id.to_string()),
            None => Self::PendingNewThread(pending_new_thread_scope_id),
        }
    }
}

impl ComposerClipboardAtom {
    pub(super) fn new(
        label: impl Into<String>,
        range: Range<usize>,
        display_text: impl Into<String>,
        fallback_text: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            range,
            display_text: display_text.into(),
            fallback_text: fallback_text.into(),
        }
    }

    #[allow(dead_code)]
    pub(super) fn label(&self) -> &str {
        &self.label
    }

    #[allow(dead_code)]
    pub(super) fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    #[allow(dead_code)]
    pub(super) fn display_text(&self) -> &str {
        &self.display_text
    }

    #[allow(dead_code)]
    pub(super) fn fallback_text(&self) -> &str {
        &self.fallback_text
    }
}

impl ComposerClipboardImage {
    pub(super) fn new(label: impl Into<String>, data: ComposerDraftImageData) -> Self {
        Self {
            label: label.into(),
            data,
        }
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    #[allow(dead_code)]
    pub(super) fn data(&self) -> &ComposerDraftImageData {
        &self.data
    }
}

impl ComposerClipboardPastePlan {
    pub(super) fn new(
        payload: &ComposerClipboardPayload,
        label_mapping: &HashMap<String, String>,
        mut allocate_atom_id: impl FnMut(&str) -> String,
    ) -> Result<Self, ComposerClipboardPastePlanError> {
        validate_label_mapping(payload, label_mapping)?;

        let mut display_text = String::new();
        let mut atoms = Vec::new();
        let mut source_atoms = payload.atoms.iter().collect::<Vec<_>>();
        source_atoms.sort_by_key(|atom| atom.range.start);
        let mut cursor = 0usize;

        for atom in source_atoms {
            display_text.push_str(&payload.selected_display_text[cursor..atom.range.start]);
            let target_label = label_mapping
                .get(atom.label())
                .ok_or(ComposerClipboardPastePlanError::MissingLabelMapping)?;
            let marker = composer_image_marker(target_label);
            let start = display_text.len();
            display_text.push_str(&marker);
            let end = display_text.len();
            atoms.push(TextInputSelectionAtom::new(
                allocate_atom_id(target_label),
                start..end,
                marker,
                composer_image_copy_text(target_label),
            ));
            cursor = atom.range.end;
        }

        display_text.push_str(&payload.selected_display_text[cursor..]);
        let images = payload
            .images
            .iter()
            .map(|image| {
                let label = label_mapping
                    .get(image.label())
                    .ok_or(ComposerClipboardPastePlanError::MissingLabelMapping)?;
                Ok(ComposerClipboardPastedImage {
                    label: label.clone(),
                    data: image.data.clone(),
                })
            })
            .collect::<Result<Vec<_>, ComposerClipboardPastePlanError>>()?;

        Ok(Self {
            display_text,
            atoms,
            images,
        })
    }

    pub(super) fn display_text(&self) -> &str {
        &self.display_text
    }

    pub(super) fn atoms(&self) -> &[TextInputSelectionAtom] {
        &self.atoms
    }

    pub(super) fn images(&self) -> &[ComposerClipboardPastedImage] {
        &self.images
    }
}

impl ComposerClipboardPastedImage {
    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn data(&self) -> &ComposerDraftImageData {
        &self.data
    }
}

impl ComposerClipboardMetadata {
    fn new(token: String) -> Self {
        Self {
            marker: COMPOSER_CLIPBOARD_METADATA_MARKER.to_string(),
            version: COMPOSER_CLIPBOARD_METADATA_VERSION,
            token,
        }
    }
}

fn composer_clipboard_metadata(item: &ClipboardItem) -> Option<ComposerClipboardMetadata> {
    let [ClipboardEntry::String(string)] = item.entries() else {
        return None;
    };
    string.metadata_json()
}

fn validate_atoms(
    selected_display_text: &str,
    atoms: &[ComposerClipboardAtom],
) -> Result<(), ComposerClipboardPayloadError> {
    let mut sorted_atoms = atoms.iter().collect::<Vec<_>>();
    sorted_atoms.sort_by_key(|atom| atom.range.start);

    let mut previous_end = 0usize;
    for atom in sorted_atoms {
        if atom.label.is_empty() || !atom.label.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(ComposerClipboardPayloadError::InvalidAtomId);
        }
        if atom.range.start >= atom.range.end
            || atom.range.end > selected_display_text.len()
            || !selected_display_text.is_char_boundary(atom.range.start)
            || !selected_display_text.is_char_boundary(atom.range.end)
            || atom.range.start < previous_end
            || &selected_display_text[atom.range.clone()] != atom.display_text
        {
            return Err(ComposerClipboardPayloadError::InvalidAtomRange);
        }
        if atom.display_text != composer_image_marker(&atom.label)
            || atom.fallback_text != composer_image_copy_text(&atom.label)
        {
            return Err(ComposerClipboardPayloadError::InvalidAtomMarker);
        }

        previous_end = atom.range.end;
    }
    Ok(())
}

fn validate_images(
    atoms: &[ComposerClipboardAtom],
    images: &[ComposerClipboardImage],
) -> Result<(), ComposerClipboardPayloadError> {
    let mut atom_labels = HashSet::new();
    for atom in atoms {
        atom_labels.insert(atom.label.clone());
    }

    let mut image_labels = HashSet::new();
    for image in images {
        if !image_labels.insert(image.label.clone()) {
            return Err(ComposerClipboardPayloadError::DuplicateImageLabel);
        }
    }

    if !atom_labels.iter().all(|label| image_labels.contains(label)) {
        return Err(ComposerClipboardPayloadError::MissingImageData);
    }
    if !image_labels.iter().all(|label| atom_labels.contains(label)) {
        return Err(ComposerClipboardPayloadError::ExtraImageData);
    }
    Ok(())
}

fn validate_label_mapping(
    payload: &ComposerClipboardPayload,
    label_mapping: &HashMap<String, String>,
) -> Result<(), ComposerClipboardPastePlanError> {
    let mut source_labels = HashSet::new();
    for image in payload.images() {
        source_labels.insert(image.label().to_string());
    }

    let mut target_labels = HashSet::new();
    for source_label in source_labels {
        let Some(target_label) = label_mapping.get(&source_label) else {
            return Err(ComposerClipboardPastePlanError::MissingLabelMapping);
        };
        if target_label.is_empty() || !target_label.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(ComposerClipboardPastePlanError::InvalidTargetLabel);
        }
        if !target_labels.insert(target_label.clone()) {
            return Err(ComposerClipboardPastePlanError::DuplicateTargetLabel);
        }
    }

    Ok(())
}

fn label_scope_retained_bytes(scope: &ComposerClipboardLabelScope) -> usize {
    match scope {
        ComposerClipboardLabelScope::PendingNewThread(_) => 0,
        ComposerClipboardLabelScope::Thread(thread_id) => thread_id.len(),
    }
}

fn composer_clipboard_atom_retained_bytes(atom: &ComposerClipboardAtom) -> usize {
    atom.label
        .len()
        .saturating_add(atom.display_text.len())
        .saturating_add(atom.fallback_text.len())
}
