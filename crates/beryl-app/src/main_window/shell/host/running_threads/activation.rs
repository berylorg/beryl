use super::*;
use crate::main_window::running_threads::activation::{
    RunningThreadActivation, RunningThreadActivationPreparation,
};

mod attachment;
mod navigation_history;
mod ordinary_intake;
#[cfg(all(test, feature = "test-faults"))]
pub(super) use attachment::RunningActivationFixtureHooks;
pub(super) use attachment::UnviewedRunningActivation;
pub(in crate::main_window::shell::host) use attachment::{
    CapturedClaimOperation, RetiringClaimOperation,
};
pub(super) use navigation_history::ThreadNavigationHistory;
pub(crate) use ordinary_intake::OrdinaryThreadActivationAcceptance;

impl MainWindowShellRoot {
    pub(super) fn begin_running_activation(
        &mut self,
        target: ProcessWorkQueryRecord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running_threads.pending_activation.is_some()
            || self.running_threads.activation_operation.is_some()
        {
            return;
        }
        let Some(reader) = self.running_threads.reader.clone() else {
            return;
        };
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let invoking = controller.window_id();
        let prior = self
            .cached_running_selection(cx)
            .map(|(selection, _)| selection);
        let generation = self.running_threads.generation.load(Ordering::Acquire);
        let thread = target.thread_id;
        self.running_threads.activation_attention = target
            .attention
            .iter()
            .map(|record| record.token().clone())
            .collect();
        self.running_threads.pending_activation = Some(thread);
        self.running_threads.activation_cancel = beryl_home_store::CommandCancellation::new();
        let cancel = self.running_threads.activation_cancel.clone();
        let source = reader.clone();
        let work = self.running_threads.workers.track(move || {
            let (home, state, syndic) = source
                .activation_sources()
                .ok_or("Running threads source retired")?;
            let observed = source.observe()?;
            let query =
                beryl_state::CatalogNormalizedQuery::new("").map_err(|error| error.to_string())?;
            let query_cancel = ProjectionCancellationToken::new();
            if cancel.is_cancelled() {
                return Err("Thread activation cancelled".to_owned());
            }
            let first = source
                .query(&query, 0, &query_cancel)
                .map_err(|error| error.to_string())?;
            let position = source
                .position(first.revision(), &query, thread, &query_cancel)
                .map_err(|error| error.to_string())?
                .ok_or("The thread left the running collection.")?;
            let page = if first.records().iter().any(|row| row.thread_id == thread) {
                first
            } else {
                source
                    .page(first.revision(), &query, position, &query_cancel)
                    .map_err(|error| error.to_string())?
            };
            let row = page
                .records()
                .iter()
                .find(|row| row.thread_id == thread)
                .cloned()
                .ok_or("Running thread source changed")?;
            let execution = row.catalog.execution();
            let target =
                beryl_state::RememberedTarget::new(execution.runtime_id(), execution.root_id());
            let prepared = RunningThreadActivation::prepare(
                &home,
                &state,
                &syndic,
                invoking,
                prior.map(|prior| prior.claim()),
                target,
                thread,
            )
            .map_err(|error| error.to_string())?;
            source.elect(&observed, || ())?;
            Ok::<_, String>((prepared, observed, row))
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.running_threads.activation_wake.take();
        self.running_threads.activation_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                if root.running_threads.generation.load(Ordering::Acquire) != generation {
                    return;
                }
                root.running_threads.activation_task = None;
                if root.running_threads.activation_cancel.is_cancelled()
                    || !root.running_threads_enabled()
                    || !reader.current()
                    || root
                        .cached_running_selection(cx)
                        .map(|(selection, _)| selection)
                        != prior
                {
                    root.finish_running_activation(
                        Err("The invoking view changed.".into()),
                        window,
                        cx,
                    );
                    return;
                }
                let result = result.and_then(|(prepared, observed, row)| {
                    reader.elect(&observed, || match prepared {
                        RunningThreadActivationPreparation::Current { claim, .. } => {
                            if !root.matches_running_claim(claim, cx) {
                                return Err("The selected thread changed.".into());
                            }
                            root.acknowledge_running_activation(&reader);
                            root.dismiss_running_picker(window, cx);
                            window.activate_window();
                            Ok(true)
                        }
                        RunningThreadActivationPreparation::ClaimedElsewhere { claim } => {
                            #[cfg(target_os = "windows")]
                            {
                                let owner =
                                    crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                                        .and_then(|owner| owner.upgrade());
                                if let Some(owner) = owner {
                                    crate::running_owner::RunningProcessOwner::reveal_running_claim(&owner, claim, cx)?;
                                } else {
                                    #[cfg(all(test, feature = "test-faults"))]
                                    if root.running_threads.fixture_reader.is_some() {
                                        crate::running_owner::RunningProcessOwner::reveal_running_claim_in_windows(&root.running_threads.fixture_windows, claim, cx)?;
                                    } else { return Err("The original window owner is unavailable.".into()); }
                                    #[cfg(not(all(test, feature = "test-faults")))]
                                    return Err("The original window owner is unavailable.".into());
                                }
                                root.dismiss_running_picker(window, cx);
                                root.acknowledge_running_activation(&reader);
                                Ok(true)
                            }
                            #[cfg(not(target_os = "windows"))]
                            {
                                let _ = claim;
                                Err("The original window owner is unavailable.".into())
                            }
                        }
                        RunningThreadActivationPreparation::Prepared(prepared) => {
                            let _ = row;
                            root.running_threads.pending_activation = Some(thread);
                            root.running_threads.prepared_activation = Some(prepared);
                            Ok(false)
                        }
                    })?
                });
                match result {
                    Ok(true) => root.finish_running_activation(Ok(()), window, cx),
                    Ok(false) => {
                        if let Err(error) =
                            root.start_unviewed_running_activation(reader.clone(), window, cx)
                        {
                            root.finish_running_activation(Err(error), window, cx);
                        }
                    }
                    Err(error) => root.finish_running_activation(Err(error), window, cx),
                }
            });
        }));
        cx.notify();
    }

    fn acknowledge_running_activation(&self, reader: &PublishedRunningThreadsReader) {
        for token in &self.running_threads.activation_attention {
            reader.acknowledge(token);
        }
    }

    fn dismiss_running_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self.running_threads.picker.clone() {
            picker.update(cx, |picker, picker_cx| picker.dismiss(window, picker_cx));
        }
    }

    fn finish_running_activation(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running_threads.activation_operation.is_some() {
            if let Err(error) = result {
                self.retain_running_activation_failure(error, window, cx);
            }
            return;
        }
        self.running_threads.pending_activation = None;
        self.running_threads.ordinary_activation = false;
        self.running_threads.prepared_activation = None;
        self.running_threads.activation_attention.clear();
        self.running_threads.selection_lease = None;
        if let Err(error) = result {
            self.running_threads.navigation_history.cancel();
            let previous = self.running_threads.failure_notice.take();
            self.running_threads.failure_notice =
                self.publish_running_activation_failure(previous, &error);
            self.running_threads.failure = Some(error);
            self.sync_notices(window, cx);
        } else {
            self.settle_thread_navigation_history(cx);
            let previous = self.running_threads.failure_notice.take();
            self.remove_running_activation_failure(previous);
            self.running_threads.failure = None;
            self.sync_notices(window, cx);
        }
        if let Some(picker) = self.running_threads.picker.clone() {
            picker.update(cx, |picker, cx| picker.finish_activation(cx));
        }
        cx.notify();
    }

    fn retain_running_activation_failure(
        &mut self,
        error: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime_setup.primary_command
            || matches!(self.runtime_setup.command, Some(PickerCommand::Confirm(_)))
        {
            let terminal = self
                .running_threads
                .activation_operation
                .as_ref()
                .is_some_and(|operation| operation.is_unavailable());
            if terminal {
                self.runtime_setup.unavailable = Some(error.clone());
            }
            self.setup_command_state("Confirm", terminal, cx);
        }
        if self.running_threads.failure.as_ref() != Some(&error) {
            let previous = self.running_threads.failure_notice.take();
            self.running_threads.failure_notice =
                self.publish_running_activation_failure(previous, &error);
            self.running_threads.failure = Some(error);
            self.sync_notices(window, cx);
        }
        cx.notify();
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/running_threads_attachment.rs"
    ));
}
