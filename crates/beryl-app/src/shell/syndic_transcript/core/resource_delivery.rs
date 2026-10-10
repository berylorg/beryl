use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceTranscriptDemand {
    pub(crate) activation_revision: u64,
    pub(crate) presentation_revision: u64,
    pub(crate) view_id: TranscriptViewId,
    pub(crate) revision: ProviderRevision,
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) provenance: SyndicSourceProvenance,
    pub(crate) resource_id: ResourceId,
    pub(crate) kind: ResourceKind,
    pub(crate) metadata: Option<ResourceMetadata>,
    pub(crate) range: Option<Range<u64>>,
    pub(crate) retained_records: Vec<ResidentPresentationRecordId>,
    pub(crate) pins: ResourceDemandPins,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResourceDemandPins {
    selection: Vec<ResidentPresentationRecordId>,
    quote: Vec<ResidentPresentationRecordId>,
    menu: Vec<ResidentPresentationRecordId>,
    media: Vec<ResidentPresentationRecordId>,
    resources: Vec<ResourceId>,
}

impl ResidentTranscriptCore {
    pub(crate) fn resource_demand(&self) -> Option<ResourceTranscriptDemand> {
        self.resource_demands().into_iter().next()
    }

    pub(crate) fn resource_demands(&self) -> Vec<ResourceTranscriptDemand> {
        let mut demands = Vec::new();
        let protected = self.protected_presentation_record_ids();
        let Some(view_id) = self.resident.view_id.as_ref() else {
            return demands;
        };
        let Some(revision) = self.resident.provider_revision else {
            return demands;
        };
        for record in self.presentation.records.iter() {
            if !protected.contains(&record.id) {
                continue;
            }
            let ResidentPresentationRecordKind::ResourceReference {
                resource_id,
                resource_kind,
                ..
            } = &record.kind
            else {
                continue;
            };
            if self.resource_has_terminal_fallback(resource_id) {
                continue;
            }
            let ResidentRecordSource::Syndic(provenance) = &record.provenance.source else {
                continue;
            };
            let metadata = self
                .resident
                .resource_metadata
                .iter()
                .find(|metadata| &metadata.resource_id == resource_id)
                .cloned();
            let range = match &metadata {
                None => None,
                Some(metadata) => {
                    let Some(preview) = resource_preview_range(metadata) else {
                        continue;
                    };
                    let Some(missing) = self.missing_resource_interval(metadata, preview) else {
                        continue;
                    };
                    Some(missing)
                }
            };
            demands.push(ResourceTranscriptDemand {
                activation_revision: self.presentation.activation_revision,
                presentation_revision: self.presentation.presentation_revision,
                view_id: view_id.clone(),
                revision,
                record_id: record.id.clone(),
                provenance: provenance.clone(),
                resource_id: resource_id.clone(),
                kind: resource_kind.clone(),
                metadata,
                range,
                retained_records: protected.clone(),
                pins: ResourceDemandPins {
                    selection: self.resident.active_selection_pins.clone(),
                    quote: self.resident.active_quote_pins.clone(),
                    menu: self.resident.active_menu_pins.clone(),
                    media: self.resident.active_media_pins.clone(),
                    resources: self.resident.active_resource_pins.clone(),
                },
            });
        }
        demands
    }

    fn missing_resource_interval(
        &self,
        metadata: &ResourceMetadata,
        range: Range<u64>,
    ) -> Option<Range<u64>> {
        let mut start = range.start;
        for slice in self.resident.resource_slices.iter().filter(|slice| {
            slice.resource_id == metadata.resource_id
                && slice.revision == metadata.revision
                && slice.kind == metadata.kind
        }) {
            if slice.range.end <= start {
                continue;
            }
            if slice.range.start > start {
                return Some(start..slice.range.start.min(range.end));
            }
            start = slice.range.end;
            if start >= range.end {
                return None;
            }
        }
        (start < range.end).then_some(start..range.end)
    }

    pub(crate) fn reconcile_resource_demand(&mut self) {
        let protected = self.protected_presentation_record_ids();
        let ids: Vec<_> = self
            .presentation
            .records
            .iter()
            .filter(|record| protected.contains(&record.id))
            .filter_map(presentation_record_resource_id)
            .collect();
        if self
            .resident
            .resource_metadata
            .iter()
            .any(|metadata| !ids.contains(&metadata.resource_id))
        {
            self.shared_presentation.get_mut().take();
            Arc::make_mut(&mut self.resident.resource_metadata)
                .retain(|metadata| ids.contains(&metadata.resource_id));
        }
        let slice_ids: Vec<_> = self
            .presentation
            .records
            .iter()
            .filter(|record| protected.contains(&record.id))
            .filter_map(|record| match &record.kind {
                ResidentPresentationRecordKind::ResourceReference { resource_id, .. } => {
                    Some(resource_id.clone())
                }
                _ => None,
            })
            .chain(self.resident.active_resource_pins.iter().cloned())
            .collect();
        if self
            .resident
            .resource_slices
            .iter()
            .any(|slice| !slice_ids.contains(&slice.resource_id))
        {
            self.shared_presentation.get_mut().take();
            Arc::make_mut(&mut self.resident.resource_slices)
                .retain(|slice| slice_ids.contains(&slice.resource_id));
        }
        self.resident
            .resource_rejections
            .retain(|rejection| match &rejection.target {
                TranscriptProviderTarget::Resource(id)
                | TranscriptProviderTarget::ResourceRange {
                    resource_id: id, ..
                } => ids.contains(id),
                _ => false,
            });
        let obsolete: Vec<_> = self
            .provider_requests
            .pending
            .iter()
            .filter_map(|pending| {
                let id = match &pending.request.kind {
                    TranscriptProviderRequestKind::ReadResourceMetadata(request) => {
                        &request.resource_id
                    }
                    TranscriptProviderRequestKind::ReadResourceRange(request) => {
                        &request.resource_id
                    }
                    _ => return None,
                };
                (!ids.contains(id)).then_some(pending.request.id)
            })
            .collect();
        for id in obsolete {
            self.provider_requests
                .finish(id, ProviderRequestOutcome::Stale);
        }
        self.refresh_budget_accounting();
    }

    pub(crate) fn prepare_resource_response(
        &self,
        demand: &ResourceTranscriptDemand,
        kind: TranscriptProviderResponseKind,
    ) -> Option<Self> {
        if !self.resource_demands().contains(demand) {
            return None;
        }
        let mut candidate = self.clone();
        let request_kind = match &demand.metadata {
            None => TranscriptProviderRequestKind::ReadResourceMetadata(ResourceMetadataRequest {
                resource_id: demand.resource_id.clone(),
                observed_revision: None,
            }),
            Some(metadata) => {
                TranscriptProviderRequestKind::ReadResourceRange(ResourceRangeRequest {
                    resource_id: demand.resource_id.clone(),
                    range: demand.range.clone()?,
                    observed_revision: Some(metadata.revision),
                })
            }
        };
        let request =
            candidate.reserve_provider_request(request_kind, ProviderRequestReason::ResourceRange);
        let response = TranscriptProviderResponse {
            request_id: request.id,
            kind,
        };
        if !candidate.resource_response_matches_request(&response) {
            return None;
        }
        if let TranscriptProviderResponseKind::ResourceRange(range) = &response.kind {
            let bytes = candidate
                .resident
                .resource_slices
                .iter()
                .map(|slice| slice.bytes.len())
                .sum::<usize>();
            if range.bytes.len() > candidate.policy.max_resource_slice_bytes
                || bytes
                    .checked_add(range.bytes.len())
                    .is_none_or(|bytes| bytes > candidate.policy.max_resource_bytes)
            {
                candidate.upsert_resource_fallback_for_demand(
                    &demand.resource_id,
                    ResidentFallbackTarget::Resource(demand.resource_id.clone()),
                    LocalPresentationReason::BudgetRejected,
                    Some(range.bytes.len()),
                    Some(candidate.policy.max_resource_bytes),
                );
                candidate.bump_resource_presentation();
                candidate.reconcile_resource_demand();
                candidate.finish_provider_request(request.id, ProviderRequestOutcome::Rejected);
                return Some(candidate);
            }
        }
        let effect = candidate.handle_provider_response(response);
        if matches!(
            effect,
            ResidentProviderResponseEffect::Ignored
                | ResidentProviderResponseEffect::UnknownRequest
        ) {
            return None;
        }
        candidate.bump_resource_presentation();
        candidate.reconcile_resource_demand();
        Some(candidate)
    }

    fn bump_resource_presentation(&mut self) {
        self.shared_presentation.get_mut().take();
        self.presentation.presentation_revision =
            self.presentation.presentation_revision.saturating_add(1);
        self.update_remaining_presentation_revision();
        self.refresh_budget_accounting();
    }

    pub(super) fn resource_response_matches_request(
        &self,
        response: &TranscriptProviderResponse,
    ) -> bool {
        let Some(pending) = self
            .provider_requests
            .pending
            .iter()
            .find(|pending| pending.request.id == response.request_id)
        else {
            return false;
        };
        match (&pending.request.kind, &response.kind) {
            (TranscriptProviderRequestKind::ReadResourceMetadata(request), TranscriptProviderResponseKind::ResourceMetadata(metadata)) => {
                request.resource_id == metadata.resource_id
                    && request.observed_revision.is_none_or(|revision| revision == metadata.revision)
                    && metadata.preview_range.as_ref().is_none_or(|range| range.start <= range.end && range.end <= metadata.byte_len)
                    && self.presentation.records.iter().any(|record| matches!(&record.kind,
                        ResidentPresentationRecordKind::ResourceReference { resource_id, resource_kind, .. }
                        if resource_id == &metadata.resource_id && resource_kind == &metadata.kind))
            }
            (TranscriptProviderRequestKind::ReadResourceRange(request), TranscriptProviderResponseKind::ResourceRange(range)) => {
                self.resident.resource_metadata.iter().any(|metadata| {
                    request.resource_id == range.resource_id && range.resource_id == metadata.resource_id
                        && request.observed_revision == Some(metadata.revision) && range.revision == metadata.revision
                        && range.kind == metadata.kind && range.range.start == request.range.start
                        && range.range.start < range.range.end && range.range.end <= request.range.end
                        && range.range.end <= metadata.byte_len
                        && range.range.end - range.range.start == range.bytes.len() as u64
                        && range.complete == (range.range.end == request.range.end)
                })
            }
            (TranscriptProviderRequestKind::ReadResourceMetadata(request), TranscriptProviderResponseKind::Rejected(rejection)) => {
                rejection.target == TranscriptProviderTarget::Resource(request.resource_id.clone())
                    || rejection.target == TranscriptProviderTarget::Request(response.request_id)
            }
            (TranscriptProviderRequestKind::ReadResourceRange(request), TranscriptProviderResponseKind::Rejected(rejection)) => {
                rejection.target == TranscriptProviderTarget::ResourceRange { resource_id: request.resource_id.clone(), range: request.range.clone() }
                    || rejection.target == TranscriptProviderTarget::Resource(request.resource_id.clone())
                    || rejection.target == TranscriptProviderTarget::Request(response.request_id)
            }
            (TranscriptProviderRequestKind::ReadResourceMetadata(request), TranscriptProviderResponseKind::Stale(stale)) => {
                stale.target == TranscriptProviderTarget::Resource(request.resource_id.clone())
            }
            (TranscriptProviderRequestKind::ReadResourceRange(request), TranscriptProviderResponseKind::Stale(stale)) => {
                stale.target == TranscriptProviderTarget::ResourceRange { resource_id: request.resource_id.clone(), range: request.range.clone() }
            }
            (TranscriptProviderRequestKind::ReadResourceMetadata(_) | TranscriptProviderRequestKind::ReadResourceRange(_), _) => false,
            (_, TranscriptProviderResponseKind::ResourceMetadata(_) | TranscriptProviderResponseKind::ResourceRange(_)) => false,
            _ => true,
        }
    }
}

pub(crate) fn resource_preview_range(metadata: &ResourceMetadata) -> Option<Range<u64>> {
    let range = metadata
        .preview_range
        .clone()
        .unwrap_or(0..metadata.byte_len);
    if range.start >= range.end || range.end > metadata.byte_len {
        return None;
    }
    Some(range.start..range.end.min(range.start.saturating_add(1_048_576)))
}
