use std::{collections::HashMap, sync::Arc};

use beryl_backend::UserInput;

use super::{
    composer_draft::{
        AcceptedComposerDraft, AcceptedComposerImage, AcceptedComposerImageOccurrence,
        ComposerDraftImageData, composer_image_marker,
    },
    composer_image_labels::ComposerImagePasteReadiness,
    execution_detail::{
        TranscriptImageInputSource, TranscriptImageLabelSource, TranscriptImagePreviewState,
        TranscriptImageSource,
    },
    execution_detail::{TurnExecutionRecord, UserInputFragment},
    transcript_presentation::TranscriptPresentedRow,
};

pub(crate) const EDIT_COMPOSER_NOT_EMPTY_TOOLTIP: &str = "Composer must be empty to edit a message";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptEditTarget {
    identity: TranscriptEditTargetIdentity,
    rollback_turn_count: u32,
    draft_seed: AcceptedComposerDraft,
    draft_seed_fragments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptEditTargetIdentity {
    source_thread_id: String,
    source_turn_id: String,
    source_turn_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptEditTargetResolution {
    Enabled(TranscriptEditTarget),
    Disabled {
        identity: TranscriptEditTargetIdentity,
        reason: TranscriptEditDisabledReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptEditDisabledReason {
    ComposerNotEmpty,
    UnsupportedInput,
    MissingImageMetadata,
    MissingImageBytes,
    StaleImageRuntimePath,
    ImageLabelScanIncomplete,
    ImageLabelScanFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptEditMenuEntry {
    Enabled(TranscriptEditTarget),
    Disabled {
        identity: TranscriptEditTargetIdentity,
        reason: TranscriptEditDisabledReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptEditRequest {
    target: TranscriptEditTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptEditMenuGate {
    pub(crate) transcript_selection_active: bool,
    pub(crate) source_thread_idle: bool,
    pub(crate) selected_thread_matches_target: bool,
    pub(crate) selected_thread_compaction_active: bool,
    pub(crate) pending_thread_activation: bool,
    pub(crate) rollback_capability_available: bool,
    pub(crate) composer_empty: bool,
    pub(crate) pending_turn_input: bool,
    pub(crate) pending_active_turn_steering: bool,
    pub(crate) conflicting_selected_thread_work: bool,
    pub(crate) image_label_readiness: ComposerImagePasteReadiness,
}

impl TranscriptEditTarget {
    #[cfg(test)]
    pub(crate) fn for_test(
        source_thread_id: impl Into<String>,
        source_turn_id: impl Into<String>,
        source_turn_index: usize,
        rollback_turn_count: u32,
        draft_seed_fragments: Vec<String>,
    ) -> Self {
        let seed_text = draft_seed_fragments.join("\n\n");
        let draft_seed = AcceptedComposerDraft::from_display_text_and_images(
            seed_text,
            Vec::<AcceptedComposerImage>::new(),
            Vec::<AcceptedComposerImageOccurrence>::new(),
        )
        .expect("test edit target seed should be non-empty");
        Self {
            identity: TranscriptEditTargetIdentity {
                source_thread_id: source_thread_id.into(),
                source_turn_id: source_turn_id.into(),
                source_turn_index,
            },
            rollback_turn_count,
            draft_seed,
            draft_seed_fragments,
        }
    }

    pub(crate) fn from_presented_row(
        row: &TranscriptPresentedRow,
        turns: &[Arc<TurnExecutionRecord>],
        current_tail_known: bool,
    ) -> Option<Self> {
        match Self::resolve_from_presented_row(row, turns, current_tail_known)? {
            TranscriptEditTargetResolution::Enabled(target) => Some(target),
            TranscriptEditTargetResolution::Disabled { .. } => None,
        }
    }

    pub(crate) fn resolve_from_presented_row(
        row: &TranscriptPresentedRow,
        turns: &[Arc<TurnExecutionRecord>],
        current_tail_known: bool,
    ) -> Option<TranscriptEditTargetResolution> {
        if !current_tail_known {
            return None;
        }

        let presented_turn = row.turn.as_ref();
        if presented_turn.is_released_history_placeholder() {
            return None;
        }

        let source_turn_index = row.source_turn_index;
        let source_turn = turns.get(source_turn_index)?;
        if source_turn.is_released_history_placeholder() {
            return None;
        }

        let source_thread_id = source_turn.thread_id.clone()?;
        let source_turn_id = source_turn.turn_id.clone()?;
        if presented_turn.thread_id.as_deref() != Some(source_thread_id.as_str())
            || presented_turn.turn_id.as_deref() != Some(source_turn_id.as_str())
        {
            return None;
        }

        let rollback_turn_count = exact_tail_rollback_count(turns, source_turn_index)?;
        let identity = TranscriptEditTargetIdentity {
            source_thread_id,
            source_turn_id,
            source_turn_index,
        };
        let draft_seed_fragments = source_turn
            .user_input_fragments()
            .iter()
            .filter(|fragment| !fragment.is_blank())
            .map(|fragment| fragment.text.clone())
            .collect::<Vec<_>>();
        let draft_seed = match draft_seed_for_turn(source_turn.as_ref()) {
            Ok(seed) => seed,
            Err(reason) => {
                return Some(TranscriptEditTargetResolution::Disabled { identity, reason });
            }
        };

        Some(TranscriptEditTargetResolution::Enabled(Self {
            identity,
            rollback_turn_count,
            draft_seed,
            draft_seed_fragments,
        }))
    }

    #[allow(dead_code)]
    pub(crate) fn source_thread_id(&self) -> &str {
        &self.identity.source_thread_id
    }

    pub(crate) fn source_turn_id(&self) -> &str {
        &self.identity.source_turn_id
    }

    #[allow(dead_code)]
    pub(crate) fn source_turn_index(&self) -> usize {
        self.identity.source_turn_index
    }

    #[allow(dead_code)]
    pub(crate) fn rollback_turn_count(&self) -> u32 {
        self.rollback_turn_count
    }

    #[allow(dead_code)]
    pub(crate) fn draft_seed_fragments(&self) -> &[String] {
        &self.draft_seed_fragments
    }

    #[allow(dead_code)]
    pub(crate) fn draft_seed_text(&self) -> String {
        self.draft_seed.display_text().to_string()
    }

    pub(crate) fn draft_seed(&self) -> &AcceptedComposerDraft {
        &self.draft_seed
    }

    fn contains_images(&self) -> bool {
        self.draft_seed.contains_images()
    }
}

impl TranscriptEditTargetIdentity {
    pub(crate) fn source_thread_id(&self) -> &str {
        &self.source_thread_id
    }

    pub(crate) fn source_turn_id(&self) -> &str {
        &self.source_turn_id
    }
}

impl TranscriptEditDisabledReason {
    pub(crate) fn tooltip(self) -> &'static str {
        match self {
            Self::ComposerNotEmpty => EDIT_COMPOSER_NOT_EMPTY_TOOLTIP,
            Self::UnsupportedInput => {
                "This message contains input Beryl cannot edit without data loss"
            }
            Self::MissingImageMetadata => "Image metadata is unavailable for this message",
            Self::MissingImageBytes => "Image bytes are unavailable for this message",
            Self::StaleImageRuntimePath => {
                "This image asset is not readable by the current runtime"
            }
            Self::ImageLabelScanIncomplete => {
                "Beryl is still scanning this thread's earlier image labels"
            }
            Self::ImageLabelScanFailed => "Beryl could not scan this thread's earlier image labels",
        }
    }
}

impl TranscriptEditMenuEntry {
    pub(crate) fn target_identity(&self) -> &TranscriptEditTargetIdentity {
        match self {
            Self::Enabled(target) => &target.identity,
            Self::Disabled { identity, .. } => identity,
        }
    }

    pub(crate) fn disabled_reason(&self) -> Option<TranscriptEditDisabledReason> {
        match self {
            Self::Enabled(_) => None,
            Self::Disabled { reason, .. } => Some(*reason),
        }
    }

    pub(crate) fn into_request(self) -> Option<TranscriptEditRequest> {
        match self {
            Self::Enabled(target) => Some(TranscriptEditRequest { target }),
            Self::Disabled { .. } => None,
        }
    }
}

impl TranscriptEditRequest {
    #[cfg(test)]
    pub(crate) fn for_test(target: TranscriptEditTarget) -> Self {
        Self { target }
    }

    #[allow(dead_code)]
    pub(crate) fn target(&self) -> &TranscriptEditTarget {
        &self.target
    }

    pub(crate) fn into_target(self) -> TranscriptEditTarget {
        self.target
    }
}

pub(crate) fn transcript_edit_menu_entry(
    target: TranscriptEditTargetResolution,
    gate: TranscriptEditMenuGate,
) -> Option<TranscriptEditMenuEntry> {
    if !transcript_edit_base_available(&gate) {
        return None;
    }

    if !gate.composer_empty {
        return Some(TranscriptEditMenuEntry::Disabled {
            identity: target.into_identity(),
            reason: TranscriptEditDisabledReason::ComposerNotEmpty,
        });
    }

    match target {
        TranscriptEditTargetResolution::Disabled { identity, reason } => {
            Some(TranscriptEditMenuEntry::Disabled { identity, reason })
        }
        TranscriptEditTargetResolution::Enabled(target) => {
            let image_label_reason = target
                .contains_images()
                .then(|| match gate.image_label_readiness {
                    ComposerImagePasteReadiness::Ready => None,
                    ComposerImagePasteReadiness::Scanning => {
                        Some(TranscriptEditDisabledReason::ImageLabelScanIncomplete)
                    }
                    ComposerImagePasteReadiness::Failed { .. } => {
                        Some(TranscriptEditDisabledReason::ImageLabelScanFailed)
                    }
                })
                .flatten();
            if let Some(reason) = image_label_reason {
                Some(TranscriptEditMenuEntry::Disabled {
                    identity: target.identity,
                    reason,
                })
            } else {
                Some(TranscriptEditMenuEntry::Enabled(target))
            }
        }
    }
}

fn transcript_edit_base_available(gate: &TranscriptEditMenuGate) -> bool {
    !gate.transcript_selection_active
        && gate.source_thread_idle
        && gate.selected_thread_matches_target
        && !gate.selected_thread_compaction_active
        && !gate.pending_thread_activation
        && gate.rollback_capability_available
        && !gate.pending_turn_input
        && !gate.pending_active_turn_steering
        && !gate.conflicting_selected_thread_work
}

impl TranscriptEditTargetResolution {
    pub(crate) fn into_identity(self) -> TranscriptEditTargetIdentity {
        match self {
            Self::Enabled(target) => target.identity,
            Self::Disabled { identity, .. } => identity,
        }
    }
}

fn exact_tail_rollback_count(
    turns: &[Arc<TurnExecutionRecord>],
    source_turn_index: usize,
) -> Option<u32> {
    let tail = turns.get(source_turn_index..)?;
    if tail.is_empty()
        || tail
            .iter()
            .any(|turn| turn.is_released_history_placeholder() || turn.turn_id.is_none())
    {
        return None;
    }
    u32::try_from(tail.len()).ok()
}

fn draft_seed_for_turn(
    turn: &TurnExecutionRecord,
) -> Result<AcceptedComposerDraft, TranscriptEditDisabledReason> {
    let mut display_text = String::new();
    let mut images_by_label: HashMap<String, AcceptedComposerImage> = HashMap::new();
    let mut occurrences = Vec::new();

    for fragment in turn.user_input_fragments() {
        if fragment.is_blank() {
            continue;
        }

        if !display_text.is_empty() {
            display_text.push_str("\n\n");
        }
        append_fragment_to_draft_seed(
            fragment,
            &mut display_text,
            &mut images_by_label,
            &mut occurrences,
        )?;
    }

    AcceptedComposerDraft::from_display_text_and_images(
        display_text,
        images_by_label.into_values(),
        occurrences,
    )
    .ok_or(TranscriptEditDisabledReason::UnsupportedInput)
}

fn append_fragment_to_draft_seed(
    fragment: &UserInputFragment,
    display_text: &mut String,
    images_by_label: &mut HashMap<String, AcceptedComposerImage>,
    occurrences: &mut Vec<AcceptedComposerImageOccurrence>,
) -> Result<(), TranscriptEditDisabledReason> {
    if !fragment
        .backend_input()
        .iter()
        .all(|input| matches!(input, UserInput::Text { .. } | UserInput::LocalImage { .. }))
    {
        return Err(TranscriptEditDisabledReason::UnsupportedInput);
    }

    if fragment.image_markers().is_empty() {
        display_text.push_str(&fragment.text);
        return Ok(());
    }

    let fragment_offset = display_text.len();
    validate_fragment_image_markers(fragment)?;
    display_text.push_str(&fragment.text);

    for marker in fragment.image_markers() {
        let image = image_for_marker(marker.label(), marker.source(), marker.label_source())?;
        if let Some(existing) = images_by_label.get(marker.label()) {
            if existing.data().asset_id() != image.data().asset_id() {
                return Err(TranscriptEditDisabledReason::MissingImageMetadata);
            }
        } else {
            images_by_label.insert(marker.label().to_string(), image);
        }
        let range = marker.display_range();
        occurrences.push(AcceptedComposerImageOccurrence::new(
            marker.label().to_string(),
            fragment_offset + range.start..fragment_offset + range.end,
        ));
    }

    Ok(())
}

fn validate_fragment_image_markers(
    fragment: &UserInputFragment,
) -> Result<(), TranscriptEditDisabledReason> {
    let mut cursor = 0usize;
    let mut markers = fragment.image_markers().iter().collect::<Vec<_>>();
    markers.sort_by_key(|marker| marker.display_range().start);
    for marker in markers {
        let range = marker.display_range();
        if range.start < cursor
            || range.start >= range.end
            || range.end > fragment.text.len()
            || !fragment.text.is_char_boundary(range.start)
            || !fragment.text.is_char_boundary(range.end)
            || &fragment.text[range.clone()] != composer_image_marker(marker.label())
        {
            return Err(TranscriptEditDisabledReason::MissingImageMetadata);
        }
        cursor = range.end;
    }
    Ok(())
}

fn image_for_marker(
    label: &str,
    source: &TranscriptImageSource,
    label_source: TranscriptImageLabelSource,
) -> Result<AcceptedComposerImage, TranscriptEditDisabledReason> {
    if label_source != TranscriptImageLabelSource::Generated {
        return Err(TranscriptEditDisabledReason::MissingImageMetadata);
    }
    if !matches!(
        source.input(),
        TranscriptImageInputSource::LocalImage { .. }
    ) {
        return Err(TranscriptEditDisabledReason::UnsupportedInput);
    }
    let Some(asset_id) = source.asset_id() else {
        return Err(TranscriptEditDisabledReason::MissingImageMetadata);
    };
    if source.preview_state() != TranscriptImagePreviewState::Available {
        return Err(TranscriptEditDisabledReason::MissingImageBytes);
    }
    let Some(format) = source.asset_format() else {
        return Err(TranscriptEditDisabledReason::MissingImageMetadata);
    };
    if !source.runtime_readable() {
        return Err(TranscriptEditDisabledReason::StaleImageRuntimePath);
    }
    Ok(AcceptedComposerImage::new(
        label.to_string(),
        ComposerDraftImageData::durable_reference(format, asset_id.to_string()),
    ))
}
