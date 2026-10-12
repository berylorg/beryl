use super::super::super::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    ResourceTranscriptDemand,
};
use super::index;
use super::selection::SourceSelection;
use std::ops::Range;

pub(in super::super) const COPY_SOURCE_BYTES: usize = 256 * 1024;
const NATIVE_ROUNDING_BYTES: usize = 64 * 1024;
const COPY_STATE_BYTES: usize = 8192;

#[derive(Default)]
pub(in super::super) struct CodeSourceActions {
    pub selection: Option<SourceSelection>,
    pub dragging: bool,
    pub binding: Option<index::Binding>,
    pub root: Option<[u8; 32]>,
    revision: Option<beryl_model::ProjectionRevision>,
    remap: Option<SelectionRemap>,
    pub copy: Option<CodeCopy>,
    pub save: Option<super::save::TextResourceSave>,
    pub save_admission: crate::main_window::WorkerAdmission,
    pub feedback: &'static str,
    sequence: u64,
    #[cfg(test)]
    pub save_choice:
        Option<futures_channel::oneshot::Receiver<Result<Option<gpui::SavePathChoice>, ()>>>,
    #[cfg(test)]
    pub save_fault: Option<super::save::SaveFault>,
    #[cfg(test)]
    pub save_hold: Option<std::sync::Arc<super::save::SaveWriterGate>>,
    #[cfg(test)]
    pub writer: Option<
        Box<
            dyn FnMut(
                &gpui::ClipboardItem,
                gpui::ClipboardLimits,
                &mut gpui::App,
            ) -> Result<(), gpui::ClipboardError>,
        >,
    >,
}

struct SelectionRemap {
    anchors: [index::SourceAnchor; 2],
    resolved: [Option<Option<u64>>; 2],
}

pub(in super::super) struct CodeCopy {
    owner: ResourceTranscriptDemand,
    binding: index::Binding,
    root: [u8; 32],
    revision: beryl_model::ProjectionRevision,
    sequence: u64,
    range: Range<u64>,
    pub bytes: Vec<u8>,
    pub budget: usize,
    pub last_query: Option<IndexedResourceDemand>,
}

pub(in super::super) fn identity(owner: &ResourceTranscriptDemand) -> ResourceTranscriptDemand {
    ResourceTranscriptDemand {
        indexed: None,
        activation_revision: owner.activation_revision,
        presentation_revision: owner.presentation_revision,
        view_id: owner.view_id.clone(),
        revision: owner.revision,
        record_id: owner.record_id.clone(),
        provenance: owner.provenance.clone(),
        resource_id: owner.resource_id.clone(),
        kind: owner.kind.clone(),
        metadata: owner.metadata.clone(),
        range: None,
        retained_records: Vec::new(),
        pins: Default::default(),
    }
}

pub(in super::super) fn identity_matches(
    saved: &ResourceTranscriptDemand,
    current: &ResourceTranscriptDemand,
) -> bool {
    saved.activation_revision == current.activation_revision
        && saved.view_id == current.view_id
        && saved.revision == current.revision
        && saved.record_id == current.record_id
        && saved.provenance == current.provenance
        && saved.resource_id == current.resource_id
        && saved.kind == current.kind
        && saved.metadata == current.metadata
}

pub(in super::super) fn owner_bytes(owner: &ResourceTranscriptDemand) -> usize {
    owner.view_id.0.len()
        + owner.record_id.0.len()
        + owner.resource_id.0.len()
        + owner.provenance.view_id.0.len()
        + owner.provenance.turn_id.as_ref().map_or(0, |id| id.0.len())
        + owner.provenance.item_id.as_ref().map_or(0, |id| id.0.len())
        + owner
            .provenance
            .projection_id
            .as_ref()
            .map_or(0, |id| id.0.len())
        + owner
            .provenance
            .resource_id
            .as_ref()
            .map_or(0, |id| id.0.len())
        + owner.metadata.as_ref().map_or(0, |metadata| {
            metadata.resource_id.0.len()
                + metadata.media_type.as_ref().map_or(0, String::len)
                + metadata.digest.as_ref().map_or(0, String::len)
        })
}

impl CodeSourceActions {
    #[cfg(test)]
    pub fn copy_progress(&self) -> Option<(Range<u64>, usize)> {
        let copy = self.copy.as_ref()?;
        Some((copy.range.clone(), copy.bytes.len()))
    }
    pub fn charge(&self) -> usize {
        4096 + self
            .copy
            .as_ref()
            .map_or(0, |copy| copy.budget + COPY_STATE_BYTES)
            + self
                .save
                .as_ref()
                .map_or(0, |_| super::save::SAVE_STATE_BYTES)
    }
    pub fn pending(&self) -> bool {
        self.remap.is_some() || self.copy.is_some() || self.save.is_some()
    }
    pub fn whole_copy_unavailable(
        &self,
        source_remapping: bool,
        presentation_pending: bool,
        owner_current: bool,
    ) -> Option<&'static str> {
        if self.copy.is_some() {
            return Some("Copy is already in progress");
        }
        if self.save.is_some() {
            return Some("Save is already in progress");
        }
        if presentation_pending {
            return Some("Waiting for the current Code presentation");
        }
        if !owner_current {
            return Some("The Code source owner is no longer available");
        }
        if source_remapping || self.remap.is_some() {
            return Some("Code source positions are being reconciled");
        }
        let Some(binding) = &self.binding else {
            return Some("Code source metadata is loading");
        };
        if self.root.is_none() || self.revision != Some(binding.metadata.revision()) {
            return Some("Waiting for the current Code source binding");
        }
        if binding.range.start == binding.range.end {
            return Some("The Code source is empty");
        }
        if binding.range.end - binding.range.start > COPY_SOURCE_BYTES as u64 {
            return Some("The Code source exceeds the 256 KiB clipboard limit");
        }
        None
    }
    pub fn selected_range(&self) -> Option<Range<u64>> {
        self.remap
            .is_none()
            .then(|| self.selection.map(SourceSelection::range))
            .flatten()
    }
    pub fn cancel_copy(&mut self, feedback: &'static str) {
        self.copy = None;
        self.feedback = feedback;
    }
    pub fn refuse_query(&mut self, owner: &ResourceTranscriptDemand) -> bool {
        if self.query().as_ref() != owner.indexed.as_ref() || owner.indexed.is_none() {
            return false;
        }
        if self
            .copy
            .as_ref()
            .is_some_and(|copy| !identity_matches(&copy.owner, owner))
        {
            return false;
        }
        if let Some(save) = &self.save {
            if !save.current(self, owner) {
                return false;
            }
            self.cancel_save("Save unavailable: source read refused");
            return true;
        }
        if self.remap.take().is_some() {
            self.selection = None;
            self.dragging = false;
        }
        self.cancel_copy("Copy unavailable: source read refused");
        true
    }
    pub fn select(&mut self, position: u64, extend: bool) {
        if self.remap.is_some()
            || self.binding.as_ref().is_none_or(|binding| {
                position < binding.range.start || position > binding.range.end
            })
        {
            return;
        }
        self.cancel_copy("");
        if self.save.is_some() {
            self.feedback = "Saving…";
        }
        self.selection = Some(SourceSelection {
            anchor: if extend {
                self.selection
                    .map_or(position, |selection| selection.anchor)
            } else {
                position
            },
            head: position,
        });
    }
    fn begin_remap(&mut self) {
        self.dragging = false;
        self.cancel_copy("Copy unavailable: source changed");
        self.cancel_save("Save unavailable: source changed");
        if let (Some(selected), Some(binding)) = (self.selection, &self.binding) {
            self.remap = Some(SelectionRemap {
                anchors: [selected.anchor, selected.head].map(|position| index::SourceAnchor {
                    source: binding.source,
                    block_start: binding.range.start,
                    kind: syndic_storage::ResourceKind::Code,
                    position,
                    visual_offset: 0,
                }),
                resolved: [None, None],
            });
        }
    }
    pub fn observe(
        &mut self,
        owner: &ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
        response: Option<&IndexedResourceResponse>,
    ) {
        if self.revision.is_some_and(|old| old != revision) {
            self.begin_remap();
            self.root = None;
        }
        self.revision = Some(revision);
        let Some(response) = response else {
            return;
        };
        let Some(root) = &response.root else {
            return;
        };
        let binding = match &response.value {
            IndexedResourceValue::Range(value) => &value.binding,
            IndexedResourceValue::Display(value) => &value.binding,
            IndexedResourceValue::Anchor(value) => &value.binding,
            IndexedResourceValue::Source(value) => &value.binding,
            IndexedResourceValue::Pending => return,
        };
        if binding.metadata.revision() != revision {
            return;
        }
        if self.binding.as_ref().is_some_and(|old| {
            !old.source.can_extend(binding.source) || old.range.start != binding.range.start
        }) {
            self.selection = None;
            self.remap = None;
            self.dragging = false;
            self.cancel_copy("Copy unavailable: source replaced");
            self.cancel_save("Save unavailable: source replaced");
        } else if self.binding.as_ref().is_some_and(|old| old != binding) && self.remap.is_none() {
            self.begin_remap();
        }
        self.binding = Some(binding.clone());
        self.root = Some(root.digest);
        if binding.range.start == binding.range.end {
            self.selection = None;
            self.remap = None;
            if self.copy.is_some() {
                self.cancel_copy("Copy unavailable: empty source");
            }
            return;
        }
        let (Some(remap), IndexedResourceValue::Anchor(value)) = (&mut self.remap, &response.value)
        else {
            return;
        };
        let Some(index) = remap.resolved.iter().position(Option::is_none) else {
            return;
        };
        if !owner.indexed.as_ref().is_some_and(|query| matches!(query.query, IndexedResourceQuery::Anchor { anchor, .. } if anchor == remap.anchors[index])) { return; }
        remap.resolved[index] = Some(value.resolved.as_ref().map(|resolved| {
            resolved
                .anchor
                .position
                .clamp(binding.range.start, binding.range.end)
        }));
        if remap.resolved.iter().all(Option::is_some) {
            self.selection = match (remap.resolved[0].flatten(), remap.resolved[1].flatten()) {
                (Some(anchor), Some(head)) => Some(SourceSelection { anchor, head }),
                _ => None,
            };
            self.remap = None;
        }
    }
    pub fn query(&self) -> Option<IndexedResourceDemand> {
        let revision = self.revision?;
        let query = if let Some(remap) = &self.remap {
            let index = remap.resolved.iter().position(Option::is_none)?;
            IndexedResourceQuery::Anchor {
                anchor: remap.anchors[index],
                bytes: 8192,
            }
        } else {
            if let Some(save) = &self.save {
                return save.query(self);
            }
            let copy = self.copy.as_ref()?;
            let start = copy.range.start.checked_add(copy.bytes.len() as u64)?;
            if start >= copy.range.end {
                return None;
            }
            IndexedResourceQuery::Source {
                range: index::ByteRange {
                    start,
                    end: copy.range.end.min(start.checked_add(8192)?),
                },
                bytes: 16384,
            }
        };
        Some(IndexedResourceDemand {
            revision,
            root: self.root,
            query,
        })
    }
    pub fn copy_budget(&self, selected: bool) -> Option<(Range<u64>, usize)> {
        if self.remap.is_some() {
            return None;
        }
        let binding = self.binding.as_ref()?;
        let range = if selected {
            self.selected_range()?
        } else {
            binding.range.start..binding.range.end
        };
        if range.start < binding.range.start || range.end > binding.range.end {
            return None;
        }
        let length = usize::try_from(range.end.checked_sub(range.start)?).ok()?;
        if length == 0 || length > COPY_SOURCE_BYTES {
            return None;
        }
        let budget = length
            .checked_mul(3)?
            .checked_add(2)?
            .checked_add(NATIVE_ROUNDING_BYTES)?;
        Some((range, budget + COPY_STATE_BYTES))
    }
    pub fn begin_copy(
        &mut self,
        owner: &ResourceTranscriptDemand,
        range: Range<u64>,
        charge: usize,
    ) -> bool {
        if self.save.is_some() || owner_bytes(owner) > 4096 {
            return false;
        }
        let (Some(binding), Some(root), Some(revision)) = (&self.binding, self.root, self.revision)
        else {
            return false;
        };
        let Some(sequence) = self.sequence.checked_add(1) else {
            return false;
        };
        let mut bytes = Vec::new();
        let Ok(length) = usize::try_from(range.end - range.start) else {
            return false;
        };
        if bytes.try_reserve_exact(length).is_err() || bytes.capacity() > length {
            return false;
        }
        self.sequence = sequence;
        self.copy = Some(CodeCopy {
            owner: identity(owner),
            binding: binding.clone(),
            root,
            revision,
            sequence,
            range,
            bytes,
            budget: charge - COPY_STATE_BYTES,
            last_query: None,
        });
        self.feedback = "Copying…";
        true
    }
    pub fn accept_copy_page(
        &mut self,
        owner: &ResourceTranscriptDemand,
        response: Option<&IndexedResourceResponse>,
    ) {
        let Some(query) = self.query() else {
            return;
        };
        let Some(copy) = &mut self.copy else {
            return;
        };
        if !identity_matches(&copy.owner, owner)
            || self.sequence != copy.sequence
            || self.binding.as_ref() != Some(&copy.binding)
            || self.root != Some(copy.root)
            || self.revision != Some(copy.revision)
        {
            self.cancel_copy("Copy unavailable: source changed");
            return;
        }
        let Some(response) = response else {
            return;
        };
        if owner.indexed.as_ref() != Some(&query) {
            return;
        }
        let (IndexedResourceQuery::Source { range, .. }, IndexedResourceValue::Source(page)) =
            (&query.query, &response.value)
        else {
            return;
        };
        if page.binding != copy.binding
            || page.range != *range
            || response
                .root
                .as_ref()
                .is_none_or(|root| root.digest != copy.root)
            || page.bytes.len() as u64 != range.end - range.start
        {
            self.cancel_copy("Copy unavailable: source changed");
            return;
        }
        if page.bytes.contains(&0) {
            self.cancel_copy("Copy unavailable: source contains NUL");
            return;
        }
        if copy
            .bytes
            .len()
            .checked_add(page.bytes.len())
            .is_none_or(|len| len > copy.bytes.capacity())
        {
            self.cancel_copy("Copy unavailable: capacity");
            return;
        }
        copy.bytes.extend_from_slice(&page.bytes);
        copy.last_query = Some(query);
    }
    pub fn ready_copy(&self, owner: &ResourceTranscriptDemand) -> bool {
        self.copy.as_ref().is_some_and(|copy| {
            copy.bytes.len() as u64 == copy.range.end - copy.range.start
                && identity_matches(&copy.owner, owner)
                && copy.sequence == self.sequence
                && self.binding.as_ref() == Some(&copy.binding)
                && self.root == Some(copy.root)
                && self.revision == Some(copy.revision)
                && owner.indexed == copy.last_query
        })
    }
    pub(super) fn save_revision(&self) -> Option<beryl_model::ProjectionRevision> {
        self.revision
    }
    pub(super) fn next_sequence(&mut self) -> Option<u64> {
        self.sequence = self.sequence.checked_add(1)?;
        Some(self.sequence)
    }
    pub(super) fn action_sequence(&self) -> u64 {
        self.sequence
    }
    pub(in super::super) fn cancel_save(&mut self, feedback: &'static str) {
        if self.save.take().is_some() {
            self.feedback = feedback;
        }
    }
}
