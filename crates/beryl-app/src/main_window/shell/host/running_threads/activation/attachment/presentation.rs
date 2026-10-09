use super::*;

impl MainWindowShellRoot {
    pub(super) fn accept_running_activation_stage(
        &mut self,
        operation: &UnviewedRunningActivation,
        source: &mut ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match source.stage {
            Stage::ThreadFence | Stage::ThreadAcceptSave | Stage::ThreadRestore => {
                return self.accept_thread_creation_stage(operation, source, window, cx);
            }
            Stage::Install => {
                let ready = if let Some(prepared) = source.presentation.take() {
                    match prepared.try_elect_current(|prepared| {
                        operation.mount.update(cx, |mount, mount_cx| {
                            mount.install_claim_pending_presentation(prepared, window, mount_cx)
                        })
                    }) {
                        Ok(result) => {
                            source.installed = true;
                            result?
                        }
                        Err((prepared, _)) => {
                            source.presentation = Some(prepared);
                            return Ok(());
                        }
                    }
                } else {
                    operation.mount.update(cx, |mount, mount_cx| {
                        mount.admit_claim_pending_presentation(
                            source.receipt()?,
                            source
                                .pending_selection
                                .ok_or("Target composer selection is missing")?,
                            mount_cx,
                        )
                    })?
                };
                source.installed = true;
                if ready {
                    source.stage = if source.creation.is_some() {
                        Stage::ThreadAdopt
                    } else {
                        Stage::PrepareFence
                    };
                }
            }
            Stage::Fence => {
                if operation.mount.update(cx, |mount, mount_cx| {
                    mount.fence_claim_publication(
                        source.receipt()?,
                        source.expected,
                        window,
                        mount_cx,
                    )
                })? {
                    self.sync_running_selected_cache(source.prior, source.expected, cx)?;
                    source.prior = source.expected;
                    source.stage = Stage::Flush;
                }
            }
            Stage::AcceptFlush => {
                let advance = source
                    .advance
                    .as_ref()
                    .ok_or("Composer flush advance is missing")?;
                let ready = operation.mount.update(cx, |mount, mount_cx| {
                    mount.accept_claim_publication_advance(advance, mount_cx)
                });
                self.sync_running_selected_cache(source.prior, advance.selected, cx)?;
                source.prior = advance.selected;
                source.expected = advance.selected;
                let ready = ready?;
                source.flush_settled =
                    matches!(advance.advance, PublishAdvance::WidgetReleaseRequired(_));
                if source.cancellation.is_cancelled() && source.flush_settled {
                    source.error = Some("Thread activation cancelled".into());
                    source.stage = Stage::Retire;
                    return Ok(());
                }
                if ready {
                    self.preflight_running_selection(&source.service, cx)?;
                    source.stage = Stage::Commit;
                } else {
                    source.stage = Stage::Flush;
                }
            }
            Stage::Release => {
                if let Some(work) = operation.mount.update(cx, |mount, mount_cx| {
                    mount.release_claim_publication_widget(
                        source.receipt()?,
                        source.expected,
                        window,
                        mount_cx,
                    )
                })? {
                    source.widget_work = Some(work);
                    source.stage = Stage::Complete;
                }
            }
            Stage::AcceptDisposal => {
                let advance = source
                    .advance
                    .as_ref()
                    .ok_or("Committed predecessor disposal advance is missing")?;
                let ready = operation.mount.update(cx, |mount, mount_cx| {
                    mount.accept_claim_publication_advance(advance, mount_cx)
                })?;
                self.sync_running_selected_cache(source.prior, advance.selected, cx)?;
                source.prior = advance.selected;
                source.expected = advance.selected;
                source.stage = if ready {
                    Stage::BeginFinal
                } else {
                    Stage::DisposePrior
                };
            }
            Stage::AcceptRelease => {
                operation.mount.update(cx, |mount, mount_cx| {
                    mount.accept_claim_widget_release(
                        source.release.ok_or("Widget release receipt is missing")?,
                        mount_cx,
                    )
                })?;
                source.stage = if source.publication.is_some() {
                    Stage::PrepareAutosave
                } else {
                    Stage::Complete
                };
            }
            Stage::Publish => {
                source
                    .lease
                    .validate_publication()
                    .map_err(|error| error.to_string())?;
                let publication = source
                    .publication
                    .as_ref()
                    .ok_or("Composer publication custody is missing")?;
                operation
                    .mount
                    .read(cx)
                    .validate_claim_promotion(publication, cx)?;
                self.preflight_running_selection(&source.service, cx)?;
                let committed = source
                    .committed_selection()
                    .ok_or("Committed selection custody is missing")?;
                let committed_window = source
                    .committed_window()
                    .ok_or("Committed window custody is missing")?
                    .clone();
                if committed != publication.selection().claim()
                    || operation.panel != self.running_threads.transcript
                {
                    return Err("Running selection publication identity is stale".into());
                }
                let provider = source
                    .provider
                    .as_ref()
                    .ok_or("Transcript provider is missing")?;
                let prepared = source
                    .transcript
                    .take()
                    .ok_or("Prepared transcript is missing")?;
                let identity = prepared.source_identity();
                let title = prepared.resolved_title().clone();
                let mut prepared = Some(prepared);
                let result = publication.try_elect_current(|| {
                    provider.publish_if_current(
                        prepared.take().unwrap(),
                        &source.request,
                        &source.transcript_cancel,
                        |seed| {
                            operation.mount.update(cx, |mount, mount_cx| {
                                mount.promote_claim_publication(publication, window, mount_cx)
                            })?;
                            operation.panel.update(cx, |panel, panel_cx| {
                                panel.publish_coherent_activation(seed);
                                panel_cx.notify();
                            });
                            self.publish_running_selection(
                                committed_window.clone(),
                                publication.selection(),
                                cx,
                            )
                        },
                    )
                });
                match result {
                    Ok(Ok(Ok(()))) => {
                        self.running_threads.transcript_provider = Some(provider.clone());
                        self.running_threads.transcript_claim = Some(committed);
                        self.running_threads.transcript_source = Some(identity);
                        self.running_threads.selected_title =
                            Some((publication.selection(), title));
                        source.error = None;
                        source.gui_published = true;
                        source.stage = Stage::Finalize;
                    }
                    Ok(Ok(Err(error))) => {
                        operation.suspend();
                        return Err(error);
                    }
                    _ => {
                        source.transcript = prepared;
                        source.stage = Stage::Revalidate;
                        source.transcript_retries = source.transcript_retries.saturating_add(1);
                    }
                }
            }
            Stage::Finalize => {
                let selected = source.publication.as_ref().unwrap().selection();
                let prepared = source
                    .autosave
                    .take()
                    .ok_or("Prepared autosave is missing")?;
                let finalized = operation.mount.update(cx, |mount, mount_cx| {
                    mount.finalize_claim_autosave(selected, prepared, window, mount_cx)
                });
                if let Err(error) = finalized {
                    source.stage = Stage::PrepareAutosave;
                    return Err(error);
                }
                source.stage = Stage::Finished;
            }
            Stage::Detach => {
                if source.installed {
                    operation.mount.update(cx, |mount, mount_cx| {
                        mount.detach_retired_claim_presentation(source.receipt()?, window, mount_cx)
                    })?;
                }
                source.stage = Stage::RestoreAutosave;
            }
            Stage::Restore => {
                operation.mount.update(cx, |mount, mount_cx| {
                    mount.restore_claim_prior_selection(source.expected, mount_cx)
                })?;
                self.sync_running_selected_cache(source.prior, source.expected, cx)?;
                source.prior = source.expected;
                let prepared = source
                    .autosave
                    .take()
                    .ok_or("Prior autosave preparation is missing")?;
                operation.mount.update(cx, |mount, mount_cx| {
                    mount.finalize_claim_autosave(source.expected, prepared, window, mount_cx)
                })?;
                source.stage = Stage::Finished;
            }
            _ => {}
        }
        Ok(())
    }
}
