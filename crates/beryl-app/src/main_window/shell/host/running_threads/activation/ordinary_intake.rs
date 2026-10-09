use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrdinaryThreadActivationAcceptance {
    Current,
    Admitted,
}

impl MainWindowShellRoot {
    #[cfg(test)]
    pub(crate) fn test_thread_navigation_history(&self) -> Vec<beryl_model::SyndicThreadId> {
        self.running_threads.navigation_history.test_entries()
    }

    pub(crate) fn request_ordinary_thread_activation(
        &mut self,
        thread: beryl_model::SyndicThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
        accepted: impl FnOnce(
            Result<OrdinaryThreadActivationAcceptance, String>,
            &mut Window,
            &mut Context<Self>,
        ) + 'static,
    ) -> Result<(), String> {
        if self.running_threads.has_activation_custody()
            || !self.running_threads_enabled()
            || self.runtime_setup.pending()
        {
            return Err(
                "Thread selection is unavailable while another operation is pending.".into(),
            );
        }
        let reader = self
            .running_threads
            .reader
            .clone()
            .ok_or("Thread source is unavailable.")?;
        let invoking = self
            .controller
            .as_ref()
            .ok_or("The invoking window is unavailable.")?
            .window_id();
        let prior = self
            .cached_running_selection(cx)
            .map(|(selection, _)| selection);
        let generation = self.running_threads.generation.load(Ordering::Acquire);
        self.running_threads.pending_activation = Some(thread);
        self.running_threads.ordinary_activation = true;
        self.running_threads.activation_attention.clear();
        self.running_threads.activation_cancel = beryl_home_store::CommandCancellation::new();
        let cancel = self.running_threads.activation_cancel.clone();
        let source = reader.clone();
        let work = self.running_threads.workers.track(move || {
            Box::new(source.prepare_ordinary_thread_activation(
                invoking,
                prior.map(|prior| prior.claim()),
                thread,
                &cancel,
            ))
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.running_threads.activation_wake.take();
        self.running_threads.activation_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = *job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                if root.running_threads.generation.load(Ordering::Acquire) != generation
                    || root.running_threads.pending_activation != Some(thread)
                {
                    accepted(Err("The invoking view changed.".into()), window, cx);
                    return;
                }
                root.running_threads.activation_task = None;
                let result = if root.running_threads.activation_cancel.is_cancelled()
                    || !root.running_threads_enabled()
                    || root.runtime_setup.pending()
                    || !reader.current()
                    || root
                        .cached_running_selection(cx)
                        .map(|(selection, _)| selection)
                        != prior
                {
                    Err("The invoking view changed.".into())
                } else {
                    result.and_then(|(prepared, observed)| {
                        reader.elect(&observed, || match prepared {
                            RunningThreadActivationPreparation::Current { claim, .. } => {
                                if root.matches_running_claim(claim, cx) {
                                    Ok(OrdinaryThreadActivationAcceptance::Current)
                                } else {
                                    Err("The selected thread changed.".into())
                                }
                            }
                            RunningThreadActivationPreparation::ClaimedElsewhere { .. } => {
                                Err("The requested thread is open in another window.".into())
                            }
                            RunningThreadActivationPreparation::Prepared(prepared) => {
                                root.running_threads.prepared_activation = Some(prepared);
                                Ok(OrdinaryThreadActivationAcceptance::Admitted)
                            }
                        })?
                    })
                };
                let result = result.and_then(|acceptance| {
                    if acceptance == OrdinaryThreadActivationAcceptance::Admitted {
                        root.running_threads
                            .navigation_history
                            .begin(prior.map(|prior| prior.claim().thread_id()), thread);
                        root.start_unviewed_running_activation(reader.clone(), window, cx)?;
                    }
                    Ok(acceptance)
                });
                if !matches!(result, Ok(OrdinaryThreadActivationAcceptance::Admitted)) {
                    root.finish_running_activation(
                        result.as_ref().map(|_| ()).map_err(Clone::clone),
                        window,
                        cx,
                    );
                }
                accepted(result, window, cx);
                cx.notify();
            });
        }));
        cx.notify();
        Ok(())
    }

    pub(in crate::main_window::shell::host::running_threads) fn settle_thread_navigation_history(
        &mut self,
        cx: &Context<Self>,
    ) {
        let selected = self
            .cached_running_selection(cx)
            .map(|(selection, _)| selection.claim().thread_id());
        self.running_threads.navigation_history.settle(selected);
    }
}
