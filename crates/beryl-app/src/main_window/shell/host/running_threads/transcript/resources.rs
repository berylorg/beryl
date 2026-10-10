use super::*;
use crate::app_services::{PreparedRunningResource, RunningResourceConstruction};
use crate::transcript_provider::{PreparedResourceDelivery, TranscriptResourceRequest};

pub(in crate::main_window::shell::host::running_threads) struct PendingMeasuredResource {
    panel: Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    provider: crate::transcript_provider::TranscriptProviderReader,
    prepared: PreparedResourceDelivery,
    reader: PublishedRunningThreadsReader,
    selection: crate::main_window::MainWindowComposerSelectionIdentity,
    request: TranscriptResourceRequest,
    cancel: Arc<AtomicBool>,
    retry_pending: bool,
    _construction: Option<RunningResourceConstruction>,
}

impl PendingMeasuredResource {
    pub(in crate::main_window::shell::host::running_threads) fn discard(
        self,
        cx: &mut Context<MainWindowShellRoot>,
    ) {
        self.panel
            .update(cx, |panel, _| panel.discard_staged_activation());
    }
}

impl MainWindowShellRoot {
    pub(super) fn sync_running_transcript_resource(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let panel = self.running_threads.transcript.clone();
        if self
            .running_threads
            .resource_demand
            .as_ref()
            .is_some_and(|demand| {
                panel.read(cx).resource_demand().as_ref() != Some(demand)
                    || self
                        .cached_running_selection(cx)
                        .map(|(selection, _)| selection.claim())
                        != self.running_threads.transcript_claim
            })
        {
            self.running_threads
                .transcript_cancel
                .store(true, Ordering::Release);
        }
        if let Some(pending) = self.running_threads.measured_resource.take() {
            self.running_threads.resource_refresh_due = true;
            let demand = &pending.request.demand;
            if pending.cancel.load(Ordering::Acquire)
                || pending.panel != panel
                || !pending.reader.current()
                || !self.running_threads_enabled()
                || pending.request.attachment.request_id != self.running_threads.transcript_request
                || !self
                    .running_threads
                    .reader
                    .as_ref()
                    .is_some_and(|reader| reader.same_publication(&pending.reader))
                || self
                    .cached_running_selection(cx)
                    .map(|(selection, _)| selection)
                    != Some(pending.selection)
                || self.running_threads.transcript_source.as_ref() != Some(&pending.request.source)
                || !panel.read(cx).resource_candidate_current(demand)
            {
                pending.discard(cx);
                return true;
            }
            let mount = self
                .controller
                .as_ref()
                .and_then(|controller| controller.composer_mount());
            let allocation = self.transcript_attachment_allocation(
                mount.as_ref(),
                pending.request.source.has_parent_lineage(),
                window,
                cx,
            );
            if !panel.update(cx, |panel, _| {
                panel.update_resource_allocation(demand, allocation)
            }) {
                panel.update(cx, |panel, _| panel.discard_staged_activation());
                return true;
            }
            if !panel.read(cx).measurement_current(window) {
                self.running_threads.measured_resource = Some(pending);
                return true;
            }
            if pending
                .provider
                .publish_resource_delivery_if_current(
                    pending.prepared,
                    &pending.request,
                    &pending.cancel,
                    |_| {
                        panel.update(cx, |panel, panel_cx| {
                            panel.publish_resource_candidate(demand);
                            if pending.retry_pending {
                                panel.refresh_pending_indexed_resource(demand);
                            }
                            panel_cx.notify();
                        });
                    },
                )
                .is_err()
            {
                panel.update(cx, |panel, _| panel.discard_staged_activation());
            }
            cx.notify();
            return true;
        }
        if self.running_threads.resource_demand.is_some() {
            return true;
        }
        if self.running_threads.resource_refresh_due {
            self.running_threads.resource_refresh_due = false;
            return false;
        }
        if !self.running_threads_enabled()
            || !self.running_read_capacity_available()
            || self.running_threads.transcript_task.is_some()
            || self.running_threads.measured_attachment.is_some()
            || self.running_threads.has_activation_custody()
            || self.running_threads.recovered_attachment.is_some()
        {
            return false;
        }
        let Some((selection, _)) = self.cached_running_selection(cx) else {
            return false;
        };
        if self.running_threads.transcript_claim != Some(selection.claim()) {
            return false;
        }
        let (Some(provider), Some(source), Some(reader), Some(demand)) = (
            self.running_threads.transcript_provider.clone(),
            self.running_threads.transcript_source.clone(),
            self.running_threads.reader.clone(),
            panel.read(cx).resource_demand(),
        ) else {
            return false;
        };
        let Some(request_id) = self.running_threads.transcript_request.checked_add(1) else {
            return true;
        };
        self.running_threads.transcript_request = request_id;
        self.running_threads.resource_demand = Some(demand.clone());
        self.running_threads.transcript_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.running_threads.transcript_cancel.clone();
        let request = TranscriptResourceRequest {
            attachment: TranscriptAttachmentRequest {
                window_id: selection.window_id(),
                host: panel.read(cx).lifetime(),
                activation: selection.binding().host_generation().get(),
                thread_id: selection.claim().thread_id(),
                request_id,
                placement: panel.read(cx).refresh_placement(),
                purpose: TranscriptAttachmentPurpose::Resource,
            },
            source,
            demand: demand.clone(),
        };
        let expected = request.clone();
        let cancelled = cancel.clone();
        let worker_provider = provider.clone();
        let worker_reader = reader.clone();
        let output_release = self
            .running_threads
            .workers
            .track(release_running_read_output as fn());
        let work = self
            .running_threads
            .workers
            .track(move || RunningReadOutput {
                result: worker_reader.prepare_running_resource(
                    &worker_provider,
                    expected,
                    &cancelled,
                ),
                _release: output_release,
            });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.running_threads.transcript_task = Some(cx.spawn_in(window, async move |this, cx| {
            let output = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                let RunningReadOutput { result, _release } = output;
                if root.running_threads.transcript_request != request_id {
                    return;
                }
                root.running_threads.transcript_task = None;
                root.running_threads.resource_demand = None;
                root.running_threads.resource_refresh_due = true;
                if cancel.load(Ordering::Acquire)
                    || !reader.current()
                    || !root.running_threads_enabled()
                    || root.running_threads.transcript != panel
                    || !root
                        .running_threads
                        .reader
                        .as_ref()
                        .is_some_and(|current| current.same_publication(&reader))
                    || root
                        .cached_running_selection(cx)
                        .map(|(current, _)| current)
                        != Some(selection)
                    || root.running_threads.transcript_source.as_ref() != Some(&request.source)
                    || panel.read(cx).resource_demand().as_ref() != Some(&demand)
                {
                    return;
                }
                match result {
                    Ok(PreparedRunningResource {
                        prepared,
                        retry_pending,
                        construction,
                    }) => {
                        let mount = root
                            .controller
                            .as_ref()
                            .and_then(|controller| controller.composer_mount());
                        let allocation = root.transcript_attachment_allocation(
                            mount.as_ref(),
                            request.source.has_parent_lineage(),
                            window,
                            cx,
                        );
                        if panel.update(cx, |panel, panel_cx| {
                            let staged = panel.stage_resource_delivery(
                                &demand,
                                prepared.response(),
                                Some(allocation),
                            );
                            if staged {
                                panel_cx.notify();
                            }
                            staged
                        }) {
                            root.running_threads.measured_resource =
                                Some(PendingMeasuredResource {
                                    panel: panel.clone(),
                                    provider,
                                    prepared,
                                    reader,
                                    selection,
                                    request,
                                    cancel,
                                    retry_pending,
                                    _construction: construction,
                                });
                            cx.notify();
                        } else {
                            panel.update(cx, |panel, _| panel.reject_resource_demand(demand));
                        }
                    }
                    Err(crate::transcript_provider::TranscriptAttachmentError::Capacity) => {
                        panel.update(cx, |panel, _| panel.reject_resource_demand(demand));
                    }
                    Err(_) => {}
                }
            });
        }));
        true
    }
}
