use super::super::core::InlineTableOwner;
use super::super::provider::resource_index as index;
use super::code_panel::save::{
    ACTIVE, CANCELLED, PUBLISHING, SAVE_STATE_BYTES, SaveOperation, SaveResult, SaveWork,
    SaveWriterOptions, run_save_writer,
};
use super::{
    SyndicTranscriptPanel,
    copy_keyboard::CopyMethod,
    table_export::{COPY_CHARGE, TableExport},
    table_viewport::{TableCellId, TableSelection},
};
use futures_channel::oneshot;
use gpui::Context;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[derive(Default)]
pub(super) struct InlineTableActions {
    pub copy: Option<InlineCopy>,
    pub save: Option<InlineSave>,
}
impl InlineTableActions {
    pub fn charge(&self) -> usize {
        self.copy.as_ref().map_or(0, |_| COPY_CHARGE)
            + self.save.as_ref().map_or(0, |_| SAVE_STATE_BYTES)
    }
}
pub(super) struct InlineCopy {
    owner: InlineTableOwner,
    selection: Option<TableSelection>,
    row: Option<TableCellId>,
    sequence: u64,
    _task: gpui::Task<()>,
}
pub(super) struct InlineSave {
    owner: InlineTableOwner,
    sequence: u64,
    range: index::ByteRange,
    position: usize,
    prepared: bool,
    busy: bool,
    gate: Arc<AtomicU8>,
    sender: Option<oneshot::Sender<SaveWork>>,
}
impl Drop for InlineSave {
    fn drop(&mut self) {
        let _ = self
            .gate
            .compare_exchange(ACTIVE, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
    }
}
impl InlineSave {
    fn work(&mut self, operation: SaveOperation) -> Option<oneshot::Receiver<SaveResult>> {
        let sender = self.sender.take()?;
        let (next_sender, next) = oneshot::channel();
        let (reply, result) = oneshot::channel();
        sender
            .send(SaveWork {
                operation,
                next,
                reply,
            })
            .ok()?;
        self.sender = Some(next_sender);
        self.busy = true;
        Some(result)
    }
}

impl SyndicTranscriptPanel {
    pub(super) fn begin_inline_table_copy(
        &mut self,
        id: u64,
        row: Option<TableCellId>,
        method: CopyMethod,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        let Some(owner) = table.viewport.owner.inline().cloned() else {
            return;
        };
        if self.staged.is_some()
            || !self.host.inline_table_current(&owner)
            || table.actions.inline.save.is_some()
        {
            table.actions.feedback = "Copy unavailable: source or presentation changing";
            cx.notify();
            return;
        }
        let export = if let Some(row) = row {
            table
                .viewport
                .fallback_rows()
                .into_iter()
                .find(|(id, _)| *id == row)
                .and_then(|(_, descriptor)| descriptor.raw)
                .ok_or("Copy unavailable: raw row missing")
                .and_then(TableExport::raw)
        } else {
            table
                .viewport
                .selection_cells()
                .ok_or("Copy unavailable: no eligible cell selection")
                .and_then(|cells| {
                    TableExport::inline(
                        cells,
                        &owner.prepared,
                        table.viewport.whole_table_selection(),
                    )
                })
        };
        let export = match export {
            Ok(export) => export,
            Err(reason) => {
                table.actions.feedback = reason;
                cx.notify();
                return;
            }
        };
        if !self
            .host
            .reserve_inline_table_layout(&owner, table.viewport.charge() + 4096 + COPY_CHARGE)
        {
            table.actions.feedback = "Copy unavailable: capacity";
            cx.notify();
            return;
        }
        let bytes = match export.assemble_inline(owner.prepared.source()) {
            Ok(bytes) => bytes,
            Err(reason) => {
                table.actions.feedback = reason;
                self.host.reserve_inline_table_layout(
                    &owner,
                    table.viewport.charge() + table.actions.charge(),
                );
                cx.notify();
                return;
            }
        };
        let Some(lease) = table.actions.save_admission.acquire() else {
            table.actions.feedback = "Copy unavailable: worker capacity";
            self.host.reserve_inline_table_layout(
                &owner,
                table.viewport.charge() + table.actions.charge(),
            );
            cx.notify();
            return;
        };
        let Some(sequence) = table.actions.sequence.checked_add(1) else {
            return;
        };
        table.actions.sequence = sequence;
        let task = cx.background_executor().spawn(async move {
            let result = export.render(bytes, method);
            drop(lease);
            result
        });
        let task = cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |panel, cx| {
                panel.finish_inline_table_copy(id, sequence, result, cx)
            });
        });
        table.actions.inline.copy = Some(InlineCopy {
            owner,
            selection: table.viewport.selection.clone(),
            row,
            sequence,
            _task: task,
        });
        table.actions.feedback = "Copying…";
        cx.notify();
    }

    fn finish_inline_table_copy(
        &mut self,
        id: u64,
        sequence: u64,
        result: Result<String, &'static str>,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        let Some(copy) = table
            .actions
            .inline
            .copy
            .as_ref()
            .filter(|copy| copy.sequence == sequence)
        else {
            return;
        };
        let owner = copy.owner.clone();
        let current = self.staged.is_none()
            && self.host.inline_table_current(&owner)
            && table.viewport.owner.inline() == Some(&owner)
            && copy.selection == table.viewport.selection
            && table.actions.sequence == sequence
            && copy.row.is_none_or(|row| table.viewport.fallback_row(row));
        let result = if current {
            result.and_then(|text| {
                super::table_source_actions::publish_copy(&mut table.actions, text, cx)
            })
        } else {
            Err("Copy unavailable: selection or source changed")
        };
        table.actions.cancel_copy(result.err().unwrap_or("Copied"));
        self.host
            .reserve_inline_table_layout(&owner, table.viewport.charge() + table.actions.charge());
        cx.notify();
    }

    pub(super) fn begin_inline_table_save(
        &mut self,
        id: u64,
        row: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        let Some(owner) = table.viewport.owner.inline().cloned() else {
            return;
        };
        if self.staged.is_some()
            || table.actions.pending()
            || !self.host.inline_table_current(&owner)
        {
            table.actions.feedback = "Save unavailable: source action or presentation pending";
            cx.notify();
            return;
        }
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
            index::ByteRange {
                start: 0,
                end: owner.prepared.source().len() as u64,
            }
        };
        if !self.host.reserve_inline_table_layout(
            &owner,
            table.viewport.charge() + table.actions.charge() + SAVE_STATE_BYTES,
        ) {
            table.actions.feedback = "Save unavailable: capacity";
            cx.notify();
            return;
        }
        let Some(lease) = table.actions.save_admission.acquire() else {
            table.actions.feedback = "Save unavailable: worker capacity";
            self.host.reserve_inline_table_layout(
                &owner,
                table.viewport.charge() + table.actions.charge(),
            );
            cx.notify();
            return;
        };
        let Some(sequence) = table.actions.sequence.checked_add(1) else {
            return;
        };
        table.actions.sequence = sequence;
        table.actions.inline.save = Some(InlineSave {
            owner,
            sequence,
            range,
            position: range.start as usize,
            prepared: false,
            busy: true,
            gate: Arc::new(AtomicU8::new(ACTIVE)),
            sender: None,
        });
        table.actions.feedback = "Choose a Save destination…";
        #[cfg(test)]
        if let Some(choice) = table.actions.save_choice.take() {
            cx.spawn(async move |weak, cx| {
                let choice = choice.await.unwrap_or(Err(()));
                let _ = weak.update(cx, |panel, cx| {
                    panel.inline_table_save_choice(id, sequence, choice, lease, cx)
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
                panel.inline_table_save_choice(id, sequence, choice, lease, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn inline_table_save_choice(
        &mut self,
        id: u64,
        sequence: u64,
        choice: Result<Option<gpui::SavePathChoice>, ()>,
        lease: crate::main_window::WorkerLease,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return;
        };
        let Some(save) = table
            .actions
            .inline
            .save
            .as_mut()
            .filter(|save| save.sequence == sequence)
        else {
            return;
        };
        let owner = save.owner.clone();
        if self.staged.is_some()
            || !self.host.inline_table_current(&owner)
            || table.viewport.owner.inline() != Some(&owner)
        {
            table
                .actions
                .cancel_save("Save unavailable: source changed");
        } else {
            match choice {
                Ok(Some(path)) => {
                    let (sender, receiver) = oneshot::channel();
                    save.sender = Some(sender);
                    let gate = save.gate.clone();
                    cx.background_executor()
                        .spawn(async move {
                            run_save_writer(
                                receiver,
                                gate,
                                SaveWriterOptions {
                                    lease: Some(lease),
                                    ..Default::default()
                                },
                            )
                            .await;
                        })
                        .detach();
                    if let Some(result) = save.work(SaveOperation::Open(path)) {
                        table.actions.feedback = "Saving…";
                        self.await_inline_table_save(id, sequence, result, cx);
                    }
                }
                Ok(None) => table.actions.cancel_save(""),
                Err(()) => table
                    .actions
                    .cancel_save("Save unavailable: native picker failed"),
            }
        }
        self.refresh_inline_table_reservation(id);
        cx.notify();
    }

    fn await_inline_table_save(
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
                panel.finish_inline_table_save(id, sequence, result, cx)
            });
        })
        .detach();
    }

    fn finish_inline_table_save(
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
            .inline
            .save
            .as_mut()
            .filter(|save| save.sequence == sequence)
        else {
            return;
        };
        if result == SaveResult::Published {
            table.actions.cancel_save("Saved");
        } else if let SaveResult::Failed(reason) = result {
            table.actions.cancel_save(reason);
        } else if !self.host.inline_table_current(&save.owner)
            || table.viewport.owner.inline() != Some(&save.owner)
        {
            table
                .actions
                .cancel_save("Save unavailable: source changed");
        } else {
            save.busy = false;
            match result {
                SaveResult::Wrote(bytes) => save.position += bytes,
                SaveResult::Prepared => save.prepared = true,
                _ => {}
            }
            if self.staged.is_none() {
                self.advance_inline_table_save(id, cx);
            }
        }
        self.refresh_inline_table_reservation(id);
        cx.notify();
    }

    pub(super) fn advance_inline_table_save(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        if self.staged.is_some() {
            return false;
        }
        let Some(table) = self.table_panels.iter_mut().find(|table| table.id == id) else {
            return false;
        };
        let Some(save) = table.actions.inline.save.as_mut() else {
            return false;
        };
        if !self.host.inline_table_current(&save.owner)
            || table.viewport.owner.inline() != Some(&save.owner)
        {
            table
                .actions
                .cancel_save("Save unavailable: source changed");
            self.refresh_inline_table_reservation(id);
            return false;
        }
        if save.busy {
            return true;
        }
        let operation = if save.position < save.range.end as usize {
            let end = (save.position + 8192).min(save.range.end as usize);
            SaveOperation::Write(
                save.owner.prepared.source().as_bytes()[save.position..end].to_vec(),
            )
        } else if !save.prepared {
            SaveOperation::Prepare
        } else {
            if save
                .gate
                .compare_exchange(ACTIVE, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return false;
            }
            SaveOperation::Publish
        };
        let sequence = save.sequence;
        if let Some(result) = save.work(operation) {
            self.await_inline_table_save(id, sequence, result, cx);
        }
        true
    }

    fn refresh_inline_table_reservation(&mut self, id: u64) {
        if let Some(table) = self.table_panels.iter().find(|table| table.id == id)
            && let Some(owner) = table.viewport.owner.inline()
        {
            self.host.reserve_inline_table_layout(
                owner,
                table.viewport.charge() + table.actions.charge(),
            );
        }
    }
}
