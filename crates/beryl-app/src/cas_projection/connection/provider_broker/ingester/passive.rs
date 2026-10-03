mod operations;

use super::*;
use crate::cas_projection::{
    outage_buffer::{
        OutageConnectionIdentity, OutageFact, OutageInventory, OutageInventoryAccess,
        OutageObservationSlot,
    },
    persistent_failure::PersistentFailureCutIdentity,
};
use beryl_backend::{
    ApprovalOperationCompletion, OrderedTurnStreamCompletion, ProviderObservationControl,
    ProviderObservationRoute, SteeringUserMessageCaptureMode,
};
use std::sync::OnceLock;

pub(super) struct PassiveIngress {
    identity: PersistentFailureCutIdentity,
    connection: OutageConnectionIdentity,
    inventory: Arc<OnceLock<Arc<OutageInventory>>>,
    ready: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    slot: Option<OutageObservationSlot>,
    provider_open: bool,
    dynamic_open: bool,
    discard_provider: bool,
    last_route: Option<ProviderObservationRoute>,
    offset: usize,
    gap: bool,
    compact_loss: bool,
    compact_route: Option<(beryl_model::CasThreadId, beryl_model::CasTurnId)>,
}

impl PassiveIngress {
    pub(super) fn connection_generation(&self) -> u64 {
        self.connection.connection
    }
    pub(super) fn new(
        identity: PersistentFailureCutIdentity,
        connection: OutageConnectionIdentity,
        inventory: Arc<OnceLock<Arc<OutageInventory>>>,
        ready: Arc<AtomicBool>,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            identity,
            connection,
            inventory,
            ready,
            cancelled,
            slot: None,
            provider_open: false,
            dynamic_open: false,
            discard_provider: false,
            last_route: None,
            offset: 0,
            gap: false,
            compact_loss: false,
            compact_route: None,
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub(super) fn clear_completed_route(&mut self) {
        self.last_route = None;
        self.compact_loss = false;
        self.compact_route = None;
    }

    pub(super) fn observe_durable(&mut self, operation: &BrokerOperation) {
        self.last_route = None;
        self.compact_loss = false;
        self.compact_route = None;
        match operation {
            BrokerOperation::Ordered(OrderedTurnStreamOperation::ProviderBegin(_)) => {
                self.provider_open = true
            }
            BrokerOperation::Ordered(OrderedTurnStreamOperation::ProviderSeal(route)) => {
                self.provider_open = false;
                self.last_route = Some(route.clone());
            }
            BrokerOperation::Ordered(OrderedTurnStreamOperation::ProviderAbandon(_)) => {
                self.provider_open = false
            }
            BrokerOperation::Ordered(OrderedTurnStreamOperation::DynamicBegin(_)) => {
                self.dynamic_open = true
            }
            BrokerOperation::Ordered(
                OrderedTurnStreamOperation::DynamicSeal
                | OrderedTurnStreamOperation::DynamicAbandon(_),
            ) => self.dynamic_open = false,
            BrokerOperation::SteeringChecked(message) => {
                self.compact_route = Some((message.thread_id().clone(), message.turn_id().clone()))
            }
            BrokerOperation::SteeringUnverified(message) => {
                self.compact_route = Some((message.thread_id().clone(), message.turn_id().clone()))
            }
            BrokerOperation::Ordered(OrderedTurnStreamOperation::NormalTurnTerminal(message)) => {
                self.compact_route = Some((message.thread_id().clone(), message.turn_id().clone()))
            }
            BrokerOperation::Ordered(OrderedTurnStreamOperation::CheckedUserMessage(message)) => {
                self.compact_route = Some((message.thread_id().clone(), message.turn_id().clone()))
            }
            BrokerOperation::Ordered(OrderedTurnStreamOperation::TurnStarted(message)) => {
                self.compact_route = Some((message.thread_id().clone(), message.turn_id().clone()))
            }
            BrokerOperation::Ordered(
                OrderedTurnStreamOperation::ThreadStatusChanged(_)
                | OrderedTurnStreamOperation::ThreadClosed(_),
            ) => self.compact_loss = true,
            _ => {}
        }
    }

    pub(super) fn enter(&mut self) {
        if self.is_active() {
            return;
        }
        if let Some(inventory) = self
            .inventory
            .get()
            .filter(|owner| owner.matches(self.identity))
        {
            self.slot = Some(OutageObservationSlot::new(
                self.connection,
                inventory.assembly_limits,
            ));
        }
        self.discard_provider = self.provider_open;
        self.gap |= self.dynamic_open || self.compact_loss;
        if let Some((thread, turn)) = self.compact_route.take() {
            self.routed(&thread, &turn, None, true);
        }
        if let Some(route) = self.last_route.take() {
            self.routed(route.thread_id(), route.turn_id(), None, true);
        }
        self.ready.store(true, Ordering::Release);
    }

    pub(super) fn with_slot(
        &mut self,
        consume: impl FnOnce(
            &mut OutageObservationSlot,
            Option<&mut crate::cas_projection::outage_buffer::OutageBuffer>,
        ),
    ) {
        let Some(slot) = self.slot.as_mut() else {
            self.gap = true;
            return;
        };
        let Some(inventory) = self.inventory.get() else {
            self.gap = true;
            return;
        };
        inventory.access(self.identity, |access| {
            if self.cancelled.load(Ordering::Acquire) {
                self.gap = true;
                slot.disable();
                return;
            }
            match access {
                OutageInventoryAccess::Pending => consume(slot, None),
                OutageInventoryAccess::Ready(buffer) => {
                    if self.gap {
                        slot.record_loss();
                    }
                    slot.flush(buffer);
                    consume(slot, Some(buffer));
                }
                OutageInventoryAccess::Unavailable => slot.disable(),
            }
        });
    }

    pub(super) fn refresh(&mut self) {
        self.with_slot(|_, _| {});
    }

    fn routed(
        &mut self,
        thread: &beryl_model::CasThreadId,
        turn: &beryl_model::CasTurnId,
        fact: Option<OutageFact<'_>>,
        gap: bool,
    ) {
        let Some(inventory) = self.inventory.get() else {
            self.gap = true;
            return;
        };
        inventory.access(self.identity, |access| match access {
            OutageInventoryAccess::Ready(buffer) => {
                if self.gap {
                    self.connection.record_gap(buffer);
                }
                let cancelled = self.cancelled.load(Ordering::Acquire);
                buffer.capture_routed(
                    self.connection,
                    thread,
                    turn,
                    if cancelled { None } else { fact },
                    gap || cancelled,
                );
            }
            OutageInventoryAccess::Pending | OutageInventoryAccess::Unavailable => self.gap = true,
        });
    }

    pub(super) fn finish(&mut self) {
        self.gap |= self.provider_open;
        if let Some(slot) = self.slot.as_mut() {
            self.gap |= slot.discard_for_retirement();
        }
        if self.is_active()
            && self.gap
            && let Some(inventory) = self.inventory.get()
        {
            inventory.record_connection_loss(self.identity, self.connection);
        }
        self.slot = None;
        self.ready.store(false, Ordering::Release);
    }
}

impl PreparedProviderBroker {
    pub(in crate::cas_projection::connection) fn bind_outage_inventory(
        &self,
        inventory: Arc<OutageInventory>,
    ) -> Result<(), ()> {
        let state = self.start.gate.state.lock().map_err(|_| ())?;
        if *state != ProviderBrokerStartState::Blocked {
            return Err(());
        }
        let identity = PersistentFailureCutIdentity::new(
            self.control.home_id,
            self.control.home_generation,
            self.control.commands.service_generation(),
            crate::cas_projection::persistent_failure::PersistentFailureGeneration::FIRST,
        );
        if !inventory.matches(identity) {
            return Err(());
        }
        self.control.outage_inventory.set(inventory).map_err(|_| ())
    }
}

impl ProviderBrokerControl {
    pub(in crate::cas_projection::connection) fn passive_ready(&self) -> bool {
        self.passive_ready.load(Ordering::Acquire) && !self.cancelled.load(Ordering::Acquire)
    }
}

impl Ingester {
    pub(super) fn passive_failure_observed(&self) -> bool {
        let health = self.home.health();
        self.home.home_id() == self.home_id
            && health.generation() == Some(self.home_generation)
            && health.state() == beryl_home_store::HomeHealthState::Failed
            && self.exact_persistent_failure()
    }

    pub(super) fn enter_passive(&mut self) {
        if self.passive.is_active() {
            return;
        }
        // Durable operation classification installs reconciliation before this disposal.
        drop(self.active.take());
        self.approval.close();
        self.steering_results.close();
        self.passive.enter();
    }
}
