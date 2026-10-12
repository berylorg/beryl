use super::super::provider::resource_index as index;
use super::super::{IndexedResourceQuery, IndexedResourceValue};
use super::SyndicTranscriptPanel;
use super::code_panel::save::{
    SAVE_STATE_BYTES, SaveOperation, SaveResult, SaveWriterOptions, TextResourceSave,
};
use crate::main_window::WorkerLease;
use futures_channel::oneshot;
use gpui::{Context, Window};

impl SyndicTranscriptPanel {
    pub(super) fn begin_table_save(
        &mut self,
        id: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_table_save_range(id, None, cx);
    }
    pub(super) fn begin_table_row_save(
        &mut self,
        id: u64,
        row: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_table_save_range(id, Some(row), cx);
    }
    fn begin_table_save_range(&mut self, id: u64, row: Option<u64>, cx: &mut Context<Self>) {
        if self
            .table_panels
            .iter()
            .any(|table| table.id == id && table.viewport.owner.inline().is_some())
        {
            self.begin_inline_table_save(id, row, cx);
            return;
        }
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        if self.staged.is_some() || table.actions.pending() {
            table.actions.feedback = "Save unavailable: source action or presentation pending";
            cx.notify();
            return;
        }
        let Some(owner) = self.host.indexed_resource_owner(
            &table
                .viewport
                .owner
                .indexed()
                .expect("indexed Table action")
                .resource_id,
        ) else {
            table.actions.feedback = "Save unavailable: retired source";
            cx.notify();
            return;
        };
        let (Some(binding), Some(root)) = (&table.viewport.binding, &table.viewport.root) else {
            table.actions.feedback = "Save unavailable: source metadata loading";
            cx.notify();
            return;
        };
        let range = if let Some(row) = row {
            let Some(range) = table
                .viewport
                .fallback_rows()
                .into_iter()
                .find(|(id, _)| id.role == index::RowRole::Body && id.row == row)
                .and_then(|(_, descriptor)| descriptor.raw)
            else {
                table.actions.feedback = "Save unavailable: raw row missing";
                cx.notify();
                return;
            };
            range
        } else {
            binding.range
        };
        if !self.host.can_reserve_indexed_layout(
            &owner,
            table.viewport.charge() + table.actions.charge() + SAVE_STATE_BYTES,
        ) {
            table.actions.feedback = "Save unavailable: capacity";
            cx.notify();
            return;
        }
        let Some(lease) = table.actions.save_admission.acquire() else {
            table.actions.feedback = "Save unavailable: worker capacity";
            cx.notify();
            return;
        };
        let Some(sequence) = table.actions.sequence.checked_add(1) else {
            return;
        };
        let Some(save) = TextResourceSave::new_exact(
            &owner,
            binding.clone(),
            root.digest,
            table.viewport.revision,
            sequence,
            range,
        ) else {
            return;
        };
        table.actions.binding = Some(binding.clone());
        table.actions.root = Some(root.digest);
        table.actions.revision = Some(table.viewport.revision);
        table.actions.sequence = sequence;
        table.actions.save = Some(save);
        table.actions.feedback = "Choose a Save destination…";
        self.host
            .reserve_indexed_layout(&owner, table.viewport.charge() + table.actions.charge());
        #[cfg(test)]
        if let Some(choice) = table.actions.save_choice.take() {
            cx.spawn(async move |weak, cx| {
                let choice = choice.await.unwrap_or(Err(()));
                let _ = weak.update(cx, |panel, cx| {
                    panel.table_save_choice(id, sequence, choice, lease, cx)
                });
            })
            .detach();
            cx.notify();
            return;
        }
        let picker = cx.prompt_for_save_path(std::path::Path::new(""), Some("table.md"));
        cx.spawn(async move |weak, cx| {
            let choice = picker
                .await
                .map_err(|_| ())
                .and_then(|choice| choice.map_err(|_| ()));
            let _ = weak.update(cx, |panel, cx| {
                panel.table_save_choice(id, sequence, choice, lease, cx)
            });
        })
        .detach();
        cx.notify();
    }
    fn table_save_choice(
        &mut self,
        id: u64,
        sequence: u64,
        choice: Result<Option<gpui::SavePathChoice>, ()>,
        lease: WorkerLease,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        let Some(save) = table
            .actions
            .save
            .as_ref()
            .filter(|save| save.sequence == sequence)
        else {
            return;
        };
        let owner = self.host.indexed_resource_owner(
            &table
                .viewport
                .owner
                .indexed()
                .expect("indexed Table action")
                .resource_id,
        );
        if owner.as_ref().is_none_or(|owner| {
            !save.current_exact(
                owner,
                table.actions.binding.as_ref(),
                table.actions.root,
                table.actions.revision,
                table.actions.sequence,
            )
        }) {
            table
                .actions
                .cancel_save("Save unavailable: source changed");
        } else {
            match choice {
                Ok(Some(path)) => {
                    let save = table.actions.save.as_mut().unwrap();
                    let terminal = save.attach_writer(
                        cx.background_executor(),
                        SaveWriterOptions {
                            lease: Some(lease),
                            ..Default::default()
                        },
                    );
                    cx.spawn(async move |weak, cx| {
                        if terminal.await.is_ok() {
                            let _ = weak.update(cx, |_, cx| cx.notify());
                        }
                    })
                    .detach();
                    if let Some(result) = save.work(SaveOperation::Open(path)) {
                        table.actions.feedback = "Saving…";
                        self.await_table_save(id, sequence, result, cx);
                    }
                }
                Ok(None) => table.actions.cancel_save(""),
                Err(()) => table
                    .actions
                    .cancel_save("Save unavailable: native picker failed"),
            }
        }
        self.refresh_table_source_reservation(id);
        cx.notify();
    }
    pub(super) fn refresh_table_source_reservation(&mut self, id: u64) {
        if let Some(table) = self.table_panels.iter().find(|table| table.id == id) {
            let bytes = table.viewport.charge() + table.actions.charge();
            match &table.viewport.owner {
                super::table_owner::TableOwner::Inline(owner) => {
                    self.host.reserve_inline_table_layout(owner, bytes);
                }
                super::table_owner::TableOwner::Indexed(owner) => {
                    if let Some(owner) = self.host.indexed_resource_owner(&owner.resource_id) {
                        self.host.reserve_indexed_layout(&owner, bytes);
                    }
                }
            }
        }
    }
    fn await_table_save(
        &mut self,
        id: u64,
        sequence: u64,
        result: oneshot::Receiver<SaveResult>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            let result = result
                .await
                .unwrap_or(SaveResult::Failed("Save unavailable: writer retired"));
            let _ = weak.update(cx, |panel, cx| {
                panel.finish_table_save_work(id, sequence, result, cx)
            });
        })
        .detach();
    }
    fn finish_table_save_work(
        &mut self,
        id: u64,
        sequence: u64,
        result: SaveResult,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        let Some(save) = table
            .actions
            .save
            .as_ref()
            .filter(|save| save.sequence == sequence)
        else {
            return;
        };
        let owner = self.host.indexed_resource_owner(
            &table
                .viewport
                .owner
                .indexed()
                .expect("indexed Table action")
                .resource_id,
        );
        if result == SaveResult::Published {
            table.actions.cancel_save("Saved");
        } else if let SaveResult::Failed(reason) = result {
            table.actions.cancel_save(reason);
        } else if owner.as_ref().is_none_or(|owner| {
            !save.current_exact(
                owner,
                table.actions.binding.as_ref(),
                table.actions.root,
                table.actions.revision,
                table.actions.sequence,
            )
        }) {
            table
                .actions
                .cancel_save("Save unavailable: source changed");
        } else {
            table.actions.save.as_mut().unwrap().acknowledged(result);
            if self.staged.is_none() {
                self.advance_table_save(id, cx);
                cx.emit(super::ResourceViewportDemandChanged);
            }
        }
        self.refresh_table_source_reservation(id);
        cx.notify();
    }
    pub(super) fn advance_table_save(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return false;
        };
        let Some(save) = &table.actions.save else {
            return false;
        };
        let Some(owner) = self.host.indexed_resource_owner(
            &table
                .viewport
                .owner
                .indexed()
                .expect("indexed Table action")
                .resource_id,
        ) else {
            table
                .actions
                .cancel_save("Save unavailable: retired source");
            return false;
        };
        if !save.current_exact(
            &owner,
            table.actions.binding.as_ref(),
            table.actions.root,
            table.actions.revision,
            table.actions.sequence,
        ) {
            table
                .actions
                .cancel_save("Save unavailable: source changed");
            return false;
        }
        if save.busy {
            return true;
        }
        let save = table.actions.save.as_mut().unwrap();
        let sequence = save.sequence;
        if let Some(result) = save.next_work() {
            self.await_table_save(id, sequence, result, cx);
            return true;
        }
        if let Some(query) = table.actions.query() {
            if std::mem::take(&mut table.actions.save.as_mut().unwrap().retry_refusal) {
                if super::table_source_actions::clear_table_source_refusal(
                    &mut self.rejected_resource,
                    &owner,
                    &query,
                ) {
                    cx.emit(super::ResourceViewportDemandChanged);
                }
            }
            if owner.indexed.as_ref() == Some(&query) {
                if let Some(response) = self.host.indexed_resource_response(&owner.resource_id)
                    && let (
                        IndexedResourceQuery::Source { range, .. },
                        IndexedResourceValue::Source(page),
                    ) = (&query.query, &response.value)
                {
                    let save = table.actions.save.as_mut().unwrap();
                    if page.binding != save.binding
                        || page.range != *range
                        || page.bytes.len() as u64 != range.end - range.start
                        || response
                            .root
                            .as_ref()
                            .is_none_or(|root| root.digest != save.root)
                    {
                        table
                            .actions
                            .cancel_save("Save unavailable: source changed");
                    } else if let Some(result) = save.work(SaveOperation::Write(page.bytes.clone()))
                    {
                        self.await_table_save(id, sequence, result, cx);
                    }
                }
            } else if self.host.set_indexed_resource_demand(&owner, Some(query)) {
                cx.emit(super::ResourceViewportDemandChanged);
                cx.notify();
            }
        }
        true
    }
}
