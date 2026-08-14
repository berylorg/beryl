#![allow(dead_code)]

#[path = "../src/shell/notifications.rs"]
mod notifications;

use std::{path::PathBuf, sync::mpsc};

use notifications::{
    DefaultOutputDeviceIdentity, LifecycleNotificationCandidate, LifecycleNotificationKind,
    NotificationSoundEnqueueResult, TurnCompletionSoundCandidate,
    should_reopen_for_default_output_device,
};

#[test]
fn notification_sound_queue_reports_enqueue_full_and_disconnected() {
    let (sender, receiver) = mpsc::sync_channel(1);

    assert_eq!(
        notifications::try_enqueue_sound_path(&sender, PathBuf::from("C:\\sound\\one.wav")),
        NotificationSoundEnqueueResult::Enqueued
    );
    assert_eq!(
        notifications::try_enqueue_sound_path(&sender, PathBuf::from("C:\\sound\\two.wav")),
        NotificationSoundEnqueueResult::QueueFull
    );

    assert_eq!(
        receiver.try_recv().unwrap(),
        PathBuf::from("C:\\sound\\one.wav")
    );
    drop(receiver);
    assert_eq!(
        notifications::try_enqueue_sound_path(&sender, PathBuf::from("C:\\sound\\three.wav")),
        NotificationSoundEnqueueResult::WorkerStopped
    );
}

#[test]
fn turn_completion_sound_candidate_carries_turn_identity() {
    assert_eq!(
        TurnCompletionSoundCandidate::new(Some("thread_1".into()), Some("turn_1".into())),
        TurnCompletionSoundCandidate {
            thread_id: Some("thread_1".into()),
            turn_id: Some("turn_1".into())
        }
    );
}

#[test]
fn lifecycle_notification_candidate_carries_distinct_event_kind() {
    assert_eq!(
        LifecycleNotificationCandidate::new(
            Some("thread_1".into()),
            Some("turn_1".into()),
            LifecycleNotificationKind::PlanComplete,
        ),
        LifecycleNotificationCandidate {
            thread_id: Some("thread_1".into()),
            turn_id: Some("turn_1".into()),
            kind: LifecycleNotificationKind::PlanComplete,
        }
    );
}

#[test]
fn default_output_device_comparison_only_reopens_for_known_changes() {
    let speakers = DefaultOutputDeviceIdentity::new("wasapi:speakers");
    let headset = DefaultOutputDeviceIdentity::new("wasapi:headset");

    assert!(!should_reopen_for_default_output_device(
        Some(&speakers),
        Some(&speakers)
    ));
    assert!(should_reopen_for_default_output_device(
        Some(&speakers),
        Some(&headset)
    ));
    assert!(should_reopen_for_default_output_device(
        None,
        Some(&headset)
    ));
    assert!(!should_reopen_for_default_output_device(
        Some(&speakers),
        None
    ));
    assert!(!should_reopen_for_default_output_device(None, None));
}
