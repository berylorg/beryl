use super::*;

impl ResidentTranscriptCore {
    pub(crate) fn prepare_indexed_resource_response(
        &self,
        demand: &ResourceTranscriptDemand,
        response: Arc<IndexedResourceResponse>,
    ) -> Option<Self> {
        if !self.resource_demands().contains(demand) {
            return None;
        }
        let inner = demand.indexed.as_ref()?;
        let allowance = match &inner.query {
            IndexedResourceQuery::Range { bytes, .. }
            | IndexedResourceQuery::Display { bytes, .. }
            | IndexedResourceQuery::Anchor { bytes, .. }
            | IndexedResourceQuery::Source { bytes, .. } => *bytes,
        };
        if response.encoded_bytes == 0
            || response.encoded_bytes > allowance.min(65_536)
            || response.encoded_bytes > self.policy.max_resource_slice_bytes
        {
            return None;
        }
        match (&inner.query, &response.value) {
            (_, IndexedResourceValue::Pending)
                if response.root.is_none() && response.encoded_bytes == 1 => {}
            (
                IndexedResourceQuery::Range {
                    request,
                    descriptors,
                    ..
                },
                IndexedResourceValue::Range(value),
            ) => {
                if !indexed_binding_matches(demand, inner, &response, &value.binding)
                    || value.cursor.request != *request
                    || value.items.len() > (*descriptors).min(256)
                    || response.root.as_ref().is_none_or(|root| {
                        root.key != value.cursor.build || root.digest != value.cursor.root_digest
                    })
                {
                    return None;
                }
                if demand.kind == ResourceKind::Table
                    && (!table_request_bounded(request)
                        || value.binding.metadata.kind() != syndic_storage::ResourceKind::Table)
                {
                    return None;
                }
            }
            (
                IndexedResourceQuery::Display { request, .. },
                IndexedResourceValue::Display(value),
            ) => {
                if !indexed_binding_matches(demand, inner, &response, &value.binding)
                    || value.cursor.request != *request
                    || response.root.as_ref().is_none_or(|root| {
                        root.key != value.cursor.build || root.digest != value.cursor.root
                    })
                {
                    return None;
                }
            }
            (IndexedResourceQuery::Anchor { .. }, IndexedResourceValue::Anchor(value)) => {
                if !indexed_binding_matches(demand, inner, &response, &value.binding) {
                    return None;
                }
            }
            (IndexedResourceQuery::Source { range, .. }, IndexedResourceValue::Source(value)) => {
                if !matches!(demand.kind, ResourceKind::Code | ResourceKind::Table)
                    || !matches!(
                        (&demand.kind, value.binding.metadata.kind()),
                        (ResourceKind::Code, syndic_storage::ResourceKind::Code)
                            | (ResourceKind::Table, syndic_storage::ResourceKind::Table)
                    )
                    || !indexed_binding_matches(demand, inner, &response, &value.binding)
                    || inner.root.is_none()
                    || value.range != *range
                    || range.start >= range.end
                    || range.start < value.binding.range.start
                    || range.end > value.binding.range.end
                    || range.end - range.start > 8192
                    || value.bytes.len() as u64 != range.end - range.start
                    || value.bytes.len() > response.encoded_bytes
                {
                    return None;
                }
            }
            _ => return None,
        }
        let mut candidate = self.clone();
        let entry = candidate
            .indexed_resources
            .entries
            .iter_mut()
            .find(|entry| {
                entry.inner == *inner && entry.demand.resource_id == demand.resource_id
            })?;
        if !entry.pages.is_empty() {
            let IndexedResourceValue::Range(range) = &response.value else {
                return None;
            };
            let IndexedResourceQuery::Range {
                request: index::Request::Code { lines },
                cursor: Some(cursor),
                ..
            } = &inner.query
            else {
                return None;
            };
            let index::Position::Code(start) = cursor.next else {
                return None;
            };
            let index::Position::Code(end) = range.cursor.next else {
                return None;
            };
            if range.items.is_empty()
                || end < start
                || (end == start && range.cursor.fragment_offset == 0)
                || end > lines.end
                || entry
                    .pages
                    .iter()
                    .map(|page| match &page.value {
                        IndexedResourceValue::Range(value) => value.items.len(),
                        _ => 0,
                    })
                    .sum::<usize>()
                    + range.items.len()
                    > (lines.end - lines.start).min(16) as usize
                || range.binding
                    != match &entry.pages.last()?.value {
                        IndexedResourceValue::Range(value) => value.binding.clone(),
                        _ => return None,
                    }
            {
                return None;
            }
        }
        entry.demand = demand.clone();
        if demand.kind == ResourceKind::Table
            && let IndexedResourceValue::Range(range) = &response.value
        {
            if !admit_table_page(entry, inner, &response, range) {
                return None;
            }
        }
        entry.response = Some(response);
        candidate.refresh_budget_accounting();
        if candidate.resident.resource_bytes > candidate.policy.max_resource_bytes {
            return None;
        }
        candidate.bump_resource_presentation();
        Some(candidate)
    }
}

fn indexed_binding_matches(
    demand: &ResourceTranscriptDemand,
    inner: &IndexedResourceDemand,
    response: &IndexedResourceResponse,
    binding: &syndic_storage::indexed_resource::Binding,
) -> bool {
    response.root.as_ref().is_some_and(|root| {
        root.key.resource == binding.resource
            && binding.metadata.revision() == inner.revision
            && binding.resource.id.to_string() == demand.resource_id.0
            && demand.provenance.source_range.as_ref()
                == Some(&(binding.range.start..binding.range.end))
            && inner.root.is_none_or(|digest| digest == root.digest)
    })
}
