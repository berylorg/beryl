use std::collections::{BTreeMap, BTreeSet};

use beryl_model::{AssetId, SyndicDraftMarkerId};

use super::MAX_COMPOSER_IMAGE_MARKERS;
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
        if image_markers > MAX_COMPOSER_IMAGE_MARKERS {
            return Err(SyndicRecordError::TooManyImageMarkers {
                kind: "composer payload",
                maximum: MAX_COMPOSER_IMAGE_MARKERS,
                actual: image_markers,
            });
        }
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

/// Immutable marker, label, and durable asset resolution admitted with input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedImageMarker {
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl ResolvedImageMarker {
    #[must_use]
    pub const fn new(
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        asset_id: AssetId,
    ) -> Self {
        Self {
            marker_id,
            label,
            asset_id,
        }
    }

    #[must_use]
    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }

    #[must_use]
    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }

    #[must_use]
    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }
}

/// One exact ordered text or resolved image atom after durable admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmittedComposerAtom {
    Text(Box<str>),
    ImageMarker(ResolvedImageMarker),
}

impl SubmittedComposerAtom {
    pub fn text(value: impl AsRef<str>) -> Result<Self, SyndicRecordError> {
        validate_composer_text("submitted composer text atom", value.as_ref()).map(Self::Text)
    }

    #[must_use]
    pub const fn image_marker(marker: ResolvedImageMarker) -> Self {
        Self::ImageMarker(marker)
    }

    #[must_use]
    pub fn text_value(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::ImageMarker(_) => None,
        }
    }

    #[must_use]
    pub const fn marker(&self) -> Option<ResolvedImageMarker> {
        match self {
            Self::Text(_) => None,
            Self::ImageMarker(marker) => Some(*marker),
        }
    }
}

/// Exact ordered submitted content with every image marker durably resolved.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubmittedComposerPayload {
    atoms: Vec<SubmittedComposerAtom>,
    utf8_bytes: usize,
    image_markers: usize,
}

impl SubmittedComposerPayload {
    /// Resolves every draft marker in exact atom order without changing text.
    ///
    /// The supplied marker vector must contain exactly one entry for each
    /// draft image atom, in the same order, with identical marker identity and
    /// final label. Missing, extra, duplicate, or reordered facts are rejected.
    pub fn resolve(
        draft: &ComposerPayload,
        resolved_markers: Vec<ResolvedImageMarker>,
    ) -> Result<Self, SyndicRecordError> {
        if resolved_markers.len() != draft.image_marker_count() {
            return Err(SyndicRecordError::MarkerResolutionCountMismatch {
                expected: draft.image_marker_count(),
                actual: resolved_markers.len(),
            });
        }
        ensure_unique_marker_ids(
            "submitted marker resolutions",
            resolved_markers.iter().map(|marker| marker.marker_id()),
        )?;

        let mut resolved = resolved_markers.into_iter();
        let mut atoms = Vec::with_capacity(draft.atoms().len());
        for (atom_index, atom) in draft.atoms().iter().enumerate() {
            match atom {
                ComposerAtom::Text(text) => {
                    atoms.push(SubmittedComposerAtom::Text(text.clone()));
                }
                ComposerAtom::ImageMarker(expected) => {
                    let marker = resolved.next().ok_or(
                        SyndicRecordError::MarkerResolutionCountMismatch {
                            expected: draft.image_marker_count(),
                            actual: draft.image_marker_count().saturating_sub(1),
                        },
                    )?;
                    if marker.marker_id() != expected.marker_id()
                        || marker.label() != expected.label()
                    {
                        return Err(SyndicRecordError::MarkerResolutionMismatch { atom_index });
                    }
                    atoms.push(SubmittedComposerAtom::ImageMarker(marker));
                }
            }
        }
        debug_assert!(resolved.next().is_none());
        Self::from_atoms(atoms)
    }

    pub(crate) fn from_atoms(atoms: Vec<SubmittedComposerAtom>) -> Result<Self, SyndicRecordError> {
        let utf8_bytes = atoms
            .iter()
            .filter_map(SubmittedComposerAtom::text_value)
            .try_fold(0usize, |total, value| total.checked_add(value.len()))
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "submitted composer payload",
            })?;
        let image_markers = atoms
            .iter()
            .filter(|atom| matches!(atom, SubmittedComposerAtom::ImageMarker(_)))
            .count();
        if image_markers > MAX_COMPOSER_IMAGE_MARKERS {
            return Err(SyndicRecordError::TooManyImageMarkers {
                kind: "submitted composer payload",
                maximum: MAX_COMPOSER_IMAGE_MARKERS,
                actual: image_markers,
            });
        }
        ensure_unique_marker_ids(
            "submitted composer payload",
            atoms
                .iter()
                .filter_map(SubmittedComposerAtom::marker)
                .map(ResolvedImageMarker::marker_id),
        )?;
        validate_label_asset_agreement(&atoms)?;
        Ok(Self {
            atoms,
            utf8_bytes,
            image_markers,
        })
    }

    #[must_use]
    pub fn atoms(&self) -> &[SubmittedComposerAtom] {
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

fn validate_label_asset_agreement(
    atoms: &[SubmittedComposerAtom],
) -> Result<(), SyndicRecordError> {
    let mut assets_by_label = BTreeMap::new();
    for marker in atoms.iter().filter_map(SubmittedComposerAtom::marker) {
        match assets_by_label.get(&marker.label()) {
            Some(asset_id) if *asset_id != marker.asset_id() => {
                return Err(SyndicRecordError::LabelAssetMismatch {
                    label: marker.label(),
                });
            }
            Some(_) => {}
            None => {
                assets_by_label.insert(marker.label(), marker.asset_id());
            }
        }
    }
    Ok(())
}
