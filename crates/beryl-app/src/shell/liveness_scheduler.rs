use std::{
    cell::BorrowMutError,
    sync::{Arc, Mutex},
    time::Duration,
};

use gpui::{AsyncApp, Context, WeakEntity, Window, prelude::*};

use super::liveness_diagnostics;
use super::liveness_diagnostics::{
    PollScheduleDecision, PollScheduleLane, PollSchedulerState, shared_liveness,
};
use super::{FRAME_POLL_INTERVAL, READY_IDLE_POLL_INTERVAL, ShellState, ShellView};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowUpdateFailureControl {
    RetrySameGeneration,
    Stop,
}

fn record_scheduler_state(scheduler: &Arc<Mutex<PollSchedulerState>>) {
    let scheduler = scheduler.lock().expect("poll scheduler mutex poisoned");
    shared_liveness().poll_scheduler_state(
        scheduler.active(PollScheduleLane::Frame),
        scheduler.last_acknowledged(PollScheduleLane::Frame),
        scheduler.active(PollScheduleLane::ReadyIdle),
        scheduler.last_acknowledged(PollScheduleLane::ReadyIdle),
    );
}

fn record_unavailable_decision(
    decision: PollScheduleDecision,
    lane: PollScheduleLane,
    generation: u64,
) {
    match decision {
        PollScheduleDecision::TerminateUnavailable => {
            shared_liveness().timer_unavailable(lane.category(), generation);
        }
        PollScheduleDecision::Stale => {
            shared_liveness().timer_stale(lane.category(), generation);
        }
        PollScheduleDecision::Retry | PollScheduleDecision::Poll => {}
    }
}

fn handle_window_update_failure(
    error: &(dyn std::error::Error + Send + Sync + 'static),
    scheduler: &Arc<Mutex<PollSchedulerState>>,
    lane: PollScheduleLane,
    generation: u64,
) -> WindowUpdateFailureControl {
    if error.downcast_ref::<BorrowMutError>().is_some() {
        let decision = scheduler
            .lock()
            .expect("poll scheduler mutex poisoned")
            .window_retry(lane, generation);
        record_scheduler_state(scheduler);
        return match decision {
            PollScheduleDecision::Retry => {
                shared_liveness().timer_retry(lane.category(), generation);
                WindowUpdateFailureControl::RetrySameGeneration
            }
            PollScheduleDecision::Stale => {
                shared_liveness().timer_stale(lane.category(), generation);
                WindowUpdateFailureControl::Stop
            }
            PollScheduleDecision::Poll | PollScheduleDecision::TerminateUnavailable => {
                WindowUpdateFailureControl::Stop
            }
        };
    }

    let decision = scheduler
        .lock()
        .expect("poll scheduler mutex poisoned")
        .window_unavailable(lane, generation);
    record_scheduler_state(scheduler);
    record_unavailable_decision(decision, lane, generation);
    WindowUpdateFailureControl::Stop
}

impl ShellView {
    pub(super) fn record_liveness_shell_state(&self) {
        let scheduler = self
            .poll_scheduler
            .lock()
            .expect("poll scheduler mutex poisoned");
        shared_liveness().shell_state(
            scheduler.active(PollScheduleLane::Frame),
            scheduler.last_acknowledged(PollScheduleLane::Frame),
            scheduler.active(PollScheduleLane::ReadyIdle),
            scheduler.last_acknowledged(PollScheduleLane::ReadyIdle),
            self.liveness_active_receiver_bits(),
        );
    }

    pub(super) fn schedule_poll_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.record_liveness_shell_state();
        if self.has_frame_poll_work() {
            if let Some(generation) = self.arm_poll_generation(PollScheduleLane::Frame) {
                self.spawn_poll_timer(
                    PollScheduleLane::Frame,
                    generation,
                    FRAME_POLL_INTERVAL,
                    window,
                    cx,
                );
            }
            return;
        }

        if self.has_ready_maintenance_poll_work()
            && let Some(generation) = self.arm_poll_generation(PollScheduleLane::ReadyIdle)
        {
            self.spawn_poll_timer(
                PollScheduleLane::ReadyIdle,
                generation,
                READY_IDLE_POLL_INTERVAL,
                window,
                cx,
            );
        }
    }

    fn arm_poll_generation(&mut self, lane: PollScheduleLane) -> Option<u64> {
        let generation = self
            .poll_scheduler
            .lock()
            .expect("poll scheduler mutex poisoned")
            .arm_if_pending(lane, true)?;
        shared_liveness().timer_arm(lane.category(), generation);
        self.record_liveness_shell_state();
        Some(generation)
    }

    fn spawn_poll_timer(
        &self,
        lane: PollScheduleLane,
        generation: u64,
        interval: Duration,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let window_handle = window.window_handle();
        let scheduler = Arc::clone(&self.poll_scheduler);
        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            Self::deliver_poll_timer(
                view,
                scheduler,
                window_handle,
                lane,
                generation,
                interval,
                cx.clone(),
            )
        })
        .detach();
    }

    async fn deliver_poll_timer(
        view: WeakEntity<Self>,
        scheduler: Arc<Mutex<PollSchedulerState>>,
        window_handle: gpui::AnyWindowHandle,
        lane: PollScheduleLane,
        generation: u64,
        interval: Duration,
        mut cx: AsyncApp,
    ) {
        loop {
            cx.background_executor().timer(interval).await;
            shared_liveness().timer_fire(lane.category(), generation);
            let delivery = scheduler
                .lock()
                .expect("poll scheduler mutex poisoned")
                .timer_delivered(lane, generation);
            record_scheduler_state(&scheduler);
            if delivery == PollScheduleDecision::Stale {
                shared_liveness().timer_stale(lane.category(), generation);
                return;
            }

            shared_liveness().window_update_attempt(lane.category(), generation);
            let callback_view = view.clone();
            let callback_scheduler = Arc::clone(&scheduler);
            let window_update = cx.update_window(window_handle, move |_, window, cx| {
                let window_decision = callback_scheduler
                    .lock()
                    .expect("poll scheduler mutex poisoned")
                    .window_updated(lane, generation);
                record_scheduler_state(&callback_scheduler);
                if window_decision == PollScheduleDecision::Stale {
                    shared_liveness().timer_stale(lane.category(), generation);
                    return true;
                }
                shared_liveness().view_update_attempt(lane.category(), generation);
                let view_update = callback_view.update(cx, |view, cx| {
                    let decision = view
                        .poll_scheduler
                        .lock()
                        .expect("poll scheduler mutex poisoned")
                        .poll_delivered(lane, generation);
                    view.record_liveness_shell_state();
                    match decision {
                        PollScheduleDecision::Poll => {
                            shared_liveness().timer_release(lane.category(), generation);
                            view.poll(window, cx);
                        }
                        PollScheduleDecision::Stale => {
                            shared_liveness().timer_stale(lane.category(), generation);
                        }
                        PollScheduleDecision::Retry
                        | PollScheduleDecision::TerminateUnavailable => {}
                    }
                });
                shared_liveness().view_update_outcome(
                    lane.category(),
                    generation,
                    view_update.is_ok(),
                );
                if view_update.is_err() {
                    let decision = callback_scheduler
                        .lock()
                        .expect("poll scheduler mutex poisoned")
                        .view_unavailable(lane, generation);
                    record_scheduler_state(&callback_scheduler);
                    record_unavailable_decision(decision, lane, generation);
                }
                view_update.is_ok()
            });
            shared_liveness().window_update_outcome(
                lane.category(),
                generation,
                window_update.is_ok(),
            );

            match window_update {
                Ok(_) => return,
                Err(error) => {
                    match handle_window_update_failure(error.as_ref(), &scheduler, lane, generation)
                    {
                        WindowUpdateFailureControl::RetrySameGeneration => continue,
                        WindowUpdateFailureControl::Stop => return,
                    }
                }
            }
        }
    }

    pub(super) fn scheduler_has_frame_poll_work(&self) -> bool {
        self.discovery_receiver.is_some()
            || self.workspace_receiver.is_some()
            || self.graph_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.member_thread_inventory_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.composer_image_label_scan_receiver.is_some()
            || self.composer_image_asset_receiver.is_some()
            || self.turn_receiver.is_some()
            || self.shell_tool_receiver.is_some()
            || self.diagnostic_target_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.composer_image_delivery_receiver.is_some()
            || !self.thread_title_receivers.is_empty()
            || !self.thread_title_update_receivers.is_empty()
            || self.status_operation_receiver.is_some()
            || self.phase_thread_transition.has_poll_work()
            || self.phase_thread_workspace_deletion.is_some()
            || self.account_rate_limits_receiver.is_some()
            || self.turn_stop_receiver.is_some()
            || self.hard_stop_receiver.is_some()
            || self.theme_candidate_install_receiver.is_some()
            || self.dynamic_theme_durable_receiver.is_some()
            || self.workspace_picker_action_receiver.is_some()
            || self.workspace_runtime_selector_distro_receiver.is_some()
            || self.workspace_title_receiver.is_some()
            || self.application_shutdown_receiver.is_some()
            || self.application_shutdown_phase_deadline.is_some()
            || self.pending_workspace_title_candidate.is_some()
            || self.workspace_persistence_pending_last_poll
            || self.workspace_persistence_queue.has_pending_work()
            || self
                .loaded_workspace()
                .is_some_and(|loaded| loaded.workspace_picker.delete_hold_active())
            || self
                .conversation_surface()
                .is_some_and(|surface| surface.status_line_operations().hard_stop_hold_active())
            || self
                .conversation_surface()
                .is_some_and(|surface| surface.graph_thread_link_menu().delete_hold_active())
    }

    fn has_ready_maintenance_poll_work(&self) -> bool {
        matches!(self.state, ShellState::Ready(_))
            && (self
                .conversation_surface()
                .is_some_and(|surface| surface.member_thread_inventory().needs_refresh())
                || self.tool_activity_nickname_resolver.has_retry_work()
                || !self.backend_servers.is_empty())
    }

    fn liveness_active_receiver_bits(&self) -> u64 {
        let mut bits = 0u64;
        macro_rules! active {
            ($condition:expr, $bit:expr) => {
                if $condition {
                    bits |= $bit;
                }
            };
        }
        active!(
            self.discovery_receiver.is_some(),
            liveness_diagnostics::RECEIVER_DISCOVERY
        );
        active!(
            self.workspace_receiver.is_some(),
            liveness_diagnostics::RECEIVER_WORKSPACE
        );
        active!(
            self.graph_receiver.is_some(),
            liveness_diagnostics::RECEIVER_GRAPH
        );
        active!(
            self.graph_thread_start_receiver.is_some(),
            liveness_diagnostics::RECEIVER_GRAPH_THREAD_START
        );
        active!(
            self.transcript_branch_receiver.is_some(),
            liveness_diagnostics::RECEIVER_TRANSCRIPT_BRANCH
        );
        active!(
            self.transcript_edit_commit_receiver.is_some(),
            liveness_diagnostics::RECEIVER_TRANSCRIPT_EDIT
        );
        active!(
            self.member_thread_inventory_receiver.is_some(),
            liveness_diagnostics::RECEIVER_MEMBER_INVENTORY
        );
        active!(
            self.thread_activation_receiver.is_some(),
            liveness_diagnostics::RECEIVER_THREAD_ACTIVATION
        );
        active!(
            self.thread_history_page_receiver.is_some(),
            liveness_diagnostics::RECEIVER_THREAD_HISTORY
        );
        active!(
            self.composer_image_label_scan_receiver.is_some(),
            liveness_diagnostics::RECEIVER_IMAGE_LABEL
        );
        active!(
            self.composer_image_asset_receiver.is_some(),
            liveness_diagnostics::RECEIVER_IMAGE_ASSET
        );
        active!(
            self.turn_receiver.is_some(),
            liveness_diagnostics::RECEIVER_TURN
        );
        active!(
            self.shell_tool_receiver.is_some(),
            liveness_diagnostics::RECEIVER_SHELL_TOOL
        );
        active!(
            self.diagnostic_target_receiver.is_some(),
            liveness_diagnostics::RECEIVER_DIAGNOSTIC_TARGET
        );
        active!(
            !self.turn_steering_receivers.is_empty(),
            liveness_diagnostics::RECEIVER_TURN_STEERING
        );
        active!(
            self.composer_image_delivery_receiver.is_some(),
            liveness_diagnostics::RECEIVER_IMAGE_DELIVERY
        );
        active!(
            !self.thread_title_receivers.is_empty(),
            liveness_diagnostics::RECEIVER_THREAD_TITLE
        );
        active!(
            !self.thread_title_update_receivers.is_empty(),
            liveness_diagnostics::RECEIVER_THREAD_TITLE_UPDATE
        );
        active!(
            self.status_operation_receiver.is_some(),
            liveness_diagnostics::RECEIVER_STATUS_OPERATION
        );
        active!(
            self.phase_thread_transition.has_poll_work(),
            liveness_diagnostics::RECEIVER_PHASE_TRANSITION
        );
        active!(
            self.phase_thread_workspace_deletion.is_some(),
            liveness_diagnostics::RECEIVER_PHASE_DELETION
        );
        active!(
            self.account_rate_limits_receiver.is_some(),
            liveness_diagnostics::RECEIVER_ACCOUNT_RATE_LIMITS
        );
        active!(
            self.turn_stop_receiver.is_some(),
            liveness_diagnostics::RECEIVER_TURN_STOP
        );
        active!(
            self.hard_stop_receiver.is_some(),
            liveness_diagnostics::RECEIVER_HARD_STOP
        );
        active!(
            self.theme_candidate_install_receiver.is_some(),
            liveness_diagnostics::RECEIVER_THEME_CANDIDATE
        );
        active!(
            self.dynamic_theme_durable_receiver.is_some(),
            liveness_diagnostics::RECEIVER_DYNAMIC_THEME
        );
        active!(
            self.workspace_picker_action_receiver.is_some(),
            liveness_diagnostics::RECEIVER_PICKER_ACTION
        );
        active!(
            self.workspace_runtime_selector_distro_receiver.is_some(),
            liveness_diagnostics::RECEIVER_RUNTIME_DISTRO
        );
        active!(
            self.workspace_title_receiver.is_some(),
            liveness_diagnostics::RECEIVER_WORKSPACE_TITLE
        );
        active!(
            self.application_shutdown_receiver.is_some()
                || self.application_shutdown_phase_deadline.is_some(),
            liveness_diagnostics::RECEIVER_SHUTDOWN
        );
        active!(
            self.workspace_persistence_queue.has_pending_work()
                || self.pending_workspace_title_candidate.is_some(),
            liveness_diagnostics::RECEIVER_AUXILIARY_HOLD
        );
        bits
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::shell::liveness_diagnostics::PollGenerationOutcome;

    #[test]
    fn typed_borrow_failure_retries_the_same_generation() {
        let scheduler = Arc::new(Mutex::new(PollSchedulerState::default()));
        let generation = scheduler
            .lock()
            .unwrap()
            .arm_if_pending(PollScheduleLane::Frame, true)
            .unwrap();
        scheduler
            .lock()
            .unwrap()
            .timer_delivered(PollScheduleLane::Frame, generation);

        let cell = RefCell::new(());
        let _borrow = cell.borrow();
        let borrow_error = cell.try_borrow_mut().unwrap_err();
        assert_eq!(
            handle_window_update_failure(
                &borrow_error,
                &scheduler,
                PollScheduleLane::Frame,
                generation,
            ),
            WindowUpdateFailureControl::RetrySameGeneration
        );

        let active = scheduler
            .lock()
            .unwrap()
            .active(PollScheduleLane::Frame)
            .unwrap();
        assert_eq!(active.generation, generation);
        assert_eq!(active.outcome, PollGenerationOutcome::WindowRetryScheduled);
    }

    #[test]
    fn non_borrow_window_failure_definitively_releases_and_stops() {
        let scheduler = Arc::new(Mutex::new(PollSchedulerState::default()));
        let generation = scheduler
            .lock()
            .unwrap()
            .arm_if_pending(PollScheduleLane::Frame, true)
            .unwrap();
        let error = std::io::Error::other("window unavailable");
        assert_eq!(
            handle_window_update_failure(&error, &scheduler, PollScheduleLane::Frame, generation,),
            WindowUpdateFailureControl::Stop
        );

        let scheduler = scheduler.lock().unwrap();
        assert_eq!(scheduler.active(PollScheduleLane::Frame), None);
        let outcome = scheduler
            .last_acknowledged(PollScheduleLane::Frame)
            .unwrap();
        assert_eq!(outcome.generation, generation);
        assert_eq!(outcome.outcome, PollGenerationOutcome::WindowUnavailable);
    }
}
