use std::{
    ops::Range,
    path::{Path, PathBuf},
};

use super::{
    context_menu::{
        ResidentContextMenuContentKind, ResidentContextMenuUnavailable,
        ResidentTranscriptContextMenuTarget,
    },
    frame::RealizedFrameRequest,
    media_action::{ResidentMediaActionUnavailable, ResidentTranscriptMediaPayload},
    provider::{ProjectionRecordId, ProviderRevision, ResourceId, SyndicSourceProvenance},
    snapshot::ResidentPresentationRecordId,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ManualTranscriptScrollCommand {
    pub(crate) viewport_height_px: f32,
    pub(crate) overscan_height_px: f32,
    pub(crate) default_record_height_px: f32,
    pub(crate) delta_px: f32,
    pub(crate) observed_presentation_revision: Option<u64>,
}

impl ManualTranscriptScrollCommand {
    pub(crate) fn new(
        viewport_height_px: f32,
        overscan_height_px: f32,
        default_record_height_px: f32,
        delta_px: f32,
        observed_presentation_revision: Option<u64>,
    ) -> Self {
        Self {
            viewport_height_px,
            overscan_height_px,
            default_record_height_px,
            delta_px,
            observed_presentation_revision,
        }
    }

    pub(crate) fn frame_request(self) -> RealizedFrameRequest {
        RealizedFrameRequest {
            viewport_height_px: self.viewport_height_px,
            overscan_height_px: self.overscan_height_px,
            default_record_height_px: self.default_record_height_px,
            manual_delta_px: self.delta_px,
            observed_presentation_revision: self.observed_presentation_revision,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DisabledTranscriptCommand {
    pub(crate) command: &'static str,
    pub(crate) reason: &'static str,
}

impl DisabledTranscriptCommand {
    pub(crate) fn new(command: &'static str) -> Self {
        Self {
            command,
            reason: "resident transcript data is not available for this command",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptCommandResult {
    NoOp,
    Unavailable(DisabledTranscriptCommand),
}

impl TranscriptCommandResult {
    pub(crate) fn unavailable(command: &'static str) -> Self {
        Self::Unavailable(DisabledTranscriptCommand::new(command))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentContextMenuCommandTarget {
    Targeted(ResidentTranscriptContextMenuTarget),
    Unavailable(ResidentContextMenuUnavailable),
}

impl ResidentContextMenuCommandTarget {
    pub(crate) fn from_active_target(target: Option<ResidentTranscriptContextMenuTarget>) -> Self {
        match target {
            Some(target) => Self::Targeted(target),
            None => Self::Unavailable(ResidentContextMenuUnavailable::NoActiveContextMenuTarget),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentActionTargetProvenance {
    pub(crate) presentation_revision: u64,
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) source: SyndicSourceProvenance,
    pub(crate) projection_id: ProjectionRecordId,
    pub(crate) projection_revision: ProviderRevision,
    pub(crate) content_kind: ResidentContextMenuContentKind,
    pub(crate) source_range: Option<Range<u64>>,
    pub(crate) resource_range: Option<Range<u64>>,
}

impl ResidentActionTargetProvenance {
    pub(crate) fn from_context_menu_target(target: ResidentTranscriptContextMenuTarget) -> Self {
        let record = target.record;
        Self {
            presentation_revision: target.presentation_revision,
            record_id: record.record_id,
            source: record.source,
            projection_id: record.projection_id,
            projection_revision: record.projection_revision,
            content_kind: record.content_kind,
            source_range: record.source_range,
            resource_range: record.resource_range,
        }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        vec![self.record_id.clone()]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentEditActionTarget {
    pub(crate) provenance: ResidentActionTargetProvenance,
}

impl ResidentEditActionTarget {
    pub(crate) fn from_context_menu_target(target: ResidentTranscriptContextMenuTarget) -> Self {
        Self {
            provenance: ResidentActionTargetProvenance::from_context_menu_target(target),
        }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        self.provenance.record_ids()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentEditCommandTarget {
    Targeted(ResidentEditActionTarget),
    Unavailable(ResidentContextMenuUnavailable),
}

impl ResidentEditCommandTarget {
    pub(crate) fn from_context_menu_command_target(
        target: ResidentContextMenuCommandTarget,
    ) -> Self {
        match target {
            ResidentContextMenuCommandTarget::Targeted(target) => {
                Self::Targeted(ResidentEditActionTarget::from_context_menu_target(target))
            }
            ResidentContextMenuCommandTarget::Unavailable(error) => Self::Unavailable(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentBranchActionTarget {
    pub(crate) provenance: ResidentActionTargetProvenance,
}

impl ResidentBranchActionTarget {
    pub(crate) fn from_context_menu_target(target: ResidentTranscriptContextMenuTarget) -> Self {
        Self {
            provenance: ResidentActionTargetProvenance::from_context_menu_target(target),
        }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        self.provenance.record_ids()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentBranchCommandTarget {
    Targeted(ResidentBranchActionTarget),
    Unavailable(ResidentContextMenuUnavailable),
}

impl ResidentBranchCommandTarget {
    pub(crate) fn from_context_menu_command_target(
        target: ResidentContextMenuCommandTarget,
    ) -> Self {
        match target {
            ResidentContextMenuCommandTarget::Targeted(target) => {
                Self::Targeted(ResidentBranchActionTarget::from_context_menu_target(target))
            }
            ResidentContextMenuCommandTarget::Unavailable(error) => Self::Unavailable(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentMediaPreviewCommandPayload {
    pub(crate) payload: ResidentTranscriptMediaPayload,
}

impl ResidentMediaPreviewCommandPayload {
    pub(crate) fn from_resident_payload(payload: ResidentTranscriptMediaPayload) -> Self {
        Self { payload }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        vec![self.payload.record.record_id.clone()]
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.payload.bytes.len()
    }

    pub(crate) fn range(&self) -> Range<u64> {
        self.payload.range.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentMediaPreviewCommandTarget {
    Targeted(ResidentMediaPreviewCommandPayload),
    Unavailable(ResidentMediaActionUnavailable),
}

impl ResidentMediaPreviewCommandTarget {
    pub(crate) fn from_resident_payload(
        payload: Result<ResidentTranscriptMediaPayload, ResidentMediaActionUnavailable>,
    ) -> Self {
        match payload {
            Ok(payload) => Self::Targeted(
                ResidentMediaPreviewCommandPayload::from_resident_payload(payload),
            ),
            Err(error) => Self::Unavailable(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentMediaCopyCommandPayload {
    pub(crate) payload: ResidentTranscriptMediaPayload,
}

impl ResidentMediaCopyCommandPayload {
    pub(crate) fn from_resident_payload(payload: ResidentTranscriptMediaPayload) -> Self {
        Self { payload }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        vec![self.payload.record.record_id.clone()]
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.payload.bytes
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.payload.bytes.len()
    }

    pub(crate) fn complete(&self) -> bool {
        self.payload.complete
    }

    pub(crate) fn media_type(&self) -> Option<&str> {
        self.payload.record.media_type.as_deref()
    }

    pub(crate) fn range(&self) -> Range<u64> {
        self.payload.range.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentMediaCopyCommandTarget {
    Targeted(ResidentMediaCopyCommandPayload),
    Unavailable(ResidentMediaActionUnavailable),
}

impl ResidentMediaCopyCommandTarget {
    pub(crate) fn from_resident_payload(
        payload: Result<ResidentTranscriptMediaPayload, ResidentMediaActionUnavailable>,
    ) -> Self {
        match payload {
            Ok(payload) => Self::Targeted(ResidentMediaCopyCommandPayload::from_resident_payload(
                payload,
            )),
            Err(error) => Self::Unavailable(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentMediaSaveCommandPayload {
    pub(crate) payload: ResidentTranscriptMediaPayload,
}

impl ResidentMediaSaveCommandPayload {
    pub(crate) fn from_resident_payload(payload: ResidentTranscriptMediaPayload) -> Self {
        Self { payload }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        vec![self.payload.record.record_id.clone()]
    }

    pub(crate) fn resource_id(&self) -> ResourceId {
        self.payload.record.resource_id.clone()
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.payload.bytes
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.payload.bytes.len()
    }

    pub(crate) fn complete(&self) -> bool {
        self.payload.complete
    }

    pub(crate) fn media_type(&self) -> Option<&str> {
        self.payload.record.media_type.as_deref()
    }

    pub(crate) fn range(&self) -> Range<u64> {
        self.payload.range.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResidentMediaSaveCommandTarget {
    Targeted(ResidentMediaSaveCommandPayload),
    Unavailable(ResidentMediaActionUnavailable),
}

impl ResidentMediaSaveCommandTarget {
    pub(crate) fn from_resident_payload(
        payload: Result<ResidentTranscriptMediaPayload, ResidentMediaActionUnavailable>,
    ) -> Self {
        match payload {
            Ok(payload) => Self::Targeted(ResidentMediaSaveCommandPayload::from_resident_payload(
                payload,
            )),
            Err(error) => Self::Unavailable(error),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentMediaSaveDestinationUnavailable {
    EmptyPath,
    RelativePath,
    MissingFileName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentMediaSaveDestination {
    path: PathBuf,
}

impl ResidentMediaSaveDestination {
    pub(crate) fn new(path: PathBuf) -> Result<Self, ResidentMediaSaveDestinationUnavailable> {
        if path.as_os_str().is_empty() {
            return Err(ResidentMediaSaveDestinationUnavailable::EmptyPath);
        }
        if !path.is_absolute() {
            return Err(ResidentMediaSaveDestinationUnavailable::RelativePath);
        }
        if path.file_name().is_none() {
            return Err(ResidentMediaSaveDestinationUnavailable::MissingFileName);
        }

        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
