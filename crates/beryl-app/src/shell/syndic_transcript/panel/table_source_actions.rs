use super::super::provider::resource_index as index;
use super::super::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    ResourceTranscriptDemand,
};
use super::SyndicTranscriptPanel;
use super::code_panel::save::{SAVE_STATE_BYTES, TextResourceSave};
use super::code_panel::source_actions::{identity, identity_matches, owner_bytes};
use super::copy_keyboard::CopyMethod;
use super::table_export::{COPY_CHARGE, TableExport};
use super::table_viewport::{TableCellId, TableSelection, TableViewport};
use gpui::Context;

#[derive(Default)]
pub(super) struct TableSourceActions {
    pub feedback: &'static str,
    pub binding: Option<index::Binding>,
    pub root: Option<[u8; 32]>,
    pub revision: Option<beryl_model::ProjectionRevision>,
    pub sequence: u64,
    pub save: Option<TextResourceSave>,
    pub save_admission: crate::main_window::WorkerAdmission,
    copy: Option<TableCopy>,
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
    #[cfg(test)]
    pub save_choice:
        Option<futures_channel::oneshot::Receiver<Result<Option<gpui::SavePathChoice>, ()>>>,
}

struct TableCopy {
    owner: ResourceTranscriptDemand,
    binding: index::Binding,
    root: [u8; 32],
    revision: beryl_model::ProjectionRevision,
    selection: Option<TableSelection>,
    raw_row: Option<TableCellId>,
    sequence: u64,
    method: CopyMethod,
    export: Option<TableExport>,
    bytes: Vec<u8>,
    range_index: usize,
    range_offset: u64,
    converting: bool,
    task: Option<gpui::Task<()>>,
}

pub(super) fn clear_table_source_refusal(
    rejected: &mut Vec<ResourceTranscriptDemand>,
    owner: &ResourceTranscriptDemand,
    query: &IndexedResourceDemand,
) -> bool {
    let before = rejected.len();
    rejected.retain(|rejected| {
        !(identity_matches(rejected, owner)
            && rejected.range == owner.range
            && rejected.indexed.as_ref() == Some(query))
    });
    rejected.len() != before
}

impl TableSourceActions {
    pub fn copy_pending(&self) -> bool {
        self.copy.is_some()
    }
    #[cfg(test)]
    pub fn assembled_copy_for_test(&self) -> Option<&[u8]> {
        self.copy.as_ref().map(|copy| copy.bytes.as_slice())
    }
    pub fn charge(&self) -> usize {
        4096 + self.copy.as_ref().map_or(0, |_| COPY_CHARGE)
            + self.save.as_ref().map_or(0, |_| SAVE_STATE_BYTES)
    }
    pub fn pending(&self) -> bool {
        self.copy.is_some() || self.save.is_some()
    }
    pub fn cancel_copy(&mut self, feedback: &'static str) {
        self.copy = None;
        self.feedback = feedback;
    }
    pub fn cancel_save(&mut self, feedback: &'static str) {
        if self.save.take().is_some() {
            self.feedback = feedback;
        }
    }
    pub fn cancel(&mut self, feedback: &'static str) {
        self.cancel_save(feedback);
        self.cancel_copy(feedback);
    }
    pub fn observe(
        &mut self,
        _owner: &ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
        response: Option<&IndexedResourceResponse>,
    ) {
        if self.revision.is_some_and(|old| old != revision) {
            self.cancel("Source action unavailable: revision changed");
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
        if self.binding.as_ref().is_some_and(|old| old != binding)
            || self.root.is_some_and(|old| old != root.digest)
        {
            self.cancel("Source action unavailable: source changed");
        }
        self.binding = Some(binding.clone());
        self.root = Some(root.digest);
    }
    pub fn begin_copy(&mut self, viewport: &TableViewport, method: CopyMethod) -> bool {
        let Some(binding) = &viewport.binding else {
            self.feedback = "Copy unavailable: source metadata loading";
            return false;
        };
        let Some(cells) = viewport.selection_cells() else {
            self.feedback = "Copy unavailable: no eligible cell selection";
            return false;
        };
        let export = TableExport::selected(
            cells,
            viewport.source_pages(),
            binding,
            viewport.whole_table_selection(),
        );
        self.begin_export(viewport, method, None, export)
    }
    pub fn begin_row_copy(
        &mut self,
        viewport: &TableViewport,
        row: TableCellId,
        method: CopyMethod,
    ) -> bool {
        let range = viewport
            .fallback_rows()
            .into_iter()
            .find(|(id, _)| *id == row)
            .and_then(|(_, descriptor)| descriptor.absolute_raw);
        let Some(range) = range else {
            self.feedback = "Copy unavailable: raw row missing";
            return false;
        };
        self.begin_export(viewport, method, Some(row), TableExport::raw(range))
    }
    fn begin_export(
        &mut self,
        viewport: &TableViewport,
        method: CopyMethod,
        raw_row: Option<TableCellId>,
        export: Result<TableExport, &'static str>,
    ) -> bool {
        if self.save.is_some() {
            self.feedback = "A source action is in progress";
            return false;
        }
        let export = match export {
            Ok(export) => export,
            Err(reason) => {
                self.feedback = reason;
                return false;
            }
        };
        let (Some(binding), Some(root)) = (&viewport.binding, &viewport.root) else {
            return false;
        };
        if owner_bytes(&viewport.owner) > 4096
            || self.binding.as_ref() != Some(binding)
            || self.root != Some(root.digest)
            || self.revision != Some(viewport.revision)
        {
            return false;
        }
        let Some(sequence) = self.sequence.checked_add(1) else {
            return false;
        };
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(export.source_bytes).is_err()
            || bytes.capacity() > export.source_bytes
        {
            self.feedback = "Copy unavailable: allocation";
            return false;
        }
        self.sequence = sequence;
        self.copy = Some(TableCopy {
            owner: identity(&viewport.owner),
            binding: binding.clone(),
            root: root.digest,
            revision: viewport.revision,
            selection: viewport.selection.clone(),
            raw_row,
            sequence,
            method,
            export: Some(export),
            bytes,
            range_index: 0,
            range_offset: 0,
            converting: false,
            task: None,
        });
        self.feedback = "Copying…";
        true
    }
    pub fn query(&self) -> Option<IndexedResourceDemand> {
        if let Some(save) = &self.save {
            return save.query_exact(self.binding.as_ref());
        }
        let copy = self.copy.as_ref()?;
        let range = copy.export.as_ref()?.ranges.get(copy.range_index)?;
        let start = range.start.checked_add(copy.range_offset)?;
        Some(IndexedResourceDemand {
            revision: copy.revision,
            root: Some(copy.root),
            query: IndexedResourceQuery::Source {
                range: index::ByteRange {
                    start,
                    end: range.end.min(start.checked_add(8192)?),
                },
                bytes: 16384,
            },
        })
    }
    fn copy_current(&self, owner: &ResourceTranscriptDemand, viewport: &TableViewport) -> bool {
        self.copy.as_ref().is_some_and(|copy| {
            identity_matches(&copy.owner, owner)
                && self.binding.as_ref() == Some(&copy.binding)
                && self.root == Some(copy.root)
                && self.revision == Some(copy.revision)
                && self.sequence == copy.sequence
                && viewport.binding.as_ref() == Some(&copy.binding)
                && viewport
                    .root
                    .as_ref()
                    .is_some_and(|root| root.digest == copy.root)
                && viewport.revision == copy.revision
                && viewport.selection == copy.selection
                && copy
                    .raw_row
                    .is_none_or(|row| viewport.fallback_rows().iter().any(|(id, _)| *id == row))
        })
    }
    pub fn accept_copy_page(
        &mut self,
        owner: &ResourceTranscriptDemand,
        response: Option<&IndexedResourceResponse>,
    ) {
        let Some(query) = self.query() else {
            return;
        };
        if owner.indexed.as_ref() != Some(&query) {
            return;
        }
        let Some(response) = response else {
            return;
        };
        let (IndexedResourceQuery::Source { range, .. }, IndexedResourceValue::Source(page)) =
            (&query.query, &response.value)
        else {
            return;
        };
        let Some(copy) = &mut self.copy else {
            return;
        };
        if !identity_matches(&copy.owner, owner)
            || page.binding != copy.binding
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
        copy.range_offset += page.bytes.len() as u64;
        let source = copy.export.as_ref().unwrap().ranges[copy.range_index];
        if source.start + copy.range_offset == source.end {
            copy.range_index += 1;
            copy.range_offset = 0;
        }
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
        if self.save.as_ref().is_some_and(|save| {
            !save.current_exact(
                owner,
                self.binding.as_ref(),
                self.root,
                self.revision,
                self.sequence,
            )
        }) {
            return false;
        }
        self.cancel("Source action unavailable: source read refused");
        true
    }
}

impl SyndicTranscriptPanel {
    pub(super) fn begin_table_copy(&mut self, id: u64, method: CopyMethod, cx: &mut Context<Self>) {
        self.begin_table_copy_action(id, None, method, cx);
    }
    pub(super) fn begin_table_row_copy(&mut self, id: u64, row: u64, cx: &mut Context<Self>) {
        self.begin_table_copy_action(
            id,
            Some(TableCellId {
                role: index::RowRole::Body,
                row,
                column: 0,
            }),
            CopyMethod::Markdown,
            cx,
        );
    }
    fn begin_table_copy_action(
        &mut self,
        id: u64,
        row: Option<TableCellId>,
        method: CopyMethod,
        cx: &mut Context<Self>,
    ) {
        self.supersede_resource_copies();
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        if self.staged.is_some() {
            table.actions.feedback = "Copy unavailable: presentation changing";
            cx.notify();
            return;
        }
        let Some(owner) = self
            .host
            .indexed_resource_owner(&table.viewport.owner.resource_id)
        else {
            table.actions.feedback = "Copy unavailable: retired source";
            cx.notify();
            return;
        };
        let charge = table.viewport.charge() + 4096 + COPY_CHARGE;
        if !self.host.reserve_indexed_layout(&owner, charge) {
            table.actions.feedback = "Copy unavailable: capacity";
            cx.notify();
            return;
        }
        table.actions.observe(
            &owner,
            table.viewport.revision,
            self.host
                .indexed_resource_response(&owner.resource_id)
                .as_deref(),
        );
        let begun = match row {
            Some(row) => table.actions.begin_row_copy(&table.viewport, row, method),
            None => table.actions.begin_copy(&table.viewport, method),
        };
        self.host
            .reserve_indexed_layout(&owner, table.viewport.charge() + table.actions.charge());
        if begun {
            if let Some(query) = table.actions.query() {
                clear_table_source_refusal(&mut self.rejected_resource, &owner, &query);
            }
            self.advance_table_source_actions(id, cx);
            cx.emit(super::ResourceViewportDemandChanged);
        }
        cx.notify();
    }
    pub(super) fn advance_table_source_actions(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        if self.staged.is_some() {
            return false;
        }
        if self.advance_table_save(id, cx) {
            return true;
        }
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return false;
        };
        let Some(owner) = self
            .host
            .indexed_resource_owner(&table.viewport.owner.resource_id)
        else {
            table
                .actions
                .cancel("Source action unavailable: retired source");
            return false;
        };
        if table.actions.copy.is_none() {
            return false;
        }
        if !table.actions.copy_current(&owner, &table.viewport) {
            table
                .actions
                .cancel_copy("Copy unavailable: selection or source changed");
            self.host
                .reserve_indexed_layout(&owner, table.viewport.charge() + table.actions.charge());
            cx.notify();
            return false;
        }
        table.actions.accept_copy_page(
            &owner,
            self.host
                .indexed_resource_response(&owner.resource_id)
                .as_deref(),
        );
        if let Some(copy) = &mut table.actions.copy
            && !copy.converting
            && copy
                .export
                .as_ref()
                .is_some_and(|export| copy.bytes.len() == export.source_bytes)
        {
            let Some(lease) = table.actions.save_admission.acquire() else {
                table
                    .actions
                    .cancel_copy("Copy unavailable: worker capacity");
                cx.notify();
                return false;
            };
            copy.converting = true;
            let export = copy.export.take().unwrap();
            let bytes = std::mem::take(&mut copy.bytes);
            let sequence = copy.sequence;
            let method = copy.method;
            let task = cx.background_executor().spawn(async move {
                let result = export.render(bytes, method);
                drop(lease);
                result
            });
            copy.task = Some(cx.spawn(async move |weak, cx| {
                let result = task.await;
                let _ = weak.update(cx, |panel, cx| {
                    panel.finish_table_copy(id, sequence, result, cx)
                });
            }));
            return true;
        }
        if let Some(query) = table.actions.query()
            && owner.indexed.as_ref() != Some(&query)
            && self.host.set_indexed_resource_demand(&owner, Some(query))
        {
            cx.emit(super::ResourceViewportDemandChanged);
            cx.notify();
        }
        table.actions.pending()
    }
    pub(super) fn finish_table_copy(
        &mut self,
        id: u64,
        sequence: u64,
        result: Result<String, &'static str>,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        if table
            .actions
            .copy
            .as_ref()
            .is_none_or(|copy| copy.sequence != sequence)
        {
            return;
        }
        let owner = self
            .host
            .indexed_resource_owner(&table.viewport.owner.resource_id);
        if self.staged.is_some()
            || owner
                .as_ref()
                .is_none_or(|owner| !table.actions.copy_current(owner, &table.viewport))
        {
            table
                .actions
                .cancel_copy("Copy unavailable: selection or source changed");
        } else {
            let result = result.and_then(|text| {
                if text.len() > super::table_export::COPY_BYTES {
                    return Err("Copy unavailable: representation exceeds 64 KiB");
                }
                let budget = text.len() * 3 + 2 + 65_536;
                let item = gpui::ClipboardItem::new_string(text);
                let limits = gpui::ClipboardLimits {
                    total_bytes: budget,
                    text_bytes: budget,
                    metadata_bytes: 1,
                    image_bytes: 1,
                };
                #[cfg(test)]
                if let Some(writer) = &mut table.actions.writer {
                    return writer(&item, limits, cx)
                        .map_err(|_| "Copy unavailable: clipboard refused");
                }
                cx.write_to_clipboard_checked(&item, limits)
                    .map_err(|_| "Copy unavailable: clipboard refused")
            });
            table.actions.cancel_copy(result.err().unwrap_or("Copied"));
        }
        if let Some(owner) = owner {
            self.host
                .reserve_indexed_layout(&owner, table.viewport.charge() + table.actions.charge());
        }
        cx.notify();
        cx.emit(super::ResourceViewportDemandChanged);
    }
    pub(crate) fn reject_table_source_candidate(
        &mut self,
        demand: &ResourceTranscriptDemand,
    ) -> bool {
        if !self.resource_candidate_current(demand)
            || !self.table_panels.iter().any(|table| {
                table.viewport.owner.resource_id == demand.resource_id
                    && table.actions.query().as_ref() == demand.indexed.as_ref()
            })
        {
            return false;
        }
        self.discard_staged_activation();
        if !self.reject_table_source_action(demand) {
            return false;
        }
        self.reject_resource_demand(demand.clone());
        true
    }
    pub(crate) fn reject_table_source_action(&mut self, demand: &ResourceTranscriptDemand) -> bool {
        if self.staged.is_some() || self.resource_demand().as_ref() != Some(demand) {
            return false;
        }
        let Some(table) = self
            .table_panels
            .iter_mut()
            .find(|table| table.viewport.owner.resource_id == demand.resource_id)
        else {
            return false;
        };
        let Some(owner) = self.host.indexed_resource_owner(&demand.resource_id) else {
            return false;
        };
        if owner.indexed != demand.indexed || !table.actions.refuse_query(&owner) {
            return false;
        }
        self.host
            .reserve_indexed_layout(&owner, table.viewport.charge() + table.actions.charge());
        true
    }
}
