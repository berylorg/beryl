use super::super::super::provider::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    resource_index as index,
};
use super::*;
#[path = "indexed/admission.rs"]
mod admission;
#[path = "indexed/table_window.rs"]
mod table_window;
use table_window::{admit_table_page, table_request_bounded};

#[derive(Clone, Default)]
pub(in super::super) struct IndexedResidency {
    entries: Vec<IndexedEntry>,
}

impl std::fmt::Debug for IndexedResidency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexedResidency")
            .field("entries", &self.entries.len())
            .field("encoded_bytes", &self.encoded_bytes())
            .finish()
    }
}

#[derive(Clone)]
pub(super) struct IndexedEntry {
    demand: ResourceTranscriptDemand,
    pub(super) inner: IndexedResourceDemand,
    pub(super) response: Option<Arc<IndexedResourceResponse>>,
    pages: Vec<Arc<IndexedResourceResponse>>,
    table_window: Vec<Arc<IndexedResourceResponse>>,
    table_retired_window: Vec<Arc<IndexedResourceResponse>>,
    table_staging: Vec<Arc<IndexedResourceResponse>>,
    layout_bytes: usize,
}

impl IndexedResidency {
    pub(in super::super) fn encoded_bytes(&self) -> usize {
        self.entries.iter().fold(0usize, |bytes, entry| {
            entry
                .pages
                .iter()
                .chain(entry.response.iter().filter(|response| {
                    !entry
                        .table_window
                        .iter()
                        .chain(entry.table_staging.iter())
                        .chain(entry.table_retired_window.iter())
                        .any(|page| Arc::ptr_eq(page, response))
                }))
                .chain(entry.table_window.iter())
                .chain(entry.table_staging.iter())
                .chain(entry.table_retired_window.iter())
                .fold(
                    bytes.saturating_add(entry.layout_bytes),
                    |bytes, response| bytes.saturating_add(response.encoded_bytes),
                )
        })
    }

    pub(super) fn demand_for(
        &self,
        view: &TranscriptViewId,
        revision: ProviderRevision,
        activation: u64,
        record: &ResidentPresentationRecordId,
        provenance: &SyndicSourceProvenance,
    ) -> Option<&IndexedEntry> {
        self.entries.iter().find(|entry| {
            entry.demand.view_id == *view
                && entry.demand.revision == revision
                && entry.demand.activation_revision == activation
                && entry.demand.record_id == *record
                && entry.demand.provenance == *provenance
        })
    }
}

impl ResidentTranscriptCore {
    pub(crate) fn refresh_pending_indexed_resource(
        &mut self,
        demand: &ResourceTranscriptDemand,
    ) -> bool {
        if !self.indexed_owner_current(demand) {
            return false;
        }
        let Some(entry) = self.indexed_resources.entries.iter_mut().find(|entry| {
            entry.demand == *demand
                && entry
                    .response
                    .as_ref()
                    .is_some_and(|response| matches!(response.value, IndexedResourceValue::Pending))
        }) else {
            return false;
        };
        entry.response = None;
        self.refresh_budget_accounting();
        true
    }

    pub(crate) fn set_indexed_resource_demand(
        &mut self,
        demand: &ResourceTranscriptDemand,
        inner: Option<IndexedResourceDemand>,
    ) -> bool {
        if !self.indexed_owner_current(demand) {
            return false;
        }
        if !self.resource_demands().contains(demand) && !self.code_panel_demands().contains(demand)
        {
            // An admitted inner response suppresses its read, but its exact original owner may
            // still replace or dispose that inner viewport without moving the outer viewport.
            let current = self
                .indexed_resources
                .entries
                .iter()
                .any(|entry| entry.demand == *demand);
            if !current {
                return false;
            }
        }
        if !matches!(demand.kind, ResourceKind::Code | ResourceKind::Table) {
            return false;
        }
        if inner.is_some()
            && self.indexed_resources.entries.iter().any(|entry| {
                entry.demand.resource_id == demand.resource_id
                    && !entry.table_retired_window.is_empty()
            })
        {
            return false;
        }
        if inner.is_some()
            && self.indexed_resources.entries.len() >= self.policy.max_pending_provider_requests
            && !self
                .indexed_resources
                .entries
                .iter()
                .any(|entry| entry.demand.resource_id == demand.resource_id)
        {
            return false;
        }
        let layout_bytes = self
            .indexed_resources
            .entries
            .iter()
            .find(|entry| {
                entry.demand.resource_id == demand.resource_id
                    && entry.demand.view_id == demand.view_id
                    && entry.demand.record_id == demand.record_id
                    && entry.demand.provenance == demand.provenance
                    && entry.demand.activation_revision == demand.activation_revision
                    && entry.demand.revision == demand.revision
            })
            .map_or(0, |entry| entry.layout_bytes);
        let pages = inner
            .as_ref()
            .and_then(|inner| {
                let entry = self.indexed_resources.entries.iter().find(|entry| {
                    entry.demand.resource_id == demand.resource_id
                        && entry.demand.view_id == demand.view_id
                        && entry.demand.record_id == demand.record_id
                        && entry.demand.kind == demand.kind
                        && entry.demand.provenance == demand.provenance
                        && entry.demand.activation_revision == demand.activation_revision
                        && entry.demand.revision == demand.revision
                })?;
                let response = entry.response.as_ref()?;
                let IndexedResourceValue::Range(range) = &response.value else {
                    return None;
                };
                let IndexedResourceQuery::Range {
                    request,
                    cursor: Some(cursor),
                    ..
                } = &inner.query
                else {
                    return None;
                };
                if !matches!(request, index::Request::Code { lines }
                    if lines.start <= lines.end && lines.end - lines.start <= 16)
                    || range.request_complete
                    || range.items.is_empty()
                    || range.cursor.fragment_offset != 0
                    || *cursor != range.cursor
                    || inner.revision != entry.inner.revision
                    || inner.root != response.root.as_ref().map(|root| root.digest)
                    || entry.pages.len() >= 15
                {
                    return None;
                }
                let mut pages = entry.pages.clone();
                pages.push(response.clone());
                Some(pages)
            })
            .unwrap_or_default();
        let retained_table = self
            .indexed_resources
            .entries
            .iter()
            .find(|entry| {
                demand.kind == ResourceKind::Table
                    && entry.demand.resource_id == demand.resource_id
                    && entry.demand.view_id == demand.view_id
                    && entry.demand.record_id == demand.record_id
                    && entry.demand.provenance == demand.provenance
                    && entry.demand.activation_revision == demand.activation_revision
                    && entry.demand.revision == demand.revision
                    && inner.as_ref().is_some_and(|next| {
                        next.revision == entry.inner.revision
                            && next.root
                                == entry
                                    .response
                                    .as_ref()
                                    .and_then(|response| {
                                        response.root.as_ref().map(|root| root.digest)
                                    })
                                    .or(entry.inner.root)
                    })
            })
            .map(|entry| {
                let staging = match (inner.as_ref().map(|inner| &inner.query),entry.table_staging.last().map(|page|&page.value)) {
                    (Some(IndexedResourceQuery::Range{request,cursor,..}),Some(IndexedResourceValue::Range(previous)))
                        if table_request_bounded(request) && match cursor {
                            Some(cursor)=>*cursor==previous.cursor && cursor.request==*request && !previous.request_complete,
                            None=>previous.request_complete && matches!((previous.cursor.request,*request),
                                (index::Request::Table{role:index::RowRole::Header,columns:prior,..},index::Request::Table{role:index::RowRole::Body,columns:next,..}) if prior==next),
                        } =>entry.table_staging.clone(),
                    _=>Vec::new(),
                };
                (entry.table_window.clone(), staging, entry.table_retired_window.clone())
            })
            .unwrap_or_default();
        self.indexed_resources
            .entries
            .retain(|entry| entry.demand.resource_id != demand.resource_id);
        if let Some(inner) = inner {
            let mut owner = demand.clone();
            owner.metadata = None;
            owner.range = None;
            owner.indexed = Some(inner.clone());
            self.indexed_resources.entries.push(IndexedEntry {
                demand: owner,
                inner,
                response: None,
                pages,
                table_window: retained_table.0,
                table_staging: retained_table.1,
                table_retired_window: retained_table.2,
                layout_bytes,
            });
        }
        self.refresh_budget_accounting();
        true
    }

    pub(crate) fn reserve_code_layout(
        &mut self,
        demand: &ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        if demand.kind != ResourceKind::Code {
            return false;
        }
        self.reserve_indexed_layout(demand, bytes)
    }

    pub(crate) fn reserve_indexed_layout(
        &mut self,
        demand: &ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        if !matches!(demand.kind, ResourceKind::Code | ResourceKind::Table)
            || !self.indexed_owner_current(demand)
        {
            return false;
        }
        let Some(position) = self
            .indexed_resources
            .entries
            .iter()
            .position(|entry| entry.demand == *demand)
        else {
            return false;
        };
        let previous = self.indexed_resources.entries[position].layout_bytes;
        let used = self
            .resident
            .resource_slice_bytes
            .saturating_add(self.indexed_resources.encoded_bytes())
            .saturating_sub(previous);
        if used.saturating_add(bytes) > self.policy.max_resource_bytes {
            return false;
        }
        self.indexed_resources.entries[position].layout_bytes = bytes;
        self.refresh_budget_accounting();
        true
    }

    pub(crate) fn can_reserve_code_layout(
        &self,
        demand: &ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        if demand.kind != ResourceKind::Code {
            return false;
        }
        self.can_reserve_indexed_layout(demand, bytes)
    }

    pub(crate) fn can_reserve_indexed_layout(
        &self,
        demand: &ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        if !matches!(demand.kind, ResourceKind::Code | ResourceKind::Table)
            || !self.indexed_owner_current(demand)
        {
            return false;
        }
        let Some(entry) = self
            .indexed_resources
            .entries
            .iter()
            .find(|entry| entry.demand == *demand)
        else {
            return false;
        };
        self.resident
            .resource_slice_bytes
            .saturating_add(self.indexed_resources.encoded_bytes())
            .saturating_sub(entry.layout_bytes)
            .saturating_add(bytes)
            <= self.policy.max_resource_bytes
    }

    pub(crate) fn indexed_resource_response(
        &self,
        resource: &ResourceId,
    ) -> Option<Arc<IndexedResourceResponse>> {
        self.indexed_resources
            .entries
            .iter()
            .find(|entry| {
                entry.demand.resource_id == *resource && self.indexed_owner_current(&entry.demand)
            })
            .and_then(|entry| entry.response.clone())
    }

    pub(crate) fn indexed_resource_pages(
        &self,
        resource: &ResourceId,
    ) -> Vec<Arc<IndexedResourceResponse>> {
        self.indexed_resources
            .entries
            .iter()
            .find(|entry| {
                entry.demand.resource_id == *resource && self.indexed_owner_current(&entry.demand)
            })
            .map(|entry| {
                entry
                    .pages
                    .iter()
                    .chain(entry.response.iter())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn indexed_resource_owner(
        &self,
        resource: &ResourceId,
    ) -> Option<ResourceTranscriptDemand> {
        self.indexed_resources
            .entries
            .iter()
            .find(|entry| {
                entry.demand.resource_id == *resource && self.indexed_owner_current(&entry.demand)
            })
            .map(|entry| entry.demand.clone())
    }

    pub(in super::super) fn reconcile_indexed_resources(
        &mut self,
        protected: &[ResidentPresentationRecordId],
    ) {
        self.indexed_resources.entries.retain(|entry| {
            entry.demand.activation_revision == self.presentation.activation_revision
                && self.resident.view_id.as_ref() == Some(&entry.demand.view_id)
                && self.resident.provider_revision == Some(entry.demand.revision)
                && protected.contains(&entry.demand.record_id)
                && self.presentation.records.iter().any(|record| {
                    record.id == entry.demand.record_id
                        && record.provenance.source
                            == ResidentRecordSource::Syndic(entry.demand.provenance.clone())
                })
        });
    }

    fn indexed_owner_current(&self, demand: &ResourceTranscriptDemand) -> bool {
        demand.activation_revision == self.presentation.activation_revision
            && self.resident.view_id.as_ref() == Some(&demand.view_id)
            && self.resident.provider_revision == Some(demand.revision)
            && self
                .protected_presentation_record_ids()
                .contains(&demand.record_id)
            && self.presentation.records.iter().any(|record| {
                record.id == demand.record_id
                    && record.provenance.source
                        == ResidentRecordSource::Syndic(demand.provenance.clone())
                    && matches!(&record.kind, ResidentPresentationRecordKind::ResourceReference {
                    resource_id, resource_kind, ..
                } if *resource_id == demand.resource_id && *resource_kind == demand.kind)
            })
    }
}
