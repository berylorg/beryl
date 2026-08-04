use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    ProviderFrameObservationSummaryV1, ProviderItemKind, ProviderLifecycleTimestampMsV1,
};

use crate::cas_projection::stop::{
    PublishedHardStopActivityKind, PublishedHardStopActivityLifecycle,
};

use super::{PublishedHardStopActivityEffect, hard_stop_activity_transition};

#[test]
fn only_command_start_publishes_an_active_effect() {
    assert_eq!(
        hard_stop_activity_transition(
            ProviderItemKind::CommandExecution,
            ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(7),),
        ),
        Some((
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Active,
        ))
    );
    assert_eq!(
        hard_stop_activity_transition(
            ProviderItemKind::CommandExecution,
            ProviderFrameObservationSummaryV1::Delta,
        ),
        None
    );
}

#[test]
fn command_completion_publishes_completed_effect() {
    assert_eq!(
        hard_stop_activity_transition(
            ProviderItemKind::CommandExecution,
            ProviderFrameObservationSummaryV1::Completed(ProviderLifecycleTimestampMsV1::new(8),),
        ),
        Some((
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Completed,
        ))
    );
}

#[test]
fn unrelated_provider_frames_do_not_enter_hard_stop_activity() {
    assert_eq!(
        hard_stop_activity_transition(
            ProviderItemKind::FileChange,
            ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(7),),
        ),
        None
    );
}

#[test]
fn completion_only_subagent_frame_maps_to_the_unsupported_child_family() {
    assert_eq!(
        hard_stop_activity_transition(
            ProviderItemKind::SubAgentActivity,
            ProviderFrameObservationSummaryV1::Completed(ProviderLifecycleTimestampMsV1::new(9),),
        ),
        Some((
            PublishedHardStopActivityKind::ChildOrSubagent,
            PublishedHardStopActivityLifecycle::Completed,
        ))
    );
}

#[test]
fn command_effect_retains_only_exact_durable_identity_and_lifecycle() {
    let syndic_thread_id = SyndicThreadId::from_bytes([1; 16]);
    let syndic_turn_id = SyndicTurnId::from_bytes([2; 16]);
    let item_id = SyndicItemId::from_bytes([3; 16]);
    let effect = PublishedHardStopActivityEffect {
        syndic_thread_id,
        syndic_turn_id,
        item_id,
        kind: PublishedHardStopActivityKind::Command,
        lifecycle: PublishedHardStopActivityLifecycle::Active,
    };

    assert_eq!(effect.syndic_thread_id, syndic_thread_id);
    assert_eq!(effect.syndic_turn_id, syndic_turn_id);
    assert_eq!(effect.item_id, item_id);
    assert_eq!(effect.kind, PublishedHardStopActivityKind::Command);
    assert_eq!(effect.lifecycle, PublishedHardStopActivityLifecycle::Active);
}
