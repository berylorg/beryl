use super::*;

impl Ingester {
    pub(super) fn run(mut self) -> ProviderBrokerTerminalReceipt {
        let service_generation = self.commands.service_generation();
        let home_generation = self.home_generation;
        let clean = catch_unwind(AssertUnwindSafe(|| self.run_loop())).is_ok();
        ProviderBrokerTerminalReceipt {
            service_generation,
            home_generation,
            clean,
        }
    }

    pub(super) fn cancel_before_start(mut self) -> ProviderBrokerTerminalReceipt {
        self.cancelled.store(true, Ordering::Release);
        self.abandon_active();
        self.approval.close();
        self.steering_results.close();
        self.ack.close();
        ProviderBrokerTerminalReceipt {
            service_generation: self.commands.service_generation(),
            home_generation: self.home_generation,
            clean: true,
        }
    }

    fn run_loop(&mut self) {
        let mut persistent_failure = false;
        while let Some(operation) = receive_next(&self.receiver, &self.cancelled) {
            let command = match self.commands.authorize() {
                Ok(command) => command,
                Err(_) => {
                    persistent_failure = self.commands.failure_observed();
                    self.cancelled.store(true, Ordering::Release);
                    let (reply, _) = self.apply(operation);
                    self.ack.complete_terminal(reply);
                    break;
                }
            };
            self.command = Some(command);
            let (reply, terminal) = self.apply(operation);
            if self.authority_lost {
                self.command
                    .take()
                    .expect("authority loss retains its outer live command")
                    .release_after_authority_loss();
                self.ack.complete_terminal(reply);
                self.cancelled.store(true, Ordering::Release);
                break;
            }
            persistent_failure = self.exact_persistent_failure();
            if terminal || persistent_failure {
                // The terminal acknowledgement may unblock an owner that immediately waits for
                // the persistent-failure cut. Release this operation's outer drain-counted permit
                // first; operation-specific nested permits have already settled inside `apply`.
                drop(self.command.take());
                self.ack.complete_terminal(reply);
                self.cancelled.store(true, Ordering::Release);
                break;
            }
            // Every acknowledgement is the public completion boundary for its operation. Release
            // the outer drain-counted permit before publishing that boundary so the submitter
            // cannot observe a completed operation that is still counted as live.
            drop(self.command.take());
            self.ack.complete(reply);
        }
        if !persistent_failure && !self.authority_lost {
            self.abandon_active();
        }
        self.approval.close();
        self.steering_results.close();
        self.ack.close();
    }

    fn apply(&mut self, operation: BrokerOperation) -> (BrokerReply, bool) {
        if self.cancelled.load(Ordering::Acquire) {
            let reply = match operation {
                BrokerOperation::Ordered(operation) => {
                    BrokerReply::Rejected(operation, OrderedTurnStreamSubmitCause::Cancelled)
                }
                BrokerOperation::SteeringSelect(selection) => {
                    BrokerReply::SteeringSelectionRejected(
                        selection,
                        OrderedTurnStreamSubmitCause::Cancelled,
                    )
                }
                BrokerOperation::SteeringChecked(message) => BrokerReply::SteeringCheckedRejected(
                    message,
                    OrderedTurnStreamSubmitCause::Cancelled,
                ),
                BrokerOperation::SteeringAbandon(_) => {
                    BrokerReply::SteeringAbandonRejected(OrderedTurnStreamSubmitCause::Cancelled)
                }
            };
            return (reply, true);
        }
        match operation {
            BrokerOperation::Ordered(operation) => match operation {
                OrderedTurnStreamOperation::ThreadStatusChanged(status) => {
                    self.thread_status_changed(status)
                }
                operation @ OrderedTurnStreamOperation::ThreadClosed(_) => (
                    BrokerReply::Rejected(
                        operation,
                        OrderedTurnStreamSubmitCause::Rejected(
                            OrderedTurnStreamRejection::InvalidControl,
                        ),
                    ),
                    true,
                ),
                OrderedTurnStreamOperation::TurnStarted(turn) => self.turn_started(turn),
                OrderedTurnStreamOperation::CheckedUserMessage(message) => {
                    self.checked_user_message(message)
                }
                OrderedTurnStreamOperation::NormalTurnTerminal(terminal) => {
                    self.normal_turn_terminal(terminal)
                }
                OrderedTurnStreamOperation::Approval(request) => self.route_approval(request),
                OrderedTurnStreamOperation::DynamicBegin(call) => self.begin_dynamic(call),
                OrderedTurnStreamOperation::DynamicArgumentControl(control) => {
                    self.control_dynamic(control)
                }
                OrderedTurnStreamOperation::DynamicAcquirePage => self.acquire_dynamic_page(),
                OrderedTurnStreamOperation::DynamicArgumentFragment(fragment) => {
                    self.fragment_dynamic(fragment)
                }
                OrderedTurnStreamOperation::DynamicSeal => self.seal_dynamic(),
                OrderedTurnStreamOperation::DynamicAbandon(reason) => self.abandon_dynamic(reason),
                OrderedTurnStreamOperation::ProviderBegin(begin) => self.begin(begin),
                OrderedTurnStreamOperation::ProviderControl(control) => self.control(control),
                OrderedTurnStreamOperation::ProviderAcquirePage => self.acquire_page(),
                OrderedTurnStreamOperation::ProviderFragment(fragment) => self.fragment(fragment),
                OrderedTurnStreamOperation::ProviderSeal(route) => self.seal(route),
                OrderedTurnStreamOperation::ProviderAbandon(reason) => {
                    self.abandon_provider(reason)
                }
            },
            BrokerOperation::SteeringSelect(selection) => {
                self.select_steering_user_message(selection)
            }
            BrokerOperation::SteeringChecked(message) => {
                self.checked_steering_user_message(message)
            }
            BrokerOperation::SteeringAbandon(reason) => self.abandon_steering_user_message(reason),
        }
    }

    pub(super) fn invalidate_target(&self, target: TargetInvalidation) -> TargetRouteOutcome {
        if self.exact_persistent_failure() {
            return TargetRouteOutcome::Terminal;
        }
        let reason = target.reason;
        if registry::invalidate_exact_generation(
            &target.key,
            self.authority.generation,
            target.owner,
            target.loaded_generation,
        )
        .is_err()
        {
            self.retire();
            return TargetRouteOutcome::Terminal;
        }
        if reason == LiveEventTargetCloseReason::ThreadClosed {
            TargetRouteOutcome::Applied
        } else {
            TargetRouteOutcome::TargetFailure
        }
    }

    pub(super) fn retire(&self) {
        self.retire_for(
            WholeConnectionRoutingFailure::Router,
            LiveEventTargetCloseReason::StreamFailure,
        );
    }

    pub(super) fn retire_for(
        &self,
        failure: WholeConnectionRoutingFailure,
        reason: LiveEventTargetCloseReason,
    ) {
        if self.exact_persistent_failure() {
            return;
        }
        self.record_failure(failure);
        let _ = self.authority.retire();
        self.router.retire(reason);
    }

    pub(super) fn record_failure(&self, failure: WholeConnectionRoutingFailure) {
        self.routing_failure.record(failure);
    }

    pub(super) fn provider_is_active(&self) -> bool {
        matches!(self.active, Some(ActiveIngress::Provider(_)))
    }

    pub(super) fn take_provider(&mut self) -> Option<ActiveObservation> {
        match self.active.take() {
            Some(ActiveIngress::Provider(observation)) => Some(observation),
            other => {
                self.active = other;
                None
            }
        }
    }

    pub(super) fn put_provider(&mut self, observation: ActiveObservation) {
        debug_assert!(self.active.is_none());
        self.active = Some(ActiveIngress::Provider(observation));
    }

    pub(super) fn take_dynamic(&mut self) -> Option<ActiveDynamicTool> {
        match self.active.take() {
            Some(ActiveIngress::Dynamic(dynamic)) => Some(dynamic),
            other => {
                self.active = other;
                None
            }
        }
    }

    pub(super) fn put_dynamic(&mut self, dynamic: ActiveDynamicTool) {
        debug_assert!(self.active.is_none());
        self.active = Some(ActiveIngress::Dynamic(dynamic));
    }

    pub(super) fn take_steering(&mut self) -> Option<ActiveSteeringLifecycle> {
        match self.active.take() {
            Some(ActiveIngress::Steering(steering)) => Some(steering),
            other => {
                self.active = other;
                None
            }
        }
    }

    pub(super) fn put_steering(&mut self, steering: ActiveSteeringLifecycle) {
        debug_assert!(self.active.is_none());
        self.active = Some(ActiveIngress::Steering(steering));
    }

    pub(super) fn abandon_active(&mut self) {
        if self.exact_persistent_failure() {
            return;
        }
        match self.active.take() {
            Some(ActiveIngress::Provider(observation)) => observation.abandon(),
            Some(ActiveIngress::Dynamic(dynamic)) => {
                self.router.abandon_dynamic_tool(&dynamic.permit);
                drop(dynamic);
            }
            Some(ActiveIngress::Steering(steering)) => {
                self.steering_results.abandon_selection();
                let _ = steering.permit.fail();
            }
            None => {}
        }
    }

    pub(super) fn current_observation_authority(&self) -> Option<(HomeGeneration, SyndicStorage)> {
        self.current_observation_authority_typed().ok()
    }

    pub(super) fn current_observation_authority_typed(
        &self,
    ) -> Result<(HomeGeneration, SyndicStorage), CurrentObservationAuthorityError> {
        let actual_home_id = self.home.home_id();
        if actual_home_id != self.home_id {
            return Err(CurrentObservationAuthorityError::HomeIdentity {
                expected: self.home_id,
                actual: actual_home_id,
            });
        }
        let before = self.home.health();
        if before.state() != HomeHealthState::Healthy {
            return Err(CurrentObservationAuthorityError::Health(before));
        }
        let generation = before.generation();
        if generation != Some(self.home_generation) {
            return Err(CurrentObservationAuthorityError::Generation {
                expected: self.home_generation,
                actual: generation,
            });
        }
        let storage = SyndicStorage::reacquire(&self.home)
            .map_err(CurrentObservationAuthorityError::Reacquire)?;
        let after = self.home.health();
        if after.state() != HomeHealthState::Healthy || after.generation() != generation {
            return Err(CurrentObservationAuthorityError::Changed { before, after });
        }
        Ok((self.home_generation, storage))
    }

    pub(super) fn exact_persistent_failure(&self) -> bool {
        let health = self.home.health();
        let failed = self.home.home_id() == self.home_id
            && health.state() == HomeHealthState::Failed
            && health.generation() == Some(self.home_generation);
        if failed {
            let _ = self.failure_notification.notify();
        }
        self.commands.is_persistent_failure_cut()
    }

    pub(super) fn authority_lost_terminal(&mut self) -> (BrokerReply, bool) {
        self.authority_lost = true;
        (
            BrokerReply::Applied(beryl_backend::OrderedTurnStreamCompletion::Applied),
            true,
        )
    }

    pub(super) fn live_command(&self) -> &LiveCommandPermit {
        self.command
            .as_ref()
            .expect("provider apply retains its operation-scoped live command")
    }

    pub(super) fn committer(
        &self,
        identity: ProviderObservationId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
    ) -> super::super::staging::StageCommitter<'_> {
        super::super::staging::StageCommitter {
            home: &self.home,
            home_id: self.home_id,
            home_generation,
            storage,
            identity,
            cancelled: &self.cancelled,
            command: self.live_command(),
            #[cfg(feature = "test-faults")]
            test_metrics: &self.test_metrics,
        }
    }
}
