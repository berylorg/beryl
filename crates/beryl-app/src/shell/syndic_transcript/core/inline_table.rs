use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InlineTableOwner {
    pub view_id: TranscriptViewId,
    pub record_id: ResidentPresentationRecordId,
    pub activation_revision: u64,
    pub provenance: ResidentRecordProvenance,
    pub prepared: Arc<syndic_storage::inline_table::PreparedInlineTable>,
}

#[derive(Clone, Debug)]
pub(super) struct InlineTableLayout {
    record_id: ResidentPresentationRecordId,
    activation_revision: u64,
    projection_revision: Option<ProviderRevision>,
    bytes: usize,
}

impl ResidentTranscriptCore {
    pub(crate) fn inline_table_current(&self, owner: &InlineTableOwner) -> bool {
        self.resident.view_id.as_ref() == Some(&owner.view_id)
            && self.presentation.activation_revision == owner.activation_revision
            && self.protected_presentation_record_ids().contains(&owner.record_id)
            && self.presentation.records.iter().any(|record| {
                record.id == owner.record_id
                    && record.provenance.projection_revision == owner.provenance.projection_revision
                    && record.provenance.source == owner.provenance.source
                    && matches!(&record.kind, ResidentPresentationRecordKind::InlineTable { prepared }
                        if prepared == &owner.prepared)
            })
    }

    pub(crate) fn reserve_inline_table_layout(
        &mut self,
        owner: &InlineTableOwner,
        bytes: usize,
    ) -> bool {
        if bytes == 0 {
            self.inline_table_layouts
                .retain(|old| old.record_id != owner.record_id);
            self.refresh_budget_accounting();
            return true;
        }
        if !self.inline_table_current(owner) {
            return false;
        }
        let previous = self
            .inline_table_layouts
            .iter()
            .find(|old| old.record_id == owner.record_id)
            .map_or(0, |old| old.bytes);
        let used = self
            .resident
            .resource_slice_bytes
            .saturating_add(self.indexed_resources.encoded_bytes())
            .saturating_add(self.inline_table_layout_bytes())
            .saturating_add(self.narrative_layout_bytes())
            .saturating_sub(previous);
        if used.saturating_add(bytes) > self.policy.max_resource_bytes {
            return false;
        }
        self.inline_table_layouts
            .retain(|old| old.record_id != owner.record_id);
        self.inline_table_layouts.push(InlineTableLayout {
            record_id: owner.record_id.clone(),
            activation_revision: owner.activation_revision,
            projection_revision: owner.provenance.projection_revision,
            bytes,
        });
        self.refresh_budget_accounting();
        true
    }

    pub(super) fn inline_table_layout_bytes(&self) -> usize {
        self.inline_table_layouts
            .iter()
            .map(|entry| entry.bytes)
            .sum()
    }

    pub(super) fn reconcile_inline_tables(&mut self) {
        let protected = self.protected_presentation_record_ids();
        self.inline_table_layouts.retain(|entry| {
            entry.activation_revision == self.presentation.activation_revision
                && protected.contains(&entry.record_id)
                && self.presentation.records.iter().any(|record| {
                    record.id == entry.record_id
                        && record.provenance.projection_revision == entry.projection_revision
                        && matches!(
                            record.kind,
                            ResidentPresentationRecordKind::InlineTable { .. }
                        )
                })
        });
    }
}
