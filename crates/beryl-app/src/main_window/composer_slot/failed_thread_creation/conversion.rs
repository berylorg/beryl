use super::*;

impl MainWindowComposerSlot {
    pub(super) fn retire_failed_thread_successors(
        &mut self,
        store: &HomeStore,
        source: &mut MainWindowThreadCreationRetirementSource,
    ) -> Result<(), String> {
        if self.failed_thread_successor.is_none() {
            if let Some(pending) = self.pending.take() {
                let PendingComposer {
                    receipt,
                    claim,
                    retirement_operation_id,
                    host,
                    dispatcher,
                    source_selector,
                    stage,
                    abandonment,
                    abandonment_outcome,
                    retain_thread_cleanup,
                    abandonment_canonical_home,
                } = pending;
                match Box::new(host).retire_failed_thread_successor(
                    store,
                    retirement_operation_id,
                    abandonment,
                    abandonment_outcome,
                ) {
                    Ok(host) => {
                        self.failed_thread_successor = Some(RetiredThreadSuccessor {
                            receipt,
                            selection: MainWindowComposerSelectionIdentity {
                                window_id: self.window_id,
                                claim,
                                binding: host.binding(),
                            },
                            host: Box::new(host),
                        })
                    }
                    Err((host, abandonment, abandonment_outcome)) => {
                        self.pending = Some(PendingComposer {
                            receipt,
                            claim,
                            retirement_operation_id,
                            host: *host,
                            dispatcher,
                            source_selector,
                            stage,
                            abandonment,
                            abandonment_outcome,
                            retain_thread_cleanup,
                            abandonment_canonical_home,
                        });
                        return Err("failed successor runtime cannot retire".into());
                    }
                }
            } else if !same_claim(
                self.selected_identity()
                    .ok_or("failed selected runtime missing")?,
                source.prior,
            ) {
                let completed = source
                    .completed_predecessor
                    .as_ref()
                    .ok_or("promoted successor has no completed predecessor custody")?;
                let SelectedComposer {
                    identity,
                    dispatcher,
                    draft_state,
                    host,
                } = self.selected.take().unwrap();
                if !dispatcher.is_drained() || Some(identity.claim()) != source.committed_target {
                    self.selected = Some(SelectedComposer {
                        identity,
                        dispatcher,
                        draft_state,
                        host,
                    });
                    return Err("promoted successor source changed".into());
                }
                match Box::new(host).retire_failed_thread_successor(
                    store,
                    completed.retirement_operation,
                    None,
                    None,
                ) {
                    Ok(host) => {
                        self.failed_thread_successor = Some(RetiredThreadSuccessor {
                            receipt: completed.receipt,
                            selection: identity,
                            host: Box::new(host),
                        })
                    }
                    Err((host, _, _)) => {
                        self.selected = Some(SelectedComposer {
                            identity,
                            dispatcher,
                            draft_state,
                            host: *host,
                        });
                        return Err("promoted successor runtime cannot retire".into());
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn take_failed_thread_predecessor(
        &mut self,
        store: &HomeStore,
        source: &mut MainWindowThreadCreationRetirementSource,
        prior_identity: MainWindowComposerSelectionIdentity,
        markers: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<Box<ComposerHostFailedThreadCreation>, String> {
        let predecessor = if let Some(completed) = source.completed_predecessor.as_mut() {
            completed
                .host
                .bind_original_save(source.saved.as_ref().unwrap().saved)?;
            if let Some(selected) = self.selected.as_ref() {
                if selected.identity != completed.prior
                    || !selected.dispatcher.is_drained()
                    || !selected.host.completed_thread_creation_runtime_is_drained(
                        store,
                        completed.prior.binding(),
                    )
                {
                    return Err("completed predecessor runtime still owns work".into());
                }
            }
            self.selected = None;
            source.completed_predecessor.take().unwrap().host
        } else {
            let selected = self
                .selected
                .take()
                .ok_or("failed predecessor runtime missing")?;
            let SelectedComposer {
                identity,
                dispatcher,
                draft_state,
                host,
            } = selected;
            if !dispatcher.is_drained()
                || identity != prior_identity
                || !host.failed_thread_creation_binding_matches(prior_identity.binding())
            {
                self.selected = Some(SelectedComposer {
                    identity,
                    dispatcher,
                    draft_state,
                    host,
                });
                return Err("failed predecessor dispatcher or exact binding changed".into());
            }
            match Box::new(host).retire_failed_thread_creation(
                store,
                source.saved.as_ref().map(|saved| saved.saved),
                source.committed_target.is_some(),
                markers,
            ) {
                Ok(host) => Box::new(host),
                Err(host) => {
                    self.selected = Some(SelectedComposer {
                        identity,
                        dispatcher,
                        draft_state,
                        host: *host,
                    });
                    return Err("failed predecessor runtime cannot retire".into());
                }
            }
        };
        Ok(predecessor)
    }
}
