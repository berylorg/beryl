use super::*;

impl RuntimeSetupFlight {
    pub(crate) fn cancellation(&self) -> CommandCancellation {
        self.cancellation.clone()
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
            || self
                .worker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
    }

    pub(crate) fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .or_else(|| {
                self.preparation_error
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
            })
    }

    pub(crate) fn take_outcome(&self) -> Option<RuntimeAdmissionOutcome> {
        if self.is_pending() {
            return None;
        }
        self.outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    pub(crate) fn retain_outcome(&self, outcome: RuntimeAdmissionOutcome) {
        let mut slot = self.outcome.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            slot.is_none(),
            "runtime setup already retains its original outcome"
        );
        *slot = Some(outcome);
    }

    pub(crate) fn take_reconciliation_outcome(&self) -> Option<AdmissionReconciliationOutcome> {
        if self.is_pending() {
            return None;
        }
        self.unavailable
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    pub(crate) fn retain_reconciliation_outcome(&self, outcome: AdmissionReconciliationOutcome) {
        let mut slot = self.unavailable.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            slot.is_none(),
            "runtime setup already retains its reconciliation outcome"
        );
        *slot = Some(outcome);
    }

    pub(crate) fn start_reconciliation(self: &Arc<Self>) -> Result<(), String> {
        if self.is_pending() {
            return Err("original admission is still settling".into());
        }
        self.worker_failure()?;
        self.start_worker("runtime-setup-reconciliation", |flight| {
            let mut outcome = flight.outcome.lock().unwrap_or_else(|e| e.into_inner());
            let mut terminal = flight.unavailable.lock().unwrap_or_else(|e| e.into_inner());
            let reconciliation = if matches!(
                outcome.as_ref(),
                Some(RuntimeAdmissionOutcome::Indeterminate { .. })
            ) {
                let Some(RuntimeAdmissionOutcome::Indeterminate { reconciliation, .. }) =
                    outcome.take()
                else {
                    unreachable!()
                };
                reconciliation
            } else if matches!(
                terminal.as_ref(),
                Some(AdmissionReconciliationOutcome::Pending { .. })
            ) {
                let Some(AdmissionReconciliationOutcome::Pending { reconciliation, .. }) =
                    terminal.take()
                else {
                    unreachable!()
                };
                reconciliation
            } else {
                return Err("runtime setup has no original pending reconciliation".into());
            };
            let home = flight.home.lock().unwrap_or_else(|e| e.into_inner());
            *terminal =
                Some(reconciliation.retry(home.as_ref().ok_or("runtime setup home retired")?));
            Ok(())
        })
    }

    pub(crate) fn with_first_conversation<T>(
        &self,
        consumer: impl FnOnce(
            &mut crate::main_window::MainWindowFirstConversationPreparation,
        ) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.is_pending() {
            return Err("first conversation worker is still settling".into());
        }
        self.worker_failure()?;
        let mut first = self.first.lock().unwrap_or_else(|e| e.into_inner());
        consumer(
            first
                .as_mut()
                .ok_or("first conversation preparation is unavailable")?,
        )
    }

    pub(crate) fn is_first_publication_ready(&self) -> bool {
        self.revalidation_ready.load(Ordering::Acquire) && !self.is_pending()
    }

    pub(crate) fn publish_first_conversation<T>(
        &self,
        publication: impl FnOnce(
            &mut crate::main_window::MainWindowFirstConversationPreparation,
        ) -> Result<T, String>,
    ) -> Result<T, String> {
        let _guard = self
            .publication_gate
            .try_lock()
            .map_err(|_| "runtime setup publication is busy")?;
        if self.retired.load(Ordering::Acquire)
            || self.cancellation.is_cancelled()
            || !self.is_first_publication_ready()
        {
            return Err(
                "first conversation publication is retired, cancelled or unqualified".into(),
            );
        }
        let outcome = self
            .outcome
            .try_lock()
            .map_err(|_| "original admission is busy")?;
        let Some(RuntimeAdmissionOutcome::Committed { admission, .. }) = outcome.as_ref() else {
            return Err("first conversation original committed admission is unavailable".into());
        };
        admission
            .validate_publication()
            .map_err(|e| e.to_string())?;
        let mut first = self
            .first
            .try_lock()
            .map_err(|_| "first conversation preparation is busy")?;
        let first = first
            .as_mut()
            .ok_or("first conversation preparation is unavailable")?;
        if !first.publication_current() {
            return Err("first conversation generation changed".into());
        }
        publication(first)
    }

    pub(crate) fn finish_first_conversation(&self) -> Result<(), String> {
        if self.is_pending() {
            return Err("first conversation worker is still settling".into());
        }
        let mut first = self.first.lock().unwrap_or_else(|e| e.into_inner());
        if !first.as_ref().is_some_and(|first| first.is_published()) {
            return Err("first conversation has not published".into());
        }
        first.take();
        self.outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.unavailable
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        Ok(())
    }

    pub(in crate::app_services) fn worker_failure(&self) -> Result<(), String> {
        self.failure().map_or(Ok(()), Err)
    }

    pub(in crate::app_services) fn home_failed(&self) -> bool {
        self.home
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|home| home.health().state() == beryl_home_store::HomeHealthState::Failed)
    }

    pub(in crate::app_services) fn has_custody(&self) -> bool {
        self.is_pending()
            || self
                .outcome
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            || self
                .unavailable
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            || self
                .first
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            || self.failure().is_some()
    }

    pub(in crate::app_services) fn start_worker(
        self: &Arc<Self>,
        name: &str,
        operation: impl FnOnce(&Arc<Self>) -> Result<(), String> + Send + 'static,
    ) -> Result<(), String> {
        if self.retired.load(Ordering::Acquire) || self.is_pending() {
            return Err("runtime setup flight is retired or still settling".into());
        }
        if self.pending.swap(true, Ordering::AcqRel) {
            return Err("runtime setup flight is busy".into());
        }
        if self
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            self.pending.store(false, Ordering::Release);
            let error = "runtime setup original worker did not join".to_owned();
            *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.clone());
            return Err(error);
        }
        let retained = self.clone();
        match std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    operation(&retained)
                })) {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        *retained
                            .preparation_error
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = Some(error)
                    }
                    Err(_) => {
                        *retained.failure.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some("runtime setup original worker unwound".into())
                    }
                }
                retained.pending.store(false, Ordering::Release);
            }) {
            Ok(worker) => {
                *self.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
                Ok(())
            }
            Err(error) => {
                self.pending.store(false, Ordering::Release);
                Err(error.to_string())
            }
        }
    }
}
