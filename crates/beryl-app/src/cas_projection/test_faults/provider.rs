use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_model::{BerylHomeId, ProviderObservationId};

use super::super::AdmittedProjectionSession;
use super::NEXT_TOKEN;

mod checked_user;
mod receiver_loss;
#[cfg(test)]
mod steering;

#[cfg(test)]
pub(crate) use checked_user::install_checked_user_publication_barrier_for_key;
pub use checked_user::{
    CheckedUserPublicationBarrierController, CheckedUserPublicationSnapshot,
    CheckedUserSubmitReceiverLossController, install_checked_user_publication_barrier,
    install_checked_user_submit_receiver_loss,
};
pub(crate) use checked_user::{
    CheckedUserPublicationGuard, CheckedUserPublicationMetrics, pause_checked_user_publication,
    take_checked_user_submit_receiver_loss,
};
pub(crate) use receiver_loss::take_provider_submit_receiver_loss;
pub use receiver_loss::{
    ProviderSubmitReceiverLossController, install_provider_submit_receiver_loss,
};
#[cfg(test)]
pub(crate) use steering::{
    install_steering_selection_barrier_for_key, pause_steering_after_selection,
};

/// Named content-free diagnostics from the most recently decoded WebSocket message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebSocketIngressSnapshot {
    message_bytes: usize,
    maximum_transport_chunk_bytes: usize,
    maximum_parser_buffer_bytes: usize,
    discarded_image_result_bytes: usize,
    retained_item_result_present: bool,
}

impl WebSocketIngressSnapshot {
    pub(crate) const fn from_backend(metrics: (usize, usize, usize, usize, bool)) -> Self {
        Self {
            message_bytes: metrics.0,
            maximum_transport_chunk_bytes: metrics.1,
            maximum_parser_buffer_bytes: metrics.2,
            discarded_image_result_bytes: metrics.3,
            retained_item_result_present: metrics.4,
        }
    }

    /// Returns total wire-message bytes accepted by the incremental ingress path.
    #[must_use]
    pub const fn message_bytes(self) -> usize {
        self.message_bytes
    }

    /// Returns the largest transport chunk lent to the message reader.
    #[must_use]
    pub const fn maximum_transport_chunk_bytes(self) -> usize {
        self.maximum_transport_chunk_bytes
    }

    /// Returns the largest resident incremental-parser input window.
    #[must_use]
    pub const fn maximum_parser_buffer_bytes(self) -> usize {
        self.maximum_parser_buffer_bytes
    }

    /// Returns image-result bytes deliberately discarded without retention.
    #[must_use]
    pub const fn discarded_image_result_bytes(self) -> usize {
        self.discarded_image_result_bytes
    }

    /// Reports whether the bounded item-result slot retained a value.
    #[must_use]
    pub const fn retained_item_result_present(self) -> bool {
        self.retained_item_result_present
    }
}

/// Reads the named ingress snapshot through this exact connection's request session.
pub fn last_websocket_ingress_snapshot(
    session: &AdmittedProjectionSession,
) -> Result<Option<WebSocketIngressSnapshot>, super::super::ProjectionExecutionError> {
    session.last_websocket_ingress_test_snapshot()
}

/// Current and high-water counts for one provider broker operation class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderOperationCountSnapshot {
    current: usize,
    high_water: usize,
}

impl ProviderOperationCountSnapshot {
    /// Returns the number of operations currently inside the observed interval.
    #[must_use]
    pub const fn current(self) -> usize {
        self.current
    }

    /// Returns the highest simultaneous count observed by this broker.
    #[must_use]
    pub const fn high_water(self) -> usize {
        self.high_water
    }
}

/// One content-free snapshot of a connection's capacity-one provider broker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderBrokerSnapshot {
    in_flight: ProviderOperationCountSnapshot,
    submitted: usize,
    acked: usize,
    provider_seal_acks: usize,
    provider_staging_batches: usize,
    staged_fragments: ProviderOperationCountSnapshot,
    staged_fragment_batches: usize,
    checked_user_publications: CheckedUserPublicationSnapshot,
}

impl ProviderBrokerSnapshot {
    /// Returns broker submissions currently awaiting acknowledgement and their high-water mark.
    #[must_use]
    pub const fn in_flight(self) -> ProviderOperationCountSnapshot {
        self.in_flight
    }

    /// Returns operations successfully submitted to the broker channel.
    #[must_use]
    pub const fn submitted(self) -> usize {
        self.submitted
    }

    /// Returns submitted operations returned through the acknowledgement cell.
    #[must_use]
    pub const fn acked(self) -> usize {
        self.acked
    }

    /// Returns provider-seal operations returned through the acknowledgement cell.
    #[must_use]
    pub const fn provider_seal_acks(self) -> usize {
        self.provider_seal_acks
    }

    /// Returns every durable provider-observation staging callback entered by this broker.
    #[must_use]
    pub const fn provider_staging_batches(self) -> usize {
        self.provider_staging_batches
    }

    /// Returns fragment stage batches transiently executing and their high-water mark.
    #[must_use]
    pub const fn staged_fragments(self) -> ProviderOperationCountSnapshot {
        self.staged_fragments
    }

    /// Returns the total number of fragment stage batches entered by this broker.
    #[must_use]
    pub const fn staged_fragment_batches(self) -> usize {
        self.staged_fragment_batches
    }

    /// Returns source-permit publication ownership and work for checked user echoes.
    #[must_use]
    pub const fn checked_user_publications(self) -> CheckedUserPublicationSnapshot {
        self.checked_user_publications
    }
}

/// Reads the feature-only counters attached to this exact admitted connection's broker.
#[must_use]
pub fn provider_broker_snapshot(session: &AdmittedProjectionSession) -> ProviderBrokerSnapshot {
    session.provider_broker_test_snapshot()
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProviderTestKey {
    home_id: BerylHomeId,
    broker_id: usize,
}

impl ProviderTestKey {
    pub(crate) const fn new(home_id: BerylHomeId, broker_id: usize) -> Self {
        Self { home_id, broker_id }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProviderBrokerTestMetrics {
    in_flight_current: AtomicUsize,
    in_flight_high_water: AtomicUsize,
    submitted: AtomicUsize,
    acked: AtomicUsize,
    provider_seal_acks: AtomicUsize,
    provider_staging_batches: AtomicUsize,
    staged_fragment_current: AtomicUsize,
    staged_fragment_high_water: AtomicUsize,
    staged_fragment_batches: AtomicUsize,
    checked_user_publications: CheckedUserPublicationMetrics,
}

impl ProviderBrokerTestMetrics {
    pub(crate) fn begin_submission(&self, provider_seal: bool) -> BrokerInFlightGuard<'_> {
        acquire(&self.in_flight_current, &self.in_flight_high_water);
        BrokerInFlightGuard {
            metrics: self,
            submitted: false,
            provider_seal,
        }
    }

    pub(crate) fn begin_staged_fragment(&self) -> StagedFragmentGuard<'_> {
        acquire(
            &self.staged_fragment_current,
            &self.staged_fragment_high_water,
        );
        increment(
            &self.staged_fragment_batches,
            "provider fragment stage batch count overflowed",
        );
        StagedFragmentGuard { metrics: self }
    }

    pub(crate) fn record_provider_staging_batch(&self) {
        increment(
            &self.provider_staging_batches,
            "provider staging batch count overflowed",
        );
    }

    pub(crate) fn begin_checked_user_publication(&self) -> CheckedUserPublicationGuard<'_> {
        self.checked_user_publications.begin()
    }

    pub(crate) fn snapshot(&self) -> ProviderBrokerSnapshot {
        ProviderBrokerSnapshot {
            in_flight: ProviderOperationCountSnapshot {
                current: self.in_flight_current.load(Ordering::SeqCst),
                high_water: self.in_flight_high_water.load(Ordering::SeqCst),
            },
            submitted: self.submitted.load(Ordering::SeqCst),
            acked: self.acked.load(Ordering::SeqCst),
            provider_seal_acks: self.provider_seal_acks.load(Ordering::SeqCst),
            provider_staging_batches: self.provider_staging_batches.load(Ordering::SeqCst),
            staged_fragments: ProviderOperationCountSnapshot {
                current: self.staged_fragment_current.load(Ordering::SeqCst),
                high_water: self.staged_fragment_high_water.load(Ordering::SeqCst),
            },
            staged_fragment_batches: self.staged_fragment_batches.load(Ordering::SeqCst),
            checked_user_publications: self.checked_user_publications.snapshot(),
        }
    }
}

pub(crate) struct BrokerInFlightGuard<'a> {
    metrics: &'a ProviderBrokerTestMetrics,
    submitted: bool,
    provider_seal: bool,
}

impl BrokerInFlightGuard<'_> {
    pub(crate) fn mark_submitted(mut self) -> Self {
        increment(
            &self.metrics.submitted,
            "provider broker submission count overflowed",
        );
        self.submitted = true;
        self
    }

    pub(crate) fn acknowledge(self) {
        assert!(
            self.submitted,
            "provider broker acknowledgement preceded channel submission"
        );
        increment(
            &self.metrics.acked,
            "provider broker acknowledgement count overflowed",
        );
        if self.provider_seal {
            increment(
                &self.metrics.provider_seal_acks,
                "provider broker seal acknowledgement count overflowed",
            );
        }
    }
}

impl Drop for BrokerInFlightGuard<'_> {
    fn drop(&mut self) {
        release(
            &self.metrics.in_flight_current,
            "provider broker in-flight count underflowed",
        );
    }
}

pub(crate) struct StagedFragmentGuard<'a> {
    metrics: &'a ProviderBrokerTestMetrics,
}

impl Drop for StagedFragmentGuard<'_> {
    fn drop(&mut self) {
        release(
            &self.metrics.staged_fragment_current,
            "provider fragment stage count underflowed",
        );
    }
}

fn acquire(current: &AtomicUsize, high_water: &AtomicUsize) {
    let previous = current
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
            value.checked_add(1)
        })
        .expect("provider test activity count overflowed");
    high_water.fetch_max(previous + 1, Ordering::SeqCst);
}

fn release(current: &AtomicUsize, message: &'static str) {
    current
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
            value.checked_sub(1)
        })
        .expect(message);
}

fn increment(counter: &AtomicUsize, message: &'static str) {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
            value.checked_add(1)
        })
        .expect(message);
}

static PROVIDER_STAGE_BARRIERS: OnceLock<
    Mutex<HashMap<ProviderTestKey, Arc<ProviderStageBarrier>>>,
> = OnceLock::new();

#[derive(Default)]
struct ProviderStageBarrierState {
    staged: bool,
    observation: Option<ProviderObservationId>,
    cancellation: bool,
    released: bool,
}

struct ProviderStageBarrier {
    token: u64,
    state: Mutex<ProviderStageBarrierState>,
    changed: Condvar,
}

impl ProviderStageBarrier {
    fn new(token: u64) -> Self {
        Self {
            token,
            state: Mutex::new(ProviderStageBarrierState::default()),
            changed: Condvar::new(),
        }
    }

    fn release(&self) {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .released = true;
        self.changed.notify_all();
    }
}

/// Controls one fragment stage paused before cancellation and durable execution.
pub struct ProviderFragmentStageBarrierController {
    key: ProviderTestKey,
    barrier: Arc<ProviderStageBarrier>,
}

/// Installs one content-free fragment-stage barrier for this exact home and broker.
pub fn install_provider_fragment_stage_barrier(
    session: &AdmittedProjectionSession,
) -> ProviderFragmentStageBarrierController {
    let key = session.provider_test_key();
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let barrier = Arc::new(ProviderStageBarrier::new(token));
    let previous = stage_barriers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(key, Arc::clone(&barrier));
    if let Some(previous) = previous {
        previous.release();
    }
    ProviderFragmentStageBarrierController { key, barrier }
}

impl ProviderFragmentStageBarrierController {
    /// Waits until the first fragment batch for this exact broker reaches staging.
    pub fn wait_for_stage(&self) {
        let mut state = self
            .barrier
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while !state.staged {
            state = self
                .barrier
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    /// Returns the opaque observation identity at the reached fragment boundary.
    #[must_use]
    pub fn observation_id(&self) -> ProviderObservationId {
        self.barrier
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .observation
            .clone()
            .expect("provider fragment stage has not been reached")
    }

    /// Waits until cancellation is requested on the exact paused broker.
    pub fn wait_for_cancellation(&self) {
        let mut state = self
            .barrier
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while !state.cancellation {
            state = self
                .barrier
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    /// Releases the paused stage so it can observe cancellation or execute.
    pub fn release(&self) {
        self.barrier.release();
    }
}

impl Drop for ProviderFragmentStageBarrierController {
    fn drop(&mut self) {
        self.barrier.release();
        let mut barriers = stage_barriers()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if barriers
            .get(&self.key)
            .is_some_and(|barrier| barrier.token == self.barrier.token)
        {
            barriers.remove(&self.key);
        }
    }
}

pub(crate) fn pause_provider_fragment_stage(
    key: ProviderTestKey,
    observation: ProviderObservationId,
) {
    let barrier = stage_barriers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .cloned();
    let Some(barrier) = barrier else {
        return;
    };
    let mut state = barrier
        .state
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if state.staged {
        return;
    }
    state.staged = true;
    state.observation = Some(observation);
    barrier.changed.notify_all();
    while !state.released {
        state = barrier
            .changed
            .wait(state)
            .unwrap_or_else(|poison| poison.into_inner());
    }
}

pub(crate) fn observe_provider_stage_cancellation(key: ProviderTestKey) {
    let barrier = stage_barriers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .cloned();
    if let Some(barrier) = barrier {
        barrier
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .cancellation = true;
        barrier.changed.notify_all();
    }
}

fn stage_barriers() -> &'static Mutex<HashMap<ProviderTestKey, Arc<ProviderStageBarrier>>> {
    PROVIDER_STAGE_BARRIERS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submission_accounting_covers_channel_delivery_and_exact_seal_ack() {
        let metrics = ProviderBrokerTestMetrics::default();
        let pending = metrics.begin_submission(true);
        let before_delivery = metrics.snapshot();
        assert_eq!(before_delivery.in_flight().current(), 1);
        assert_eq!(before_delivery.submitted(), 0);
        assert_eq!(before_delivery.acked(), 0);
        assert_eq!(before_delivery.provider_seal_acks(), 0);

        let submitted = pending.mark_submitted();
        let before_ack = metrics.snapshot();
        assert_eq!(before_ack.in_flight().current(), 1);
        assert_eq!(before_ack.submitted(), 1);
        assert_eq!(before_ack.acked(), 0);
        assert_eq!(before_ack.provider_seal_acks(), 0);

        submitted.acknowledge();
        let settled = metrics.snapshot();
        assert_eq!(settled.in_flight().current(), 0);
        assert_eq!(settled.submitted(), 1);
        assert_eq!(settled.acked(), 1);
        assert_eq!(settled.provider_seal_acks(), 1);
    }
}
