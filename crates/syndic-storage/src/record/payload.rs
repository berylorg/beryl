use std::collections::BTreeSet;

use beryl_model::SyndicDraftMarkerId;

use crate::{ImageLabelOrdinal, SyndicRecordError};

fn validate_composer_text(kind: &'static str, value: &str) -> Result<Box<str>, SyndicRecordError> {
    if let Some(index) = value.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(SyndicRecordError::NulByte { kind, index });
    }
    Ok(value.into())
}

/// Stable identity and final per-thread label of one draft image marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerImageMarker {
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
}

impl ComposerImageMarker {
    #[must_use]
    pub const fn new(marker_id: SyndicDraftMarkerId, label: ImageLabelOrdinal) -> Self {
        Self { marker_id, label }
    }

    #[must_use]
    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }

    #[must_use]
    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }
}

/// One ordered atom in a durable composer payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerAtom {
    Text(Box<str>),
    ImageMarker(ComposerImageMarker),
}

impl ComposerAtom {
    pub fn text(value: impl AsRef<str>) -> Result<Self, SyndicRecordError> {
        validate_composer_text("composer text atom", value.as_ref()).map(Self::Text)
    }

    #[must_use]
    pub const fn image_marker(marker_id: SyndicDraftMarkerId, label: ImageLabelOrdinal) -> Self {
        Self::ImageMarker(ComposerImageMarker::new(marker_id, label))
    }

    #[must_use]
    pub fn text_value(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::ImageMarker(_) => None,
        }
    }

    #[must_use]
    pub const fn marker_id(&self) -> Option<SyndicDraftMarkerId> {
        match self {
            Self::Text(_) => None,
            Self::ImageMarker(marker) => Some(marker.marker_id()),
        }
    }

    #[must_use]
    pub const fn image_marker_value(&self) -> Option<ComposerImageMarker> {
        match self {
            Self::Text(_) => None,
            Self::ImageMarker(marker) => Some(*marker),
        }
    }
}

/// Exact ordered mutable composer content stored by one draft or accepted input.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComposerPayload {
    atoms: Vec<ComposerAtom>,
    utf8_bytes: usize,
    image_markers: usize,
}

impl ComposerPayload {
    pub fn new(atoms: Vec<ComposerAtom>) -> Result<Self, SyndicRecordError> {
        let utf8_bytes = atoms
            .iter()
            .filter_map(ComposerAtom::text_value)
            .try_fold(0usize, |total, value| total.checked_add(value.len()))
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "composer payload",
            })?;
        let image_markers = atoms
            .iter()
            .filter(|atom| matches!(atom, ComposerAtom::ImageMarker(_)))
            .count();
        ensure_unique_marker_ids(
            "composer payload",
            atoms.iter().filter_map(ComposerAtom::marker_id),
        )?;
        Ok(Self {
            atoms,
            utf8_bytes,
            image_markers,
        })
    }

    #[must_use]
    pub fn atoms(&self) -> &[ComposerAtom] {
        &self.atoms
    }

    #[must_use]
    pub const fn utf8_bytes(&self) -> usize {
        self.utf8_bytes
    }

    #[must_use]
    pub const fn image_marker_count(&self) -> usize {
        self.image_markers
    }
}

fn ensure_unique_marker_ids(
    kind: &'static str,
    marker_ids: impl IntoIterator<Item = SyndicDraftMarkerId>,
) -> Result<(), SyndicRecordError> {
    let mut seen = BTreeSet::new();
    for marker_id in marker_ids {
        if !seen.insert(marker_id) {
            return Err(SyndicRecordError::DuplicateImageMarker { kind, marker_id });
        }
    }
    Ok(())
}
