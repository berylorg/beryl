use super::*;

impl MainWindowConversationComposerMount {
    pub(super) fn advance_native_lineage_disposal_task(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountDisposalAdvance, String> {
        if let Some(error) = self.native_lineage_failure.as_ref() {
            return Err(error.clone());
        }
        if self.native_lineage_disposal_task.is_none() {
            let (selection, flush) = self.native_lineage_disposal_flush.unwrap();
            let release = self
                .native_lineage_widget_release
                .ok_or_else(|| "native lineage disposal has no widget release proof".to_owned())?;
            let cancellation = self
                .native_lineage_disposal_cancellation
                .get_or_insert_with(CommandCancellation::new)
                .clone();
            let worker_cancellation = cancellation.clone();
            let service = self.service.clone();
            let assets = self.submission_assets();
            let seals = self.submission_marker_seals();
            let executor = cx.background_executor().clone();
            let task = executor.spawn(async move {
                service.advance_native_lineage_disposal(
                    selection,
                    flush,
                    release,
                    assets,
                    &seals,
                    &worker_cancellation,
                )
            });
            let service = self.service.clone();
            let assets = self.submission_assets();
            let seals = self.submission_marker_seals();
            let task_cancellation = cancellation.clone();
            let task = cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let next_selection = result.as_ref().ok().and_then(|(_, _, selection)| *selection).unwrap_or(selection);
                let applied = this.update_in(cx, |this, _, cx| {
                    if this.native_lineage_disposal_flush != Some((selection, flush)) {
                        return false;
                    }
                    this.native_lineage_disposal_task = None;
                    match result {
                        Ok((advance, capture, current)) => {
                            this.native_lineage_last_disposal_advance = Some(advance);
                            if let Some(capture) = capture {
                                this.native_lineage_disposal_capture = Some(capture);
                            }
                            if advance == MainWindowComposerDisposalAdvance::Disposed {
                                this.native_lineage_widget_release = None;
                                this.native_lineage_disposal_flush = None;
                                this.native_lineage_disposal_capture = None;
                                this.native_lineage_disposal_cancellation = None;
                                this.contribution = None;
                                this.contribution_subscription = None;
                                this.clear_native_lineage_mount_state();
                            } else if let Some(current) = current {
                                this.native_lineage_disposal_flush = Some((current, flush));
                            }
                            if advance == MainWindowComposerDisposalAdvance::Failed {
                                this.preserve_native_lineage_disposal_failure("Composer disposal did not commit. The preserved draft remains available for recovery.".to_owned(), cx);
                            }
                        }
                        Err(error) => {
                            this.preserve_native_lineage_disposal_failure(error, cx);
                        }
                    }
                    cx.notify();
                    true
                });
                if !matches!(applied, Ok(true)) {
                    task_cancellation.cancel();
                    executor.clone().spawn(async move {
                        drain_unmounted_native_disposal(service, next_selection, flush, release, assets, seals, task_cancellation, executor).await;
                    }).detach();
                }
            });
            self.native_lineage_disposal_task = Some(task);
        }
        Ok(
            MainWindowConversationComposerMountDisposalAdvance::Retained(
                self.native_lineage_last_disposal_advance.unwrap_or(
                    MainWindowComposerDisposalAdvance::Progress(
                        ComposerHostFlushState::CaptureRequired,
                    ),
                ),
            ),
        )
    }

    pub(super) fn cancel_native_lineage_disposal_on_drop(&mut self) -> bool {
        let cancellation = self
            .native_lineage_disposal_cancellation
            .take()
            .unwrap_or_else(CommandCancellation::new);
        cancellation.cancel();
        if let Some(task) = self.native_lineage_disposal_task.take() {
            task.detach();
            return true;
        }
        let Some((selection, flush)) = self.native_lineage_disposal_flush else {
            return false;
        };
        let Some(release) = self.native_lineage_widget_release else {
            return false;
        };
        let service = self.service.clone();
        let assets = self.submission_assets();
        let seals = self.submission_marker_seals();
        let executor = self.submission.executor();
        executor
            .clone()
            .spawn(async move {
                drain_unmounted_native_disposal(
                    service,
                    selection,
                    flush,
                    release,
                    assets,
                    seals,
                    cancellation,
                    executor,
                )
                .await;
            })
            .detach();
        true
    }
}

#[allow(clippy::too_many_arguments)]
async fn drain_unmounted_native_disposal(
    service: Arc<MainWindowConversationComposerService>,
    mut selection: MainWindowComposerSelectionIdentity,
    flush: ComposerHostFlushTicket,
    release: MainWindowComposerWidgetRelease,
    assets: beryl_state::AssetState,
    seals: DraftMarkerSealService,
    cancellation: CommandCancellation,
    executor: gpui::BackgroundExecutor,
) {
    let mut delay = Duration::from_millis(1);
    for _ in 0..32 {
        let result = service.advance_native_lineage_disposal(
            selection,
            flush,
            release,
            assets.clone(),
            &seals,
            &cancellation,
        );
        match result {
            Ok((
                MainWindowComposerDisposalAdvance::Disposed
                | MainWindowComposerDisposalAdvance::Failed,
                _,
                _,
            ))
            | Err(_) => break,
            Ok((_, _, Some(current))) => selection = current,
            Ok((_, _, None)) => break,
        }
        executor.timer(delay).await;
        delay = delay.saturating_mul(2).min(Duration::from_millis(100));
    }
    let _ = service.cancel_native_lineage_suspension(release.selection());
}
