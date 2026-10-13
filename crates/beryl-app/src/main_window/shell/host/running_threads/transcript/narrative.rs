use super::*;
use crate::transcript_provider::TranscriptNarrativeRequest;

impl MainWindowShellRoot {
    pub(super) fn sync_running_transcript_narrative(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.running_threads_enabled()
            || !self.running_read_capacity_available()
            || self.running_threads.transcript_task.is_some()
            || self.running_threads.measured_attachment.is_some()
            || self.running_threads.measured_resource.is_some()
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
        let panel = self.running_threads.transcript.clone();
        if panel.read(cx).adjacent_demand().is_some() {
            return false;
        }
        let (Some(provider), Some(source), Some(reader), Some(demand)) = (
            self.running_threads.transcript_provider.clone(),
            self.running_threads.transcript_source.clone(),
            self.running_threads.reader.clone(),
            panel.read(cx).narrative_demand(),
        ) else {
            return false;
        };
        if !provider.narrative_source_current(&source) {
            return false;
        }
        let Some(request_id) = self.running_threads.transcript_request.checked_add(1) else {
            return true;
        };
        let Some(allowance) =
            panel.update(cx, |panel, _| panel.reserve_narrative_preparation(&demand))
        else {
            return false;
        };
        self.running_threads.transcript_request = request_id;
        self.running_threads.transcript_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.running_threads.transcript_cancel.clone();
        let request = TranscriptNarrativeRequest {
            attachment: TranscriptAttachmentRequest {
                window_id: selection.window_id(),
                host: panel.read(cx).lifetime(),
                activation: selection.binding().host_generation().get(),
                thread_id: selection.claim().thread_id(),
                request_id,
                placement: panel.read(cx).refresh_placement(),
                purpose: TranscriptAttachmentPurpose::Narrative,
            },
            source,
            demand: demand.clone(),
            allowance,
        };
        let expected = request.clone();
        let cancelled = cancel.clone();
        let worker_provider = provider.clone();
        let output_release = self
            .running_threads
            .workers
            .track(release_running_read_output as fn());
        let work = self
            .running_threads
            .workers
            .track(move || RunningReadOutput {
                result: worker_provider.prepare_narrative(expected, &cancelled),
                _release: output_release,
            });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.running_threads.transcript_task = Some(cx.spawn_in(window, async move |this, cx| {
            let RunningReadOutput { result, _release } = job.await;
            let mut prepared = result.ok();
            let mut staged = false;
            loop {
                let finished = this
                    .update_in(cx, |root, window, cx| {
                        let current = root.running_threads.transcript_request == request_id
                            && !cancel.load(Ordering::Acquire)
                            && reader.current()
                            && root.running_threads_enabled()
                            && root.running_threads.transcript == panel
                            && root
                                .running_threads
                                .reader
                                .as_ref()
                                .is_some_and(|current| current.same_publication(&reader))
                            && root
                                .cached_running_selection(cx)
                                .map(|(current, _)| current)
                                == Some(selection)
                            && root.running_threads.transcript_source.as_ref()
                                == Some(&request.source)
                            && provider.narrative_source_current(&request.source)
                            && panel.read(cx).adjacent_demand().is_none()
                            && panel.read(cx).narrative_demand_current(&demand);
                        if !current || prepared.is_none() {
                            panel.update(cx, |panel, panel_cx| {
                                panel.discard_narrative_candidate(&demand);
                                panel.finish_narrative_preparation(&demand);
                                panel_cx.notify();
                            });
                            if root.running_threads.transcript_request == request_id {
                                root.running_threads.transcript_task = None;
                            }
                            return true;
                        }
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
                        if !staged {
                            staged = panel.update(cx, |panel, panel_cx| {
                                let accepted = panel.stage_narrative_delivery(
                                    &demand,
                                    prepared.as_ref().unwrap().response(),
                                    Some(allocation),
                                );
                                panel_cx.notify();
                                accepted
                            });
                            if !staged {
                                panel.update(cx, |panel, _| {
                                    panel.finish_narrative_preparation(&demand)
                                });
                                root.running_threads.transcript_task = None;
                                return true;
                            }
                        } else if !panel.read(cx).narrative_candidate_current(&demand)
                            || !panel.update(cx, |panel, _| {
                                panel.update_narrative_allocation(&demand, allocation)
                            })
                        {
                            panel.update(cx, |panel, panel_cx| {
                                panel.discard_narrative_candidate(&demand);
                                panel.finish_narrative_preparation(&demand);
                                panel_cx.notify();
                            });
                            root.running_threads.transcript_task = None;
                            return true;
                        }
                        if !panel.read(cx).measurement_current(window) {
                            cx.notify();
                            return false;
                        }
                        let result = provider.publish_narrative_if_current(
                            prepared.take().unwrap(),
                            &request,
                            &cancel,
                            |_| {
                                panel.update(cx, |panel, panel_cx| {
                                    panel.publish_narrative_candidate(&demand);
                                    panel.finish_narrative_preparation(&demand);
                                    panel_cx.notify();
                                });
                            },
                        );
                        if result.is_err() {
                            panel.update(cx, |panel, panel_cx| {
                                panel.discard_narrative_candidate(&demand);
                                panel.finish_narrative_preparation(&demand);
                                panel_cx.notify();
                            });
                        }
                        root.running_threads.transcript_task = None;
                        cx.notify();
                        true
                    })
                    .unwrap_or(true);
                if finished {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            }
        }));
        true
    }
}
