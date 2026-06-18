use std::ops::Range;

use super::{
    activation::{
        PreparedTranscriptActivation, TranscriptActivationOutcome, TranscriptActivationSeed,
        TranscriptActivationSource,
    },
    context_menu::{
        ResidentContextMenuCommand, ResidentContextMenuOutcome, ResidentContextMenuRecord,
        ResidentContextMenuUnavailable, ResidentTranscriptContextMenuTarget,
        resident_context_menu_record,
    },
    demand::{DemandFact, DemandFactKind, DemandFactSink, DemandFactSinkSnapshot},
    media_action::{
        ResidentMediaActionCommand, ResidentMediaActionOutcome, ResidentMediaActionUnavailable,
        ResidentMediaRangeAvailability, ResidentTranscriptMediaActionTarget,
        ResidentTranscriptMediaPayload, resident_media_action_record, resident_media_reference,
    },
    provider::{
        ProjectionPayload, ProjectionRecord, ProjectionRecordId, ProjectionRecordSet,
        ProjectionRecordsRequest, ProviderRequestId, ProviderRevision, ResourceId, ResourceKind,
        ResourceMetadata, ResourceMetadataRequest, ResourceRangeRequest, ResourceRangeResponse,
        SyndicSourceProvenance, TranscriptCursor, TranscriptPageAnchor, TranscriptPageDirection,
        TranscriptProviderHistoryState, TranscriptProviderRejection,
        TranscriptProviderRejectionReason, TranscriptProviderRequest,
        TranscriptProviderRequestKind, TranscriptProviderResponse, TranscriptProviderResponseKind,
        TranscriptProviderStale, TranscriptProviderTarget, TranscriptViewId, TranscriptViewPage,
        TranscriptViewPageRequest, TranscriptViewPosition, TranscriptViewRecord,
    },
    selection::{
        ResidentQuoteCommand, ResidentQuoteOutcome, ResidentSelectedRecord,
        ResidentSelectionCommand, ResidentSelectionOutcome, ResidentSelectionRecordGeometry,
        ResidentSelectionUnavailable, ResidentTranscriptCopyPayload,
        ResidentTranscriptQuotePayload, ResidentTranscriptQuoteTarget, ResidentTranscriptSelection,
        resident_copy_markdown, resident_quote_markdown, resident_selected_record,
    },
    snapshot::{
        LocalPresentationReason, ResidentFallbackTarget, ResidentPresentationRecord,
        ResidentPresentationRecordId, ResidentPresentationRecordKind, ResidentRecordProvenance,
        ResidentRecordSource, ResidentResourceSlice, ResidentResourceSnapshot,
        ResidentTranscriptSnapshot, ResidentTranscriptSnapshotState,
    },
};

const FALLBACK_RECORD_ESTIMATED_BYTES: usize = 96;
const GEOMETRY_PLACEHOLDER_BYTES_PER_RECORD: usize = 32;
const PIN_PLACEHOLDER_BYTES: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct ResidentTranscriptCore {
    policy: ResidentTranscriptPolicy,
    resident: ResidentSyndicDataSnapshot,
    presentation: ResidentPresentationSnapshot,
    demand_facts: DemandFactSink,
    provider_requests: ProviderRequestBook,
    generation: ResidentGeneration,
    active_selection: Option<ResidentTranscriptSelection>,
    active_quote_target: Option<ResidentTranscriptQuoteTarget>,
    active_context_menu_target: Option<ResidentTranscriptContextMenuTarget>,
    active_media_action_target: Option<ResidentTranscriptMediaActionTarget>,
}

impl Default for ResidentTranscriptCore {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProviderRequestRecord {
    pub(crate) request: TranscriptProviderRequest,
    pub(crate) reason: ProviderRequestReason,
    pub(crate) generation: ResidentGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderRequestReason {
    ActivationSeed,
    VisibleRange,
    AdjacentRange,
    ProjectionAdmission,
    ResourceMetadata,
    ResourceRange,
    Retry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderRequestOutcome {
    Admitted,
    Stale,
    Rejected,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResidentProviderResponseEffect {
    ViewPageAdmitted {
        admitted_count: usize,
    },
    ProjectionRecordsAdmitted {
        admitted_count: usize,
        rejected_count: usize,
    },
    ResourceMetadataAdmitted {
        admitted_count: usize,
    },
    ResourceRangeAdmitted {
        admitted_count: usize,
        byte_count: usize,
    },
    Stale,
    Rejected,
    Ignored,
    UnknownRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProviderRequestBookSnapshot {
    pub(crate) pending_count: usize,
    pub(crate) completed_count: usize,
    pub(crate) stale_result_count: usize,
    pub(crate) rejected_result_count: usize,
    pub(crate) error_count: usize,
    pub(crate) next_request_id: ProviderRequestId,
}

impl ResidentTranscriptCore {
    pub(crate) fn empty() -> Self {
        Self::new(ResidentTranscriptPolicy::default())
    }

    pub(crate) fn new(policy: ResidentTranscriptPolicy) -> Self {
        Self {
            demand_facts: DemandFactSink::with_limit(policy.demand_fact_limit),
            policy,
            resident: ResidentSyndicDataSnapshot::default(),
            presentation: ResidentTranscriptSnapshot::empty(),
            provider_requests: ProviderRequestBook::default(),
            generation: ResidentGeneration::default(),
            active_selection: None,
            active_quote_target: None,
            active_context_menu_target: None,
            active_media_action_target: None,
        }
    }

    pub(crate) fn core_snapshot(&self) -> ResidentCoreSnapshot {
        ResidentCoreSnapshot {
            resident: self.resident.clone(),
            presentation: self.presentation_snapshot(),
            demand_facts: self.demand_facts.snapshot(),
            provider_requests: self.provider_requests.snapshot(),
            policy: self.policy,
            generation: self.generation,
        }
    }

    pub(crate) fn presentation_snapshot(&self) -> ResidentPresentationSnapshot {
        let mut snapshot = self.presentation.clone();
        snapshot.resources = ResidentResourceSnapshot {
            metadata: self.resident.resource_metadata.clone(),
            slices: self.resident.resource_slices.clone(),
        };
        snapshot
    }

    pub(crate) fn demand_fact_snapshot(&self) -> DemandFactSinkSnapshot {
        self.demand_facts.snapshot()
    }

    pub(crate) fn push_demand_fact(&mut self, fact: DemandFact) {
        let demand_is_stale = fact.presentation_revision != self.presentation.presentation_revision;
        if demand_is_stale {
            self.record_stale_measurement_decision(fact.presentation_revision);
            self.demand_facts.push(fact);
            return;
        }

        match &fact.kind {
            DemandFactKind::CurrentAnchor {
                record_id,
                position,
            } => {
                self.resident.current_anchor_record_id = record_id
                    .as_ref()
                    .filter(|record_id| self.presentation_record_exists(record_id))
                    .cloned();
                self.resident.current_anchor_position = *position;
            }
            DemandFactKind::VisibleRange { range } => {
                self.resident.visible_range = self.bounded_presentation_range(range);
                self.presentation.visible_range = self.resident.visible_range.clone();
            }
            DemandFactKind::OverscanRange { range } => {
                self.resident.overscan_range = self.bounded_presentation_range(range);
                self.presentation.realized_range = self.resident.overscan_range.clone();
            }
            DemandFactKind::ActiveSelectionPin { record_id } => {
                if self.presentation_record_exists(record_id)
                    && !self
                        .resident
                        .active_selection_pins
                        .iter()
                        .any(|pinned_id| pinned_id == record_id)
                {
                    self.resident.active_selection_pins.push(record_id.clone());
                    self.refresh_budget_accounting();
                }
            }
            DemandFactKind::OpenMenuPin { record_id } => {
                if self.presentation_record_exists(record_id)
                    && !self
                        .resident
                        .active_menu_pins
                        .iter()
                        .any(|pinned_id| pinned_id == record_id)
                {
                    self.resident.active_menu_pins.push(record_id.clone());
                    self.refresh_budget_accounting();
                }
            }
            DemandFactKind::MediaPreviewPin { resource_id } => {
                if self.resource_referenced_by_presentation(resource_id)
                    && !self
                        .resident
                        .active_resource_pins
                        .iter()
                        .any(|pinned_id| pinned_id == resource_id)
                {
                    self.resident.active_resource_pins.push(resource_id.clone());
                    self.refresh_budget_accounting();
                }
            }
            DemandFactKind::ObsoleteRange { range } => {
                if let Some(range) = self.bounded_presentation_range(range) {
                    self.resident.obsolete_ranges.push(range);
                }
            }
            DemandFactKind::StaleMeasurement { observed_revision } => {
                if *observed_revision != self.presentation.presentation_revision {
                    self.record_stale_measurement_decision(*observed_revision);
                }
            }
            DemandFactKind::MissingBefore { .. }
            | DemandFactKind::MissingAfter { .. }
            | DemandFactKind::Viewport { .. }
            | DemandFactKind::MeasuredRecord { .. }
            | DemandFactKind::AdjacentRange { .. }
            | DemandFactKind::ResourceRange { .. } => {}
        }
        self.demand_facts.push(fact);
    }

    pub(crate) fn drain_demand_facts(&mut self) -> Vec<DemandFact> {
        self.demand_facts.drain()
    }

    pub(crate) fn apply_resident_selection(
        &mut self,
        command: ResidentSelectionCommand,
    ) -> ResidentSelectionOutcome {
        if command.records.is_empty() {
            self.clear_active_selection();
            return ResidentSelectionOutcome::Cleared;
        }

        match self.validate_resident_selection(&command) {
            Ok(selection) => {
                self.resident.active_selection_pins = selection.record_ids();
                self.active_selection = Some(selection.clone());
                self.refresh_budget_accounting();
                ResidentSelectionOutcome::Selected(selection)
            }
            Err(error) => {
                self.clear_active_selection();
                ResidentSelectionOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn clear_resident_selection(&mut self) -> ResidentSelectionOutcome {
        self.clear_active_selection();
        ResidentSelectionOutcome::Cleared
    }

    pub(crate) fn resident_copy_payload(
        &self,
    ) -> Result<ResidentTranscriptCopyPayload, ResidentSelectionUnavailable> {
        let Some(selection) = &self.active_selection else {
            return Err(ResidentSelectionUnavailable::NoActiveSelection);
        };
        self.copy_payload_for_selection(selection)
    }

    pub(crate) fn resident_selection(&self) -> Option<ResidentTranscriptSelection> {
        self.active_selection.clone()
    }

    pub(crate) fn apply_resident_quote_target(
        &mut self,
        command: ResidentQuoteCommand,
    ) -> ResidentQuoteOutcome {
        if command.records.is_empty() {
            self.clear_active_quote_target();
            return ResidentQuoteOutcome::Cleared;
        }

        match self.validate_resident_quote_target(&command) {
            Ok(target) => {
                self.resident.active_quote_pins = target.record_ids();
                self.active_quote_target = Some(target.clone());
                self.refresh_budget_accounting();
                ResidentQuoteOutcome::Targeted(target)
            }
            Err(error) => {
                self.clear_active_quote_target();
                ResidentQuoteOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn clear_resident_quote_target(&mut self) -> ResidentQuoteOutcome {
        self.clear_active_quote_target();
        ResidentQuoteOutcome::Cleared
    }

    pub(crate) fn resident_quote_payload(
        &self,
    ) -> Result<ResidentTranscriptQuotePayload, ResidentSelectionUnavailable> {
        let Some(target) = &self.active_quote_target else {
            return Err(ResidentSelectionUnavailable::NoActiveQuoteTarget);
        };
        self.quote_payload_for_target(target)
    }

    pub(crate) fn resident_quote_target(&self) -> Option<ResidentTranscriptQuoteTarget> {
        self.active_quote_target.clone()
    }

    pub(crate) fn apply_resident_context_menu_target(
        &mut self,
        command: ResidentContextMenuCommand,
    ) -> ResidentContextMenuOutcome {
        match self.validate_resident_context_menu_target(&command) {
            Ok(target) => {
                self.resident.active_menu_pins = target.record_ids();
                self.active_context_menu_target = Some(target.clone());
                self.refresh_budget_accounting();
                ResidentContextMenuOutcome::Targeted(target)
            }
            Err(error) => {
                self.clear_active_context_menu_target();
                ResidentContextMenuOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn clear_resident_context_menu_target(&mut self) -> ResidentContextMenuOutcome {
        self.clear_active_context_menu_target();
        ResidentContextMenuOutcome::Cleared
    }

    pub(crate) fn resident_context_menu_target(
        &self,
    ) -> Option<ResidentTranscriptContextMenuTarget> {
        self.active_context_menu_target.clone()
    }

    pub(crate) fn apply_resident_media_action_target(
        &mut self,
        command: ResidentMediaActionCommand,
    ) -> ResidentMediaActionOutcome {
        match self.validate_resident_media_action_target(&command) {
            Ok(target) => {
                self.resident.active_media_pins = target.record_ids();
                self.active_media_action_target = Some(target.clone());
                self.refresh_budget_accounting();
                ResidentMediaActionOutcome::Targeted(target)
            }
            Err(error) => {
                self.clear_active_media_action_target();
                ResidentMediaActionOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn clear_resident_media_action_target(&mut self) -> ResidentMediaActionOutcome {
        self.clear_active_media_action_target();
        ResidentMediaActionOutcome::Cleared
    }

    pub(crate) fn resident_media_action_payload(
        &self,
    ) -> Result<ResidentTranscriptMediaPayload, ResidentMediaActionUnavailable> {
        let Some(target) = &self.active_media_action_target else {
            return Err(ResidentMediaActionUnavailable::NoActiveMediaActionTarget);
        };
        self.media_payload_for_target(target)
    }

    pub(crate) fn resident_media_action_target(
        &self,
    ) -> Option<ResidentTranscriptMediaActionTarget> {
        self.active_media_action_target.clone()
    }

    pub(crate) fn policy(&self) -> ResidentTranscriptPolicy {
        self.policy
    }

    pub(crate) fn provider_request_snapshot(&self) -> ProviderRequestBookSnapshot {
        self.provider_requests.snapshot()
    }

    pub(crate) fn begin_activation(
        &mut self,
        seed: TranscriptActivationSeed,
    ) -> TranscriptActivationOutcome {
        let retained_previous_snapshot =
            seed.view_id.is_some() && self.has_coherent_presentation_snapshot();

        self.bump_generation();
        self.clear_active_selection_state();
        self.clear_active_context_menu_target_state();
        self.clear_active_media_action_target_state();
        self.presentation.activation_revision =
            self.presentation.activation_revision.saturating_add(1);
        self.presentation.presentation_revision =
            self.presentation.presentation_revision.saturating_add(1);
        self.update_remaining_presentation_revision();

        if !retained_previous_snapshot {
            self.clear_resident_data_for_activation();
            self.presentation.records.clear();
            self.presentation.realized_range = None;
            self.presentation.visible_range = None;
            self.presentation.state = if seed.view_id.is_some() {
                ResidentTranscriptSnapshotState::Empty
            } else {
                ResidentTranscriptSnapshotState::Unavailable {
                    reason: "transcript view unavailable for activation".to_string(),
                }
            };
        }

        self.refresh_resident_record_counts();
        self.refresh_budget_accounting();

        let provider_request = seed.view_id.clone().map(|view_id| {
            let (anchor, direction) = seed.placement.provider_page_shape();
            self.request_view_page(
                view_id,
                anchor,
                direction,
                ProviderRequestReason::ActivationSeed,
            )
        });

        TranscriptActivationOutcome {
            activation_revision: self.presentation.activation_revision,
            presentation_revision: self.presentation.presentation_revision,
            state: self.presentation.state.clone(),
            retained_previous_snapshot,
            provider_request,
        }
    }

    pub(crate) fn apply_prepared_activation(
        &mut self,
        prepared: PreparedTranscriptActivation,
        source: TranscriptActivationSource,
    ) -> TranscriptActivationOutcome {
        let seed = prepared.seed(source);
        let activation = self.begin_activation(seed);
        if let (Some(request), Some(kind)) = (
            activation.provider_request.as_ref(),
            prepared.view_page_response,
        ) {
            self.handle_provider_response(TranscriptProviderResponse {
                request_id: request.id,
                kind,
            });
        }

        if let Some(kind) = prepared.projection_records_response
            && let Some(request) = self.request_projection_records_for_resident_view(
                ProviderRequestReason::ProjectionAdmission,
            )
        {
            self.handle_provider_response(TranscriptProviderResponse {
                request_id: request.id,
                kind,
            });
        }

        TranscriptActivationOutcome {
            activation_revision: self.presentation.activation_revision,
            presentation_revision: self.presentation.presentation_revision,
            state: self.presentation.state.clone(),
            retained_previous_snapshot: activation.retained_previous_snapshot,
            provider_request: None,
        }
    }

    pub(crate) fn reserve_provider_request(
        &mut self,
        kind: TranscriptProviderRequestKind,
        reason: ProviderRequestReason,
    ) -> TranscriptProviderRequest {
        self.provider_requests
            .reserve(kind, reason, self.generation)
    }

    pub(crate) fn request_view_page(
        &mut self,
        view_id: TranscriptViewId,
        anchor: TranscriptPageAnchor,
        direction: TranscriptPageDirection,
        reason: ProviderRequestReason,
    ) -> TranscriptProviderRequest {
        let observed_revision = self.observed_revision_for_view(&view_id);
        let limit = self
            .policy
            .view_page_limit
            .min(self.policy.max_resident_view_records);

        self.reserve_provider_request(
            TranscriptProviderRequestKind::ReadViewPage(TranscriptViewPageRequest {
                view_id,
                anchor,
                direction,
                limit,
                observed_revision,
            }),
            reason,
        )
    }

    pub(crate) fn request_projection_records_for_resident_view(
        &mut self,
        reason: ProviderRequestReason,
    ) -> Option<TranscriptProviderRequest> {
        let view_id = self.resident.view_id.clone()?;
        let remaining_capacity = self
            .policy
            .max_resident_projection_records
            .saturating_sub(self.resident.projection_records.len());
        if remaining_capacity == 0 {
            return None;
        }

        let mut projection_ids = Vec::new();
        for view_record in &self.resident.view_records {
            if self
                .resident
                .projection_records
                .iter()
                .any(|record| record.id == view_record.projection_id)
            {
                continue;
            }
            if projection_ids
                .iter()
                .any(|projection_id| projection_id == &view_record.projection_id)
            {
                continue;
            }

            projection_ids.push(view_record.projection_id.clone());
            if projection_ids.len() == remaining_capacity {
                break;
            }
        }

        (!projection_ids.is_empty()).then(|| {
            self.reserve_provider_request(
                TranscriptProviderRequestKind::ReadProjectionRecords(ProjectionRecordsRequest {
                    view_id,
                    projection_ids,
                    observed_revision: self.resident.provider_revision,
                }),
                reason,
            )
        })
    }

    pub(crate) fn request_resource_metadata_for_presentation_records(
        &mut self,
        reason: ProviderRequestReason,
    ) -> Option<TranscriptProviderRequest> {
        let resource_id = self
            .presentation_resource_ids()
            .into_iter()
            .find(|resource_id| {
                !self.resource_metadata_is_resident(resource_id)
                    && !self.resource_has_terminal_fallback(resource_id)
                    && !self.has_pending_resource_metadata_request(resource_id)
            })?;

        Some(self.reserve_provider_request(
            TranscriptProviderRequestKind::ReadResourceMetadata(ResourceMetadataRequest {
                resource_id,
                observed_revision: self.resident.provider_revision,
            }),
            reason,
        ))
    }

    pub(crate) fn request_resource_range(
        &mut self,
        resource_id: ResourceId,
        range: Range<u64>,
        reason: ProviderRequestReason,
    ) -> Option<TranscriptProviderRequest> {
        if !self.resource_referenced_by_presentation(&resource_id) {
            return None;
        }

        if self.resource_has_terminal_fallback(&resource_id) {
            return None;
        }

        let Some(bounded_range) = self.bound_resource_range(range.clone()) else {
            self.upsert_resource_fallback_for_demand(
                &resource_id,
                ResidentFallbackTarget::ResourceRange {
                    resource_id: resource_id.clone(),
                    range,
                },
                LocalPresentationReason::BudgetRejected,
                None,
                Some(0),
            );
            return None;
        };
        if self.resource_range_is_resident(&resource_id, &bounded_range)
            || self.has_pending_resource_range_request(&resource_id, &bounded_range)
        {
            return None;
        }

        let observed_revision = self
            .resident
            .resource_metadata
            .iter()
            .find(|metadata| metadata.resource_id == resource_id)
            .map(|metadata| metadata.revision)
            .or(self.resident.provider_revision);

        Some(self.reserve_provider_request(
            TranscriptProviderRequestKind::ReadResourceRange(ResourceRangeRequest {
                resource_id,
                range: bounded_range,
                observed_revision,
            }),
            reason,
        ))
    }

    pub(crate) fn handle_provider_response(
        &mut self,
        response: TranscriptProviderResponse,
    ) -> ResidentProviderResponseEffect {
        let request_id = response.request_id;
        match response.kind {
            TranscriptProviderResponseKind::ViewPage(page) => {
                if let Err(effect) = self
                    .finish_current_provider_response(request_id, ProviderRequestOutcome::Admitted)
                {
                    return effect;
                }

                let admitted_count = self.admit_view_page(page);
                ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count }
            }
            TranscriptProviderResponseKind::ProjectionRecords(set) => {
                if let Err(effect) = self
                    .finish_current_provider_response(request_id, ProviderRequestOutcome::Admitted)
                {
                    return effect;
                }

                let (admitted_count, rejected_count) = self.admit_projection_record_set(set);
                ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
                    admitted_count,
                    rejected_count,
                }
            }
            TranscriptProviderResponseKind::ResourceMetadata(metadata) => {
                if let Err(effect) = self
                    .finish_current_provider_response(request_id, ProviderRequestOutcome::Admitted)
                {
                    return effect;
                }

                let admitted_count = self.admit_resource_metadata(metadata);
                ResidentProviderResponseEffect::ResourceMetadataAdmitted { admitted_count }
            }
            TranscriptProviderResponseKind::ResourceRange(range) => {
                if let Err(effect) = self
                    .finish_current_provider_response(request_id, ProviderRequestOutcome::Admitted)
                {
                    return effect;
                }

                let (admitted_count, byte_count) = self.admit_resource_range(range);
                ResidentProviderResponseEffect::ResourceRangeAdmitted {
                    admitted_count,
                    byte_count,
                }
            }
            TranscriptProviderResponseKind::Rejected(rejection) => {
                if let Err(effect) = self
                    .finish_current_provider_response(request_id, ProviderRequestOutcome::Rejected)
                {
                    return effect;
                }

                self.admit_provider_rejection(rejection);
                ResidentProviderResponseEffect::Rejected
            }
            TranscriptProviderResponseKind::Stale(stale) => {
                if let Err(effect) =
                    self.finish_current_provider_response(request_id, ProviderRequestOutcome::Stale)
                {
                    return effect;
                }

                self.admit_stale_result(stale);
                ResidentProviderResponseEffect::Stale
            }
        }
    }

    pub(crate) fn finish_provider_request(
        &mut self,
        id: ProviderRequestId,
        outcome: ProviderRequestOutcome,
    ) -> Option<ProviderRequestRecord> {
        self.provider_requests.finish(id, outcome)
    }

    pub(crate) fn bump_generation(&mut self) -> ResidentGeneration {
        self.generation = ResidentGeneration(self.generation.0.saturating_add(1));
        self.generation
    }

    pub(crate) fn notice_provider_invalidation(
        &mut self,
        current_revision: ProviderRevision,
    ) -> ResidentGeneration {
        let previous_revision = self.resident.provider_revision;
        if previous_revision == Some(current_revision) {
            return self.generation;
        }

        let generation = self.bump_generation();
        self.resident.provider_revision = Some(current_revision);
        self.record_release_decision(ResidentReleaseDecision {
            generation,
            reason: ResidentReleaseReason::ProviderInvalidation,
            target: ResidentReleaseTarget::ProviderRevision {
                previous: previous_revision,
                current: current_revision,
            },
            released_presentation_record_count: 0,
            preserved_presentation_record_count: self.presentation.records.len(),
            released_view_record_count: 0,
            released_projection_record_count: 0,
            released_resource_metadata_count: 0,
            released_resource_slice_count: 0,
            released_fallback_record_count: 0,
        });
        generation
    }

    pub(crate) fn release_obsolete_resident_data(&mut self) -> usize {
        if self.resident.obsolete_ranges.is_empty() {
            return 0;
        }

        let obsolete_ranges = std::mem::take(&mut self.resident.obsolete_ranges);
        let obsolete_record_ids = self.obsolete_presentation_record_ids(&obsolete_ranges);
        if obsolete_record_ids.is_empty() {
            return 0;
        }

        let protected_record_ids = self.protected_presentation_record_ids();
        let visible_record_ids =
            self.presentation_record_ids_for_optional_range(self.resident.visible_range.as_ref());
        let overscan_record_ids =
            self.presentation_record_ids_for_optional_range(self.resident.overscan_range.as_ref());
        let release_record_ids: Vec<_> = obsolete_record_ids
            .iter()
            .filter(|record_id| !record_id_in(&protected_record_ids, record_id))
            .cloned()
            .collect();
        let preserved_count = obsolete_record_ids
            .len()
            .saturating_sub(release_record_ids.len());
        let target_range = combined_range(&obsolete_ranges);

        let release_projection_ids =
            self.presentation_projection_ids_for_records(&release_record_ids);
        let released_presentation_record_count =
            self.release_presentation_records(&release_record_ids);
        let released_fallback_record_count =
            self.release_fallback_records(&release_record_ids, &release_projection_ids);
        let released_view_record_count = self.release_view_records(&release_projection_ids);
        let released_projection_record_count =
            self.release_projection_records(&release_projection_ids);
        let (released_resource_metadata_count, released_resource_slice_count) =
            self.release_unreferenced_resources();

        if released_presentation_record_count > 0 {
            self.presentation.presentation_revision =
                self.presentation.presentation_revision.saturating_add(1);
            self.update_remaining_presentation_revision();
            self.reconcile_retained_ranges_after_release(&visible_record_ids, &overscan_record_ids);
        }
        self.prune_stale_pins();
        self.refresh_resident_record_counts();
        self.refresh_budget_accounting();

        self.record_release_decision(ResidentReleaseDecision {
            generation: self.generation,
            reason: ResidentReleaseReason::ObsoleteResidentRange,
            target: ResidentReleaseTarget::PresentationRange(target_range),
            released_presentation_record_count,
            preserved_presentation_record_count: preserved_count,
            released_view_record_count,
            released_projection_record_count,
            released_resource_metadata_count,
            released_resource_slice_count,
            released_fallback_record_count,
        });

        1
    }

    fn finish_current_provider_response(
        &mut self,
        request_id: ProviderRequestId,
        current_generation_outcome: ProviderRequestOutcome,
    ) -> Result<ProviderRequestRecord, ResidentProviderResponseEffect> {
        let Some(request_generation) = self
            .provider_requests
            .pending
            .iter()
            .find(|record| record.request.id == request_id)
            .map(|record| record.generation)
        else {
            return Err(ResidentProviderResponseEffect::UnknownRequest);
        };
        let outcome = if request_generation == self.generation {
            current_generation_outcome
        } else {
            ProviderRequestOutcome::Stale
        };
        let Some(record) = self.provider_requests.finish(request_id, outcome) else {
            return Err(ResidentProviderResponseEffect::UnknownRequest);
        };
        if record.generation == self.generation {
            Ok(record)
        } else {
            Err(ResidentProviderResponseEffect::Ignored)
        }
    }

    fn has_coherent_presentation_snapshot(&self) -> bool {
        !self.presentation.records.is_empty()
            && !matches!(
                self.presentation.state,
                ResidentTranscriptSnapshotState::Unavailable { .. }
            )
    }

    fn clear_resident_data_for_activation(&mut self) {
        self.resident.view_id = None;
        self.resident.provider_revision = None;
        self.resident.history_state = None;
        self.resident.view_records.clear();
        self.resident.projection_records.clear();
        self.resident.projection_rejections.clear();
        self.resident.previous_cursor = None;
        self.resident.next_cursor = None;
        self.resident.at_start = false;
        self.resident.at_end = false;
        self.clear_resource_residency();
        self.clear_retention_state();
    }

    fn bounded_presentation_range(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let record_count = self.presentation.records.len();
        let start = range.start.min(record_count);
        let end = range.end.min(record_count);
        (start < end).then_some(start..end)
    }

    fn presentation_record_exists(&self, record_id: &ResidentPresentationRecordId) -> bool {
        self.presentation
            .records
            .iter()
            .any(|record| &record.id == record_id)
    }

    fn validate_resident_selection(
        &self,
        command: &ResidentSelectionCommand,
    ) -> Result<ResidentTranscriptSelection, ResidentSelectionUnavailable> {
        if command.presentation_revision != self.presentation.presentation_revision {
            return Err(ResidentSelectionUnavailable::StalePresentationRevision {
                observed: command.presentation_revision,
                current: self.presentation.presentation_revision,
            });
        }
        let Some(realized_range) = &self.presentation.realized_range else {
            return Err(ResidentSelectionUnavailable::NoRealizedFrame);
        };

        let mut selected = Vec::new();
        for geometry in &command.records {
            if !geometry.is_stable() {
                return Err(ResidentSelectionUnavailable::UnstableGeometry {
                    record_id: geometry.record_id.clone(),
                });
            }
            if selected.iter().any(
                |(record_id, _, _): &(
                    ResidentPresentationRecordId,
                    usize,
                    ResidentSelectedRecord,
                )| record_id == &geometry.record_id,
            ) {
                return Err(ResidentSelectionUnavailable::DuplicateRecord {
                    record_id: geometry.record_id.clone(),
                });
            }

            let (index, record) = self.presentation_record_for_selection(geometry)?;
            if !realized_range.contains(&index) {
                return Err(ResidentSelectionUnavailable::RecordNotRealized {
                    record_id: geometry.record_id.clone(),
                });
            }
            if record.provenance.presentation_revision != self.presentation.presentation_revision {
                return Err(ResidentSelectionUnavailable::StaleRecord {
                    record_id: record.id.clone(),
                });
            }

            let selected_record = resident_selected_record(record)?;
            selected.push((record.id.clone(), index, selected_record));
        }

        if selected.is_empty() {
            return Err(ResidentSelectionUnavailable::EmptySelection);
        }

        selected.sort_by_key(|(_, index, _)| *index);
        Ok(ResidentTranscriptSelection::new(
            self.presentation.presentation_revision,
            selected
                .into_iter()
                .map(|(_, _, selected_record)| selected_record)
                .collect(),
        ))
    }

    fn validate_resident_quote_target(
        &self,
        command: &ResidentQuoteCommand,
    ) -> Result<ResidentTranscriptQuoteTarget, ResidentSelectionUnavailable> {
        let selection_command =
            ResidentSelectionCommand::new(command.presentation_revision, command.records.clone());
        let selection = self.validate_resident_selection(&selection_command)?;
        Ok(ResidentTranscriptQuoteTarget::new(
            selection.presentation_revision,
            selection.records,
        ))
    }

    fn validate_resident_context_menu_target(
        &self,
        command: &ResidentContextMenuCommand,
    ) -> Result<ResidentTranscriptContextMenuTarget, ResidentContextMenuUnavailable> {
        if command.presentation_revision != self.presentation.presentation_revision {
            return Err(ResidentContextMenuUnavailable::StalePresentationRevision {
                observed: command.presentation_revision,
                current: self.presentation.presentation_revision,
            });
        }
        let Some(realized_range) = &self.presentation.realized_range else {
            return Err(ResidentContextMenuUnavailable::NoRealizedFrame);
        };
        if !command.record.is_stable() {
            return Err(ResidentContextMenuUnavailable::UnstableGeometry {
                record_id: command.record.record_id.clone(),
            });
        }

        let (index, record) = self.presentation_record_for_context_menu(&command.record)?;
        if !realized_range.contains(&index) {
            return Err(ResidentContextMenuUnavailable::RecordNotRealized {
                record_id: command.record.record_id.clone(),
            });
        }
        if record.provenance.presentation_revision != self.presentation.presentation_revision {
            return Err(ResidentContextMenuUnavailable::StaleRecord {
                record_id: record.id.clone(),
            });
        }

        let target_record = resident_context_menu_record(record, command.record.clone())?;
        Ok(ResidentTranscriptContextMenuTarget::new(
            self.presentation.presentation_revision,
            target_record,
        ))
    }

    fn validate_resident_media_action_target(
        &self,
        command: &ResidentMediaActionCommand,
    ) -> Result<ResidentTranscriptMediaActionTarget, ResidentMediaActionUnavailable> {
        if command.presentation_revision != self.presentation.presentation_revision {
            return Err(ResidentMediaActionUnavailable::StalePresentationRevision {
                observed: command.presentation_revision,
                current: self.presentation.presentation_revision,
            });
        }
        let Some(realized_range) = &self.presentation.realized_range else {
            return Err(ResidentMediaActionUnavailable::NoRealizedFrame);
        };
        if !command.record.is_stable() {
            return Err(ResidentMediaActionUnavailable::UnstableGeometry {
                record_id: command.record.record_id.clone(),
            });
        }

        let (index, record) = self.presentation_record_for_media_action(&command.record)?;
        if !realized_range.contains(&index) {
            return Err(ResidentMediaActionUnavailable::RecordNotRealized {
                record_id: command.record.record_id.clone(),
            });
        }
        if record.provenance.presentation_revision != self.presentation.presentation_revision {
            return Err(ResidentMediaActionUnavailable::StaleRecord {
                record_id: record.id.clone(),
            });
        }

        let reference = resident_media_reference(record)?;
        if let Some(rejection) = self.resource_rejection_for_resource(&reference.resource_id) {
            return Err(ResidentMediaActionUnavailable::RejectedResource {
                resource_id: reference.resource_id,
                reason: rejection.reason.clone(),
            });
        }
        let Some(metadata) = self.resource_metadata_for(&reference.resource_id).cloned() else {
            return Err(ResidentMediaActionUnavailable::MissingResourceMetadata {
                record_id: record.id.clone(),
                resource_id: reference.resource_id,
            });
        };
        let range_availability =
            self.media_range_availability(&reference.resource_id, &reference.resource_range)?;
        let target_record = resident_media_action_record(
            reference,
            metadata,
            range_availability,
            command.record.clone(),
        )?;

        Ok(ResidentTranscriptMediaActionTarget::new(
            self.presentation.presentation_revision,
            target_record,
        ))
    }

    fn presentation_record_for_selection(
        &self,
        geometry: &ResidentSelectionRecordGeometry,
    ) -> Result<(usize, &ResidentPresentationRecord), ResidentSelectionUnavailable> {
        self.presentation
            .records
            .iter()
            .enumerate()
            .find(|(_, record)| record.id == geometry.record_id)
            .ok_or_else(|| ResidentSelectionUnavailable::RecordNotResident {
                record_id: geometry.record_id.clone(),
            })
    }

    fn presentation_record_for_context_menu(
        &self,
        geometry: &ResidentSelectionRecordGeometry,
    ) -> Result<(usize, &ResidentPresentationRecord), ResidentContextMenuUnavailable> {
        self.presentation
            .records
            .iter()
            .enumerate()
            .find(|(_, record)| record.id == geometry.record_id)
            .ok_or_else(|| ResidentContextMenuUnavailable::RecordNotResident {
                record_id: geometry.record_id.clone(),
            })
    }

    fn presentation_record_for_media_action(
        &self,
        geometry: &ResidentSelectionRecordGeometry,
    ) -> Result<(usize, &ResidentPresentationRecord), ResidentMediaActionUnavailable> {
        self.presentation
            .records
            .iter()
            .enumerate()
            .find(|(_, record)| record.id == geometry.record_id)
            .ok_or_else(|| ResidentMediaActionUnavailable::RecordNotResident {
                record_id: geometry.record_id.clone(),
            })
    }

    fn copy_payload_for_selection(
        &self,
        selection: &ResidentTranscriptSelection,
    ) -> Result<ResidentTranscriptCopyPayload, ResidentSelectionUnavailable> {
        let mut markdown = String::new();
        let mut records = Vec::new();

        for selected_record in &selection.records {
            let Some(record) = self
                .presentation
                .records
                .iter()
                .find(|record| record.id == selected_record.record_id)
            else {
                return Err(ResidentSelectionUnavailable::RecordNotResident {
                    record_id: selected_record.record_id.clone(),
                });
            };
            let current_selected_record = resident_selected_record(record)?;
            if &current_selected_record != selected_record {
                return Err(ResidentSelectionUnavailable::StaleRecord {
                    record_id: selected_record.record_id.clone(),
                });
            }

            markdown.push_str(resident_copy_markdown(record)?);
            records.push(current_selected_record);
        }

        if markdown.is_empty() {
            return Err(ResidentSelectionUnavailable::EmptySelection);
        }

        Ok(ResidentTranscriptCopyPayload {
            presentation_revision: self.presentation.presentation_revision,
            markdown,
            plain_text: None,
            records,
        })
    }

    fn quote_payload_for_target(
        &self,
        target: &ResidentTranscriptQuoteTarget,
    ) -> Result<ResidentTranscriptQuotePayload, ResidentSelectionUnavailable> {
        let copy_payload = self.copy_payload_for_selection(&ResidentTranscriptSelection::new(
            target.presentation_revision,
            target.records.clone(),
        ))?;
        let Some(quoted_markdown) = resident_quote_markdown(&copy_payload.markdown) else {
            return Err(ResidentSelectionUnavailable::EmptySelection);
        };

        Ok(ResidentTranscriptQuotePayload {
            presentation_revision: copy_payload.presentation_revision,
            quoted_markdown,
            records: copy_payload.records,
        })
    }

    fn media_payload_for_target(
        &self,
        target: &ResidentTranscriptMediaActionTarget,
    ) -> Result<ResidentTranscriptMediaPayload, ResidentMediaActionUnavailable> {
        let command = ResidentMediaActionCommand::new(
            target.presentation_revision,
            target.record.geometry.clone(),
        );
        let current_target = self.validate_resident_media_action_target(&command)?;
        if !current_target
            .record
            .has_same_resident_identity_as(&target.record)
        {
            return Err(ResidentMediaActionUnavailable::StaleRecord {
                record_id: target.record.record_id.clone(),
            });
        }

        let range = current_target.record.resource_range.clone();
        let Some((bytes, complete)) =
            self.resident_media_bytes(&current_target.record.resource_id, &range)
        else {
            return Err(ResidentMediaActionUnavailable::ResourceRangeNotResident {
                resource_id: current_target.record.resource_id,
                range,
            });
        };

        Ok(ResidentTranscriptMediaPayload {
            presentation_revision: self.presentation.presentation_revision,
            record: current_target.record,
            range,
            bytes,
            complete,
        })
    }

    fn media_range_availability(
        &self,
        resource_id: &ResourceId,
        range: &Range<u64>,
    ) -> Result<ResidentMediaRangeAvailability, ResidentMediaActionUnavailable> {
        if let Some(rejection) = self.resource_rejection_for_range(resource_id, range) {
            return Err(ResidentMediaActionUnavailable::RejectedResourceRange {
                resource_id: resource_id.clone(),
                range: range.clone(),
                reason: rejection.reason.clone(),
            });
        }
        if let Some(slice) = self.resident_resource_slice_covering(resource_id, range) {
            return Ok(ResidentMediaRangeAvailability::Resident {
                requested_range: range.clone(),
                resident_range: slice.range.clone(),
                complete: slice.complete,
            });
        }
        let Some(bounded_range) = self.bound_resource_range(range.clone()) else {
            return Err(ResidentMediaActionUnavailable::InvalidResourceRange {
                resource_id: resource_id.clone(),
                range: range.clone(),
            });
        };
        if let Some(rejection) = self.resource_rejection_for_range(resource_id, &bounded_range) {
            return Err(ResidentMediaActionUnavailable::RejectedResourceRange {
                resource_id: resource_id.clone(),
                range: bounded_range,
                reason: rejection.reason.clone(),
            });
        }

        Ok(ResidentMediaRangeAvailability::Demandable {
            range: bounded_range,
        })
    }

    fn resident_media_bytes(
        &self,
        resource_id: &ResourceId,
        range: &Range<u64>,
    ) -> Option<(Vec<u8>, bool)> {
        let slice = self.resident_resource_slice_covering(resource_id, range)?;
        let start = usize::try_from(range.start.saturating_sub(slice.range.start)).ok()?;
        let end = usize::try_from(range.end.saturating_sub(slice.range.start)).ok()?;
        (end <= slice.bytes.len()).then(|| (slice.bytes[start..end].to_vec(), slice.complete))
    }

    fn obsolete_presentation_record_ids(
        &self,
        obsolete_ranges: &[Range<usize>],
    ) -> Vec<ResidentPresentationRecordId> {
        let mut record_ids = Vec::new();
        for range in obsolete_ranges {
            let Some(range) = self.bounded_presentation_range(range) else {
                continue;
            };
            for record in &self.presentation.records[range] {
                push_unique_record_id(&mut record_ids, record.id.clone());
            }
        }
        record_ids
    }

    fn protected_presentation_record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        let mut record_ids = Vec::new();
        if let Some(range) = &self.resident.visible_range {
            self.push_record_ids_in_range(&mut record_ids, range);
        }
        if let Some(range) = &self.resident.overscan_range {
            self.push_record_ids_in_range(&mut record_ids, range);
        }
        if let Some(record_id) = &self.resident.current_anchor_record_id {
            push_unique_record_id(&mut record_ids, record_id.clone());
        }
        if let Some(position) = self.resident.current_anchor_position {
            for record in &self.presentation.records {
                if presentation_record_position(record) == Some(position) {
                    push_unique_record_id(&mut record_ids, record.id.clone());
                }
            }
        }
        for record_id in &self.resident.active_selection_pins {
            push_unique_record_id(&mut record_ids, record_id.clone());
        }
        for record_id in &self.resident.active_quote_pins {
            push_unique_record_id(&mut record_ids, record_id.clone());
        }
        for record_id in &self.resident.active_menu_pins {
            push_unique_record_id(&mut record_ids, record_id.clone());
        }
        for record_id in &self.resident.active_media_pins {
            push_unique_record_id(&mut record_ids, record_id.clone());
        }
        for record in &self.presentation.records {
            let Some(resource_id) = presentation_record_resource_id(record) else {
                continue;
            };
            if self
                .resident
                .active_resource_pins
                .iter()
                .any(|pinned_id| pinned_id == &resource_id)
            {
                push_unique_record_id(&mut record_ids, record.id.clone());
            }
        }
        record_ids
    }

    fn push_record_ids_in_range(
        &self,
        record_ids: &mut Vec<ResidentPresentationRecordId>,
        range: &Range<usize>,
    ) {
        let Some(range) = self.bounded_presentation_range(range) else {
            return;
        };
        for record in &self.presentation.records[range] {
            push_unique_record_id(record_ids, record.id.clone());
        }
    }

    fn presentation_record_ids_for_optional_range(
        &self,
        range: Option<&Range<usize>>,
    ) -> Vec<ResidentPresentationRecordId> {
        let Some(range) = range.and_then(|range| self.bounded_presentation_range(range)) else {
            return Vec::new();
        };
        self.presentation.records[range]
            .iter()
            .map(|record| record.id.clone())
            .collect()
    }

    fn presentation_projection_ids_for_records(
        &self,
        record_ids: &[ResidentPresentationRecordId],
    ) -> Vec<ProjectionRecordId> {
        let mut projection_ids = Vec::new();
        for record in &self.presentation.records {
            if !record_id_in(record_ids, &record.id) {
                continue;
            }
            let Some(projection_id) = &record.provenance.projection_id else {
                continue;
            };
            push_unique_projection_id(&mut projection_ids, projection_id.clone());
        }
        projection_ids
    }

    fn release_presentation_records(
        &mut self,
        record_ids: &[ResidentPresentationRecordId],
    ) -> usize {
        let before_count = self.presentation.records.len();
        self.presentation
            .records
            .retain(|record| !record_id_in(record_ids, &record.id));
        let released_count = before_count.saturating_sub(self.presentation.records.len());
        self.presentation.state =
            self.presentation_state_for_records(!self.presentation.records.is_empty());
        released_count
    }

    fn release_fallback_records(
        &mut self,
        record_ids: &[ResidentPresentationRecordId],
        projection_ids: &[ProjectionRecordId],
    ) -> usize {
        let before_count = self.resident.fallback_records.len();
        let retained_resource_ids = self.retained_resource_ids();
        self.resident.fallback_records.retain(|fallback| {
            if record_id_in(record_ids, &fallback.id) {
                return false;
            }
            match &fallback.target {
                ResidentFallbackTarget::ProjectionRecord(projection_id) => {
                    !projection_id_in(projection_ids, projection_id)
                }
                ResidentFallbackTarget::Resource(resource_id)
                | ResidentFallbackTarget::ResourceRange { resource_id, .. } => {
                    resource_id_in(&retained_resource_ids, resource_id)
                }
            }
        });
        before_count.saturating_sub(self.resident.fallback_records.len())
    }

    fn release_view_records(&mut self, projection_ids: &[ProjectionRecordId]) -> usize {
        let before_count = self.resident.view_records.len();
        self.resident
            .view_records
            .retain(|record| !projection_id_in(projection_ids, &record.projection_id));
        before_count.saturating_sub(self.resident.view_records.len())
    }

    fn release_projection_records(&mut self, projection_ids: &[ProjectionRecordId]) -> usize {
        let before_count = self.resident.projection_records.len();
        self.resident
            .projection_records
            .retain(|record| !projection_id_in(projection_ids, &record.id));
        self.resident.projection_rejections.retain(|rejection| {
            !matches!(
                &rejection.target,
                TranscriptProviderTarget::ProjectionRecord(projection_id)
                    if projection_id_in(projection_ids, projection_id)
            )
        });
        before_count.saturating_sub(self.resident.projection_records.len())
    }

    fn release_unreferenced_resources(&mut self) -> (usize, usize) {
        let retained_resource_ids = self.retained_resource_ids();
        let before_metadata_count = self.resident.resource_metadata.len();
        self.resident
            .resource_metadata
            .retain(|metadata| resource_id_in(&retained_resource_ids, &metadata.resource_id));

        let before_slice_count = self.resident.resource_slices.len();
        self.resident
            .resource_slices
            .retain(|slice| resource_id_in(&retained_resource_ids, &slice.resource_id));
        self.resident.resource_rejections.retain(|rejection| {
            rejection_resource_id(rejection)
                .as_ref()
                .is_some_and(|resource_id| resource_id_in(&retained_resource_ids, resource_id))
        });

        (
            before_metadata_count.saturating_sub(self.resident.resource_metadata.len()),
            before_slice_count.saturating_sub(self.resident.resource_slices.len()),
        )
    }

    fn retained_resource_ids(&self) -> Vec<ResourceId> {
        let mut resource_ids = self.presentation_resource_ids();
        for resource_id in &self.resident.active_resource_pins {
            push_unique_resource_id(&mut resource_ids, resource_id.clone());
        }
        resource_ids
    }

    fn update_remaining_presentation_revision(&mut self) {
        let presentation_revision = self.presentation.presentation_revision;
        for record in &mut self.presentation.records {
            record.provenance.presentation_revision = presentation_revision;
        }
    }

    fn reconcile_retained_ranges_after_release(
        &mut self,
        visible_record_ids: &[ResidentPresentationRecordId],
        overscan_record_ids: &[ResidentPresentationRecordId],
    ) {
        self.resident.visible_range = self.range_covering_existing_ids(visible_record_ids);
        self.presentation.visible_range = self.resident.visible_range.clone();
        self.resident.overscan_range = self.range_covering_existing_ids(overscan_record_ids);
        self.presentation.realized_range = self.resident.overscan_range.clone();
    }

    fn range_covering_existing_ids(
        &self,
        record_ids: &[ResidentPresentationRecordId],
    ) -> Option<Range<usize>> {
        let mut first = None;
        let mut last = None;
        for (index, record) in self.presentation.records.iter().enumerate() {
            if !record_id_in(record_ids, &record.id) {
                continue;
            }
            first.get_or_insert(index);
            last = Some(index);
        }
        first
            .zip(last)
            .map(|(start, end)| start..end.saturating_add(1))
    }

    fn prune_stale_pins(&mut self) {
        let presentation_record_ids = self
            .presentation
            .records
            .iter()
            .map(|record| record.id.clone())
            .collect::<Vec<_>>();
        self.resident
            .active_selection_pins
            .retain(|record_id| record_id_in(&presentation_record_ids, record_id));
        if self.active_selection.as_ref().is_some_and(|selection| {
            selection
                .records
                .iter()
                .any(|record| !record_id_in(&presentation_record_ids, &record.record_id))
        }) {
            self.clear_active_selection_state();
        }
        self.resident
            .active_quote_pins
            .retain(|record_id| record_id_in(&presentation_record_ids, record_id));
        if self.active_quote_target.as_ref().is_some_and(|target| {
            target
                .records
                .iter()
                .any(|record| !record_id_in(&presentation_record_ids, &record.record_id))
        }) {
            self.clear_active_quote_target_state();
        }
        self.resident
            .active_menu_pins
            .retain(|record_id| record_id_in(&presentation_record_ids, record_id));
        if self
            .active_context_menu_target
            .as_ref()
            .is_some_and(|target| {
                target
                    .record_ids()
                    .iter()
                    .any(|record_id| !record_id_in(&presentation_record_ids, record_id))
            })
        {
            self.clear_active_context_menu_target_state();
        }
        self.resident
            .active_media_pins
            .retain(|record_id| record_id_in(&presentation_record_ids, record_id));
        if self
            .active_media_action_target
            .as_ref()
            .is_some_and(|target| {
                target
                    .record_ids()
                    .iter()
                    .any(|record_id| !record_id_in(&presentation_record_ids, record_id))
            })
        {
            self.clear_active_media_action_target_state();
        }

        let retained_resource_ids = self.retained_resource_ids();
        self.resident
            .active_resource_pins
            .retain(|resource_id| resource_id_in(&retained_resource_ids, resource_id));
        if self
            .resident
            .current_anchor_record_id
            .as_ref()
            .is_some_and(|record_id| !record_id_in(&presentation_record_ids, record_id))
        {
            self.resident.current_anchor_record_id = None;
        }
    }

    fn refresh_resident_record_counts(&mut self) {
        self.resident.view_record_count = self.resident.view_records.len();
        self.resident.projection_record_count = self.resident.projection_records.len();
        self.resident.projection_rejection_count = self.resident.projection_rejections.len();
        self.resident.resource_metadata_count = self.resident.resource_metadata.len();
        self.resident.resource_slice_count = self.resident.resource_slices.len();
        self.resident.resource_rejection_count = self.resident.resource_rejections.len();
        self.resident.fallback_record_count = self.resident.fallback_records.len();
        self.resident.release_decision_count = self.resident.release_decisions.len();
    }

    fn record_stale_measurement_decision(&mut self, observed_revision: u64) {
        self.record_release_decision(ResidentReleaseDecision {
            generation: self.generation,
            reason: ResidentReleaseReason::StaleMeasurement,
            target: ResidentReleaseTarget::PresentationRevision {
                observed: observed_revision,
                current: self.presentation.presentation_revision,
            },
            released_presentation_record_count: 0,
            preserved_presentation_record_count: self.presentation.records.len(),
            released_view_record_count: 0,
            released_projection_record_count: 0,
            released_resource_metadata_count: 0,
            released_resource_slice_count: 0,
            released_fallback_record_count: 0,
        });
    }

    fn record_release_decision(&mut self, decision: ResidentReleaseDecision) {
        self.resident.release_decisions.push(decision);
        self.resident.release_decision_count = self.resident.release_decisions.len();
    }

    fn observed_revision_for_view(&self, view_id: &TranscriptViewId) -> Option<ProviderRevision> {
        (self.resident.view_id.as_ref() == Some(view_id))
            .then_some(self.resident.provider_revision)
            .flatten()
    }

    fn admit_view_page(&mut self, page: TranscriptViewPage) -> usize {
        let same_view = self.resident.view_id.as_ref() == Some(&page.view_id);
        let current_min_position = self
            .resident
            .view_records
            .iter()
            .map(|record| record.position)
            .min();
        let current_max_position = self
            .resident
            .view_records
            .iter()
            .map(|record| record.position)
            .max();
        let page_min_position = page.records.iter().map(|record| record.position).min();
        let page_max_position = page.records.iter().map(|record| record.position).max();

        if !same_view {
            self.resident.view_records.clear();
            self.resident.projection_records.clear();
            self.resident.projection_rejections.clear();
            self.clear_resource_residency();
            self.clear_retention_state();
            self.resident.previous_cursor = None;
            self.resident.next_cursor = None;
            self.resident.at_start = false;
            self.resident.at_end = false;
            self.resident.projection_record_count = 0;
            self.resident.projection_rejection_count = 0;
            self.clear_presentation_records();
        }

        self.update_page_boundaries(
            &page,
            current_min_position,
            current_max_position,
            page_min_position,
            page_max_position,
        );

        let before_count = self.resident.view_records.len();
        for record in page.records {
            if let Some(existing) = self
                .resident
                .view_records
                .iter_mut()
                .find(|existing| existing.id == record.id)
            {
                *existing = record;
            } else {
                self.resident.view_records.push(record);
            }
        }

        sort_view_records(&mut self.resident.view_records);
        self.resident
            .view_records
            .truncate(self.policy.max_resident_view_records);

        self.resident.view_id = Some(page.view_id);
        self.resident.provider_revision = Some(page.revision);
        self.resident.history_state = Some(page.history_state);
        self.resident.view_record_count = self.resident.view_records.len();

        if !self.resident.projection_records.is_empty() || !self.presentation.records.is_empty() {
            self.rebuild_presentation_records();
        } else {
            self.refresh_empty_presentation_state_from_history();
        }

        self.resident
            .view_records
            .len()
            .saturating_sub(before_count)
    }

    fn update_page_boundaries(
        &mut self,
        page: &TranscriptViewPage,
        current_min_position: Option<TranscriptViewPosition>,
        current_max_position: Option<TranscriptViewPosition>,
        page_min_position: Option<TranscriptViewPosition>,
        page_max_position: Option<TranscriptViewPosition>,
    ) {
        let resident_empty = self.resident.view_records.is_empty();

        if resident_empty
            || page.at_start
            || extends_leading(page_min_position, current_min_position)
        {
            self.resident.previous_cursor = page.previous_cursor.clone();
        }
        if resident_empty
            || page.at_end
            || extends_trailing(page_max_position, current_max_position)
        {
            self.resident.next_cursor = page.next_cursor.clone();
        }

        self.resident.at_start |= page.at_start;
        self.resident.at_end |= page.at_end;
    }

    fn admit_projection_record_set(&mut self, set: ProjectionRecordSet) -> (usize, usize) {
        if self.resident.view_id.as_ref() != Some(&set.view_id) {
            return (0, 0);
        }

        let before_count = self.resident.projection_records.len();
        let mut should_rebuild = false;

        for record in set.records {
            if !self.projection_referenced_by_resident_view(&record.id) {
                continue;
            }

            if let Some(existing) = self
                .resident
                .projection_records
                .iter_mut()
                .find(|existing| existing.id == record.id)
            {
                *existing = record;
            } else {
                self.resident.projection_records.push(record);
            }
            should_rebuild = true;
        }

        sort_projection_records_for_view(
            &mut self.resident.projection_records,
            &self.resident.view_records,
        );
        self.resident
            .projection_records
            .truncate(self.policy.max_resident_projection_records);

        let mut rejected_count = 0;
        for rejection in set.rejections {
            if !self.rejection_references_resident_projection(&rejection) {
                continue;
            }

            self.upsert_projection_fallback_for_rejection(&rejection);
            self.upsert_projection_rejection(rejection);
            rejected_count += 1;
            should_rebuild = true;
        }
        self.trim_projection_rejections();

        self.resident.provider_revision = Some(set.revision);
        self.resident.projection_record_count = self.resident.projection_records.len();
        self.resident.projection_rejection_count = self.resident.projection_rejections.len();

        if should_rebuild {
            self.rebuild_presentation_records();
        }

        (
            self.resident
                .projection_records
                .len()
                .saturating_sub(before_count),
            rejected_count,
        )
    }

    fn projection_referenced_by_resident_view(&self, projection_id: &ProjectionRecordId) -> bool {
        self.resident
            .view_records
            .iter()
            .any(|record| record.projection_id == *projection_id)
    }

    fn rejection_references_resident_projection(
        &self,
        rejection: &TranscriptProviderRejection,
    ) -> bool {
        match &rejection.target {
            super::provider::TranscriptProviderTarget::ProjectionRecord(projection_id) => {
                self.projection_referenced_by_resident_view(projection_id)
            }
            _ => false,
        }
    }

    fn upsert_projection_rejection(&mut self, rejection: TranscriptProviderRejection) {
        if let Some(existing) = self
            .resident
            .projection_rejections
            .iter_mut()
            .find(|existing| existing.target == rejection.target)
        {
            *existing = rejection;
        } else {
            self.resident.projection_rejections.push(rejection);
        }
    }

    fn trim_projection_rejections(&mut self) {
        let excess = self
            .resident
            .projection_rejections
            .len()
            .saturating_sub(self.policy.max_resident_projection_records);
        if excess > 0 {
            self.resident.projection_rejections.drain(0..excess);
        }
    }

    fn admit_resource_metadata(&mut self, metadata: ResourceMetadata) -> usize {
        if !self.resource_referenced_by_presentation(&metadata.resource_id) {
            return 0;
        }

        let resource_id = metadata.resource_id.clone();
        let revision = metadata.revision;
        let should_budget_fallback = self.metadata_exceeds_media_budget(&metadata);
        let budget_limit = self.policy.max_resource_bytes;
        let byte_len = metadata.byte_len;
        let admitted_count = if let Some(existing) = self
            .resident
            .resource_metadata
            .iter_mut()
            .find(|existing| existing.resource_id == resource_id)
        {
            let changed = *existing != metadata;
            *existing = metadata;
            usize::from(changed)
        } else {
            self.resident.resource_metadata.push(metadata);
            1
        };

        self.remove_resource_rejections_for_resource(&resource_id);
        self.remove_fallbacks_for_resource(&resource_id);
        self.sort_resource_metadata();
        self.resident.provider_revision = Some(revision);
        self.resident.resource_metadata_count = self.resident.resource_metadata.len();
        if should_budget_fallback {
            self.upsert_resource_fallback_for_demand(
                &resource_id,
                ResidentFallbackTarget::Resource(resource_id.clone()),
                LocalPresentationReason::BudgetRejected,
                Some(byte_len as usize),
                Some(budget_limit),
            );
        } else {
            self.refresh_budget_accounting();
        }
        admitted_count
    }

    fn admit_resource_range(&mut self, range: ResourceRangeResponse) -> (usize, usize) {
        if !self.resource_referenced_by_presentation(&range.resource_id) {
            return (0, 0);
        }

        let resource_id = range.resource_id.clone();
        let revision = range.revision;
        let slice = self.bounded_resource_slice(range);
        let byte_count = slice.bytes.len();
        let admitted_count = if let Some(existing) =
            self.resident.resource_slices.iter_mut().find(|existing| {
                existing.resource_id == slice.resource_id && existing.range == slice.range
            }) {
            let changed = *existing != slice;
            *existing = slice;
            usize::from(changed)
        } else {
            self.resident.resource_slices.push(slice);
            1
        };

        self.remove_resource_rejections_for_resource(&resource_id);
        self.remove_fallbacks_for_resource(&resource_id);
        self.sort_resource_slices();
        self.resident.provider_revision = Some(revision);
        self.trim_resource_slices_to_policy();
        self.refresh_budget_accounting();
        (admitted_count, byte_count)
    }

    fn admit_resource_rejection(&mut self, rejection: TranscriptProviderRejection) {
        if !self.rejection_references_resident_resource(&rejection) {
            return;
        }

        if let Some(existing) = self
            .resident
            .resource_rejections
            .iter_mut()
            .find(|existing| existing.target == rejection.target)
        {
            *existing = rejection;
        } else {
            self.resident.resource_rejections.push(rejection);
        }

        self.resident.resource_rejection_count = self.resident.resource_rejections.len();
    }

    fn admit_provider_rejection(&mut self, rejection: TranscriptProviderRejection) {
        self.upsert_resource_fallback_for_rejection(&rejection);
        self.admit_resource_rejection(rejection);
    }

    fn admit_stale_result(&mut self, stale: TranscriptProviderStale) {
        let target = match stale.target {
            TranscriptProviderTarget::Resource(resource_id) => {
                if !self.resource_referenced_by_presentation(&resource_id) {
                    return;
                }
                ResidentFallbackTarget::Resource(resource_id)
            }
            TranscriptProviderTarget::ResourceRange { resource_id, range } => {
                if !self.resource_referenced_by_presentation(&resource_id) {
                    return;
                }
                ResidentFallbackTarget::ResourceRange { resource_id, range }
            }
            _ => return,
        };

        let Some(resource_id) = fallback_target_resource_id(&target) else {
            return;
        };
        self.upsert_resource_fallback_for_demand(
            &resource_id,
            target,
            LocalPresentationReason::PendingCoherentData,
            None,
            None,
        );
    }

    fn upsert_projection_fallback_for_rejection(
        &mut self,
        rejection: &TranscriptProviderRejection,
    ) {
        let TranscriptProviderTarget::ProjectionRecord(projection_id) = &rejection.target else {
            return;
        };
        let Some(reason) = fallback_reason_for_rejection(&rejection.reason) else {
            return;
        };
        let Some(view_record) = self
            .resident
            .view_records
            .iter()
            .find(|record| record.projection_id == *projection_id)
        else {
            return;
        };

        let fallback = ResidentFallbackRecord {
            id: ResidentPresentationRecordId(format!(
                "fallback:view:{}:projection:{}",
                view_record.id.0, projection_id.0
            )),
            reason,
            target: ResidentFallbackTarget::ProjectionRecord(projection_id.clone()),
            provenance: ResidentRecordProvenance {
                source: ResidentRecordSource::LocalUiForSyndic(view_record.provenance.clone()),
                projection_id: Some(projection_id.clone()),
                projection_revision: rejection.revision,
                presentation_revision: 0,
                copy_source_range: view_record.provenance.copy_source_range.clone(),
            },
            provider_rejection_reason: Some(rejection.reason.clone()),
            estimated_bytes: FALLBACK_RECORD_ESTIMATED_BYTES,
            rejected_bytes: None,
            limit_bytes: None,
        };
        self.upsert_fallback_record(fallback);
    }

    fn upsert_resource_fallback_for_rejection(&mut self, rejection: &TranscriptProviderRejection) {
        let Some(reason) = fallback_reason_for_rejection(&rejection.reason) else {
            return;
        };
        let target = match &rejection.target {
            TranscriptProviderTarget::Resource(resource_id) => {
                ResidentFallbackTarget::Resource(resource_id.clone())
            }
            TranscriptProviderTarget::ResourceRange { resource_id, range } => {
                ResidentFallbackTarget::ResourceRange {
                    resource_id: resource_id.clone(),
                    range: range.clone(),
                }
            }
            _ => return,
        };
        let Some(resource_id) = fallback_target_resource_id(&target) else {
            return;
        };
        if !self.resource_referenced_by_presentation(&resource_id) {
            return;
        }

        let mut fallback = self.resource_fallback_record(&resource_id, target, reason, None, None);
        fallback.provider_rejection_reason = Some(rejection.reason.clone());
        self.upsert_fallback_record(fallback);
    }

    fn upsert_resource_fallback_for_demand(
        &mut self,
        resource_id: &ResourceId,
        target: ResidentFallbackTarget,
        reason: LocalPresentationReason,
        rejected_bytes: Option<usize>,
        limit_bytes: Option<usize>,
    ) {
        if !self.resource_referenced_by_presentation(resource_id) {
            return;
        }

        let fallback =
            self.resource_fallback_record(resource_id, target, reason, rejected_bytes, limit_bytes);
        self.upsert_fallback_record(fallback);
    }

    fn resource_fallback_record(
        &self,
        resource_id: &ResourceId,
        target: ResidentFallbackTarget,
        reason: LocalPresentationReason,
        rejected_bytes: Option<usize>,
        limit_bytes: Option<usize>,
    ) -> ResidentFallbackRecord {
        let source_record =
            self.presentation.records.iter().find(|record| {
                presentation_record_resource_id(record).as_ref() == Some(resource_id)
            });
        let source = source_record.and_then(|record| syndic_source_for_record(record));
        let projection_id =
            source_record.and_then(|record| record.provenance.projection_id.clone());
        let projection_revision =
            source_record.and_then(|record| record.provenance.projection_revision);
        let copy_source_range =
            source_record.and_then(|record| record.provenance.copy_source_range.clone());
        let source_identity = source
            .as_ref()
            .and_then(|source| source.projection_id.as_ref())
            .map(|projection_id| projection_id.0.as_str())
            .unwrap_or(resource_id.0.as_str());

        ResidentFallbackRecord {
            id: ResidentPresentationRecordId(format!(
                "fallback:resource:{}:{}",
                resource_id.0, source_identity
            )),
            reason,
            target,
            provenance: ResidentRecordProvenance {
                source: source
                    .map(ResidentRecordSource::LocalUiForSyndic)
                    .unwrap_or(ResidentRecordSource::LocalUi),
                projection_id,
                projection_revision,
                presentation_revision: 0,
                copy_source_range,
            },
            provider_rejection_reason: None,
            estimated_bytes: FALLBACK_RECORD_ESTIMATED_BYTES,
            rejected_bytes,
            limit_bytes,
        }
    }

    fn upsert_fallback_record(&mut self, fallback: ResidentFallbackRecord) {
        if let Some(existing) = self
            .resident
            .fallback_records
            .iter_mut()
            .find(|existing| existing.target == fallback.target)
        {
            *existing = fallback;
        } else {
            self.resident.fallback_records.push(fallback);
        }

        self.sort_fallback_records();
        self.resident.fallback_record_count = self.resident.fallback_records.len();
        self.resident.budget_rejection_count = self
            .resident
            .fallback_records
            .iter()
            .filter(|fallback| fallback.reason == LocalPresentationReason::BudgetRejected)
            .count();
        self.apply_fallback_records_to_presentation();
        self.refresh_budget_accounting();
    }

    fn remove_fallbacks_for_resource(&mut self, resource_id: &ResourceId) {
        let before_count = self.resident.fallback_records.len();
        self.resident.fallback_records.retain(|fallback| {
            fallback_target_resource_id(&fallback.target).as_ref() != Some(resource_id)
        });
        if self.resident.fallback_records.len() == before_count {
            return;
        }

        self.resident.fallback_record_count = self.resident.fallback_records.len();
        self.resident.budget_rejection_count = self
            .resident
            .fallback_records
            .iter()
            .filter(|fallback| fallback.reason == LocalPresentationReason::BudgetRejected)
            .count();
        self.rebuild_presentation_records();
    }

    fn sort_fallback_records(&mut self) {
        let view_records = &self.resident.view_records;
        let presentation_resource_ids = self.presentation_resource_ids();
        self.resident.fallback_records.sort_by(|left, right| {
            fallback_sort_key(left, view_records, &presentation_resource_ids).cmp(
                &fallback_sort_key(right, view_records, &presentation_resource_ids),
            )
        });
    }

    fn apply_fallback_records_to_presentation(&mut self) {
        if self.resident.fallback_records.is_empty() {
            return;
        }

        let presentation_revision = self.presentation.presentation_revision;
        for fallback in &self.resident.fallback_records {
            let fallback_record = presentation_record_for_fallback(fallback, presentation_revision);

            if let Some(existing) = self.presentation.records.iter_mut().find(|record| {
                presentation_record_matches_fallback_target(record, &fallback.target)
            }) {
                *existing = fallback_record;
                continue;
            }

            if self.presentation.records.len() == self.policy.max_presentation_records {
                continue;
            }
            self.presentation.records.push(fallback_record);
        }

        let presentation_resource_ids = self.presentation_resource_ids();
        sort_presentation_records(
            &mut self.presentation.records,
            &self.resident.view_records,
            &presentation_resource_ids,
        );
    }

    fn metadata_exceeds_media_budget(&self, metadata: &ResourceMetadata) -> bool {
        matches!(
            metadata.kind,
            ResourceKind::Image | ResourceKind::GeneratedImage
        ) && metadata.byte_len as usize > self.policy.max_resource_bytes
    }

    fn resource_has_terminal_fallback(&self, resource_id: &ResourceId) -> bool {
        self.resident.fallback_records.iter().any(|fallback| {
            fallback_target_resource_id(&fallback.target).as_ref() == Some(resource_id)
                && fallback.reason != LocalPresentationReason::PendingCoherentData
        })
    }

    fn rejection_references_resident_resource(
        &self,
        rejection: &TranscriptProviderRejection,
    ) -> bool {
        match &rejection.target {
            TranscriptProviderTarget::Resource(resource_id) => {
                self.resource_referenced_by_presentation(resource_id)
            }
            TranscriptProviderTarget::ResourceRange { resource_id, .. } => {
                self.resource_referenced_by_presentation(resource_id)
            }
            _ => false,
        }
    }

    fn resource_referenced_by_presentation(&self, resource_id: &ResourceId) -> bool {
        self.presentation
            .records
            .iter()
            .any(|record| presentation_record_resource_id(record).as_ref() == Some(resource_id))
    }

    fn presentation_resource_ids(&self) -> Vec<ResourceId> {
        let mut resource_ids = Vec::new();
        for record in &self.presentation.records {
            let Some(resource_id) = presentation_record_resource_id(record) else {
                continue;
            };
            if !resource_ids
                .iter()
                .any(|resident_id: &ResourceId| resident_id == &resource_id)
            {
                resource_ids.push(resource_id);
            }
        }
        resource_ids
    }

    fn resource_metadata_is_resident(&self, resource_id: &ResourceId) -> bool {
        self.resident
            .resource_metadata
            .iter()
            .any(|metadata| &metadata.resource_id == resource_id)
    }

    fn resource_metadata_for(&self, resource_id: &ResourceId) -> Option<&ResourceMetadata> {
        self.resident
            .resource_metadata
            .iter()
            .find(|metadata| &metadata.resource_id == resource_id)
    }

    fn resource_range_is_resident(&self, resource_id: &ResourceId, range: &Range<u64>) -> bool {
        self.resident.resource_slices.iter().any(|slice| {
            &slice.resource_id == resource_id
                && slice.range.start <= range.start
                && slice.range.end >= range.end
        })
    }

    fn resident_resource_slice_covering(
        &self,
        resource_id: &ResourceId,
        range: &Range<u64>,
    ) -> Option<&ResidentResourceSlice> {
        self.resident.resource_slices.iter().find(|slice| {
            &slice.resource_id == resource_id
                && slice.range.start <= range.start
                && slice.range.end >= range.end
        })
    }

    fn resource_rejection_for_resource(
        &self,
        resource_id: &ResourceId,
    ) -> Option<&TranscriptProviderRejection> {
        self.resident.resource_rejections.iter().find(|rejection| {
            matches!(
                &rejection.target,
                TranscriptProviderTarget::Resource(target_resource_id)
                    if target_resource_id == resource_id
            )
        })
    }

    fn resource_rejection_for_range(
        &self,
        resource_id: &ResourceId,
        range: &Range<u64>,
    ) -> Option<&TranscriptProviderRejection> {
        self.resident.resource_rejections.iter().find(|rejection| {
            matches!(
                &rejection.target,
                TranscriptProviderTarget::ResourceRange {
                    resource_id: target_resource_id,
                    range: target_range,
                } if target_resource_id == resource_id && target_range == range
            )
        })
    }

    fn has_pending_resource_metadata_request(&self, resource_id: &ResourceId) -> bool {
        self.provider_requests.pending.iter().any(|record| {
            matches!(
                &record.request.kind,
                TranscriptProviderRequestKind::ReadResourceMetadata(request)
                    if &request.resource_id == resource_id
            )
        })
    }

    fn has_pending_resource_range_request(
        &self,
        resource_id: &ResourceId,
        range: &Range<u64>,
    ) -> bool {
        self.provider_requests.pending.iter().any(|record| {
            matches!(
                &record.request.kind,
                TranscriptProviderRequestKind::ReadResourceRange(request)
                    if &request.resource_id == resource_id && request.range == *range
            )
        })
    }

    fn bound_resource_range(&self, range: Range<u64>) -> Option<Range<u64>> {
        if range.start >= range.end || self.policy.max_resource_slice_bytes == 0 {
            return None;
        }

        let max_end = range
            .start
            .saturating_add(self.policy.max_resource_slice_bytes as u64);
        Some(range.start..range.end.min(max_end))
    }

    fn bounded_resource_slice(&self, range: ResourceRangeResponse) -> ResidentResourceSlice {
        let mut bytes = range.bytes;
        let mut admitted_range = range.range;
        let mut complete = range.complete;
        if bytes.len() > self.policy.max_resource_slice_bytes {
            bytes.truncate(self.policy.max_resource_slice_bytes);
            admitted_range.end = admitted_range.start.saturating_add(bytes.len() as u64);
            complete = false;
        }

        ResidentResourceSlice {
            resource_id: range.resource_id,
            revision: range.revision,
            kind: range.kind,
            range: admitted_range,
            bytes,
            complete,
        }
    }

    fn sort_resource_metadata(&mut self) {
        let presentation_resource_ids = self.presentation_resource_ids();
        self.resident.resource_metadata.sort_by_key(|metadata| {
            resource_sort_key(&presentation_resource_ids, &metadata.resource_id)
        });
    }

    fn sort_resource_slices(&mut self) {
        let presentation_resource_ids = self.presentation_resource_ids();
        self.resident.resource_slices.sort_by(|left, right| {
            resource_sort_key(&presentation_resource_ids, &left.resource_id)
                .cmp(&resource_sort_key(
                    &presentation_resource_ids,
                    &right.resource_id,
                ))
                .then_with(|| left.range.start.cmp(&right.range.start))
                .then_with(|| left.range.end.cmp(&right.range.end))
        });
    }

    fn trim_resource_slices_to_policy(&mut self) {
        while resource_slice_bytes(&self.resident.resource_slices) > self.policy.max_resource_bytes
            && !self.resident.resource_slices.is_empty()
        {
            self.resident.resource_slices.remove(0);
        }
    }

    fn remove_resource_rejections_for_resource(&mut self, resource_id: &ResourceId) {
        self.resident
            .resource_rejections
            .retain(|rejection| !rejection_targets_resource(rejection, resource_id));
        self.resident.resource_rejection_count = self.resident.resource_rejections.len();
    }

    fn clear_resource_residency(&mut self) {
        self.clear_active_media_action_target_state();
        self.resident.resource_metadata.clear();
        self.resident.resource_slices.clear();
        self.resident.resource_rejections.clear();
        self.resident.fallback_records.clear();
        self.resident.active_resource_pins.clear();
        self.resident.fallback_record_count = 0;
        self.resident.budget_rejection_count = 0;
        self.refresh_budget_accounting();
        self.resident.resource_rejection_count = 0;
        self.resident.active_pin_count = 0;
    }

    fn clear_retention_state(&mut self) {
        self.resident.current_anchor_record_id = None;
        self.resident.current_anchor_position = None;
        self.resident.visible_range = None;
        self.resident.overscan_range = None;
        self.clear_active_selection_state();
        self.clear_active_quote_target_state();
        self.clear_active_context_menu_target_state();
        self.clear_active_media_action_target_state();
        self.resident.obsolete_ranges.clear();
        self.refresh_budget_accounting();
    }

    fn clear_active_selection(&mut self) {
        self.clear_active_selection_state();
        self.refresh_budget_accounting();
    }

    fn clear_active_selection_state(&mut self) {
        self.active_selection = None;
        self.resident.active_selection_pins.clear();
    }

    fn clear_active_quote_target(&mut self) {
        self.clear_active_quote_target_state();
        self.refresh_budget_accounting();
    }

    fn clear_active_quote_target_state(&mut self) {
        self.active_quote_target = None;
        self.resident.active_quote_pins.clear();
    }

    fn clear_active_context_menu_target(&mut self) {
        self.clear_active_context_menu_target_state();
        self.refresh_budget_accounting();
    }

    fn clear_active_context_menu_target_state(&mut self) {
        self.active_context_menu_target = None;
        self.resident.active_menu_pins.clear();
    }

    fn clear_active_media_action_target(&mut self) {
        self.clear_active_media_action_target_state();
        self.refresh_budget_accounting();
    }

    fn clear_active_media_action_target_state(&mut self) {
        self.active_media_action_target = None;
        self.resident.active_media_pins.clear();
    }

    fn refresh_budget_accounting(&mut self) {
        self.resident.resource_metadata_count = self.resident.resource_metadata.len();
        self.resident.resource_slice_count = self.resident.resource_slices.len();
        self.resident.projection_bytes =
            projection_estimated_bytes(&self.resident.projection_records);
        self.resident.presentation_bytes = presentation_estimated_bytes(&self.presentation);
        self.resident.resource_slice_bytes = resource_slice_bytes(&self.resident.resource_slices);
        self.resident.resource_bytes = self.resident.resource_slice_bytes;
        self.resident.geometry_bytes = self
            .presentation
            .records
            .len()
            .saturating_mul(GEOMETRY_PLACEHOLDER_BYTES_PER_RECORD);
        self.resident.active_pin_count = self
            .resident
            .active_resource_pins
            .len()
            .saturating_add(self.resident.active_selection_pins.len())
            .saturating_add(self.resident.active_quote_pins.len())
            .saturating_add(self.resident.active_menu_pins.len())
            .saturating_add(self.resident.active_media_pins.len());
        self.resident.pin_bytes = self
            .resident
            .active_pin_count
            .saturating_mul(PIN_PLACEHOLDER_BYTES);
        self.resident.fallback_record_count = self.resident.fallback_records.len();
        self.resident.budget_rejection_count = self
            .resident
            .fallback_records
            .iter()
            .filter(|fallback| fallback.reason == LocalPresentationReason::BudgetRejected)
            .count();
        self.resident.estimated_resident_bytes = self
            .resident
            .projection_bytes
            .saturating_add(self.resident.presentation_bytes)
            .saturating_add(self.resident.resource_slice_bytes)
            .saturating_add(self.resident.decoded_or_uploaded_media_bytes)
            .saturating_add(self.resident.geometry_bytes)
            .saturating_add(self.resident.pin_bytes);
    }

    fn rebuild_presentation_records(&mut self) {
        self.clear_active_selection_state();
        self.clear_active_context_menu_target_state();
        self.clear_active_media_action_target_state();
        let presentation_revision = self.presentation.presentation_revision.saturating_add(1);
        let mut records = Vec::new();

        for view_record in &self.resident.view_records {
            let Some(projection_record) = self
                .resident
                .projection_records
                .iter()
                .find(|projection_record| projection_record.id == view_record.projection_id)
            else {
                continue;
            };

            records.push(presentation_record_for_projection(
                view_record,
                projection_record,
                presentation_revision,
            ));
            if records.len() == self.policy.max_presentation_records {
                break;
            }
        }

        self.presentation.presentation_revision = presentation_revision;
        self.presentation.realized_range = None;
        self.presentation.visible_range = None;
        self.presentation.state = self.presentation_state_for_records(!records.is_empty());
        self.presentation.records = records;
        self.apply_fallback_records_to_presentation();
        self.refresh_budget_accounting();
    }

    fn refresh_empty_presentation_state_from_history(&mut self) {
        if !self.presentation.records.is_empty() {
            return;
        }
        let next_state = self.presentation_state_for_records(false);
        if self.presentation.state != next_state {
            self.presentation.presentation_revision =
                self.presentation.presentation_revision.saturating_add(1);
            self.presentation.state = next_state;
        }
    }

    fn presentation_state_for_records(&self, has_records: bool) -> ResidentTranscriptSnapshotState {
        match self.resident.history_state.as_ref() {
            Some(TranscriptProviderHistoryState::Incomplete { reason, detail }) => {
                ResidentTranscriptSnapshotState::Incomplete {
                    reason: reason.clone(),
                    detail: detail.clone(),
                }
            }
            Some(TranscriptProviderHistoryState::Unavailable { reason, detail }) => {
                let detail = detail
                    .clone()
                    .unwrap_or_else(|| format!("history unavailable: {reason:?}"));
                ResidentTranscriptSnapshotState::Unavailable { reason: detail }
            }
            Some(TranscriptProviderHistoryState::Complete) | None if has_records => {
                ResidentTranscriptSnapshotState::ProviderBacked {
                    label: "resident-syndic-projections".to_string(),
                }
            }
            Some(TranscriptProviderHistoryState::Complete) | None => {
                ResidentTranscriptSnapshotState::Empty
            }
        }
    }

    fn clear_presentation_records(&mut self) {
        self.clear_active_selection_state();
        self.clear_active_context_menu_target_state();
        self.clear_active_media_action_target_state();
        let had_presentation_state = !self.presentation.records.is_empty()
            || self.presentation.realized_range.is_some()
            || self.presentation.visible_range.is_some()
            || !matches!(
                self.presentation.state,
                ResidentTranscriptSnapshotState::Empty
            );

        if had_presentation_state {
            self.presentation.presentation_revision =
                self.presentation.presentation_revision.saturating_add(1);
        }

        self.presentation.records.clear();
        self.presentation.realized_range = None;
        self.presentation.visible_range = None;
        self.presentation.state = ResidentTranscriptSnapshotState::Empty;
        self.refresh_budget_accounting();
    }
}

fn extends_leading(
    page_min_position: Option<TranscriptViewPosition>,
    current_min_position: Option<TranscriptViewPosition>,
) -> bool {
    match (page_min_position, current_min_position) {
        (Some(page_min_position), Some(current_min_position)) => {
            page_min_position <= current_min_position
        }
        (Some(_), None) => true,
        _ => false,
    }
}

fn extends_trailing(
    page_max_position: Option<TranscriptViewPosition>,
    current_max_position: Option<TranscriptViewPosition>,
) -> bool {
    match (page_max_position, current_max_position) {
        (Some(page_max_position), Some(current_max_position)) => {
            page_max_position >= current_max_position
        }
        (Some(_), None) => true,
        _ => false,
    }
}

fn combined_range(ranges: &[Range<usize>]) -> Range<usize> {
    let start = ranges.iter().map(|range| range.start).min().unwrap_or(0);
    let end = ranges.iter().map(|range| range.end).max().unwrap_or(start);
    start..end
}

fn record_id_in(
    record_ids: &[ResidentPresentationRecordId],
    record_id: &ResidentPresentationRecordId,
) -> bool {
    record_ids.iter().any(|candidate| candidate == record_id)
}

fn push_unique_record_id(
    record_ids: &mut Vec<ResidentPresentationRecordId>,
    record_id: ResidentPresentationRecordId,
) {
    if !record_id_in(record_ids, &record_id) {
        record_ids.push(record_id);
    }
}

fn projection_id_in(
    projection_ids: &[ProjectionRecordId],
    projection_id: &ProjectionRecordId,
) -> bool {
    projection_ids
        .iter()
        .any(|candidate| candidate == projection_id)
}

fn push_unique_projection_id(
    projection_ids: &mut Vec<ProjectionRecordId>,
    projection_id: ProjectionRecordId,
) {
    if !projection_id_in(projection_ids, &projection_id) {
        projection_ids.push(projection_id);
    }
}

fn resource_id_in(resource_ids: &[ResourceId], resource_id: &ResourceId) -> bool {
    resource_ids
        .iter()
        .any(|candidate| candidate == resource_id)
}

fn push_unique_resource_id(resource_ids: &mut Vec<ResourceId>, resource_id: ResourceId) {
    if !resource_id_in(resource_ids, &resource_id) {
        resource_ids.push(resource_id);
    }
}

fn sort_view_records(records: &mut [TranscriptViewRecord]) {
    records.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
}

fn sort_projection_records_for_view(
    records: &mut [ProjectionRecord],
    view_records: &[TranscriptViewRecord],
) {
    records.sort_by(|left, right| {
        projection_sort_key(view_records, &left.id)
            .cmp(&projection_sort_key(view_records, &right.id))
    });
}

fn projection_sort_key(
    view_records: &[TranscriptViewRecord],
    projection_id: &ProjectionRecordId,
) -> (u8, TranscriptViewPosition, String) {
    view_records
        .iter()
        .find(|record| record.projection_id == *projection_id)
        .map(|record| (0, record.position, record.id.0.clone()))
        .unwrap_or_else(|| (1, TranscriptViewPosition(u64::MAX), projection_id.0.clone()))
}

fn resource_sort_key(resource_ids: &[ResourceId], resource_id: &ResourceId) -> (u8, usize, String) {
    resource_ids
        .iter()
        .position(|resident_id| resident_id == resource_id)
        .map(|index| (0, index, resource_id.0.clone()))
        .unwrap_or_else(|| (1, usize::MAX, resource_id.0.clone()))
}

fn fallback_sort_key(
    fallback: &ResidentFallbackRecord,
    view_records: &[TranscriptViewRecord],
    resource_ids: &[ResourceId],
) -> (u8, u64, String) {
    match &fallback.target {
        ResidentFallbackTarget::ProjectionRecord(projection_id) => view_records
            .iter()
            .find(|record| record.projection_id == *projection_id)
            .map(|record| (0, record.position.0, fallback.id.0.clone()))
            .unwrap_or_else(|| (2, u64::MAX, fallback.id.0.clone())),
        ResidentFallbackTarget::Resource(resource_id)
        | ResidentFallbackTarget::ResourceRange { resource_id, .. } => resource_ids
            .iter()
            .position(|resident_id| resident_id == resource_id)
            .map(|index| (1, index as u64, fallback.id.0.clone()))
            .unwrap_or_else(|| (2, u64::MAX, fallback.id.0.clone())),
    }
}

fn presentation_record_sort_key(
    record: &ResidentPresentationRecord,
    view_records: &[TranscriptViewRecord],
    resource_ids: &[ResourceId],
) -> (u8, u64, String) {
    if let Some(projection_id) = &record.provenance.projection_id {
        return view_records
            .iter()
            .find(|view_record| view_record.projection_id == *projection_id)
            .map(|view_record| (0, view_record.position.0, record.id.0.clone()))
            .unwrap_or_else(|| (2, u64::MAX, record.id.0.clone()));
    }

    if let Some(resource_id) = presentation_record_resource_id(record) {
        return resource_ids
            .iter()
            .position(|resident_id| resident_id == &resource_id)
            .map(|index| (1, index as u64, record.id.0.clone()))
            .unwrap_or_else(|| (2, u64::MAX, record.id.0.clone()));
    }

    (2, u64::MAX, record.id.0.clone())
}

fn sort_presentation_records(
    records: &mut [ResidentPresentationRecord],
    view_records: &[TranscriptViewRecord],
    resource_ids: &[ResourceId],
) {
    records.sort_by(|left, right| {
        presentation_record_sort_key(left, view_records, resource_ids).cmp(
            &presentation_record_sort_key(right, view_records, resource_ids),
        )
    });
}

fn resource_slice_bytes(slices: &[ResidentResourceSlice]) -> usize {
    slices.iter().map(|slice| slice.bytes.len()).sum()
}

fn projection_estimated_bytes(records: &[ProjectionRecord]) -> usize {
    records
        .iter()
        .map(|record| match &record.payload {
            ProjectionPayload::Text { text } => text.len(),
            ProjectionPayload::ResourceReference { label, .. } => {
                64 + label.as_deref().map(str::len).unwrap_or(0)
            }
        })
        .sum()
}

fn presentation_estimated_bytes(presentation: &ResidentPresentationSnapshot) -> usize {
    presentation
        .records
        .iter()
        .map(|record| record.estimated_bytes)
        .sum()
}

fn fallback_target_resource_id(target: &ResidentFallbackTarget) -> Option<ResourceId> {
    match target {
        ResidentFallbackTarget::Resource(resource_id) => Some(resource_id.clone()),
        ResidentFallbackTarget::ResourceRange { resource_id, .. } => Some(resource_id.clone()),
        ResidentFallbackTarget::ProjectionRecord(_) => None,
    }
}

fn presentation_record_resource_id(record: &ResidentPresentationRecord) -> Option<ResourceId> {
    match &record.kind {
        ResidentPresentationRecordKind::ResourceReference { resource_id, .. } => {
            Some(resource_id.clone())
        }
        ResidentPresentationRecordKind::LocalUiFallback { target, .. } => {
            fallback_target_resource_id(target)
        }
        _ => None,
    }
}

fn presentation_record_matches_fallback_target(
    record: &ResidentPresentationRecord,
    target: &ResidentFallbackTarget,
) -> bool {
    match target {
        ResidentFallbackTarget::ProjectionRecord(projection_id) => {
            record.provenance.projection_id.as_ref() == Some(projection_id)
        }
        ResidentFallbackTarget::Resource(resource_id)
        | ResidentFallbackTarget::ResourceRange { resource_id, .. } => {
            presentation_record_resource_id(record).as_ref() == Some(resource_id)
        }
    }
}

fn syndic_source_for_record(record: &ResidentPresentationRecord) -> Option<SyndicSourceProvenance> {
    match &record.provenance.source {
        ResidentRecordSource::Syndic(source) | ResidentRecordSource::LocalUiForSyndic(source) => {
            Some(source.clone())
        }
        ResidentRecordSource::LocalUi => None,
    }
}

fn fallback_reason_for_rejection(
    reason: &TranscriptProviderRejectionReason,
) -> Option<LocalPresentationReason> {
    match reason {
        TranscriptProviderRejectionReason::BudgetExceeded => {
            Some(LocalPresentationReason::BudgetRejected)
        }
        TranscriptProviderRejectionReason::PolicyDenied => {
            Some(LocalPresentationReason::PolicyDenied)
        }
        TranscriptProviderRejectionReason::UnsupportedResourceKind => {
            Some(LocalPresentationReason::Unsupported)
        }
        TranscriptProviderRejectionReason::ProjectionStale
        | TranscriptProviderRejectionReason::ProjectionIncomplete => {
            Some(LocalPresentationReason::PendingCoherentData)
        }
        TranscriptProviderRejectionReason::MissingResource
        | TranscriptProviderRejectionReason::RangeOutOfBounds
        | TranscriptProviderRejectionReason::InvalidRequest => {
            Some(LocalPresentationReason::ResourceUnavailable)
        }
        TranscriptProviderRejectionReason::MissingView
        | TranscriptProviderRejectionReason::MissingCursor
        | TranscriptProviderRejectionReason::MissingProjectionRecord => None,
    }
}

fn rejection_targets_resource(
    rejection: &TranscriptProviderRejection,
    resource_id: &ResourceId,
) -> bool {
    match &rejection.target {
        TranscriptProviderTarget::Resource(target_resource_id) => target_resource_id == resource_id,
        TranscriptProviderTarget::ResourceRange {
            resource_id: target_resource_id,
            ..
        } => target_resource_id == resource_id,
        _ => false,
    }
}

fn rejection_resource_id(rejection: &TranscriptProviderRejection) -> Option<ResourceId> {
    match &rejection.target {
        TranscriptProviderTarget::Resource(resource_id) => Some(resource_id.clone()),
        TranscriptProviderTarget::ResourceRange { resource_id, .. } => Some(resource_id.clone()),
        _ => None,
    }
}

fn presentation_record_position(
    record: &ResidentPresentationRecord,
) -> Option<TranscriptViewPosition> {
    match &record.provenance.source {
        ResidentRecordSource::Syndic(source) | ResidentRecordSource::LocalUiForSyndic(source) => {
            source.position
        }
        ResidentRecordSource::LocalUi => None,
    }
}

fn presentation_record_for_projection(
    view_record: &TranscriptViewRecord,
    projection_record: &ProjectionRecord,
    presentation_revision: u64,
) -> ResidentPresentationRecord {
    let (kind, estimated_bytes) = match &projection_record.payload {
        ProjectionPayload::Text { text } => (
            ResidentPresentationRecordKind::TextChunk {
                narrative_kind: view_record.narrative_kind.clone(),
                text: text.clone(),
            },
            text.len(),
        ),
        ProjectionPayload::ResourceReference {
            resource_id,
            resource_kind,
            label,
        } => (
            ResidentPresentationRecordKind::ResourceReference {
                resource_id: resource_id.clone(),
                resource_kind: resource_kind.clone(),
                label: label.clone(),
            },
            64 + label.as_deref().map(str::len).unwrap_or(0),
        ),
    };

    ResidentPresentationRecord {
        id: ResidentPresentationRecordId(format!(
            "view:{}:projection:{}",
            view_record.id.0, projection_record.id.0
        )),
        kind,
        provenance: ResidentRecordProvenance {
            source: ResidentRecordSource::Syndic(projection_record.provenance.clone()),
            projection_id: Some(projection_record.id.clone()),
            projection_revision: Some(projection_record.revision),
            presentation_revision,
            copy_source_range: projection_record.provenance.copy_source_range.clone(),
        },
        estimated_bytes,
    }
}

fn presentation_record_for_fallback(
    fallback: &ResidentFallbackRecord,
    presentation_revision: u64,
) -> ResidentPresentationRecord {
    let mut provenance = fallback.provenance.clone();
    provenance.presentation_revision = presentation_revision;

    ResidentPresentationRecord {
        id: fallback.id.clone(),
        kind: ResidentPresentationRecordKind::LocalUiFallback {
            reason: fallback.reason.clone(),
            target: fallback.target.clone(),
        },
        provenance,
        estimated_bytes: fallback.estimated_bytes,
    }
}

pub(crate) type ResidentPresentationSnapshot = ResidentTranscriptSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentCoreSnapshot {
    pub(crate) resident: ResidentSyndicDataSnapshot,
    pub(crate) presentation: ResidentPresentationSnapshot,
    pub(crate) demand_facts: DemandFactSinkSnapshot,
    pub(crate) provider_requests: ProviderRequestBookSnapshot,
    pub(crate) policy: ResidentTranscriptPolicy,
    pub(crate) generation: ResidentGeneration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentFallbackRecord {
    pub(crate) id: ResidentPresentationRecordId,
    pub(crate) reason: LocalPresentationReason,
    pub(crate) target: ResidentFallbackTarget,
    pub(crate) provenance: ResidentRecordProvenance,
    pub(crate) provider_rejection_reason: Option<TranscriptProviderRejectionReason>,
    pub(crate) estimated_bytes: usize,
    pub(crate) rejected_bytes: Option<usize>,
    pub(crate) limit_bytes: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentReleaseDecision {
    pub(crate) generation: ResidentGeneration,
    pub(crate) reason: ResidentReleaseReason,
    pub(crate) target: ResidentReleaseTarget,
    pub(crate) released_presentation_record_count: usize,
    pub(crate) preserved_presentation_record_count: usize,
    pub(crate) released_view_record_count: usize,
    pub(crate) released_projection_record_count: usize,
    pub(crate) released_resource_metadata_count: usize,
    pub(crate) released_resource_slice_count: usize,
    pub(crate) released_fallback_record_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentReleaseReason {
    ObsoleteResidentRange,
    ProviderInvalidation,
    StaleMeasurement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentReleaseTarget {
    PresentationRange(Range<usize>),
    ProviderRevision {
        previous: Option<ProviderRevision>,
        current: ProviderRevision,
    },
    PresentationRevision {
        observed: u64,
        current: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentSyndicDataSnapshot {
    pub(crate) view_id: Option<TranscriptViewId>,
    pub(crate) provider_revision: Option<ProviderRevision>,
    pub(crate) history_state: Option<TranscriptProviderHistoryState>,
    pub(crate) view_records: Vec<TranscriptViewRecord>,
    pub(crate) projection_records: Vec<ProjectionRecord>,
    pub(crate) projection_rejections: Vec<TranscriptProviderRejection>,
    pub(crate) resource_metadata: Vec<ResourceMetadata>,
    pub(crate) resource_slices: Vec<ResidentResourceSlice>,
    pub(crate) resource_rejections: Vec<TranscriptProviderRejection>,
    pub(crate) fallback_records: Vec<ResidentFallbackRecord>,
    pub(crate) previous_cursor: Option<TranscriptCursor>,
    pub(crate) next_cursor: Option<TranscriptCursor>,
    pub(crate) at_start: bool,
    pub(crate) at_end: bool,
    pub(crate) view_record_count: usize,
    pub(crate) projection_record_count: usize,
    pub(crate) projection_rejection_count: usize,
    pub(crate) resource_metadata_count: usize,
    pub(crate) resource_slice_count: usize,
    pub(crate) resource_rejection_count: usize,
    pub(crate) fallback_record_count: usize,
    pub(crate) budget_rejection_count: usize,
    pub(crate) estimated_resident_bytes: usize,
    pub(crate) projection_bytes: usize,
    pub(crate) presentation_bytes: usize,
    pub(crate) resource_bytes: usize,
    pub(crate) resource_slice_bytes: usize,
    pub(crate) decoded_or_uploaded_media_bytes: usize,
    pub(crate) geometry_bytes: usize,
    pub(crate) pin_bytes: usize,
    pub(crate) active_pin_count: usize,
    pub(crate) active_resource_pins: Vec<ResourceId>,
    pub(crate) active_selection_pins: Vec<ResidentPresentationRecordId>,
    pub(crate) active_quote_pins: Vec<ResidentPresentationRecordId>,
    pub(crate) active_menu_pins: Vec<ResidentPresentationRecordId>,
    pub(crate) active_media_pins: Vec<ResidentPresentationRecordId>,
    pub(crate) current_anchor_record_id: Option<ResidentPresentationRecordId>,
    pub(crate) current_anchor_position: Option<TranscriptViewPosition>,
    pub(crate) visible_range: Option<Range<usize>>,
    pub(crate) overscan_range: Option<Range<usize>>,
    pub(crate) obsolete_ranges: Vec<Range<usize>>,
    pub(crate) release_decisions: Vec<ResidentReleaseDecision>,
    pub(crate) release_decision_count: usize,
}

impl Default for ResidentSyndicDataSnapshot {
    fn default() -> Self {
        Self {
            view_id: None,
            provider_revision: None,
            history_state: None,
            view_records: Vec::new(),
            projection_records: Vec::new(),
            projection_rejections: Vec::new(),
            resource_metadata: Vec::new(),
            resource_slices: Vec::new(),
            resource_rejections: Vec::new(),
            fallback_records: Vec::new(),
            previous_cursor: None,
            next_cursor: None,
            at_start: false,
            at_end: false,
            view_record_count: 0,
            projection_record_count: 0,
            projection_rejection_count: 0,
            resource_metadata_count: 0,
            resource_slice_count: 0,
            resource_rejection_count: 0,
            fallback_record_count: 0,
            budget_rejection_count: 0,
            estimated_resident_bytes: 0,
            projection_bytes: 0,
            presentation_bytes: 0,
            resource_bytes: 0,
            resource_slice_bytes: 0,
            decoded_or_uploaded_media_bytes: 0,
            geometry_bytes: 0,
            pin_bytes: 0,
            active_pin_count: 0,
            active_resource_pins: Vec::new(),
            active_selection_pins: Vec::new(),
            active_quote_pins: Vec::new(),
            active_menu_pins: Vec::new(),
            active_media_pins: Vec::new(),
            current_anchor_record_id: None,
            current_anchor_position: None,
            visible_range: None,
            overscan_range: None,
            obsolete_ranges: Vec::new(),
            release_decisions: Vec::new(),
            release_decision_count: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptPolicy {
    pub(crate) view_page_limit: usize,
    pub(crate) max_resident_view_records: usize,
    pub(crate) max_resident_projection_records: usize,
    pub(crate) max_presentation_records: usize,
    pub(crate) max_resource_bytes: usize,
    pub(crate) max_resource_slice_bytes: usize,
    pub(crate) max_decoded_or_uploaded_media_bytes: usize,
    pub(crate) max_pending_provider_requests: usize,
    pub(crate) demand_fact_limit: usize,
}

impl Default for ResidentTranscriptPolicy {
    fn default() -> Self {
        Self {
            view_page_limit: 64,
            max_resident_view_records: 512,
            max_resident_projection_records: 1024,
            max_presentation_records: 1024,
            max_resource_bytes: 8 * 1024 * 1024,
            max_resource_slice_bytes: 1024 * 1024,
            max_decoded_or_uploaded_media_bytes: 32 * 1024 * 1024,
            max_pending_provider_requests: 32,
            demand_fact_limit: 128,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct ResidentGeneration(pub(crate) u64);

#[derive(Clone, Debug)]
pub(crate) struct ProviderRequestBook {
    next_request_id: u64,
    pending: Vec<ProviderRequestRecord>,
    completed_count: usize,
    stale_result_count: usize,
    rejected_result_count: usize,
    error_count: usize,
}

impl Default for ProviderRequestBook {
    fn default() -> Self {
        Self {
            next_request_id: 1,
            pending: Vec::new(),
            completed_count: 0,
            stale_result_count: 0,
            rejected_result_count: 0,
            error_count: 0,
        }
    }
}

impl ProviderRequestBook {
    pub(crate) fn reserve(
        &mut self,
        kind: TranscriptProviderRequestKind,
        reason: ProviderRequestReason,
        generation: ResidentGeneration,
    ) -> TranscriptProviderRequest {
        let id = ProviderRequestId(self.next_request_id);
        self.next_request_id = self.next_request_id.saturating_add(1);
        let request = TranscriptProviderRequest { id, kind };
        self.pending.push(ProviderRequestRecord {
            request: request.clone(),
            reason,
            generation,
        });
        request
    }

    pub(crate) fn finish(
        &mut self,
        id: ProviderRequestId,
        outcome: ProviderRequestOutcome,
    ) -> Option<ProviderRequestRecord> {
        let index = self
            .pending
            .iter()
            .position(|record| record.request.id == id)?;
        let record = self.pending.remove(index);
        match outcome {
            ProviderRequestOutcome::Admitted => {
                self.completed_count = self.completed_count.saturating_add(1);
            }
            ProviderRequestOutcome::Stale => {
                self.stale_result_count = self.stale_result_count.saturating_add(1);
            }
            ProviderRequestOutcome::Rejected => {
                self.rejected_result_count = self.rejected_result_count.saturating_add(1);
            }
            ProviderRequestOutcome::Error => {
                self.error_count = self.error_count.saturating_add(1);
            }
        }
        Some(record)
    }

    pub(crate) fn snapshot(&self) -> ProviderRequestBookSnapshot {
        ProviderRequestBookSnapshot {
            pending_count: self.pending.len(),
            completed_count: self.completed_count,
            stale_result_count: self.stale_result_count,
            rejected_result_count: self.rejected_result_count,
            error_count: self.error_count,
            next_request_id: ProviderRequestId(self.next_request_id),
        }
    }
}
