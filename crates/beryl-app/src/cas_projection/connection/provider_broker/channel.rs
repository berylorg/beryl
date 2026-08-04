use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};

use beryl_backend::{
    CheckedSteeringUserMessage, CheckedSteeringUserMessageSubmitError, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, OrderedTurnStreamSubmitCause,
    OrderedTurnStreamSubmitError, SteeringUserMessageAbandonReason, SteeringUserMessageSelection,
    SteeringUserMessageSelectionError, SteeringUserMessageSource,
};
use beryl_stream::{FixedChannelReceiver, FixedChannelSender, SendError};

use super::BROKER_POLL_INTERVAL;
pub(super) type BrokerSender = FixedChannelSender<BrokerOperation>;
pub(super) type BrokerReceiver = FixedChannelReceiver<BrokerOperation>;

pub(super) enum BrokerOperation {
    Ordered(OrderedTurnStreamOperation),
    SteeringSelect(SteeringUserMessageSelection),
    SteeringChecked(CheckedSteeringUserMessage),
    SteeringAbandon(SteeringUserMessageAbandonReason),
}

impl BrokerOperation {
    pub(super) fn new(operation: OrderedTurnStreamOperation) -> Self {
        Self::Ordered(operation)
    }

    pub(super) fn into_operation(self) -> OrderedTurnStreamOperation {
        match self {
            Self::Ordered(operation) => operation,
            _ => panic!("broker operation did not contain an ordinary ordered operation"),
        }
    }
}

pub(super) enum BrokerReply {
    Applied(OrderedTurnStreamCompletion),
    Rejected(OrderedTurnStreamOperation, OrderedTurnStreamSubmitCause),
    SteeringSelected(SteeringUserMessageSource),
    SteeringSelectionRejected(SteeringUserMessageSelection, OrderedTurnStreamSubmitCause),
    SteeringChecked,
    SteeringCheckedRejected(CheckedSteeringUserMessage, OrderedTurnStreamSubmitCause),
    SteeringAbandoned,
    SteeringAbandonRejected(OrderedTurnStreamSubmitCause),
}

#[derive(Default)]
struct AckState {
    reply: Option<BrokerReply>,
    closed: bool,
}

/// One preallocated acknowledgement cell. It is deliberately not a queue.
pub(super) struct AckSlot {
    state: Mutex<AckState>,
    changed: Condvar,
}

impl AckSlot {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(AckState::default()),
            changed: Condvar::new(),
        }
    }

    pub(super) fn prepare(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed || state.reply.is_some() {
            return false;
        }
        true
    }

    pub(super) fn complete(&self, reply: BrokerReply) {
        self.complete_inner(reply, false);
    }

    /// Installs the ownership-preserving terminal reply and closes later
    /// admission at one acknowledgement-slot linearization point.
    pub(super) fn complete_terminal(&self, reply: BrokerReply) {
        self.complete_inner(reply, true);
    }

    fn complete_inner(&self, reply: BrokerReply, terminal: bool) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.reply.is_none() {
            state.reply = Some(reply);
        }
        if terminal {
            state.closed = true;
        }
        drop(state);
        self.changed.notify_all();
    }

    pub(super) fn wait(&self, cancelled: &AtomicBool) -> Option<BrokerReply> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        loop {
            if let Some(reply) = state.reply.take() {
                return Some(reply);
            }
            if state.closed {
                return None;
            }
            if cancelled.load(Ordering::Acquire) {
                // The ingester still owns the exact operation. Continue bounded
                // waits until it returns that ownership in its terminal ack.
                self.changed.notify_all();
            }
            (state, _) = self
                .changed
                .wait_timeout(state, BROKER_POLL_INTERVAL)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    pub(super) fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
        drop(state);
        self.changed.notify_all();
    }

    pub(super) fn wake(&self) {
        self.changed.notify_all();
    }
}

pub(super) struct BrokerSink {
    sender: BrokerSender,
    ack: Arc<AckSlot>,
    cancelled: Arc<AtomicBool>,
    #[cfg(feature = "test-faults")]
    test_home_id: beryl_model::BerylHomeId,
    #[cfg(feature = "test-faults")]
    test_metrics: Arc<crate::cas_projection::test_faults::ProviderBrokerTestMetrics>,
}

impl BrokerSink {
    pub(super) const fn new(
        sender: BrokerSender,
        ack: Arc<AckSlot>,
        cancelled: Arc<AtomicBool>,
        #[cfg(feature = "test-faults")] home_id: beryl_model::BerylHomeId,
        #[cfg(feature = "test-faults")] test_metrics: Arc<
            crate::cas_projection::test_faults::ProviderBrokerTestMetrics,
        >,
    ) -> Self {
        Self {
            sender,
            ack,
            cancelled,
            #[cfg(feature = "test-faults")]
            test_home_id: home_id,
            #[cfg(feature = "test-faults")]
            test_metrics,
        }
    }

    fn exchange_special(
        &mut self,
        operation: BrokerOperation,
    ) -> Result<BrokerReply, (BrokerOperation, OrderedTurnStreamSubmitCause)> {
        if self.cancelled.load(Ordering::Acquire) || !self.ack.prepare() {
            return Err((operation, OrderedTurnStreamSubmitCause::Cancelled));
        }
        match self.sender.send_timeout(operation, BROKER_POLL_INTERVAL) {
            Ok(()) => {}
            Err(SendError::Full(operation)) => {
                return Err((operation, OrderedTurnStreamSubmitCause::CapacityFull));
            }
            Err(SendError::Timeout(operation)) => {
                return Err((operation, OrderedTurnStreamSubmitCause::Timeout));
            }
            Err(SendError::Closed(operation)) => {
                return Err((operation, OrderedTurnStreamSubmitCause::ReceiverLost));
            }
        }
        self.ack.wait(&self.cancelled).map_or_else(
            || panic!("provider broker closed without returning the specialized operation"),
            Ok,
        )
    }
}

impl OrderedTurnStreamSink for BrokerSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        #[cfg(feature = "test-faults")]
        if let OrderedTurnStreamOperation::Approval(request) = &operation
            && let Some(thread_id) = request.thread_id()
        {
            crate::cas_projection::test_faults::pause_approval_submit(
                thread_id,
                Arc::as_ptr(&self.cancelled) as usize,
            );
        }
        #[cfg(feature = "test-faults")]
        if is_provider_operation(&operation)
            && crate::cas_projection::test_faults::take_provider_submit_receiver_loss(
                crate::cas_projection::test_faults::ProviderTestKey::new(
                    self.test_home_id,
                    Arc::as_ptr(&self.cancelled) as usize,
                ),
            )
        {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::ReceiverLost,
            ));
        }
        #[cfg(feature = "test-faults")]
        if matches!(
            &operation,
            OrderedTurnStreamOperation::CheckedUserMessage(_)
        ) && crate::cas_projection::test_faults::take_checked_user_submit_receiver_loss(
            crate::cas_projection::test_faults::ProviderTestKey::new(
                self.test_home_id,
                Arc::as_ptr(&self.cancelled) as usize,
            ),
        ) {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::ReceiverLost,
            ));
        }
        if self.cancelled.load(Ordering::Acquire) || !self.ack.prepare() {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Cancelled,
            ));
        }
        #[cfg(feature = "test-faults")]
        let provider_seal = matches!(&operation, OrderedTurnStreamOperation::ProviderSeal(_));
        let admitted = BrokerOperation::new(operation);
        // Test accounting covers the pre-channel interval so an idle snapshot
        // cannot overtake an operation that is about to become receiver-visible.
        #[cfg(feature = "test-faults")]
        let pending_submission = self.test_metrics.begin_submission(provider_seal);
        match self.sender.send_timeout(admitted, BROKER_POLL_INTERVAL) {
            Ok(()) => {}
            Err(SendError::Full(admitted)) => {
                return Err(OrderedTurnStreamSubmitError::new(
                    admitted.into_operation(),
                    OrderedTurnStreamSubmitCause::CapacityFull,
                ));
            }
            Err(SendError::Timeout(admitted)) => {
                return Err(OrderedTurnStreamSubmitError::new(
                    admitted.into_operation(),
                    OrderedTurnStreamSubmitCause::Timeout,
                ));
            }
            Err(SendError::Closed(admitted)) => {
                return Err(OrderedTurnStreamSubmitError::new(
                    admitted.into_operation(),
                    OrderedTurnStreamSubmitCause::ReceiverLost,
                ));
            }
        }
        #[cfg(feature = "test-faults")]
        let in_flight = pending_submission.mark_submitted();
        let reply = self.ack.wait(&self.cancelled);
        #[cfg(feature = "test-faults")]
        if reply.is_some() {
            in_flight.acknowledge();
        }
        match reply {
            Some(BrokerReply::Applied(completion)) => Ok(completion),
            Some(BrokerReply::Rejected(operation, cause)) => {
                Err(OrderedTurnStreamSubmitError::new(operation, cause))
            }
            Some(_) => panic!("provider broker returned the wrong ordinary-operation reply"),
            None => {
                // Closure without an ownership-preserving acknowledgement is a
                // process invariant violation; the ingester guard normally
                // prevents this branch.
                panic!("provider broker closed without returning the submitted operation")
            }
        }
    }

    fn select_steering_user_message(
        &mut self,
        selection: SteeringUserMessageSelection,
    ) -> Result<SteeringUserMessageSource, SteeringUserMessageSelectionError> {
        let reply = match self.exchange_special(BrokerOperation::SteeringSelect(selection)) {
            Ok(reply) => reply,
            Err((BrokerOperation::SteeringSelect(selection), cause)) => {
                return Err(SteeringUserMessageSelectionError::new(selection, cause));
            }
            Err(_) => unreachable!("special exchange returned the wrong operation"),
        };
        match reply {
            BrokerReply::SteeringSelected(source) => Ok(source),
            BrokerReply::SteeringSelectionRejected(selection, cause) => {
                Err(SteeringUserMessageSelectionError::new(selection, cause))
            }
            _ => panic!("provider broker returned the wrong steering-selection reply"),
        }
    }

    fn submit_checked_steering_user_message(
        &mut self,
        message: CheckedSteeringUserMessage,
    ) -> Result<(), CheckedSteeringUserMessageSubmitError> {
        let reply = match self.exchange_special(BrokerOperation::SteeringChecked(message)) {
            Ok(reply) => reply,
            Err((BrokerOperation::SteeringChecked(message), cause)) => {
                return Err(CheckedSteeringUserMessageSubmitError::new(message, cause));
            }
            Err(_) => unreachable!("special exchange returned the wrong operation"),
        };
        match reply {
            BrokerReply::SteeringChecked => Ok(()),
            BrokerReply::SteeringCheckedRejected(message, cause) => {
                Err(CheckedSteeringUserMessageSubmitError::new(message, cause))
            }
            _ => panic!("provider broker returned the wrong checked-steering reply"),
        }
    }

    fn abandon_steering_user_message(
        &mut self,
        reason: SteeringUserMessageAbandonReason,
    ) -> Result<(), OrderedTurnStreamSubmitCause> {
        let reply = match self.exchange_special(BrokerOperation::SteeringAbandon(reason)) {
            Ok(reply) => reply,
            Err((BrokerOperation::SteeringAbandon(_), cause)) => return Err(cause),
            Err(_) => unreachable!("special exchange returned the wrong operation"),
        };
        match reply {
            BrokerReply::SteeringAbandoned => Ok(()),
            BrokerReply::SteeringAbandonRejected(cause) => Err(cause),
            _ => panic!("provider broker returned the wrong steering-abandon reply"),
        }
    }
}

#[cfg(feature = "test-faults")]
fn is_provider_operation(operation: &OrderedTurnStreamOperation) -> bool {
    matches!(
        operation,
        OrderedTurnStreamOperation::ProviderBegin(_)
            | OrderedTurnStreamOperation::ProviderControl(_)
            | OrderedTurnStreamOperation::ProviderAcquirePage
            | OrderedTurnStreamOperation::ProviderFragment(_)
            | OrderedTurnStreamOperation::ProviderSeal(_)
            | OrderedTurnStreamOperation::ProviderAbandon(_)
    )
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_channel.rs"
    ));
}
