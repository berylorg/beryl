use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use beryl_stream::ReceiveError;

use super::super::{
    BROKER_POLL_INTERVAL,
    channel::{BrokerOperation, BrokerReceiver},
};
use crate::cas_projection::connection::driver_outcome::ConnectionRoutingFailure;

#[derive(Default)]
pub(super) struct StickyRoutingFailure {
    failure: Mutex<Option<WholeConnectionRoutingFailure>>,
}

impl StickyRoutingFailure {
    pub(super) fn record(&self, failure: WholeConnectionRoutingFailure) {
        let mut slot = self
            .failure
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if slot.is_none() {
            *slot = Some(failure);
        }
    }

    pub(super) fn get(&self) -> Option<WholeConnectionRoutingFailure> {
        *self
            .failure
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

#[derive(Clone, Copy)]
pub(super) enum WholeConnectionRoutingFailure {
    Backend,
    Router,
}

impl From<WholeConnectionRoutingFailure> for ConnectionRoutingFailure {
    fn from(failure: WholeConnectionRoutingFailure) -> Self {
        match failure {
            WholeConnectionRoutingFailure::Backend => Self::Backend,
            WholeConnectionRoutingFailure::Router => Self::Router,
        }
    }
}

pub(super) fn receive_next(
    receiver: &BrokerReceiver,
    cancelled: &AtomicBool,
) -> Option<BrokerOperation> {
    loop {
        match receiver.receive_timeout(BROKER_POLL_INTERVAL) {
            Ok(Some(operation)) => return Some(operation),
            Ok(None) if cancelled.load(Ordering::Acquire) => return None,
            Ok(None) => {}
            Err(ReceiveError::Closed) => return None,
            Err(ReceiveError::Empty) => unreachable!("blocking receive never reports empty"),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum TargetRouteOutcome {
    Applied,
    TargetFailure,
    Terminal,
}
