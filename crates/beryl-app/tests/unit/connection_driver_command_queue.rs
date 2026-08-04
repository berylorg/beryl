use std::{sync::Arc, time::Duration};

use beryl_stream::SendError;

use super::*;

fn no_op_command() -> DriverCommand {
    test_driver_command(Box::new(|_, _, _| {}))
}

#[test]
fn command_queue_backpressures_at_exact_capacity_and_reuses_its_ring() {
    let (sender, receiver) = connection_command_channel().unwrap();
    let ring_observer = sender.observer();

    for _ in 0..CONNECTION_COMMAND_QUEUE_LIMIT {
        assert!(sender.try_send(no_op_command()).is_ok());
    }
    let full = sender.diagnostics();
    assert_eq!(full.capacity, CONNECTION_COMMAND_QUEUE_LIMIT);
    assert_eq!(full.len, CONNECTION_COMMAND_QUEUE_LIMIT);
    assert_eq!(full.high_water, CONNECTION_COMMAND_QUEUE_LIMIT);

    let ownership = Arc::new(());
    let ownership_observer = Arc::downgrade(&ownership);
    let retained_ownership = Arc::clone(&ownership);
    drop(ownership);
    let overflow = test_driver_command(Box::new(move |_, _, _| drop(retained_ownership)));
    let returned = match sender.send_timeout(overflow, Duration::from_millis(5)) {
        Err(SendError::Timeout(returned)) => returned,
        Ok(()) => panic!("the one-over command must not enter the full queue"),
        Err(SendError::Full(_)) => panic!("timed send must wait before reporting capacity"),
        Err(SendError::Closed(_)) => panic!("both command-queue endpoints remain open"),
    };
    assert!(ownership_observer.upgrade().is_some());
    let backpressured = sender.diagnostics();
    assert_eq!(backpressured.len, CONNECTION_COMMAND_QUEUE_LIMIT);
    assert_eq!(backpressured.send_waits, 1);
    assert_eq!(backpressured.send_timeouts, 1);
    assert_eq!(backpressured.high_water, CONNECTION_COMMAND_QUEUE_LIMIT);

    drop(receiver.try_receive().unwrap());
    assert!(sender.try_send(returned).is_ok());
    for _ in 0..CONNECTION_COMMAND_QUEUE_LIMIT {
        drop(receiver.try_receive().unwrap());
    }
    assert!(ownership_observer.upgrade().is_none());
    assert_eq!(sender.diagnostics().len, 0);

    for _ in 0..3 {
        for _ in 0..CONNECTION_COMMAND_QUEUE_LIMIT {
            assert!(sender.try_send(no_op_command()).is_ok());
        }
        assert_eq!(sender.diagnostics().len, CONNECTION_COMMAND_QUEUE_LIMIT);
        for _ in 0..CONNECTION_COMMAND_QUEUE_LIMIT {
            drop(receiver.try_receive().unwrap());
        }
        let drained = sender.diagnostics();
        assert_eq!(drained.len, 0);
        assert_eq!(drained.high_water, CONNECTION_COMMAND_QUEUE_LIMIT);
    }

    drop(sender);
    let sender_closed = ring_observer.diagnostics().unwrap();
    assert!(!sender_closed.sender_open);
    assert!(sender_closed.receiver_open);
    drop(receiver);
    assert!(ring_observer.diagnostics().is_none());
}
