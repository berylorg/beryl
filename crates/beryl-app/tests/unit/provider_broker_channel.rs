use std::{num::NonZeroUsize, sync::mpsc, thread, time::Duration};

use beryl_backend::{
    lifecycle_test_support::{provider_observation_fragment, thread_closed_operation},
    ProviderField, ProviderValueContext,
};
use beryl_model::CasThreadId;
use beryl_stream::{fixed_channel, PagePool};

use super::*;

fn broker_sink(
    sender: BrokerSender,
    ack: Arc<AckSlot>,
    cancelled: Arc<AtomicBool>,
    _seed: u8,
) -> BrokerSink {
    BrokerSink::new(
        sender,
        ack,
        cancelled,
        #[cfg(feature = "test-faults")]
        beryl_model::BerylHomeId::from_bytes([_seed; 16]),
        #[cfg(feature = "test-faults")]
        Arc::new(crate::cas_projection::test_faults::ProviderBrokerTestMetrics::default()),
    )
}

#[test]
fn terminal_completion_installs_ownership_while_closing_admission() {
    let ack = AckSlot::new();
    let cancelled = AtomicBool::new(false);
    ack.complete_terminal(BrokerReply::Rejected(
        OrderedTurnStreamOperation::ProviderAbandon(
            beryl_backend::ProviderObservationAbandonReason::Cancelled,
        ),
        OrderedTurnStreamSubmitCause::Cancelled,
    ));

    assert!(!cancelled.load(Ordering::Acquire));
    assert!(!ack.prepare());
    assert!(matches!(
        ack.wait(&cancelled),
        Some(BrokerReply::Rejected(
            OrderedTurnStreamOperation::ProviderAbandon(
                beryl_backend::ProviderObservationAbandonReason::Cancelled
            ),
            OrderedTurnStreamSubmitCause::Cancelled
        ))
    ));
    assert!(ack.wait(&cancelled).is_none());
}

#[test]
fn cancelled_broker_returns_thread_close_ownership() {
    let (sender, receiver) = fixed_channel(NonZeroUsize::MIN).unwrap();
    drop(receiver);
    let ack = Arc::new(AckSlot::new());
    ack.close();
    let cancelled = Arc::new(AtomicBool::new(true));
    let mut sink = broker_sink(sender, ack, cancelled, 180);
    let thread_id = CasThreadId::new("phase-80-post-cut-close").unwrap();
    let error = match sink.submit(thread_closed_operation(thread_id.clone())) {
        Ok(_) => panic!("cancelled broker accepted a thread-close operation"),
        Err(error) => error,
    };
    assert_eq!(error.cause(), OrderedTurnStreamSubmitCause::Cancelled);
    match error.into_operation() {
        OrderedTurnStreamOperation::ThreadClosed(closed) => {
            assert_eq!(closed.thread_id(), &thread_id);
        }
        operation => panic!("broker returned the wrong operation: {operation:?}"),
    }
}

#[test]
fn cancellation_during_fragment_ack_wait_releases_lease_for_later_broker_work() {
    let pages = PagePool::new(NonZeroUsize::new(16).unwrap(), NonZeroUsize::MIN).unwrap();
    let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
    let mut lease = pages.try_lease().unwrap();
    lease.buffer_mut()[..5].copy_from_slice(b"first");
    lease.set_len(5).unwrap();
    let first_generation = lease.generation();
    let fragment = provider_observation_fragment(context, lease);
    assert_eq!(pages.diagnostics().leased, 1);
    assert_eq!(pages.diagnostics().available, 0);

    let (sender, receiver) = fixed_channel(NonZeroUsize::MIN).unwrap();
    let ack = Arc::new(AckSlot::new());
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut sink = broker_sink(sender, Arc::clone(&ack), Arc::clone(&cancelled), 181);
    let (completed, completion) = mpsc::sync_channel(1);
    let submitter = thread::spawn(move || {
        completed
            .send(sink.submit(OrderedTurnStreamOperation::ProviderFragment(fragment)))
            .unwrap();
    });

    let operation = receiver
        .receive_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap()
        .into_operation();
    cancelled.store(true, Ordering::Release);
    ack.wake();
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    ack.complete(BrokerReply::Rejected(
        operation,
        OrderedTurnStreamSubmitCause::Cancelled,
    ));

    let error = completion
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap_err();
    assert_eq!(error.cause(), OrderedTurnStreamSubmitCause::Cancelled);
    let returned = match error.into_operation() {
        OrderedTurnStreamOperation::ProviderFragment(fragment) => fragment,
        operation => panic!("expected returned provider fragment, got {operation:?}"),
    };
    assert_eq!(returned.context(), context);
    assert_eq!(returned.bytes(), b"first");
    let mut returned_lease = returned.into_lease();
    assert_eq!(returned_lease.generation(), first_generation);
    returned_lease.clear();
    assert!(returned_lease.is_empty());
    drop(returned_lease);
    submitter.join().unwrap();
    drop(receiver);

    let released = pages.diagnostics();
    assert_eq!(released.leased, 0);
    assert_eq!(released.available, 1);

    let mut next_lease = pages.try_lease().unwrap();
    next_lease.buffer_mut()[..4].copy_from_slice(b"next");
    next_lease.set_len(4).unwrap();
    let next_generation = next_lease.generation();
    assert!(next_generation > first_generation);
    let next_fragment = provider_observation_fragment(context, next_lease);
    let (next_sender, next_receiver) = fixed_channel(NonZeroUsize::MIN).unwrap();
    let next_ack = Arc::new(AckSlot::new());
    let mut next_sink = broker_sink(
        next_sender,
        Arc::clone(&next_ack),
        Arc::new(AtomicBool::new(false)),
        182,
    );
    let (next_completed, next_completion) = mpsc::sync_channel(1);
    let next_submitter = thread::spawn(move || {
        next_completed
            .send(next_sink.submit(OrderedTurnStreamOperation::ProviderFragment(next_fragment)))
            .unwrap();
    });
    let next_operation = next_receiver
        .receive_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap()
        .into_operation();
    let next_returned = match next_operation {
        OrderedTurnStreamOperation::ProviderFragment(fragment) => fragment,
        operation => panic!("expected later provider fragment, got {operation:?}"),
    };
    assert_eq!(next_returned.context(), context);
    assert_eq!(next_returned.bytes(), b"next");
    let mut next_returned_lease = next_returned.into_lease();
    next_returned_lease.clear();
    next_ack.complete(BrokerReply::Applied(
        OrderedTurnStreamCompletion::PageLease(next_returned_lease),
    ));
    let next_completion = next_completion
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    let next_returned_lease = match next_completion {
        OrderedTurnStreamCompletion::PageLease(lease) => lease,
        OrderedTurnStreamCompletion::Applied => {
            panic!("later provider fragment returned the wrong completion")
        }
        OrderedTurnStreamCompletion::Approval(_) => {
            panic!("later provider fragment returned an approval completion")
        }
    };
    assert_eq!(next_returned_lease.generation(), next_generation);
    assert!(next_returned_lease.is_empty());
    drop(next_returned_lease);
    next_submitter.join().unwrap();

    let final_diagnostics = pages.diagnostics();
    assert_eq!(final_diagnostics.leased, 0);
    assert_eq!(final_diagnostics.available, 1);
    assert_eq!(final_diagnostics.high_water, 1);
    assert_eq!(final_diagnostics.total_leases, 2);
}
