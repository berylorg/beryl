#![allow(dead_code, unused_imports)]

mod activation;
mod command;
mod context_menu;
mod core;
mod demand;
mod diagnostics;
mod frame;
mod host;
mod media_action;
mod panel;
mod provider;
mod renderer_context_menu;
mod renderer_media_action;
mod renderer_quote;
mod renderer_selection;
mod selection;
mod snapshot;
mod status_facts;

pub(crate) use activation::{
    PreparedTranscriptActivation, TranscriptActivationOutcome, TranscriptActivationPlacement,
    TranscriptActivationSeed, TranscriptActivationSource,
};
pub(crate) use command::{
    DisabledTranscriptCommand, ManualTranscriptScrollCommand, ResidentActionTargetProvenance,
    ResidentBranchActionTarget, ResidentBranchCommandTarget, ResidentContextMenuCommandTarget,
    ResidentEditActionTarget, ResidentEditCommandTarget, ResidentMediaCopyCommandPayload,
    ResidentMediaCopyCommandTarget, ResidentMediaPreviewCommandPayload,
    ResidentMediaPreviewCommandTarget, ResidentMediaSaveCommandPayload,
    ResidentMediaSaveCommandTarget, ResidentMediaSaveDestination,
    ResidentMediaSaveDestinationUnavailable, TranscriptCommandResult,
};
pub(crate) use context_menu::{
    ResidentContextMenuCommand, ResidentContextMenuContentKind, ResidentContextMenuOutcome,
    ResidentContextMenuRecord, ResidentContextMenuUnavailable, ResidentTranscriptContextMenuTarget,
};
pub(crate) use core::{
    ProviderRequestBook, ProviderRequestBookSnapshot, ProviderRequestOutcome,
    ProviderRequestReason, ProviderRequestRecord, ResidentCoreSnapshot, ResidentFallbackRecord,
    ResidentGeneration, ResidentPresentationSnapshot, ResidentProviderResponseEffect,
    ResidentReleaseDecision, ResidentReleaseReason, ResidentReleaseTarget,
    ResidentSyndicDataSnapshot, ResidentTranscriptCore, ResidentTranscriptPolicy,
};
pub(crate) use demand::{DemandFact, DemandFactKind, DemandFactSink, DemandFactSinkSnapshot};
pub(crate) use diagnostics::{
    ResidentDataDiagnostics, ResidentFrameDiagnostics, SyndicTranscriptDiagnosticSnapshot,
};
pub(crate) use frame::{
    RealizedFrameAnchor, RealizedFrameClamp, RealizedFrameRecord, RealizedFrameRequest,
    RealizedFrameScrollController, RealizedFrameScrollMode, RealizedFrameScrollStateSnapshot,
    RealizedFrameWindow, RealizedRecordMeasurement,
};
pub(crate) use host::SyndicTranscriptHost;
pub(crate) use media_action::{
    ResidentMediaActionCommand, ResidentMediaActionOutcome, ResidentMediaActionRecord,
    ResidentMediaActionUnavailable, ResidentMediaRangeAvailability,
    ResidentTranscriptMediaActionTarget, ResidentTranscriptMediaPayload,
};
pub(crate) use panel::{SYNDIC_TRANSCRIPT_KEY_CONTEXT, SyndicTranscriptPanel};
pub(crate) use provider::*;
pub(crate) use renderer_context_menu::{
    realized_resident_context_menu_record_ids,
    resident_context_menu_command_for_realized_record_id, resident_context_menu_frame_loss,
};
pub(crate) use renderer_media_action::{
    realized_resident_media_action_record_ids,
    resident_media_action_command_for_realized_record_id, resident_media_action_frame_loss,
};
pub(crate) use renderer_quote::{
    realized_resident_quotable_record_ids, resident_quote_command_for_realized_record_ids,
    resident_quote_frame_loss,
};
pub(crate) use renderer_selection::{
    realized_resident_selectable_record_ids, resident_selection_command_for_realized_record_ids,
    resident_selection_frame_loss,
};
pub(crate) use selection::{
    ResidentQuoteCommand, ResidentQuoteOutcome, ResidentSelectedRecord, ResidentSelectionCommand,
    ResidentSelectionOutcome, ResidentSelectionRecordGeometry, ResidentSelectionUnavailable,
    ResidentTranscriptCopyPayload, ResidentTranscriptQuotePayload, ResidentTranscriptQuoteTarget,
    ResidentTranscriptSelection,
};
pub(crate) use snapshot::{
    LocalPresentationReason, ResidentFallbackTarget, ResidentPresentationRecord,
    ResidentPresentationRecordId, ResidentPresentationRecordKind, ResidentRecordProvenance,
    ResidentRecordSource, ResidentResourceSlice, ResidentResourceSnapshot,
    ResidentTranscriptSnapshot, ResidentTranscriptSnapshotState,
};
pub(crate) use status_facts::{
    ResidentTranscriptStatusFacts, ResidentTranscriptStatusScrollMode,
    ResidentTranscriptStatusState, ResidentTranscriptTurnViewFacts,
};
