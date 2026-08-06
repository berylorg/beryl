use std::{
    io::Cursor,
    sync::{Arc, mpsc},
    time::Duration,
};

#[path = "../src/diagnostic_child_protocol.rs"]
mod diagnostic_child_protocol;
#[path = "../src/shell/liveness_diagnostics.rs"]
pub(crate) mod liveness_diagnostics;

mod shell {
    pub(crate) use crate::liveness_diagnostics;
}

#[path = "../src/diagnostic_child_target.rs"]
mod diagnostic_child_target;

use diagnostic_child_protocol::{
    DiagnosticProtocolResponse, parse_request_frame, parse_response_frame, response_frame,
};
use diagnostic_child_target::{
    DiagnosticTargetShellRequest, diagnostic_target_request_channel_for_test,
    run_diagnostic_target_stdio_loop,
};
use liveness_diagnostics::{
    LivenessCategory, LivenessFlags, LivenessStage, LivenessTransition, PollGenerationOutcome,
    PollGenerationSnapshot, PollScheduleDecision, PollScheduleLane, PollSchedulerState,
    RECEIVER_DIAGNOSTIC_TARGET, RECEIVER_SHELL_TOOL, RECEIVER_TURN, ShellLivenessDiagnostics,
    TRACE_CAPACITY,
};
#[test]
fn overwrite_ring_is_bounded_and_metadata_only() {
    let diagnostics = ShellLivenessDiagnostics::new();
    for _ in 0..(TRACE_CAPACITY + 17) {
        diagnostics.record(
            LivenessTransition::TurnEventIngress,
            LivenessCategory::AgentMessageDelta,
            LivenessFlags::default(),
        );
    }

    let snapshot = diagnostics.snapshot_value();
    let events = snapshot["events"].as_array().unwrap();
    assert_eq!(snapshot["capacity"], TRACE_CAPACITY);
    assert_eq!(events.len(), TRACE_CAPACITY);
    assert_eq!(events.first().unwrap()["sequence"], 18);
    assert_eq!(
        events.last().unwrap()["sequence"],
        (TRACE_CAPACITY + 17) as u64
    );
    for event in events {
        let keys = event.as_object().unwrap().keys().map(String::as_str);
        assert!(keys.clone().all(|key| matches!(
            key,
            "generation"
                | "sequence"
                | "elapsedMicros"
                | "stage"
                | "transition"
                | "category"
                | "flags"
                | "diagnosticQueueOccupancy"
        )));
    }
    let encoded = serde_json::to_string(&snapshot).unwrap();
    for forbidden in [
        "transcript",
        "developerInstructions",
        "arguments",
        "results",
        "settings",
        "media",
        "path",
        "errorText",
    ] {
        assert!(!encoded.contains(forbidden));
    }
}

#[test]
fn heartbeat_generation_sequence_and_stage_are_monotonic() {
    let diagnostics = ShellLivenessDiagnostics::new();
    diagnostics.shell_state(
        Some(poll_snapshot(1, PollGenerationOutcome::Armed)),
        None,
        None,
        None,
        RECEIVER_DIAGNOSTIC_TARGET | RECEIVER_SHELL_TOOL | RECEIVER_TURN,
    );
    diagnostics.timer_arm(LivenessCategory::FrameTimer, 1);
    let before = serialized_heartbeat(&diagnostics);
    diagnostics.timer_fire(LivenessCategory::FrameTimer, 1);
    {
        let _stage = diagnostics.enter_stage(LivenessStage::TurnUpdates);
        diagnostics.record(
            LivenessTransition::TurnUpdateApplyEnter,
            LivenessCategory::TurnEventUpdate,
            LivenessFlags::default(),
        );
        let active = serialized_heartbeat(&diagnostics);
        assert_eq!(active["stage"], "turn_updates");
        assert!(active["sequence"].as_u64() > before["sequence"].as_u64());
        assert_eq!(active["generation"], 1);
        assert_eq!(active["framePollScheduled"], true);
        assert_eq!(active["activeReceiverBits"], 14_336);
    }
    let after = serialized_heartbeat(&diagnostics);
    assert_eq!(after["stage"], "none");
    assert!(after["sequence"].as_u64() > before["sequence"].as_u64());
    assert!(after["elapsedMicros"].as_u64() >= before["elapsedMicros"].as_u64());
}

#[test]
fn capacity_sixteen_timeout_full_expiry_and_drain_keep_exact_occupancy() {
    let diagnostics = Arc::new(ShellLivenessDiagnostics::new());
    let (sender, receiver) =
        diagnostic_target_request_channel_for_test(16, Duration::ZERO, Arc::clone(&diagnostics));

    for index in 0..16 {
        let request = request(index);
        let response = sender.request(request);
        let response = parse_response_frame(&response_frame(response))
            .unwrap()
            .unwrap();
        let error = response.into_result().unwrap_err();
        assert_eq!(error.kind(), "shell_timeout");
    }
    let full = sender.request(request(16));
    let full = parse_response_frame(&response_frame(full))
        .unwrap()
        .unwrap();
    assert_eq!(full.into_result().unwrap_err().kind(), "shell_busy");

    let full_heartbeat = serialized_heartbeat(&diagnostics);
    assert_eq!(full_heartbeat["diagnosticQueueOccupancy"], 16);
    assert_eq!(full_heartbeat["diagnosticQueueHighWater"], 16);

    for _ in 0..16 {
        let accounting = diagnostics.try_diagnostic_dequeue_begin().unwrap();
        let queued = receiver.try_recv().unwrap();
        accounting.commit();
        match queued {
            DiagnosticTargetShellRequest::Execute(request) => {
                assert!(!request.try_claim());
            }
            DiagnosticTargetShellRequest::Shutdown => panic!("test did not enqueue shutdown"),
        }
    }
    assert!(receiver.try_recv().is_err());
    assert_eq!(
        serialized_heartbeat(&diagnostics)["diagnosticQueueOccupancy"],
        0
    );
}

#[test]
fn successful_publication_commits_before_nonblocking_dequeue() {
    let diagnostics = ShellLivenessDiagnostics::new();
    let (sender, receiver) = mpsc::sync_channel(1);

    let publication = diagnostics.diagnostic_enqueue_begin();
    sender.try_send(()).unwrap();
    assert!(diagnostics.try_diagnostic_dequeue_begin().is_none());
    assert_eq!(
        serialized_heartbeat(&diagnostics)["diagnosticQueueOccupancy"],
        0
    );

    publication.commit();
    let consumption = diagnostics.try_diagnostic_dequeue_begin().unwrap();
    receiver.try_recv().unwrap();
    consumption.commit();

    let snapshot = diagnostics.snapshot_value();
    let heartbeat = &snapshot["heartbeat"];
    assert_eq!(heartbeat["diagnosticQueueHighWater"], 1);
    assert_eq!(heartbeat["diagnosticQueueOccupancy"], 0);
    let transitions = snapshot["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["transition"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(transitions, ["diagnostic_enqueue", "diagnostic_dequeue"]);
}

#[test]
fn full_publication_rollback_does_not_change_occupancy() {
    let diagnostics = ShellLivenessDiagnostics::new();
    let (sender, _receiver) = mpsc::sync_channel(1);
    sender.try_send(()).unwrap();

    let publication = diagnostics.diagnostic_enqueue_begin();
    assert!(matches!(
        sender.try_send(()),
        Err(mpsc::TrySendError::Full(()))
    ));
    publication.rollback(true);

    let snapshot = diagnostics.snapshot_value();
    assert_eq!(snapshot["heartbeat"]["diagnosticQueueOccupancy"], 0);
    assert_eq!(snapshot["heartbeat"]["diagnosticQueueHighWater"], 0);
    assert_eq!(snapshot["events"][0]["transition"], "diagnostic_full");
}

#[test]
fn direct_reads_remain_available_during_work_and_after_simulated_gpui_stall() {
    let diagnostics = Arc::new(ShellLivenessDiagnostics::new());
    diagnostics.shell_state(
        Some(poll_snapshot(1, PollGenerationOutcome::Armed)),
        None,
        None,
        None,
        RECEIVER_DIAGNOSTIC_TARGET | RECEIVER_SHELL_TOOL | RECEIVER_TURN,
    );
    diagnostics.timer_arm(LivenessCategory::FrameTimer, 1);
    diagnostics.timer_fire(LivenessCategory::FrameTimer, 1);
    diagnostics.window_update_attempt(LivenessCategory::FrameTimer, 1);
    diagnostics.record(
        LivenessTransition::ShellToolEnqueue,
        LivenessCategory::ShellDynamicTool,
        LivenessFlags::default(),
    );
    diagnostics.record(
        LivenessTransition::TurnEventIngress,
        LivenessCategory::DynamicToolCall,
        LivenessFlags::default(),
    );

    let response = {
        let _stage = diagnostics.enter_stage(LivenessStage::ShellDynamicTool);
        scripted_direct_read(Arc::clone(&diagnostics), "active")
    };
    let active = response.into_result().unwrap();
    assert_eq!(active["heartbeat"]["stage"], "shell_dynamic_tool");
    assert!(active["events"].as_array().unwrap().iter().any(|event| {
        event["transition"] == "shell_tool_enqueue" && event["category"] == "shell_dynamic_tool"
    }));

    let stalled = scripted_direct_read(Arc::clone(&diagnostics), "stalled")
        .into_result()
        .unwrap();
    assert_eq!(stalled["heartbeat"]["framePollScheduled"], true);
    assert!(stalled["heartbeat"]["lastWindowUpdateAttemptMicros"].is_number());
    assert!(stalled["heartbeat"]["lastWindowUpdateOutcome"].is_null());
}

#[test]
fn successful_timer_delivery_releases_exact_generation_and_rearms_pending_work() {
    let mut scheduler = PollSchedulerState::default();
    let generation = scheduler
        .arm_if_pending(PollScheduleLane::Frame, true)
        .unwrap();
    assert!(
        scheduler
            .arm_if_pending(PollScheduleLane::Frame, true)
            .is_none()
    );
    assert_eq!(
        scheduler.timer_delivered(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Retry
    );
    assert_eq!(
        scheduler.active(PollScheduleLane::Frame),
        Some(poll_snapshot(
            generation,
            PollGenerationOutcome::TimerDelivered
        ))
    );
    assert_eq!(
        scheduler.window_updated(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Retry
    );
    assert_eq!(
        scheduler.poll_delivered(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Poll
    );
    assert_eq!(scheduler.active(PollScheduleLane::Frame), None);

    let replacement = scheduler
        .arm_if_pending(PollScheduleLane::Frame, true)
        .unwrap();
    assert!(replacement > generation);
    assert_eq!(
        scheduler.active(PollScheduleLane::Frame),
        Some(poll_snapshot(replacement, PollGenerationOutcome::Armed))
    );
}

#[test]
fn transient_window_failure_retries_same_generation_without_sticking() {
    let mut scheduler = PollSchedulerState::default();
    let generation = scheduler
        .arm_if_pending(PollScheduleLane::Frame, true)
        .unwrap();
    assert_eq!(
        scheduler.timer_delivered(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Retry
    );
    assert_eq!(
        scheduler.window_retry(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Retry
    );
    assert_eq!(
        scheduler.active(PollScheduleLane::Frame),
        Some(poll_snapshot(
            generation,
            PollGenerationOutcome::WindowRetryScheduled
        ))
    );
    assert_eq!(
        scheduler.timer_delivered(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Retry
    );
    assert_eq!(
        scheduler.window_updated(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Retry
    );
    assert_eq!(
        scheduler.poll_delivered(PollScheduleLane::Frame, generation),
        PollScheduleDecision::Poll
    );
    assert_eq!(scheduler.active(PollScheduleLane::Frame), None);
}

#[test]
fn definitive_window_and_view_failures_release_live_generations() {
    let mut scheduler = PollSchedulerState::default();
    let window_generation = scheduler
        .arm_if_pending(PollScheduleLane::Frame, true)
        .unwrap();
    assert_eq!(
        scheduler.window_unavailable(PollScheduleLane::Frame, window_generation),
        PollScheduleDecision::TerminateUnavailable
    );
    assert_eq!(scheduler.active(PollScheduleLane::Frame), None);
    assert_eq!(
        scheduler.last_acknowledged(PollScheduleLane::Frame),
        Some(poll_snapshot(
            window_generation,
            PollGenerationOutcome::WindowUnavailable
        ))
    );

    let view_generation = scheduler
        .arm_if_pending(PollScheduleLane::Frame, true)
        .unwrap();
    assert_eq!(
        scheduler.window_updated(PollScheduleLane::Frame, view_generation),
        PollScheduleDecision::Retry
    );
    assert_eq!(
        scheduler.view_unavailable(PollScheduleLane::Frame, view_generation),
        PollScheduleDecision::TerminateUnavailable
    );
    assert_eq!(scheduler.active(PollScheduleLane::Frame), None);
    assert_eq!(
        scheduler.last_acknowledged(PollScheduleLane::Frame),
        Some(poll_snapshot(
            view_generation,
            PollGenerationOutcome::ViewUnavailable
        ))
    );
}

#[test]
fn stale_callback_cannot_release_a_newer_generation() {
    let mut scheduler = PollSchedulerState::default();
    let stale_generation = scheduler
        .arm_if_pending(PollScheduleLane::ReadyIdle, true)
        .unwrap();
    assert_eq!(
        scheduler.cancel(PollScheduleLane::ReadyIdle),
        Some(stale_generation)
    );
    let current_generation = scheduler
        .arm_if_pending(PollScheduleLane::ReadyIdle, true)
        .unwrap();

    assert_eq!(
        scheduler.poll_delivered(PollScheduleLane::ReadyIdle, stale_generation),
        PollScheduleDecision::Stale
    );
    assert_eq!(
        scheduler.window_unavailable(PollScheduleLane::ReadyIdle, stale_generation),
        PollScheduleDecision::Stale
    );
    assert_eq!(
        scheduler.active(PollScheduleLane::ReadyIdle),
        Some(poll_snapshot(
            current_generation,
            PollGenerationOutcome::Armed
        ))
    );
}

#[test]
fn no_pending_work_definitively_disarms_without_duplicate_task() {
    let mut scheduler = PollSchedulerState::default();
    assert!(
        scheduler
            .arm_if_pending(PollScheduleLane::Frame, false)
            .is_none()
    );
    assert!(
        scheduler
            .arm_if_pending(PollScheduleLane::ReadyIdle, false)
            .is_none()
    );

    let generation = scheduler
        .arm_if_pending(PollScheduleLane::ReadyIdle, true)
        .unwrap();
    assert!(
        scheduler
            .arm_if_pending(PollScheduleLane::ReadyIdle, true)
            .is_none()
    );
    assert_eq!(
        scheduler.poll_delivered(PollScheduleLane::ReadyIdle, generation),
        PollScheduleDecision::Poll
    );
    assert!(
        scheduler
            .arm_if_pending(PollScheduleLane::ReadyIdle, false)
            .is_none()
    );
    assert_eq!(scheduler.active(PollScheduleLane::ReadyIdle), None);
}

fn poll_snapshot(generation: u64, outcome: PollGenerationOutcome) -> PollGenerationSnapshot {
    PollGenerationSnapshot {
        generation,
        outcome,
    }
}

fn serialized_heartbeat(diagnostics: &ShellLivenessDiagnostics) -> serde_json::Value {
    serde_json::to_value(diagnostics.heartbeat()).unwrap()
}

fn request(index: usize) -> diagnostic_child_protocol::DiagnosticProtocolRequest {
    parse_request_frame(
        format!("{{\"id\":\"request-{index}\",\"command\":\"read_ui_state\",\"params\":{{}}}}")
            .as_bytes(),
    )
    .unwrap()
    .unwrap()
}

fn scripted_direct_read(
    diagnostics: Arc<ShellLivenessDiagnostics>,
    request_id: &str,
) -> DiagnosticProtocolResponse {
    let (sender, _receiver) =
        diagnostic_target_request_channel_for_test(0, Duration::ZERO, diagnostics);
    let input =
        format!("{{\"id\":\"{request_id}\",\"command\":\"read_liveness\",\"params\":{{}}}}\n");
    let mut output = Vec::new();
    run_diagnostic_target_stdio_loop(sender, Cursor::new(input.into_bytes()), &mut output);
    parse_response_frame(&output).unwrap().unwrap()
}
