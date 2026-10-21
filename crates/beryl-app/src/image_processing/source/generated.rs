use super::*;
use crate::syndic_transcript::{TranscriptGeneratedImage, TranscriptGeneratedImageLifecycle};

impl crate::syndic_transcript::ResidentContextMenuRecord {
    pub(crate) fn image_demand(&self) -> Option<ImageSourceDemand> {
        use crate::syndic_transcript::ResidentContextMenuContentKind;
        let thread_id = self.source.view_id.0.parse().ok()?;
        let entry = self.source.transcript_entry?;
        let projection_id = self.projection_id.0.parse().ok()?;
        let projection_revision = ProjectionRevision::new(self.projection_revision.0).ok()?;
        match self.content_kind {
            ResidentContextMenuContentKind::ImageMarker(marker) => {
                Some(ImageSourceDemand::TranscriptMarker {
                    thread_id,
                    entry,
                    projection_id,
                    projection_revision,
                    marker,
                })
            }
            ResidentContextMenuContentKind::GeneratedImage(image)
                if matches!(
                    image.lifecycle,
                    TranscriptGeneratedImageLifecycle::Admitted(_)
                ) =>
            {
                Some(ImageSourceDemand::TranscriptGeneratedResource {
                    thread_id,
                    entry,
                    projection_id,
                    projection_revision,
                    image,
                })
            }
            _ => None,
        }
    }
}

impl ImageSourceProvider {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn resolve_generated(
        &self,
        thread_id: SyndicThreadId,
        entry_proof: CurrentTranscriptEntryProof,
        projection_id: SyndicProjectionId,
        projection_revision: ProjectionRevision,
        image: TranscriptGeneratedImage,
        cancellation: &CommandCancellation,
    ) -> Result<AssetId, ImageSourceError> {
        let TranscriptGeneratedImageLifecycle::Admitted(asset) = image.lifecycle else {
            return Err(ImageSourceError::Unavailable);
        };
        let home_revision = self
            .home
            .home_revision()
            .map_err(|_| ImageSourceError::Unavailable)?;
        let point = SyndicPointReadLimit::new(SOURCE_RECORD_BYTES).expect("positive source bound");
        let head = self
            .syndic
            .transcript_view_head(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        check_cancelled(cancellation)?;
        let thread = self
            .syndic
            .thread(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        if head.thread_id() != thread_id
            || head.generation() != entry_proof.generation()
            || head.lifecycle() != ProjectionLifecycle::Current
            || head.committed_tail() != thread.committed_tail()
            || head.selected_path_digest() != thread.selected_path_digest()
            || entry_proof.position().get() > head.entry_count()
        {
            return Err(ImageSourceError::Stale);
        }
        check_cancelled(cancellation)?;
        let after = entry_proof
            .position()
            .get()
            .checked_sub(1)
            .filter(|value| *value != 0)
            .and_then(|value| TranscriptPosition::new(value).ok());
        let entries = self
            .syndic
            .transcript_entries(
                &self.home,
                thread_id,
                entry_proof.generation(),
                after,
                CursorReadLimits::new(1, SOURCE_RECORD_BYTES).expect("positive source bound"),
            )
            .map_err(|_| ImageSourceError::Unavailable)?;
        let entry = entries.records().first().ok_or(ImageSourceError::Stale)?;
        if entry.position() != entry_proof.position()
            || entry.projection_id() != projection_id
            || entry.projection_revision() != projection_revision
        {
            return Err(ImageSourceError::Stale);
        }
        check_cancelled(cancellation)?;
        let projection = self
            .syndic
            .projection(&self.home, projection_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        let proof = self
            .syndic
            .generated_media_source(&self.home, image.resource_id)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Identity)?;
        if projection.id() != projection_id
            || projection.revision() != projection_revision
            || !matches!(projection.payload(), ProjectionPayload::GeneratedResource { resource_id } if *resource_id == image.resource_id)
            || projection.item_id() != entry.item_id()
            || projection.item_id() != proof.item_id()
            || projection.turn_id() != proof.turn_id()
            || entry.item_revision() != proof.item().revision()
            || proof.resource().revision() != image.resource_revision
            || proof.source_fingerprint() != image.source_fingerprint
            || proof.resource().backing()
                != syndic_storage::ResourceBacking::GeneratedMedia(
                    syndic_storage::GeneratedMediaResourceDisposition::Asset(asset),
                )
        {
            return Err(ImageSourceError::Identity);
        }
        check_cancelled(cancellation)?;
        let target = beryl_state::GeneratedMediaTarget::new(
            image.resource_id,
            proof.thread_id(),
            proof.turn_id(),
            proof.item_id(),
            image.source_fingerprint,
        );
        let admission = self
            .assets
            .generated_media_admission(&self.home, target)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Identity)?;
        if admission.lifecycle() != beryl_state::GeneratedMediaLifecycle::Published
            || admission.target() != target
            || admission.facts().asset() != asset
        {
            return Err(ImageSourceError::Identity);
        }
        check_cancelled(cancellation)?;
        let current = self
            .syndic
            .transcript_view_head(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?;
        let current_thread = self
            .syndic
            .thread(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?;
        if current.as_ref() != Some(&head)
            || current_thread
                .as_ref()
                .is_none_or(|value| value.revision() != thread.revision())
        {
            return Err(ImageSourceError::Stale);
        }
        if self
            .home
            .home_revision()
            .map_err(|_| ImageSourceError::Unavailable)?
            != home_revision
        {
            return Err(ImageSourceError::Stale);
        }
        check_cancelled(cancellation)?;
        Ok(asset)
    }
}
