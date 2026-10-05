use super::super::{PropagatedCutScan, read_cut_page};
use super::*;

#[derive(Debug, thiserror::Error)]
pub(super) enum PastePreparationError {
    #[error("paste cancelled")]
    Cancelled,
    #[error("paste representation exceeds the configured mutation page allowance")]
    TooLarge,
    #[error("paste source or destination unavailable: {0}")]
    Unavailable(String),
    #[error("private clipboard correlation changed")]
    PrivateCorrelation(String),
    #[error("paste image admission failed: {0}")]
    Image(crate::composer_host::paste_image::ClipboardImageAdmissionError),
}

impl From<String> for PastePreparationError {
    fn from(error: String) -> Self {
        Self::Unavailable(error)
    }
}

pub(super) enum PreparedPasteSource {
    Text(String),
    Image(beryl_model::AssetId),
    Private {
        text: String,
        descriptor: super::super::source::MainWindowPrivateClipboardDescriptor,
        storage: syndic_storage::SyndicStorage,
    },
}

pub(super) struct PreparedPaste {
    pub(super) selection: MainWindowComposerSelectionIdentity,
    pub(super) positions: MutationPositions,
    pub(super) replacement: SourceRange,
    pub(super) replacement_breaks: u64,
    pub(super) inserted_bytes: u64,
    pub(super) inserted_breaks: u64,
    pub(super) inserted_markers: u64,
    pub(super) leading_markers: u64,
    pub(super) trailing_markers: u64,
    pub(super) marker_seed: u128,
    pub(super) source: Arc<PreparedPasteSource>,
    pub(super) private: Option<(
        String,
        super::super::source::MainWindowPrivateClipboardDescriptor,
    )>,
    pub(super) proof: crate::main_window::MainWindowComposerSuccessorProof,
    pub(super) scan: PropagatedCutScan,
    pub(super) initial_scan: PropagatedCutScan,
    pub(super) removal_items: Vec<gpui_text_input::MutationPageItem>,
    pub(super) mutation_limits: gpui_text_input::MutationLimits,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    service: &Arc<super::super::super::MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    positions: MutationPositions,
    replacement: SourceRange,
    source: PasteSource,
    proof_limits: crate::main_window::MainWindowComposerSuccessorProofLimits,
    mutation_limits: gpui_text_input::MutationLimits,
    allowance: usize,
    sidecar_page_bytes: std::num::NonZeroUsize,
    cancellation: CommandCancellation,
) -> Result<PreparedPaste, PastePreparationError> {
    if cancellation.is_cancelled() {
        return Err(PastePreparationError::Cancelled);
    }
    let storage = service
        .slot
        .lock()
        .map_err(|_| PastePreparationError::Unavailable("composer service lock failed".into()))?
        .clipboard_storage();
    let page_bytes = mutation_limits.max_page_bytes();
    let replacement_breaks = replay::count_line_breaks(
        &storage,
        &service.store,
        selection.binding().root(),
        replacement,
        page_bytes,
        &cancellation,
    )?;
    let (
        source,
        private,
        inserted_bytes,
        inserted_breaks,
        inserted_markers,
        leading_markers,
        trailing_markers,
    ) = match source {
        PasteSource::Text(text) => {
            if text
                .chars()
                .any(|scalar| scalar.len_utf8() > mutation_limits.max_page_bytes())
            {
                return Err(PastePreparationError::TooLarge);
            }
            let bytes = text.len() as u64;
            let breaks = text.bytes().filter(|byte| *byte == b'\n').count() as u64;
            (
                PreparedPasteSource::Text(text),
                None,
                bytes,
                breaks,
                0,
                0,
                0,
            )
        }
        PasteSource::Image(image) => {
            let asset = crate::composer_host::paste_image::admit_clipboard_image(
                &service.store,
                service.assets().map_err(|_| {
                    PastePreparationError::Image(
                        crate::composer_host::paste_image::ClipboardImageAdmissionError::Storage,
                    )
                })?,
                image.bytes(),
                std::num::NonZeroU64::new(allowance as u64)
                    .ok_or_else(|| "invalid paste byte allowance".to_owned())?,
                sidecar_page_bytes,
                &cancellation,
            )
            .map_err(PastePreparationError::Image)?;
            validate_marker_weight(
                crate::composer_host::ComposerHostImageMarkerMetadata::new(
                    gpui_text_input::InlineObjectId::new(0),
                    asset,
                ),
                mutation_limits,
            )?;
            (PreparedPasteSource::Image(asset), None, 0, 0, 1, 1, 1)
        }
        PasteSource::Private {
            text,
            token,
            descriptor,
            storage,
        } => {
            let mut replay = replay::PrivateReplay::new(descriptor);
            let mut bytes = 0u64;
            let mut breaks = 0u64;
            let mut markers = 0u64;
            let mut leading = 0u64;
            let mut trailing = 0u64;
            let mut oversized = false;
            let range = descriptor
                .selection
                .range()
                .map_err(|_| "private paste selection is incoherent".to_owned())?;
            let text_extent = range.end().byte_offset.get() - range.start().byte_offset.get();
            while let Some(item) = replay
                .next(&storage, &service.store, &text, page_bytes)
                .map_err(|error| match error {
                    replay::PrivateReplayError::Correlation => {
                        PastePreparationError::PrivateCorrelation(token.clone())
                    }
                    replay::PrivateReplayError::Unavailable(error) => {
                        PastePreparationError::Unavailable(error)
                    }
                })?
            {
                if cancellation.is_cancelled() {
                    return Err(PastePreparationError::Cancelled);
                }
                match item {
                    replay::ReplayItem::Text { text, .. } => {
                        bytes = bytes
                            .checked_add(text.len() as u64)
                            .ok_or_else(|| "private paste byte extent overflowed".to_owned())?;
                        breaks = breaks
                            .checked_add(text.bytes().filter(|byte| *byte == b'\n').count() as u64)
                            .ok_or_else(|| "private paste line extent overflowed".to_owned())?;
                    }
                    replay::ReplayItem::Marker { offset, marker } => {
                        oversized |= validate_marker_weight(
                            crate::composer_host::ComposerHostImageMarkerMetadata::from_source(
                                gpui_text_input::InlineObjectId::new(0),
                                marker.asset_id(),
                                descriptor.source.marker_selector(marker.marker_id()),
                            ),
                            mutation_limits,
                        )
                        .is_err();
                        markers = markers
                            .checked_add(1)
                            .ok_or_else(|| "private paste marker extent overflowed".to_owned())?;
                        if offset == 0 {
                            leading = leading.checked_add(1).ok_or_else(|| {
                                "private paste marker extent overflowed".to_owned()
                            })?;
                        }
                        if offset == text_extent {
                            trailing = trailing.checked_add(1).ok_or_else(|| {
                                "private paste marker extent overflowed".to_owned()
                            })?;
                        }
                    }
                }
            }
            if oversized {
                return Err(PastePreparationError::TooLarge);
            }
            (
                PreparedPasteSource::Private {
                    text,
                    descriptor,
                    storage,
                },
                Some((token, descriptor)),
                bytes,
                breaks,
                markers,
                leading,
                trailing,
            )
        }
    };
    if cancellation.is_cancelled() {
        return Err(PastePreparationError::Cancelled);
    }
    let mut identity = [0; 16];
    if inserted_markers != 0 {
        getrandom::fill(&mut identity)
            .map_err(|_| "paste marker identity entropy unavailable".to_owned())?;
    }
    let marker_seed = u128::from_be_bytes(identity);
    marker_seed
        .checked_add(u128::from(inserted_markers))
        .ok_or_else(|| "paste marker identity exhausted".to_owned())?;
    let mut slot = service
        .slot
        .lock()
        .map_err(|_| PastePreparationError::Unavailable("composer service lock failed".into()))?;
    let proof = slot
        .build_selected_successor_proof_with_extra_positions(
            &service.store,
            selection,
            positions,
            &[replacement.start(), replacement.end()],
            proof_limits,
        )
        .map_err(|_| "captured paste position proof unavailable".to_owned())?;
    let mut scan = PropagatedCutScan::new(
        replacement,
        mutation_limits,
        proof_limits.objects.max_pending_bytes(),
        proof_limits.presentation_generation,
    )?;
    let initial_scan = scan;
    let prepared = read_cut_page(&mut slot, &service.store, selection, scan.request()?)?;
    scan.admit(&prepared)?;
    Ok(PreparedPaste {
        selection,
        positions,
        replacement,
        replacement_breaks,
        inserted_bytes,
        inserted_breaks,
        inserted_markers,
        leading_markers,
        trailing_markers,
        marker_seed,
        source: Arc::new(source),
        private,
        proof,
        scan,
        initial_scan,
        removal_items: prepared.items,
        mutation_limits,
    })
}

fn validate_marker_weight(
    metadata: crate::composer_host::ComposerHostImageMarkerMetadata,
    limits: gpui_text_input::MutationLimits,
) -> Result<(), PastePreparationError> {
    if metadata.retained_bytes() > limits.max_page_object_bytes() {
        return Err(PastePreparationError::TooLarge);
    }
    Ok(())
}
