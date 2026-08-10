use super::*;

impl ProviderBrokerUnstarted {
    pub(super) fn spawn(
        self,
        connection_generation: u64,
        build_fault: ProviderBrokerBuildFault,
    ) -> Result<PreparedProviderBroker, ProviderBrokerBuildError> {
        let thread_gate = Arc::clone(&self.start_gate);
        let thread_worker = Arc::clone(&self.worker);
        let thread_startup_gate = Arc::clone(&self.startup_gate);
        let thread_launch = Arc::clone(&self.launch);
        let handle = if let Some(error) = build_fault.spawn_failure() {
            Err(error)
        } else {
            std::thread::Builder::new()
                .name(format!("beryl-provider-ingester-{connection_generation}"))
                .spawn(move || {
                    let _worker_terminal = ProviderBrokerWorkerTerminalGuard::new(thread_worker);
                    let ingester = thread_launch.take();
                    if thread_gate.wait_until_started() {
                        if thread_startup_gate.wait() {
                            ingester.run()
                        } else {
                            ingester.cancel_before_start()
                        }
                    } else {
                        ingester.cancel_before_start()
                    }
                })
        };
        let handle = match handle {
            Ok(handle) => handle,
            Err(error) => {
                return Err(ProviderBrokerBuildError::new(
                    ProviderBrokerBuildFailure::Spawn {
                        message: error.to_string(),
                    },
                    ProviderBrokerBuildResources::Unstarted(self),
                ));
            }
        };
        let Self {
            sink,
            control,
            launch: _,
            start_gate,
            startup_gate,
            worker,
        } = self;
        Ok(PreparedProviderBroker {
            sink: Box::new(sink),
            control: Arc::clone(&control),
            ingester: StartBlockedProviderBrokerIngester {
                control: Arc::clone(&control),
                worker,
                handle: Some(handle),
                startup_gate,
            },
            start: ProviderBrokerStartToken {
                gate: start_gate,
                control,
                active: true,
            },
        })
    }
}

impl ProviderBrokerLaunchEscrow {
    fn take(&self) -> Ingester {
        self.ingester
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .expect("the provider broker launch escrow owns one ingester")
    }

    #[cfg(test)]
    pub(super) fn retains_ingester(&self) -> bool {
        self.ingester
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_some()
    }
}

impl ProviderBrokerStartGate {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(ProviderBrokerStartState::Blocked),
            changed: Condvar::new(),
        }
    }

    fn wait_until_started(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while *state == ProviderBrokerStartState::Blocked {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        *state == ProviderBrokerStartState::Started
    }

    fn start(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if *state != ProviderBrokerStartState::Blocked {
            return false;
        }
        *state = ProviderBrokerStartState::Started;
        drop(state);
        self.changed.notify_all();
        true
    }

    fn cancel(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if *state == ProviderBrokerStartState::Blocked {
            *state = ProviderBrokerStartState::Cancelled;
        }
        drop(state);
        self.changed.notify_all();
    }
}

impl ProviderBrokerWorkerEscrow {
    pub(super) fn new(worker: ProjectionWorkerPermit) -> Self {
        Self {
            state: Mutex::new(ProviderBrokerWorkerState {
                terminal: false,
                disposition: ProviderBrokerWorkerDisposition::Undecided,
                worker: Some(worker),
            }),
        }
    }

    pub(super) fn arm_ordinary_release(
        &self,
    ) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        let release = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| ProviderBrokerWorkerDispositionArmError::Poisoned)?;
            match state.disposition {
                ProviderBrokerWorkerDisposition::Undecided => {
                    state.disposition = ProviderBrokerWorkerDisposition::OrdinaryRelease;
                }
                ProviderBrokerWorkerDisposition::OrdinaryRelease => {}
                ProviderBrokerWorkerDisposition::RetainForAdoption(_) => {
                    return Err(ProviderBrokerWorkerDispositionArmError::Contended);
                }
            }
            state.terminal.then(|| state.worker.take()).flatten()
        };
        drop(release);
        Ok(())
    }

    pub(super) fn arm_retain_for_adoption(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ProviderBrokerWorkerDispositionArmError::Poisoned)?;
        match state.disposition {
            ProviderBrokerWorkerDisposition::Undecided => {
                state.disposition = ProviderBrokerWorkerDisposition::RetainForAdoption(cut);
                Ok(())
            }
            ProviderBrokerWorkerDisposition::RetainForAdoption(existing) if existing == cut => {
                Ok(())
            }
            ProviderBrokerWorkerDisposition::OrdinaryRelease
            | ProviderBrokerWorkerDisposition::RetainForAdoption(_) => {
                Err(ProviderBrokerWorkerDispositionArmError::Contended)
            }
        }
    }

    pub(super) fn mark_terminal(&self) {
        let release = match self.state.lock() {
            Ok(mut state) => {
                state.terminal = true;
                matches!(
                    state.disposition,
                    ProviderBrokerWorkerDisposition::OrdinaryRelease
                )
                .then(|| state.worker.take())
                .flatten()
            }
            Err(poison) => {
                // Poison cannot prove which disposition won. Record terminal progress while
                // retaining the exact admission for a later consuming owner.
                poison.into_inner().terminal = true;
                None
            }
        };
        drop(release);
    }

    pub(super) fn take_joined_worker(&self) -> ProviderBrokerJoinedWorker {
        match self.state.lock() {
            Ok(mut state) => ProviderBrokerJoinedWorker {
                worker: state.worker.take(),
                disposition: Some(state.disposition),
            },
            Err(poison) => ProviderBrokerJoinedWorker {
                worker: poison.into_inner().worker.take(),
                disposition: None,
            },
        }
    }

    #[cfg(test)]
    pub(super) fn retains_worker(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .worker
            .is_some()
    }
}

impl ProviderBrokerWorkerTerminalGuard {
    pub(super) fn new(worker: Arc<ProviderBrokerWorkerEscrow>) -> Self {
        Self { worker }
    }
}

impl Drop for ProviderBrokerWorkerTerminalGuard {
    fn drop(&mut self) {
        self.worker.mark_terminal();
    }
}

impl ProviderBrokerStartToken {
    fn activate(mut self) {
        assert!(
            self.gate.start(),
            "provider ingester start token is one-shot"
        );
        self.active = false;
    }

    fn try_activate(mut self) -> Result<(), Self> {
        if !self.gate.start() {
            return Err(self);
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for ProviderBrokerStartToken {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.control.request_cancel();
        self.gate.cancel();
    }
}

impl StartBlockedProviderBrokerIngester {
    pub(super) fn start(
        mut self,
        token: ProviderBrokerStartToken,
    ) -> RunningProviderBrokerIngester {
        assert!(
            Arc::ptr_eq(&self.control, &token.control),
            "provider ingester start token belongs to its exact broker"
        );
        token.activate();
        RunningProviderBrokerIngester {
            control: Arc::clone(&self.control),
            worker: Arc::clone(&self.worker),
            handle: self.handle.take(),
            active: true,
        }
    }

    /// Arms this replacement ingester while retaining the exact closed process-publication gate.
    pub(in crate::cas_projection::connection) fn arm_for_publication(
        mut self,
        token: ProviderBrokerStartToken,
        startup_gate: &Arc<ServiceStartupGate>,
    ) -> Result<
        RunningProviderBrokerIngester,
        (StartBlockedProviderBrokerIngester, ProviderBrokerStartToken),
    > {
        let startup_guard = match startup_gate.lock_for_publication() {
            Ok(guard) => guard,
            Err(()) => return Err((self, token)),
        };
        if !Arc::ptr_eq(&self.startup_gate, startup_gate)
            || !Arc::ptr_eq(&self.control, &token.control)
        {
            return Err((self, token));
        }
        if let Err(token) = token.try_activate() {
            return Err((self, token));
        }
        drop(startup_guard);
        Ok(RunningProviderBrokerIngester {
            control: Arc::clone(&self.control),
            worker: Arc::clone(&self.worker),
            handle: self.handle.take(),
            active: true,
        })
    }

    pub(in crate::cas_projection::connection) fn cancel_and_join(
        mut self,
        token: ProviderBrokerStartToken,
    ) -> ProviderBrokerStopped {
        drop(token);
        let receipt = join_provider_ingester(
            self.handle.take(),
            self.control.home_id,
            self.control.commands.service_generation(),
            self.control.home_generation,
        );
        let joined = self.worker.take_joined_worker();
        ProviderBrokerStopped {
            worker: joined.worker,
            worker_disposition: joined.disposition,
            receipt,
        }
    }
}

impl RunningProviderBrokerIngester {
    pub(in crate::cas_projection::connection) fn arm_ordinary_worker_release(
        &self,
    ) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        self.worker.arm_ordinary_release()
    }

    pub(in crate::cas_projection::connection) fn arm_worker_retention_for_adoption(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<(), ProviderBrokerWorkerDispositionArmError> {
        self.worker.arm_retain_for_adoption(cut)
    }

    pub(in crate::cas_projection::connection) fn request_cancel(&self) {
        self.control.request_cancel();
    }

    pub(in crate::cas_projection::connection) fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    pub(in crate::cas_projection::connection) fn stop_and_join(mut self) -> ProviderBrokerStopped {
        self.control.request_cancel();
        let receipt = join_provider_ingester(
            self.handle.take(),
            self.control.home_id,
            self.control.commands.service_generation(),
            self.control.home_generation,
        );
        self.active = false;
        let joined = self.worker.take_joined_worker();
        ProviderBrokerStopped {
            worker: joined.worker,
            worker_disposition: joined.disposition,
            receipt,
        }
    }
}

fn join_provider_ingester(
    handle: Option<JoinHandle<ProviderBrokerTerminalReceipt>>,
    _home_id: BerylHomeId,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
    home_generation: HomeGeneration,
) -> ProviderBrokerTerminalReceipt {
    let receipt =
        handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or(ProviderBrokerTerminalReceipt {
                service_generation,
                home_generation,
                clean: false,
            });
    #[cfg(test)]
    if take_forced_provider_broker_join_failure(_home_id, home_generation, service_generation) {
        return ProviderBrokerTerminalReceipt {
            clean: false,
            ..receipt
        };
    }
    receipt
}

#[cfg(test)]
fn take_forced_provider_broker_join_failure(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
) -> bool {
    let failure = ForcedProviderBrokerJoinFailure {
        home_id,
        home_generation,
        service_generation,
    };
    let Some(failures) = FORCED_PROVIDER_BROKER_JOIN_FAILURES.get() else {
        return false;
    };
    let mut failures = failures
        .lock()
        .expect("the provider-ingester join-failure test hook is usable");
    let Some(index) = failures.iter().position(|armed| *armed == failure) else {
        return false;
    };
    failures.swap_remove(index);
    true
}

impl Drop for RunningProviderBrokerIngester {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            self.control.request_cancel();
            drop(self.handle.take());
        }
    }
}
