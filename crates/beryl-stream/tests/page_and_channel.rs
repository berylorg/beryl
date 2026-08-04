use std::{num::NonZeroUsize, sync::mpsc, thread, time::Duration};

use beryl_stream::{
    ChannelBuildError, PagePool, PagePoolError, ReceiveError, SendError, fixed_channel,
};

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("test capacity is nonzero")
}

#[test]
fn page_pool_is_fixed_transferable_and_weakly_observable() {
    let pool = PagePool::new(nonzero(8), nonzero(2)).unwrap();
    let observer = pool.observer();
    assert_eq!(pool.diagnostics().page_capacity, 8);
    assert_eq!(pool.diagnostics().page_count, 2);

    let mut first = pool.try_lease().unwrap();
    first.buffer_mut()[..4].copy_from_slice(b"page");
    first.set_len(4).unwrap();
    let generation = thread::spawn(move || {
        assert_eq!(first.as_slice(), b"page");
        first.generation()
    })
    .join()
    .unwrap();
    assert_eq!(generation, 1);

    let mut second = pool.try_lease().unwrap();
    assert!(second.is_empty());
    assert!(second.buffer_mut().iter().all(|byte| *byte == 0));
    let third = pool.try_lease().unwrap();
    assert!(matches!(pool.try_lease(), Err(PagePoolError::Exhausted)));
    drop((second, third));

    let diagnostics = pool.diagnostics();
    assert_eq!(diagnostics.available, 2);
    assert_eq!(diagnostics.leased, 0);
    assert_eq!(diagnostics.high_water, 2);
    assert_eq!(diagnostics.total_leases, 3);
    assert_eq!(diagnostics.exhausted, 1);
    drop(pool);
    assert!(observer.diagnostics().is_none());
}

#[test]
fn page_lengths_are_checked_and_reuse_clears_all_bytes() {
    let pool = PagePool::new(nonzero(4), nonzero(1)).unwrap();
    let mut lease = pool.try_lease().unwrap();
    lease.buffer_mut().fill(0xff);
    assert!(matches!(
        lease.set_len(5),
        Err(PagePoolError::InvalidLength {
            requested: 5,
            capacity: 4
        })
    ));
    assert!(lease.is_empty());
    lease.set_len(4).unwrap();
    lease.clear();
    assert!(lease.is_empty());
    drop(lease);

    let mut reused = pool.try_lease().unwrap();
    assert!(reused.is_empty());
    assert_eq!(reused.buffer_mut(), [0; 4]);
}

#[test]
fn page_pool_rejects_capacity_multiplication_overflow() {
    assert!(matches!(
        PagePool::new(NonZeroUsize::MAX, nonzero(2)),
        Err(PagePoolError::SizeOverflow)
    ));
}

#[test]
fn fixed_channel_preserves_capacity_order_and_exact_messages() {
    let (sender, receiver) = fixed_channel::<String>(nonzero(2)).unwrap();
    let observer = sender.observer();
    sender.try_send("one".to_owned()).unwrap();
    sender.try_send("two".to_owned()).unwrap();
    assert_eq!(
        sender.try_send("three".to_owned()),
        Err(SendError::Full("three".to_owned()))
    );
    assert_eq!(receiver.try_receive().unwrap(), "one");
    assert_eq!(receiver.try_receive().unwrap(), "two");
    assert_eq!(receiver.try_receive(), Err(ReceiveError::Empty));

    let diagnostics = receiver.diagnostics();
    assert_eq!(diagnostics.capacity, 2);
    assert_eq!(diagnostics.sends, 2);
    assert_eq!(diagnostics.receives, 2);
    assert_eq!(diagnostics.full, 1);
    assert_eq!(diagnostics.high_water, 2);
    drop(sender);
    assert_eq!(receiver.try_receive(), Err(ReceiveError::Closed));
    drop(receiver);
    assert!(observer.diagnostics().is_none());
}

#[test]
fn fixed_channel_applies_backpressure_until_capacity_returns() {
    let (sender, receiver) = fixed_channel::<u64>(nonzero(1)).unwrap();
    let observer = sender.observer();
    sender.try_send(1).unwrap();
    let (ready_sender, ready_receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        ready_sender.send(()).unwrap();
        sender.send_timeout(2, Duration::from_secs(5))
    });
    ready_receiver.recv().unwrap();
    while observer.diagnostics().unwrap().send_waits == 0 {
        thread::yield_now();
    }

    assert_eq!(
        receiver.receive_timeout(Duration::from_secs(1)).unwrap(),
        Some(1)
    );
    assert_eq!(worker.join().unwrap(), Ok(()));
    assert_eq!(receiver.try_receive().unwrap(), 2);
}

#[test]
fn dropping_receiver_wakes_a_blocked_sender_with_its_message() {
    let (sender, receiver) = fixed_channel::<u64>(nonzero(1)).unwrap();
    let observer = sender.observer();
    sender.try_send(1).unwrap();
    let (ready_sender, ready_receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        ready_sender.send(()).unwrap();
        sender.send_timeout(2, Duration::from_secs(5))
    });
    ready_receiver.recv().unwrap();
    while observer.diagnostics().unwrap().send_waits == 0 {
        thread::yield_now();
    }
    drop(receiver);

    assert_eq!(worker.join().unwrap(), Err(SendError::Closed(2)));
}

#[test]
fn sender_close_preserves_queued_order_before_closed() {
    let (sender, receiver) = fixed_channel::<u64>(nonzero(2)).unwrap();
    sender.try_send(1).unwrap();
    sender.try_send(2).unwrap();
    drop(sender);

    assert_eq!(receiver.try_receive(), Ok(1));
    assert_eq!(receiver.try_receive(), Ok(2));
    assert_eq!(receiver.try_receive(), Err(ReceiveError::Closed));
}

#[test]
fn zero_timeout_results_are_distinct_and_preserve_messages() {
    let (sender, receiver) = fixed_channel::<u64>(nonzero(1)).unwrap();
    sender.try_send(1).unwrap();
    assert_eq!(
        sender.send_timeout(2, Duration::ZERO),
        Err(SendError::Timeout(2))
    );
    assert_eq!(receiver.try_receive(), Ok(1));
    assert_eq!(receiver.receive_timeout(Duration::ZERO), Ok(None));

    let diagnostics = receiver.diagnostics();
    assert_eq!(diagnostics.full, 0);
    assert_eq!(diagnostics.send_timeouts, 1);
    assert_eq!(diagnostics.receive_timeouts, 1);
}

#[test]
fn unrepresentable_deadlines_time_out_without_blocking() {
    let (sender, receiver) = fixed_channel::<u64>(nonzero(1)).unwrap();
    sender.try_send(1).unwrap();
    assert_eq!(
        sender.send_timeout(2, Duration::MAX),
        Err(SendError::Timeout(2))
    );
    assert_eq!(receiver.try_receive(), Ok(1));
    assert_eq!(receiver.receive_timeout(Duration::MAX), Ok(None));

    let diagnostics = receiver.diagnostics();
    assert_eq!(diagnostics.send_waits, 0);
    assert_eq!(diagnostics.receive_waits, 0);
    assert_eq!(diagnostics.send_timeouts, 1);
    assert_eq!(diagnostics.receive_timeouts, 1);
}

#[test]
fn fixed_channel_rejects_backing_size_overflow() {
    assert!(matches!(
        fixed_channel::<u64>(NonZeroUsize::MAX),
        Err(ChannelBuildError::SizeOverflow)
    ));
}
