use std::{collections::HashSet, ops::Range};

use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ComposerDraft {
    display_text: String,
    images: Vec<ComposerDraftImage>,
    image_atoms: Vec<ComposerDraftImageAtom>,
    next_image_atom_ordinal: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerDraftImage {
    label: String,
    data: ComposerDraftImageData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerDraftImageAtom {
    atom_id: String,
    label: String,
    range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerDraftImageData {
    format: ImageFormat,
    bytes: Vec<u8>,
    asset_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ComposerImageInsertion {
    label: String,
    marker: String,
    atom_id: String,
    copy_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcceptedComposerDraft {
    display_text: String,
    parts: Vec<AcceptedComposerDraftPart>,
    image_occurrences: Vec<AcceptedComposerImageOccurrence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AcceptedComposerDraftPart {
    Text(String),
    Image(AcceptedComposerImage),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcceptedComposerImage {
    label: String,
    data: ComposerDraftImageData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AcceptedComposerImageOccurrence {
    label: String,
    range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveImageOccurrence {
    start: usize,
    end: usize,
    image: ComposerDraftImage,
}

impl ComposerDraft {
    #[allow(dead_code)]
    pub(super) fn display_text(&self) -> &str {
        &self.display_text
    }

    #[allow(dead_code)]
    pub(super) fn sync_display_text(&mut self, display_text: impl Into<String>) -> bool {
        self.sync_from_input(display_text, Vec::new())
    }

    pub(super) fn sync_from_input(
        &mut self,
        display_text: impl Into<String>,
        image_atoms: impl IntoIterator<Item = ComposerDraftImageAtom>,
    ) -> bool {
        let display_text = display_text.into();
        let image_atoms = sanitize_image_atoms(
            &display_text,
            image_atoms,
            self.images.iter().map(|image| image.label.as_str()),
        );
        self.observe_image_atom_ordinals(&image_atoms);
        let images = images_with_active_markers(&self.images, &image_atoms);
        let changed = self.display_text != display_text
            || self.image_atoms != image_atoms
            || self.images != images;

        if changed {
            self.display_text = display_text;
            self.image_atoms = image_atoms;
            self.images = images;
        }

        changed
    }

    pub(super) fn clear(&mut self) {
        self.display_text.clear();
        self.images.clear();
        self.image_atoms.clear();
        self.next_image_atom_ordinal = 0;
    }

    pub(super) fn replace_with_accepted(
        &mut self,
        accepted: &AcceptedComposerDraft,
    ) -> Vec<ComposerDraftImageAtom> {
        self.clear();
        self.display_text = accepted.display_text.clone();
        self.images = accepted
            .images()
            .map(|image| ComposerDraftImage {
                label: image.label.clone(),
                data: image.data.clone(),
            })
            .collect();

        let mut atoms = Vec::new();
        for occurrence in accepted.image_occurrences() {
            atoms.push(ComposerDraftImageAtom::new_with_atom_id(
                self.allocate_image_atom_id(occurrence.label()),
                occurrence.label().to_string(),
                occurrence.range(),
            ));
        }
        let active_labels = self
            .images
            .iter()
            .map(|image| image.label.clone())
            .collect::<Vec<_>>();
        self.image_atoms = sanitize_image_atoms(
            &self.display_text,
            atoms,
            active_labels.iter().map(String::as_str),
        );
        self.image_atoms.clone()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.display_text.is_empty() && self.active_image_occurrences().is_empty()
    }

    #[allow(dead_code)]
    pub(super) fn image_labels(&self) -> Vec<String> {
        self.image_atoms
            .iter()
            .map(|atom| atom.label.clone())
            .collect()
    }

    pub(super) fn image_atoms(&self) -> &[ComposerDraftImageAtom] {
        &self.image_atoms
    }

    pub(super) fn image_data_for_label(&self, label: &str) -> Option<&ComposerDraftImageData> {
        self.images
            .iter()
            .find(|image| image.label == label)
            .map(|image| &image.data)
    }

    pub(super) fn active_image_asset_ids(&self) -> Vec<String> {
        self.images
            .iter()
            .filter_map(|image| image.data.asset_id().map(str::to_string))
            .collect()
    }

    pub(super) fn has_active_image_marker(&self, label: &str) -> bool {
        self.images.iter().any(|image| image.label == label)
            && self.image_atoms.iter().any(|atom| atom.label == label)
    }

    pub(super) fn has_active_image_atom(&self, atom_id: &str) -> bool {
        let Some(atom) = self.image_atoms.iter().find(|atom| atom.atom_id == atom_id) else {
            return false;
        };
        self.image_data_for_label(&atom.label).is_some()
    }

    pub(super) fn remove_image_by_label(&mut self, label: &str) -> bool {
        let mut removed = false;
        let atom_ids = self
            .image_atoms
            .iter()
            .filter(|atom| atom.label == label)
            .map(|atom| atom.atom_id.clone())
            .collect::<Vec<_>>();
        for atom_id in atom_ids {
            removed |= self.remove_image_atom_by_id(&atom_id);
        }

        let before_len = self.images.len();
        self.images.retain(|image| image.label != label);
        removed || self.images.len() != before_len
    }

    pub(super) fn remove_image_atom_by_id(&mut self, atom_id: &str) -> bool {
        let Some(index) = self
            .image_atoms
            .iter()
            .position(|atom| atom.atom_id == atom_id)
        else {
            return false;
        };

        let range = self.image_atoms[index].range.clone();
        self.display_text.replace_range(range.clone(), "");
        self.image_atoms.remove(index);
        shift_image_atoms_after_removal(&mut self.image_atoms, range);
        self.prune_orphaned_images();
        true
    }

    #[allow(dead_code)]
    pub(super) fn replace_range_with_image(
        &mut self,
        range: Range<usize>,
        label: impl Into<String>,
        data: ComposerDraftImageData,
    ) -> ComposerImageInsertion {
        let insertion = self.stage_image(label, data);
        let label = insertion.label().to_string();
        let marker = composer_image_marker(&label);
        let range = clamp_to_char_boundary_range(&self.display_text, range);
        let inserted_range = range.start..range.start + marker.len();
        self.image_atoms = transform_image_atoms_after_edit(
            &self.image_atoms,
            &range,
            marker.len(),
            Some(ComposerDraftImageAtom::new_with_atom_id(
                insertion.atom_id().to_string(),
                label.clone(),
                inserted_range,
            )),
        );
        self.display_text.replace_range(range.clone(), &marker);

        insertion
    }

    pub(super) fn stage_image(
        &mut self,
        label: impl Into<String>,
        data: ComposerDraftImageData,
    ) -> ComposerImageInsertion {
        let label = label.into();
        self.set_image_payload(label.clone(), data);
        self.allocate_image_reference(&label)
    }

    pub(super) fn ensure_image_payload(
        &mut self,
        label: impl Into<String>,
        data: ComposerDraftImageData,
    ) -> bool {
        let label = label.into();
        if self.images.iter().any(|image| image.label == label) {
            return false;
        }

        self.images.push(ComposerDraftImage { label, data });
        true
    }

    pub(super) fn allocate_image_reference(&mut self, label: &str) -> ComposerImageInsertion {
        let atom_id = self.allocate_image_atom_id(label);
        ComposerImageInsertion::new(label.to_string(), atom_id)
    }

    fn set_image_payload(&mut self, label: String, data: ComposerDraftImageData) {
        if let Some(image) = self.images.iter_mut().find(|image| image.label == label) {
            image.data = data;
        } else {
            self.images.push(ComposerDraftImage { label, data });
        }
    }

    pub(super) fn accepted(&self) -> Option<AcceptedComposerDraft> {
        let occurrences = self.active_image_occurrences();
        if self.display_text.trim().is_empty() && occurrences.is_empty() {
            return None;
        }

        let mut parts = Vec::new();
        let mut cursor = 0usize;
        let mut emitted_image_labels = HashSet::new();
        let mut image_occurrences = Vec::new();
        for occurrence in occurrences {
            push_text_part(&mut parts, &self.display_text[cursor..occurrence.start]);
            image_occurrences.push(AcceptedComposerImageOccurrence {
                label: occurrence.image.label.clone(),
                range: occurrence.start..occurrence.end,
            });
            if emitted_image_labels.insert(occurrence.image.label.clone()) {
                parts.push(AcceptedComposerDraftPart::Image(AcceptedComposerImage {
                    label: occurrence.image.label,
                    data: occurrence.image.data,
                }));
            } else {
                push_text_part(
                    &mut parts,
                    &composer_image_copy_text(&occurrence.image.label),
                );
            }
            cursor = occurrence.end;
        }
        push_text_part(&mut parts, &self.display_text[cursor..]);

        Some(AcceptedComposerDraft {
            display_text: self.display_text.clone(),
            parts,
            image_occurrences,
        })
    }

    fn active_image_occurrences(&self) -> Vec<ActiveImageOccurrence> {
        let mut occurrences = Vec::new();
        for atom in &self.image_atoms {
            if let Some(image) = self.images.iter().find(|image| image.label == atom.label) {
                occurrences.push(ActiveImageOccurrence {
                    start: atom.range.start,
                    end: atom.range.end,
                    image: image.clone(),
                });
            }
        }
        occurrences.sort_by_key(|occurrence| occurrence.start);
        occurrences
    }

    fn allocate_image_atom_id(&mut self, label: &str) -> String {
        loop {
            let ordinal = self.next_image_atom_ordinal;
            self.next_image_atom_ordinal = self.next_image_atom_ordinal.saturating_add(1);
            let atom_id = composer_image_occurrence_atom_id(label, ordinal);
            if !self.image_atoms.iter().any(|atom| atom.atom_id == atom_id) {
                return atom_id;
            }
        }
    }

    fn observe_image_atom_ordinals(&mut self, image_atoms: &[ComposerDraftImageAtom]) {
        for atom in image_atoms {
            let Some(ordinal) = composer_image_atom_ordinal(&atom.atom_id) else {
                continue;
            };
            if ordinal >= self.next_image_atom_ordinal {
                self.next_image_atom_ordinal = ordinal.saturating_add(1);
            }
        }
    }

    fn prune_orphaned_images(&mut self) {
        self.images = images_with_active_markers(&self.images, &self.image_atoms);
    }
}

impl ComposerImageInsertion {
    fn new(label: impl Into<String>, atom_id: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            marker: composer_image_marker(&label),
            atom_id: atom_id.into(),
            copy_text: composer_image_copy_text(&label),
            label,
        }
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn marker(&self) -> &str {
        &self.marker
    }

    pub(super) fn atom_id(&self) -> &str {
        &self.atom_id
    }

    pub(super) fn copy_text(&self) -> &str {
        &self.copy_text
    }
}

impl ComposerDraftImageAtom {
    #[allow(dead_code)]
    pub(super) fn new(label: impl Into<String>, range: Range<usize>) -> Self {
        let label = label.into();
        Self::new_with_atom_id(composer_image_atom_id(&label), label, range)
    }

    pub(super) fn new_with_atom_id(
        atom_id: impl Into<String>,
        label: impl Into<String>,
        range: Range<usize>,
    ) -> Self {
        Self {
            atom_id: atom_id.into(),
            label: label.into(),
            range,
        }
    }

    pub(super) fn atom_id(&self) -> &str {
        &self.atom_id
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

impl AcceptedComposerDraft {
    pub(super) fn from_display_text_and_images(
        display_text: impl Into<String>,
        images: impl IntoIterator<Item = AcceptedComposerImage>,
        image_occurrences: impl IntoIterator<Item = AcceptedComposerImageOccurrence>,
    ) -> Option<Self> {
        let display_text = display_text.into();
        let mut image_occurrences = image_occurrences.into_iter().collect::<Vec<_>>();
        image_occurrences.sort_by_key(|occurrence| occurrence.range.start);
        if display_text.trim().is_empty() && image_occurrences.is_empty() {
            return None;
        }

        let images = images
            .into_iter()
            .map(|image| (image.label.clone(), image))
            .collect::<Vec<_>>();
        let mut parts = Vec::new();
        let mut cursor = 0usize;
        let mut emitted_image_labels = HashSet::new();
        for occurrence in &image_occurrences {
            push_text_part(&mut parts, &display_text[cursor..occurrence.range.start]);
            if emitted_image_labels.insert(occurrence.label.clone()) {
                let image = images
                    .iter()
                    .find(|(label, _)| label == &occurrence.label)?
                    .1
                    .clone();
                parts.push(AcceptedComposerDraftPart::Image(image));
            } else {
                push_text_part(&mut parts, &composer_image_copy_text(&occurrence.label));
            }
            cursor = occurrence.range.end;
        }
        push_text_part(&mut parts, &display_text[cursor..]);

        Some(Self {
            display_text,
            parts,
            image_occurrences,
        })
    }

    pub(super) fn display_text(&self) -> &str {
        &self.display_text
    }

    pub(super) fn contains_images(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, AcceptedComposerDraftPart::Image(_)))
    }

    pub(super) fn text_only(&self) -> Option<String> {
        if self.contains_images() {
            return None;
        }

        let mut text = String::new();
        for part in &self.parts {
            if let AcceptedComposerDraftPart::Text(value) = part {
                text.push_str(value);
            }
        }
        (!text.trim().is_empty()).then_some(text)
    }

    pub(super) fn images(&self) -> impl Iterator<Item = &AcceptedComposerImage> {
        self.parts.iter().filter_map(|part| match part {
            AcceptedComposerDraftPart::Image(image) => Some(image),
            AcceptedComposerDraftPart::Text(_) => None,
        })
    }

    pub(crate) fn image_asset_ids(&self) -> Vec<String> {
        self.images()
            .filter_map(|image| image.data().asset_id().map(str::to_string))
            .collect()
    }

    pub(crate) fn parts(&self) -> &[AcceptedComposerDraftPart] {
        &self.parts
    }

    pub(super) fn image_occurrences(&self) -> &[AcceptedComposerImageOccurrence] {
        &self.image_occurrences
    }
}

impl AcceptedComposerImage {
    pub(super) fn new(label: impl Into<String>, data: ComposerDraftImageData) -> Self {
        Self {
            label: label.into(),
            data,
        }
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn data(&self) -> &ComposerDraftImageData {
        &self.data
    }
}

impl AcceptedComposerImageOccurrence {
    pub(super) fn new(label: impl Into<String>, range: Range<usize>) -> Self {
        Self {
            label: label.into(),
            range,
        }
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

impl ComposerDraftImageData {
    pub(super) fn new(format: ImageFormat, bytes: Vec<u8>) -> Self {
        Self {
            format,
            bytes,
            asset_id: None,
        }
    }

    pub(super) fn with_asset_id(
        format: ImageFormat,
        bytes: Vec<u8>,
        asset_id: impl Into<String>,
    ) -> Self {
        Self {
            format,
            bytes,
            asset_id: Some(asset_id.into()),
        }
    }

    pub(super) fn durable_reference(format: ImageFormat, asset_id: impl Into<String>) -> Self {
        Self::with_asset_id(format, Vec::new(), asset_id)
    }

    pub(super) fn from_gpui_image(image: &Image) -> Self {
        Self::new(image.format, image.bytes.clone())
    }

    pub(super) fn format(&self) -> ImageFormat {
        self.format
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn asset_id(&self) -> Option<&str> {
        self.asset_id.as_deref()
    }
}

pub(super) fn first_clipboard_image(item: &ClipboardItem) -> Option<Image> {
    item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image.clone()),
        ClipboardEntry::String(_) => None,
    })
}

fn push_text_part(parts: &mut Vec<AcceptedComposerDraftPart>, text: &str) {
    if text.is_empty() {
        return;
    }

    parts.push(AcceptedComposerDraftPart::Text(text.to_string()));
}

pub(super) fn composer_image_marker(label: &str) -> String {
    format!("[{label}]")
}

#[allow(dead_code)]
pub(super) fn composer_image_atom_id(label: &str) -> String {
    format!("composer-image:{label}")
}

pub(super) fn composer_image_label_from_atom_id(atom_id: &str) -> Option<&str> {
    composer_image_atom_id_parts(atom_id).map(|(label, _)| label)
}

fn composer_image_occurrence_atom_id(label: &str, ordinal: u64) -> String {
    format!("composer-image:{label}:{ordinal}")
}

fn composer_image_atom_ordinal(atom_id: &str) -> Option<u64> {
    composer_image_atom_id_parts(atom_id).and_then(|(_, ordinal)| ordinal)
}

fn composer_image_atom_id_parts(atom_id: &str) -> Option<(&str, Option<u64>)> {
    let rest = atom_id.strip_prefix("composer-image:")?;
    let mut parts = rest.split(':');
    let label = parts.next()?;
    if label.is_empty() || !label.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return None;
    }

    let ordinal = match (parts.next(), parts.next()) {
        (None, None) => None,
        (Some(ordinal), None) => Some(ordinal.parse().ok()?),
        _ => return None,
    };
    Some((label, ordinal))
}

pub(super) fn composer_image_copy_text(label: &str) -> String {
    format!("[Image {label}]")
}

fn sanitize_image_atoms(
    display_text: &str,
    image_atoms: impl IntoIterator<Item = ComposerDraftImageAtom>,
    active_labels: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<ComposerDraftImageAtom> {
    let active_labels = active_labels
        .into_iter()
        .map(|label| label.as_ref().to_string())
        .collect::<HashSet<_>>();
    let mut image_atoms = image_atoms.into_iter().collect::<Vec<_>>();
    image_atoms.sort_by_key(|atom| atom.range.start);

    let mut sanitized = Vec::new();
    let mut previous_end = 0usize;
    let mut seen_atom_ids = HashSet::new();
    for atom in image_atoms {
        let atom_label = composer_image_label_from_atom_id(&atom.atom_id);
        if !active_labels.contains(&atom.label)
            || atom_label != Some(atom.label.as_str())
            || !seen_atom_ids.insert(atom.atom_id.clone())
            || atom.range.start >= atom.range.end
            || atom.range.end > display_text.len()
            || !display_text.is_char_boundary(atom.range.start)
            || !display_text.is_char_boundary(atom.range.end)
            || atom.range.start < previous_end
            || &display_text[atom.range.clone()] != composer_image_marker(&atom.label)
        {
            continue;
        }

        previous_end = atom.range.end;
        sanitized.push(atom);
    }

    sanitized
}

fn images_with_active_markers(
    images: &[ComposerDraftImage],
    image_atoms: &[ComposerDraftImageAtom],
) -> Vec<ComposerDraftImage> {
    let active_labels = image_atoms
        .iter()
        .map(|atom| atom.label.as_str())
        .collect::<HashSet<_>>();
    let mut retained_labels = HashSet::new();
    images
        .iter()
        .filter(|image| {
            active_labels.contains(image.label.as_str())
                && retained_labels.insert(image.label.clone())
        })
        .cloned()
        .collect()
}

fn transform_image_atoms_after_edit(
    atoms: &[ComposerDraftImageAtom],
    range: &Range<usize>,
    replacement_len: usize,
    inserted_atom: Option<ComposerDraftImageAtom>,
) -> Vec<ComposerDraftImageAtom> {
    let mut next_atoms =
        Vec::with_capacity(atoms.len() + if inserted_atom.is_some() { 1 } else { 0 });

    for atom in atoms {
        if atom.range.end <= range.start {
            next_atoms.push(atom.clone());
        } else if atom.range.start >= range.end {
            let start = range.start + replacement_len + (atom.range.start - range.end);
            let end = range.start + replacement_len + (atom.range.end - range.end);
            next_atoms.push(ComposerDraftImageAtom::new_with_atom_id(
                atom.atom_id.clone(),
                atom.label.clone(),
                start..end,
            ));
        }
    }

    if let Some(atom) = inserted_atom {
        next_atoms.push(atom);
    }
    next_atoms.sort_by_key(|atom| atom.range.start);
    next_atoms
}

fn shift_image_atoms_after_removal(atoms: &mut Vec<ComposerDraftImageAtom>, range: Range<usize>) {
    *atoms = transform_image_atoms_after_edit(atoms, &range, 0, None);
}

#[allow(dead_code)]
fn clamp_to_char_boundary_range(text: &str, range: Range<usize>) -> Range<usize> {
    clamp_to_char_boundary(text, range.start)..clamp_to_char_boundary(text, range.end)
}

#[allow(dead_code)]
fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
