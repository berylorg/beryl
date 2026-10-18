use super::source_actions::{CodeSourceActions, identity, identity_matches, owner_bytes};
use super::*;
use crate::main_window::{WorkerAdmission, WorkerLease};
use crate::syndic_transcript::{
    IndexedResourceDemand, IndexedResourceQuery, ResourceTranscriptDemand,
};
use futures_channel::oneshot;
use std::sync::{Arc, atomic::AtomicU8};

pub(in super::super) const SAVE_STATE_BYTES: usize = 256 * 1024 + 8192 + 8192;
pub(in super::super) const ACTIVE: u8 = 0;
pub(in super::super) const CANCELLED: u8 = 1;
pub(in super::super) const PUBLISHING: u8 = 2;

pub(in super::super) struct TextResourceSave {
    owner: ResourceTranscriptDemand,
    pub(in super::super) binding: index::Binding,
    pub(in super::super) root: [u8; 32],
    pub(in super::super) revision: beryl_model::ProjectionRevision,
    pub(in super::super) sequence: u64,
    range: index::ByteRange,
    pub(in super::super) position: u64,
    pub(in super::super) busy: bool,
    prepared: bool,
    pub(in super::super) retry_refusal: bool,
    gate: Arc<AtomicU8>,
    sender: Option<oneshot::Sender<SaveWork>>,
}

impl Drop for TextResourceSave {
    fn drop(&mut self) {
        let _ = self
            .gate
            .compare_exchange(ACTIVE, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
    }
}

#[path = "save_writer.rs"]
mod writer;
pub(in super::super) use writer::run as run_save_writer;
#[cfg(test)]
pub(in super::super) use writer::{SaveFault, SaveWriterGate};
pub(in super::super) use writer::{SaveOperation, SaveResult, SaveWork, SaveWriterOptions};
impl TextResourceSave {
    pub(in super::super) fn new(
        actions: &mut CodeSourceActions,
        owner: &ResourceTranscriptDemand,
    ) -> Option<Self> {
        if owner_bytes(owner) > 4096 {
            return None;
        }
        let sequence = actions.next_sequence()?;
        let binding = actions.binding.clone()?;
        Self::new_exact(
            owner,
            binding.clone(),
            actions.root?,
            actions.save_revision()?,
            sequence,
            binding.range,
        )
    }
    pub(in super::super) fn new_exact(
        owner: &ResourceTranscriptDemand,
        binding: index::Binding,
        root: [u8; 32],
        revision: beryl_model::ProjectionRevision,
        sequence: u64,
        range: index::ByteRange,
    ) -> Option<Self> {
        if owner_bytes(owner) > 4096
            || range.start < binding.range.start
            || range.end > binding.range.end
            || range.end < range.start
        {
            return None;
        }
        Some(Self {
            owner: identity(owner),
            position: range.start,
            binding,
            root,
            revision,
            sequence,
            range,
            busy: true,
            prepared: false,
            retry_refusal: true,
            gate: Arc::new(AtomicU8::new(ACTIVE)),
            sender: None,
        })
    }
    pub(in super::super) fn attach_writer(
        &mut self,
        executor: &gpui::BackgroundExecutor,
        options: SaveWriterOptions,
    ) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        let (terminal, completion) = oneshot::channel();
        self.sender = Some(sender);
        let gate = self.gate.clone();
        executor
            .spawn(async move {
                writer::run(receiver, gate, options).await;
                let _ = terminal.send(());
            })
            .detach();
        completion
    }
    pub(in super::super) fn elect(&self) -> bool {
        self.gate
            .compare_exchange(ACTIVE, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub(in super::super) fn current(
        &self,
        actions: &CodeSourceActions,
        owner: &ResourceTranscriptDemand,
    ) -> bool {
        let current = identity_matches(&self.owner, owner)
            && actions.binding.as_ref() == Some(&self.binding)
            && actions.root == Some(self.root)
            && actions.save_revision() == Some(self.revision)
            && actions.action_sequence() == self.sequence;
        #[cfg(test)]
        if !current {
            eprintln!(
                "Code Save custody refusal: owner={} binding={} root={} revision={} action={}",
                identity_matches(&self.owner, owner),
                actions.binding.as_ref() == Some(&self.binding),
                actions.root == Some(self.root),
                actions.save_revision() == Some(self.revision),
                actions.action_sequence() == self.sequence
            );
        }
        current
    }
    pub(in super::super) fn current_exact(
        &self,
        owner: &ResourceTranscriptDemand,
        binding: Option<&index::Binding>,
        root: Option<[u8; 32]>,
        revision: Option<beryl_model::ProjectionRevision>,
        sequence: u64,
    ) -> bool {
        identity_matches(&self.owner, owner)
            && binding == Some(&self.binding)
            && root == Some(self.root)
            && revision == Some(self.revision)
            && sequence == self.sequence
    }
    pub(in super::super) fn query(
        &self,
        actions: &CodeSourceActions,
    ) -> Option<IndexedResourceDemand> {
        self.query_exact(actions.binding.as_ref())
    }
    pub(in super::super) fn query_exact(
        &self,
        binding: Option<&index::Binding>,
    ) -> Option<IndexedResourceDemand> {
        if self.busy
            || self.sender.is_none()
            || self.position >= self.range.end
            || binding != Some(&self.binding)
        {
            return None;
        }
        Some(IndexedResourceDemand {
            revision: self.revision,
            root: Some(self.root),
            query: IndexedResourceQuery::Source {
                range: index::ByteRange {
                    start: self.position,
                    end: self.range.end.min(self.position.checked_add(8192)?),
                },
                bytes: 16384,
            },
        })
    }
    pub(in super::super) fn acknowledged(&mut self, result: SaveResult) {
        self.busy = false;
        match result {
            SaveResult::Wrote(bytes) => {
                self.position += bytes as u64;
                self.retry_refusal = true;
            }
            SaveResult::Prepared => self.prepared = true,
            _ => {}
        }
    }
    pub(in super::super) fn next_work(&mut self) -> Option<oneshot::Receiver<SaveResult>> {
        if self.busy || self.position != self.range.end {
            return None;
        }
        if !self.prepared {
            return self.work(SaveOperation::Prepare);
        }
        if !self.elect() {
            return None;
        }
        self.work(SaveOperation::Publish)
    }
    pub(in super::super) fn work(
        &mut self,
        operation: SaveOperation,
    ) -> Option<oneshot::Receiver<SaveResult>> {
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

pub(super) fn unavailable(
    code: &MountedCodePanel,
    host: &SyndicTranscriptHost,
    pending: bool,
) -> Option<&'static str> {
    if !cfg!(target_os = "windows") {
        return Some("Native file saving is unavailable on this platform");
    }
    if pending {
        return Some("Waiting for the current Code presentation");
    }
    if !code.actions.save_admission.available() {
        return Some(match code.actions.save_admission.suspended() {
            None => "The window source owner is no longer available",
            Some(true) => "Code Save is suspended while the window closes",
            Some(false) => "Code Save is waiting for window resource capacity",
        });
    }
    if code.actions.pending() {
        return Some("A Code source action is in progress");
    }
    if code.viewport.remapping {
        return Some("Code source positions are being reconciled");
    }
    let Some(owner) = host.indexed_resource_owner(&code.viewport.owner.resource_id) else {
        return Some("The Code source owner is no longer available");
    };
    let Some(binding) = &code.actions.binding else {
        return Some("Code source metadata is loading");
    };
    if code.actions.root.is_none()
        || code.actions.save_revision() != Some(binding.metadata.revision())
    {
        return Some("Waiting for the current Code source binding");
    }
    if !host.can_reserve_code_layout(
        &owner,
        code.layout
            .charge()
            .saturating_add(code.actions.charge())
            .saturating_add(SAVE_STATE_BYTES),
    ) {
        return Some("Code Save exceeds available resource capacity");
    }
    None
}

impl SyndicTranscriptPanel {
    pub(crate) fn set_code_save_admission(&mut self, admission: WorkerAdmission) {
        self.save_admission = admission;
        for code in &mut self.code_panels {
            code.actions.save_admission = self.save_admission.clone();
        }
        for table in &mut self.table_panels {
            table.actions.save_admission = self.save_admission.clone();
        }
    }

    pub(crate) fn fence_code_saves(&mut self, cx: &mut Context<Self>) {
        for code in &mut self.code_panels {
            code.actions.cancel_save("");
        }
        for table in &mut self.table_panels {
            table.actions.cancel("");
        }
        for position in 0..self.code_panels.len() {
            self.refresh_save_reservation(self.code_panels[position].id);
        }
        for position in 0..self.table_panels.len() {
            self.refresh_table_source_reservation(self.table_panels[position].id);
        }
        cx.notify();
    }

    pub(super) fn begin_code_save(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.initial_history_loading {
            return;
        }
        let Some(code) = self
            .code_panels
            .iter_mut()
            .find(|code| code.id == id && !code.overridden)
        else {
            return;
        };
        if let Some(reason) = unavailable(code, &self.host, self.staged.is_some()) {
            code.actions.feedback = reason;
            cx.notify();
            return;
        }
        let owner = self
            .host
            .indexed_resource_owner(&code.viewport.owner.resource_id)
            .unwrap();
        if owner_bytes(&owner) > 4096 {
            code.actions.feedback = "Save unavailable: source identity capacity";
            cx.notify();
            return;
        }
        let Some(save) = TextResourceSave::new(&mut code.actions, &owner) else {
            return;
        };
        let Some(lease) = code.actions.save_admission.acquire() else {
            return;
        };
        let sequence = save.sequence;
        code.actions.save = Some(save);
        self.host.reserve_code_layout(
            &owner,
            code.layout.charge().saturating_add(code.actions.charge()),
        );
        code.actions.feedback = "Choose a Save destination…";
        #[cfg(test)]
        if let Some(choice) = code.actions.save_choice.take() {
            cx.spawn(async move |weak, cx| {
                let choice = choice.await.unwrap_or(Err(()));
                let _ = weak.update(cx, |panel, cx| {
                    panel.code_save_choice(id, sequence, choice, lease, cx)
                });
            })
            .detach();
            cx.notify();
            return;
        }
        let picker = cx.prompt_for_save_path(std::path::Path::new(""), Some("code.txt"));
        cx.spawn(async move |weak, cx| {
            let choice = picker
                .await
                .map_err(|_| ())
                .and_then(|choice| choice.map_err(|_| ()));
            let _ = weak.update(cx, |panel, cx| {
                panel.code_save_choice(id, sequence, choice, lease, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn code_save_choice(
        &mut self,
        id: u64,
        sequence: u64,
        choice: Result<Option<gpui::SavePathChoice>, ()>,
        lease: WorkerLease,
        cx: &mut Context<Self>,
    ) {
        let mut lease = Some(lease);
        let Some(code) = self
            .code_panels
            .iter_mut()
            .find(|code| code.id == id && !code.overridden)
        else {
            return;
        };
        let Some(save) = code
            .actions
            .save
            .as_ref()
            .filter(|save| save.sequence == sequence)
        else {
            return;
        };
        let owner = self
            .host
            .indexed_resource_owner(&code.viewport.owner.resource_id);
        if owner
            .as_ref()
            .is_none_or(|owner| !save.current(&code.actions, owner))
        {
            code.actions.cancel_save("Save unavailable: source changed");
        } else {
            match choice {
                Ok(Some(path)) => {
                    let save = code.actions.save.as_mut().unwrap();
                    #[cfg(not(test))]
                    let options = SaveWriterOptions {
                        lease: lease.take(),
                    };
                    #[cfg(test)]
                    let options = SaveWriterOptions {
                        fault: code.actions.save_fault.take(),
                        lease: lease.take(),
                        hold: code.actions.save_hold.take(),
                    };
                    let terminal = save.attach_writer(cx.background_executor(), options);
                    cx.spawn(async move |weak, cx| {
                        if terminal.await.is_ok() {
                            let _ = weak.update(cx, |_, cx| cx.notify());
                        }
                    })
                    .detach();
                    let result = save.work(SaveOperation::Open(path)).unwrap();
                    code.actions.feedback = "Saving…";
                    self.await_code_save(id, sequence, result, cx);
                }
                Ok(None) => code.actions.cancel_save(""),
                Err(()) => code
                    .actions
                    .cancel_save("Save unavailable: native picker failed"),
            }
        }
        drop(lease);
        self.refresh_save_reservation(id);
        cx.notify();
    }

    fn refresh_save_reservation(&mut self, id: u64) {
        if let Some(code) = self.code_panels.iter().find(|code| code.id == id) {
            if let Some(owner) = self
                .host
                .indexed_resource_owner(&code.viewport.owner.resource_id)
            {
                self.host.reserve_code_layout(
                    &owner,
                    code.layout.charge().saturating_add(code.actions.charge()),
                );
            }
        }
    }

    fn await_code_save(
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
                panel.finish_code_save_work(id, sequence, result, cx)
            });
        })
        .detach();
    }

    fn finish_code_save_work(
        &mut self,
        id: u64,
        sequence: u64,
        result: SaveResult,
        cx: &mut Context<Self>,
    ) {
        let Some(code) = self
            .code_panels
            .iter_mut()
            .find(|code| code.id == id && !code.overridden)
        else {
            return;
        };
        let Some(save) = code
            .actions
            .save
            .as_ref()
            .filter(|save| save.sequence == sequence)
        else {
            return;
        };
        let owner = self
            .host
            .indexed_resource_owner(&code.viewport.owner.resource_id);
        if result == SaveResult::Published {
            code.actions.cancel_save("Saved");
        } else if let SaveResult::Failed(reason) = result {
            code.actions.cancel_save(reason);
        } else if owner
            .as_ref()
            .is_none_or(|owner| !save.current(&code.actions, owner))
        {
            code.actions.cancel_save("Save unavailable: source changed");
        } else {
            let save = code.actions.save.as_mut().unwrap();
            save.busy = false;
            match result {
                SaveResult::Wrote(bytes) => {
                    save.position += bytes as u64;
                    save.retry_refusal = true;
                }
                SaveResult::Prepared => save.prepared = true,
                _ => {}
            }
            if self.staged.is_none() {
                if let Some(result) = Self::next_save_work(save) {
                    self.await_code_save(id, sequence, result, cx);
                } else {
                    self.advance_code_source_actions(id, cx);
                    cx.emit(super::super::ResourceViewportDemandChanged);
                }
            } else {
                #[cfg(test)]
                eprintln!(
                    "Code Save acknowledges work while ordinary Resource publication is staged"
                );
            }
        }
        self.refresh_save_reservation(id);
        cx.notify();
    }

    fn next_save_work(save: &mut TextResourceSave) -> Option<oneshot::Receiver<SaveResult>> {
        save.next_work()
    }

    pub(super) fn advance_code_save(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        let Some(code) = self
            .code_panels
            .iter_mut()
            .find(|code| code.id == id && !code.overridden)
        else {
            return false;
        };
        let Some(save) = &code.actions.save else {
            return false;
        };
        let Some(owner) = self
            .host
            .indexed_resource_owner(&code.viewport.owner.resource_id)
        else {
            code.actions.cancel_save("Save unavailable: retired source");
            return false;
        };
        if !save.current(&code.actions, &owner) {
            code.actions.cancel_save("Save unavailable: source changed");
            return false;
        }
        if save.busy {
            return true;
        }
        let save = code.actions.save.as_mut().unwrap();
        let sequence = save.sequence;
        if let Some(result) = Self::next_save_work(save) {
            self.await_code_save(id, sequence, result, cx);
            return true;
        }
        let query = code.actions.query();
        if let Some(query) = query {
            let retry = std::mem::take(&mut code.actions.save.as_mut().unwrap().retry_refusal);
            if retry
                && super::commands::clear_action_refusal(
                    &mut self.rejected_resource,
                    &owner,
                    &query,
                )
            {
                cx.emit(super::super::ResourceViewportDemandChanged);
            }
            let response = self.host.indexed_resource_response(&owner.resource_id);
            if owner.indexed.as_ref() == Some(&query) {
                if let Some(response) = response {
                    if let (
                        IndexedResourceQuery::Source { range, .. },
                        IndexedResourceValue::Source(page),
                    ) = (&query.query, &response.value)
                    {
                        let save = code.actions.save.as_mut().unwrap();
                        if page.binding != save.binding
                            || page.range != *range
                            || page.bytes.len() as u64 != range.end - range.start
                            || response
                                .root
                                .as_ref()
                                .is_none_or(|root| root.digest != save.root)
                        {
                            code.actions.cancel_save("Save unavailable: source changed");
                        } else {
                            let sequence = save.sequence;
                            let result =
                                save.work(SaveOperation::Write(page.bytes.clone())).unwrap();
                            self.await_code_save(id, sequence, result, cx);
                        }
                    }
                }
            } else if self.host.set_indexed_resource_demand(&owner, Some(query)) {
                cx.emit(super::super::ResourceViewportDemandChanged);
                cx.notify();
            }
        }
        true
    }
}
