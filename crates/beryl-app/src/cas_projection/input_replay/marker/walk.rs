//! Ordered, constant-residency descriptor traversal.

use beryl_home_store::HomeStore;
use beryl_state::{AssetLabelDisposition, AssetReferenceEntryRecord};
use syndic_storage::{SyndicContentTextSegmentBoundary, SyndicStorage};

use super::{
    error::MarkerReplayError,
    identity::{TextRunBlueprint, TextRunBuilder},
    source::MarkerSource,
};
use crate::cas_projection::ProjectionCancellationToken;

pub(super) enum DescriptorBlueprint {
    Text(TextRunBlueprint),
    Image(ImageBlueprint),
}

pub(super) struct ImageBlueprint {
    pub(super) descriptor_ordinal: u64,
    pub(super) entry: AssetReferenceEntryRecord,
}

pub(super) struct DescriptorWalk {
    run_start: Option<SyndicContentTextSegmentBoundary>,
    pending_image: Option<(SyndicContentTextSegmentBoundary, AssetReferenceEntryRecord)>,
    emitted_image_boundary: Option<SyndicContentTextSegmentBoundary>,
    next_descriptor_ordinal: u64,
    next_run_ordinal: u64,
    finished: bool,
}

impl DescriptorWalk {
    pub(super) const fn new() -> Self {
        Self {
            run_start: None,
            pending_image: None,
            emitted_image_boundary: None,
            next_descriptor_ordinal: 1,
            next_run_ordinal: 1,
            finished: false,
        }
    }

    pub(super) fn next(
        &mut self,
        source: &MarkerSource,
        store: &HomeStore,
        storage: &SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<DescriptorBlueprint>, MarkerReplayError> {
        if self.finished {
            return Ok(None);
        }
        if let Some(boundary) = self.emitted_image_boundary.take() {
            self.run_start = Some(boundary);
        }
        if let Some((boundary, entry)) = self.pending_image.take() {
            let descriptor_ordinal = self.take_descriptor_ordinal()?;
            self.emitted_image_boundary = Some(boundary);
            return Ok(Some(DescriptorBlueprint::Image(ImageBlueprint {
                descriptor_ordinal,
                entry,
            })));
        }

        let descriptor_ordinal = self.next_descriptor_ordinal;
        let run_ordinal = self.next_run_ordinal;
        let mut builder =
            TextRunBuilder::new(source, descriptor_ordinal, run_ordinal, self.run_start);
        let mut after_marker = self.run_start;
        loop {
            let segment = source.prove_segment(store, storage, cancellation, after_marker)?;
            let following_marker = segment.following_marker();
            let Some(boundary) = following_marker else {
                source.require_entry_eof(store, cancellation, after_marker)?;
                builder.push_segment(&segment, None)?;
                self.finished = true;
                if builder.is_empty() {
                    return Ok(None);
                }
                self.take_descriptor_ordinal()?;
                self.take_run_ordinal()?;
                return builder
                    .finish(source, None)
                    .map(DescriptorBlueprint::Text)
                    .map(Some);
            };

            let entry = source.marker_entry(store, cancellation, boundary)?;
            source.validate_marker_entry(store, storage, cancellation, &entry)?;
            builder.push_segment(&segment, Some(&entry))?;
            match entry.label_disposition() {
                AssetLabelDisposition::First => {
                    let text = builder.finish(source, Some(boundary))?;
                    self.pending_image = Some((boundary, entry));
                    self.take_descriptor_ordinal()?;
                    self.take_run_ordinal()?;
                    return Ok(Some(DescriptorBlueprint::Text(text)));
                }
                AssetLabelDisposition::Repeated { .. } => {
                    after_marker = Some(boundary);
                }
            }
        }
    }

    fn take_descriptor_ordinal(&mut self) -> Result<u64, MarkerReplayError> {
        let current = self.next_descriptor_ordinal;
        self.next_descriptor_ordinal = current
            .checked_add(1)
            .ok_or(MarkerReplayError::InvalidDescriptor)?;
        Ok(current)
    }

    fn take_run_ordinal(&mut self) -> Result<u64, MarkerReplayError> {
        let current = self.next_run_ordinal;
        self.next_run_ordinal = current
            .checked_add(1)
            .ok_or(MarkerReplayError::InvalidDescriptor)?;
        Ok(current)
    }
}
