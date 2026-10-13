use super::*;
use syndic_storage::narrative::{NarrativePreparationErrorKind, PreparedNarrative};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NarrativeTranscriptDemand {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) activation_revision: u64,
    pub(crate) projection_id: ProjectionRecordId,
    pub(crate) projection_revision: ProviderRevision,
    pub(crate) source: Arc<syndic_storage::ProjectionPayload>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NarrativePreparationResponse {
    Ready(Arc<PreparedNarrative>),
    Unavailable {
        source: Arc<syndic_storage::ProjectionPayload>,
        reason: NarrativePreparationErrorKind,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NarrativeOwner {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) activation_revision: u64,
    pub(crate) provenance: ResidentRecordProvenance,
    pub(crate) prepared: Arc<PreparedNarrative>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiteralNarrativeOwner {
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) activation_revision: u64,
    pub(crate) provenance: ResidentRecordProvenance,
    pub(crate) source: Arc<str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NarrativeLayoutOwner {
    Prepared(NarrativeOwner),
    Literal(LiteralNarrativeOwner),
}

#[derive(Clone, Debug)]
pub(super) struct NarrativeReservation {
    demand: NarrativeTranscriptDemand,
    bytes: usize,
}

#[derive(Clone, Debug)]
pub(super) struct NarrativeLayout {
    owner: NarrativeLayoutOwner,
    bytes: usize,
}

pub(crate) fn narrative_source_bytes(source: &syndic_storage::ProjectionPayload) -> usize {
    let source_bytes = match source {
        syndic_storage::ProjectionPayload::InlineMarkdown { source, .. } => source.len(),
        _ => 0,
    };
    source_bytes
        .next_multiple_of(std::mem::align_of::<usize>())
        .saturating_add(std::mem::size_of::<syndic_storage::ProjectionPayload>())
        .saturating_add(2 * std::mem::size_of::<usize>())
}

fn same_prepared_source(
    prepared: &PreparedNarrative,
    source: &syndic_storage::ProjectionPayload,
) -> bool {
    matches!(source, syndic_storage::ProjectionPayload::InlineMarkdown {
        block_id, block_kind, span_ordinal, source_range, source,
    } if *block_id == prepared.block_id() && *block_kind == prepared.block_kind()
        && *span_ordinal == prepared.span_ordinal() && *source_range == prepared.source_range()
        && source.as_ref() == prepared.source())
}

impl ResidentTranscriptCore {
    pub(crate) fn narrative_demand(&self) -> Option<NarrativeTranscriptDemand> {
        let view_id = self.resident.view_id.clone()?;
        let realized = self.presentation.realized_range.as_ref()?;
        let visible = self
            .presentation
            .visible_range
            .clone()
            .unwrap_or(realized.start..realized.start);
        for index in visible.chain(realized.clone()) {
            if !realized.contains(&index) {
                continue;
            }
            let Some(current) = self.presentation.records.get(index) else {
                continue;
            };
            for projection in &self.resident.projection_records {
                let ProjectionPayload::NarrativePending { source } = &projection.payload else {
                    continue;
                };
                let Some(view) = self
                    .resident
                    .view_records
                    .iter()
                    .find(|view| view.projection_id == projection.id)
                else {
                    continue;
                };
                let record = presentation_record_for_projection(
                    view,
                    projection,
                    self.presentation.presentation_revision,
                );
                if current.id == record.id
                    && !self
                        .narrative_preparations
                        .iter()
                        .any(|pending| pending.demand.record_id == record.id)
                {
                    return Some(NarrativeTranscriptDemand {
                        view_id,
                        record_id: record.id,
                        activation_revision: self.presentation.activation_revision,
                        projection_id: projection.id.clone(),
                        projection_revision: projection.revision,
                        source: source.clone(),
                    });
                }
            }
        }
        None
    }

    pub(crate) fn narrative_demand_current(&self, demand: &NarrativeTranscriptDemand) -> bool {
        self.resident.view_id.as_ref() == Some(&demand.view_id)
            && self.presentation.activation_revision == demand.activation_revision
            && self.presentation.records.iter().enumerate().any(|(index, record)| record.id == demand.record_id
                && self.presentation.realized_range.as_ref().is_some_and(|range| range.contains(&index)))
            && self.resident.projection_records.iter().any(|record| {
                record.id == demand.projection_id && record.revision == demand.projection_revision
                    && matches!(&record.payload, ProjectionPayload::NarrativePending { source } if source == &demand.source)
            })
    }

    pub(crate) fn reserve_narrative_preparation(
        &mut self,
        demand: &NarrativeTranscriptDemand,
    ) -> Option<usize> {
        if !self.narrative_demand_current(demand)
            || self
                .narrative_preparations
                .iter()
                .any(|pending| pending.demand.record_id == demand.record_id)
            || self
                .narrative_preparations
                .len()
                .saturating_add(self.provider_requests.pending.len())
                >= self.policy.max_pending_provider_requests
        {
            return None;
        }
        let retained = self
            .retained_narratives
            .iter()
            .filter(|record| record.id == demand.record_id)
            .map(|record| record.estimated_bytes)
            .sum::<usize>();
        let used = narrative_source_bytes(&demand.source)
            .saturating_add(retained)
            .saturating_add(1024);
        let allowance = super::super::provider::TRANSCRIPT_RESPONSE_MAX_BYTES.saturating_sub(used);
        self.narrative_preparations.push(NarrativeReservation {
            demand: demand.clone(),
            bytes: allowance,
        });
        self.refresh_budget_accounting();
        Some(allowance)
    }

    pub(crate) fn finish_narrative_preparation(&mut self, demand: &NarrativeTranscriptDemand) {
        self.narrative_preparations
            .retain(|pending| pending.demand != *demand);
        self.refresh_budget_accounting();
    }

    pub(crate) fn cancel_narrative_preparations(&mut self) {
        self.narrative_preparations.clear();
        self.refresh_budget_accounting();
    }

    pub(crate) fn prepare_narrative_delivery(
        &self,
        demand: &NarrativeTranscriptDemand,
        response: NarrativePreparationResponse,
    ) -> Option<Self> {
        if !self.narrative_demand_current(demand) {
            return None;
        }
        let reservation = self
            .narrative_preparations
            .iter()
            .find(|pending| pending.demand == *demand)?;
        let payload = match response {
            NarrativePreparationResponse::Ready(prepared) => {
                if !same_prepared_source(&prepared, &demand.source)
                    || prepared
                        .retained_bytes()
                        .saturating_add(2 * std::mem::size_of::<usize>())
                        > reservation.bytes
                {
                    return None;
                }
                ProjectionPayload::NarrativeReady { prepared }
            }
            NarrativePreparationResponse::Unavailable { source, reason } => {
                if source != demand.source {
                    return None;
                }
                ProjectionPayload::NarrativeUnavailable { source, reason }
            }
        };
        let mut candidate = self.clone();
        candidate.previous_narrative_layout_bytes = self.narrative_layout_bytes();
        candidate.narrative_layouts.clear();
        candidate
            .narrative_preparations
            .retain(|pending| pending.demand != *demand);
        candidate
            .retained_narratives
            .retain(|record| record.id != demand.record_id);
        candidate
            .resident
            .projection_records
            .iter_mut()
            .find(|record| record.id == demand.projection_id)?
            .payload = payload;
        candidate.rebuild_presentation_records();
        Some(candidate)
    }

    pub(crate) fn inherit_narratives(&mut self, old: &Self) {
        self.previous_narrative_layout_bytes = old.narrative_layout_bytes();
        self.retained_narratives.clear();
        for record in &mut self.resident.projection_records {
            let ProjectionPayload::NarrativePending { source } = &record.payload else {
                continue;
            };
            if let Some(prepared) = old
                .resident
                .projection_records
                .iter()
                .find_map(|previous| match &previous.payload {
                    ProjectionPayload::NarrativeReady { prepared }
                        if same_prepared_source(prepared, source) =>
                    {
                        Some(prepared.clone())
                    }
                    _ => None,
                })
                .or_else(|| {
                    old.presentation
                        .records
                        .iter()
                        .find_map(|previous| match &previous.kind {
                            ResidentPresentationRecordKind::NarrativeReady { prepared }
                                if same_prepared_source(prepared, source) =>
                            {
                                Some(prepared.clone())
                            }
                            _ => None,
                        })
                })
            {
                record.payload = ProjectionPayload::NarrativeReady { prepared };
                continue;
            }
            if let Some((source, reason)) =
                old.resident.projection_records.iter().find_map(|previous| {
                    match &previous.payload {
                        ProjectionPayload::NarrativeUnavailable {
                            source: previous,
                            reason,
                        } if previous == source => Some((previous.clone(), *reason)),
                        _ => None,
                    }
                })
            {
                record.payload = ProjectionPayload::NarrativeUnavailable { source, reason };
                continue;
            }
            let Some(view) = self
                .resident
                .view_records
                .iter()
                .find(|view| view.projection_id == record.id)
            else {
                continue;
            };
            let id = presentation_record_for_projection(
                view,
                record,
                self.presentation.presentation_revision,
            )
            .id;
            if let Some(previous) = old.presentation.records.iter().find(|previous| {
                previous.id == id
                    && matches!(
                        previous.kind,
                        ResidentPresentationRecordKind::NarrativeReady { .. }
                    )
            }) {
                self.retained_narratives.push(previous.clone());
            }
        }
        self.rebuild_presentation_records();
    }

    pub(super) fn retain_narrative_presentation(&self, record: &mut ResidentPresentationRecord) {
        if !matches!(
            record.kind,
            ResidentPresentationRecordKind::NarrativePending { .. }
        ) {
            return;
        }
        if let Some(previous) = self
            .retained_narratives
            .iter()
            .find(|previous| previous.id == record.id)
        {
            let revision = record.provenance.presentation_revision;
            *record = previous.clone();
            record.provenance.presentation_revision = revision;
        }
    }

    pub(crate) fn narrative_current(&self, owner: &NarrativeOwner) -> bool {
        self.resident.view_id.as_ref() == Some(&owner.view_id)
            && self.presentation.activation_revision == owner.activation_revision
            && self.presentation.records.iter().any(|record| record.id == owner.record_id
                && record.provenance == owner.provenance
                && matches!(&record.kind, ResidentPresentationRecordKind::NarrativeReady { prepared } if prepared == &owner.prepared))
    }

    pub(crate) fn narrative_layout_allowance(&self, owner: &NarrativeOwner) -> usize {
        if !self.narrative_current(owner) {
            return 0;
        }
        let previous = self.narrative_layouts.iter().filter(|entry| matches!(&entry.owner, NarrativeLayoutOwner::Prepared(current) if current == owner)).map(|entry| entry.bytes).sum::<usize>();
        self.policy
            .max_resource_bytes
            .saturating_sub(self.resident.resource_bytes.saturating_sub(previous))
    }

    pub(crate) fn reserve_narrative_layout(
        &mut self,
        owner: &NarrativeOwner,
        bytes: usize,
    ) -> bool {
        if bytes != 0 && bytes > self.narrative_layout_allowance(owner) {
            return false;
        }
        self.narrative_layouts.retain(|entry| !matches!(&entry.owner, NarrativeLayoutOwner::Prepared(current) if current == owner));
        if bytes != 0 {
            self.narrative_layouts.push(NarrativeLayout {
                owner: NarrativeLayoutOwner::Prepared(owner.clone()),
                bytes,
            });
        }
        self.refresh_budget_accounting();
        true
    }

    pub(crate) fn literal_narrative_current(&self, owner: &LiteralNarrativeOwner) -> bool {
        self.presentation.activation_revision == owner.activation_revision
            && matches!(owner.provenance.source, ResidentRecordSource::Syndic(_) | ResidentRecordSource::LocalContext { .. })
            && self.presentation.records.iter().any(|record| record.id == owner.record_id
                && record.provenance == owner.provenance
                && matches!(&record.kind, ResidentPresentationRecordKind::TextChunk { text, .. } if text == &owner.source))
    }

    pub(crate) fn literal_narrative_layout_allowance(
        &self,
        owner: &LiteralNarrativeOwner,
    ) -> usize {
        if !self.literal_narrative_current(owner) {
            return 0;
        }
        let previous = self.narrative_layouts.iter().filter(|entry| matches!(&entry.owner, NarrativeLayoutOwner::Literal(current) if current == owner)).map(|entry| entry.bytes).sum::<usize>();
        self.policy
            .max_resource_bytes
            .saturating_sub(self.resident.resource_bytes.saturating_sub(previous))
    }

    pub(crate) fn reserve_literal_narrative_layout(
        &mut self,
        owner: &LiteralNarrativeOwner,
        bytes: usize,
    ) -> bool {
        if bytes != 0 && bytes > self.literal_narrative_layout_allowance(owner) {
            return false;
        }
        self.narrative_layouts.retain(|entry| !matches!(&entry.owner, NarrativeLayoutOwner::Literal(current) if current == owner));
        if bytes != 0 {
            self.narrative_layouts.push(NarrativeLayout {
                owner: NarrativeLayoutOwner::Literal(owner.clone()),
                bytes,
            });
        }
        self.refresh_budget_accounting();
        true
    }

    pub(super) fn narrative_reserved_bytes(&self) -> usize {
        self.narrative_preparations
            .iter()
            .map(|entry| entry.bytes)
            .sum()
    }

    pub(super) fn narrative_layout_bytes(&self) -> usize {
        self.narrative_layouts
            .iter()
            .map(|entry| entry.bytes)
            .sum::<usize>()
            .saturating_add(self.previous_narrative_layout_bytes)
    }

    pub(crate) fn release_previous_narrative_layout(&mut self) {
        self.previous_narrative_layout_bytes = 0;
        self.refresh_budget_accounting();
    }

    pub(crate) fn hold_previous_narrative_layout(&mut self, bytes: usize) {
        self.previous_narrative_layout_bytes = bytes;
        self.narrative_layouts.clear();
        self.refresh_budget_accounting();
    }

    pub(crate) fn carry_narrative_layout_from(&mut self, old: &Self) {
        self.previous_narrative_layout_bytes = old.narrative_layout_bytes();
        self.narrative_layouts.clear();
        self.refresh_budget_accounting();
    }

    pub(super) fn reconcile_narratives(&mut self) {
        let pending = std::mem::take(&mut self.narrative_preparations);
        self.narrative_preparations = pending
            .into_iter()
            .filter(|entry| self.narrative_demand_current(&entry.demand))
            .collect();
        let layouts = std::mem::take(&mut self.narrative_layouts);
        self.narrative_layouts = layouts
            .into_iter()
            .filter(|entry| match &entry.owner {
                NarrativeLayoutOwner::Prepared(owner) => self.narrative_current(owner),
                NarrativeLayoutOwner::Literal(owner) => self.literal_narrative_current(owner),
            })
            .collect();
        self.retained_narratives.retain(|entry| {
            self.presentation
                .records
                .iter()
                .any(|record| record.id == entry.id)
        });
    }
}
