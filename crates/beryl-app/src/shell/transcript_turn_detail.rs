use std::{collections::BTreeSet, sync::mpsc::TryRecvError, time::Instant};

use tracing::warn;

use crate::diagnostic_dynamic_tools::{TranscriptDetailLoadEvent, diagnostic_duration_micros};

use super::*;

const TRANSCRIPT_TURN_DETAIL_REQUEST_BATCH_LIMIT: usize = 1;
const TRANSCRIPT_TURN_DETAIL_OVERSCAN_ROWS: usize = 2;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TranscriptTurnDetailSchedulerDiagnostics {
    pub(super) retention_turns: usize,
    pub(super) last_requested_turns: usize,
    pub(super) last_released_turns: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TranscriptTurnDetailApplyCounts {
    pub(super) applied: usize,
    pub(super) stale: usize,
}

impl ConversationSurfaceState {
    pub(super) fn schedule_transcript_turn_details_for_viewport(
        &mut self,
        priority_range: std::ops::Range<usize>,
        retention_visible_range: std::ops::Range<usize>,
        order: TranscriptTurnDetailViewportOrder,
        max_requested_tickets: usize,
    ) -> Option<TranscriptTurnDetailSchedule> {
        let thread_id = self.selected_thread_id()?.to_string();
        let turn_count = self.transcript_presentation.len();
        let priority_range = clamp_transcript_turn_detail_range(priority_range, turn_count);
        let retention_visible_range =
            clamp_transcript_turn_detail_range(retention_visible_range, turn_count);
        let retained_range =
            transcript_turn_detail_retention_range(retention_visible_range, turn_count);
        let priority_turn_ids = self.transcript_turn_ids_for_range(priority_range);
        let retained_turn_ids = self.transcript_turn_ids_for_range(retained_range);
        let plan = TranscriptTurnDetailViewportPlan::from_priority_and_retained(
            priority_turn_ids,
            retained_turn_ids,
            order,
        );
        let schedule = self
            .transcript_turn_detail_cache
            .schedule_viewport_full_details(thread_id.as_str(), plan, max_requested_tickets);
        self.transcript_turn_detail_scheduler_diagnostics =
            TranscriptTurnDetailSchedulerDiagnostics {
                retention_turns: schedule.retained_turns,
                last_requested_turns: schedule.requested_tickets.len(),
                last_released_turns: schedule
                    .released
                    .full_detail_turns
                    .saturating_add(schedule.released.loading_detail_turns)
                    .saturating_add(schedule.released.failed_detail_turns),
            };
        self.release_transcript_turn_detail_rows(thread_id.as_str(), &schedule.released);
        self.prune_transcript_turn_detail_skeletons_for_current_history();
        Some(schedule)
    }

    pub(super) fn should_start_transcript_turn_detail_ticket(
        &self,
        ticket: &TranscriptTurnDetailLoadTicket,
    ) -> bool {
        self.selected_thread_id() == Some(ticket.thread_id())
            && self
                .transcript_turn_detail_cache
                .should_start_loading(ticket)
    }

    pub(super) fn sync_transcript_turn_detail_ui_pins(&mut self) {
        let context_menu_turn_ids = self.active_context_menu_detail_pin_turn_ids();
        let edit_target_turn_ids = self.active_edit_target_detail_pin_turn_ids();
        let media_action_turn_ids = self.active_media_action_detail_pin_turn_ids();
        let active_turn_ids = self.active_turn_detail_pin_turn_ids();

        self.transcript_turn_detail_cache.replace_pins(
            TranscriptTurnDetailPinKind::ActiveContextMenu,
            context_menu_turn_ids,
        );
        self.transcript_turn_detail_cache.replace_pins(
            TranscriptTurnDetailPinKind::EditTarget,
            edit_target_turn_ids,
        );
        self.transcript_turn_detail_cache.replace_pins(
            TranscriptTurnDetailPinKind::MediaActionTarget,
            media_action_turn_ids,
        );
        self.transcript_turn_detail_cache
            .replace_pins(TranscriptTurnDetailPinKind::ActiveTurn, active_turn_ids);
        self.release_unpinned_transcript_turn_details_for_current_viewport();
    }

    pub(super) fn skip_transcript_turn_detail_ticket(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
    ) -> TranscriptTurnDetailApplyResult {
        let result = self.transcript_turn_detail_cache.skip_loading(ticket);
        self.prune_transcript_turn_detail_skeletons_for_current_history();
        result
    }

    pub(super) fn transcript_turn_details_for_image_resolution(
        &self,
        ticket: &TranscriptTurnDetailLoadTicket,
        turns: Vec<beryl_backend::TurnInfo>,
    ) -> Vec<beryl_backend::TurnInfo> {
        if self.selected_thread_id() != Some(ticket.thread_id()) {
            return Vec::new();
        }

        let current_turn_ids = self
            .transcript_turn_detail_cache
            .current_loading_coalesced_turn_ids(ticket)
            .into_iter()
            .collect::<BTreeSet<_>>();
        turns
            .into_iter()
            .filter(|turn| current_turn_ids.contains(turn.id.as_str()))
            .collect()
    }

    fn transcript_turn_ids_for_range(&self, range: std::ops::Range<usize>) -> Vec<String> {
        self.transcript_presentation
            .window_for_range(range)
            .rows()
            .iter()
            .filter_map(|row| row.turn.turn_id.as_deref())
            .map(str::to_string)
            .collect()
    }

    fn turn_id_for_transcript_row_identity(&self, row_identity: &str) -> Option<String> {
        let row_index = self
            .transcript_presentation
            .row_index_for_identity(row_identity)?;
        self.transcript_presentation
            .turn_at(row_index)?
            .turn
            .turn_id
            .clone()
    }

    fn active_context_menu_detail_pin_turn_ids(&self) -> Vec<String> {
        let Some(open) = self.transcript_branch_menu.active() else {
            return Vec::new();
        };
        let mut turn_ids = Vec::new();
        if let Some(target) = open.branch_target() {
            turn_ids.push(target.source_turn_id().to_string());
        }
        if let Some(identity) = open.edit_entry().and_then(|entry| entry.target_identity()) {
            turn_ids.push(identity.source_turn_id().to_string());
        }
        if let Some(identity) = open
            .title_update_entry()
            .and_then(|entry| entry.target_identity())
        {
            turn_ids.push(identity.source_turn_id().to_string());
        }
        if let Some(turn_id) = open
            .image_target()
            .and_then(|target| self.turn_id_for_transcript_row_identity(target.row_identity()))
        {
            turn_ids.push(turn_id);
        }
        unique_turn_ids(turn_ids)
    }

    fn active_edit_target_detail_pin_turn_ids(&self) -> Vec<String> {
        self.transcript_edit_mode
            .as_ref()
            .map(|edit_mode| edit_mode.target().source_turn_id().to_string())
            .into_iter()
            .collect()
    }

    fn active_media_action_detail_pin_turn_ids(&self) -> Vec<String> {
        self.transcript_branch_menu
            .active()
            .and_then(|open| open.image_target())
            .and_then(|target| self.turn_id_for_transcript_row_identity(target.row_identity()))
            .into_iter()
            .collect()
    }

    fn active_turn_detail_pin_turn_ids(&self) -> Vec<String> {
        let Some(selected_thread_id) = self.selected_thread_id() else {
            return Vec::new();
        };
        let Some(active) = self.execution_details.active_turn_identity() else {
            return Vec::new();
        };
        if active.thread_id.as_deref() != Some(selected_thread_id) {
            return Vec::new();
        }
        active.turn_id.into_iter().collect()
    }

    fn release_unpinned_transcript_turn_details_for_current_viewport(&mut self) {
        let Some(thread_id) = self.selected_thread_id().map(str::to_string) else {
            return;
        };
        let turn_count = self.transcript_presentation.len();
        let retained_range = transcript_turn_detail_retention_range(
            self.transcript_list_state.visible_range(),
            turn_count,
        );
        let retained_turn_ids = self.transcript_turn_ids_for_range(retained_range);
        let retention = TranscriptTurnDetailRetention::from_turn_ids(retained_turn_ids);
        let released = self
            .transcript_turn_detail_cache
            .release_unretained_details(&retention);
        self.release_transcript_turn_detail_rows(thread_id.as_str(), &released);
        self.prune_transcript_turn_detail_skeletons_for_current_history();
    }

    fn latest_source_turn_presentation_range(&self) -> Option<std::ops::Range<usize>> {
        let latest_source_turn_index = self.execution_details.turns().len().checked_sub(1)?;
        self.transcript_presentation
            .presentation_index_for_source_turn(latest_source_turn_index)
            .map(|index| index..index.saturating_add(1))
    }

    fn latest_source_turn_missing_detail_range(&self) -> Option<std::ops::Range<usize>> {
        let range = self.latest_source_turn_presentation_range()?;
        let row = self.transcript_presentation.turn_at(range.start)?;
        let turn_id = row.turn.turn_id.as_deref()?;
        self.transcript_turn_detail_cache
            .is_missing_detail_requestable(turn_id)
            .then_some(range)
    }

    pub(super) fn finish_loading_transcript_turn_details(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
        turns: Vec<beryl_backend::TurnInfo>,
        image_resolver: &TranscriptImagePathResolver,
    ) -> TranscriptTurnDetailApplyCounts {
        let mut counts = TranscriptTurnDetailApplyCounts::default();
        let mut returned_turn_ids = BTreeSet::new();
        let visible_range_before_apply = self.transcript_list_state.visible_range();
        let content_anchor_before_apply = match self.transcript_list_state.scroll_position() {
            ListScrollPosition::Content(anchor) => Some(anchor),
            ListScrollPosition::Bottom | ListScrollPosition::VirtualTail { .. } => None,
        };
        for turn in turns {
            let turn_id = turn.id;
            let items = turn.items;
            returned_turn_ids.insert(turn_id.clone());
            let item_count = items.len();
            let applied = self
                .transcript_turn_detail_cache
                .finish_coalesced_loading(ticket, &turn_id, item_count);
            match applied {
                TranscriptTurnDetailApplyResult::Applied => {
                    counts.applied = counts.applied.saturating_add(1);
                    self.composer_image_labels
                        .observe_thread_items(ticket.thread_id(), &items);
                    if let Some(replacement) = self.execution_details.apply_history_turn_items(
                        ticket.thread_id(),
                        &turn_id,
                        items,
                        image_resolver,
                    ) && let Some(row_index) = self
                        .transcript_presentation
                        .replace_turn(replacement.index, replacement.turn)
                    {
                        let preserve_loaded_row_anchor = self.transcript_user_scrolled
                            && (visible_range_before_apply.contains(&row_index)
                                || content_anchor_before_apply
                                    .is_some_and(|anchor| anchor.item_ix == row_index))
                            && matches!(
                                self.transcript_list_state.scroll_position(),
                                ListScrollPosition::Content(_)
                            );
                        self.transcript_list_state
                            .invalidate_item_measurement(row_index);
                        if preserve_loaded_row_anchor {
                            self.transcript_list_state.scroll_to_position(
                                ListScrollPosition::Content(ListOffset {
                                    item_ix: row_index,
                                    offset_in_item: px(0.0),
                                }),
                            );
                        }
                    }
                }
                TranscriptTurnDetailApplyResult::Stale => {
                    counts.stale = counts.stale.saturating_add(1);
                }
            }
        }
        for turn_id in ticket.coalesced_turn_ids() {
            if returned_turn_ids.contains(turn_id) {
                continue;
            }
            if self
                .transcript_turn_detail_cache
                .skip_coalesced_loading(ticket, turn_id)
                == TranscriptTurnDetailApplyResult::Applied
            {
                counts.stale = counts.stale.saturating_add(1);
            }
        }
        self.prune_transcript_turn_detail_skeletons_for_current_history();
        counts
    }

    pub(super) fn fail_loading_transcript_turn_details(
        &mut self,
        ticket: &TranscriptTurnDetailLoadTicket,
    ) -> TranscriptTurnDetailApplyResult {
        let failed_turn_ids = self.transcript_turn_detail_cache.fail_loading_group(ticket);
        for turn_id in &failed_turn_ids {
            if let Some(replacement) = self
                .execution_details
                .fail_history_turn_detail(ticket.thread_id(), turn_id)
                && let Some(row_index) = self
                    .transcript_presentation
                    .replace_turn(replacement.index, replacement.turn)
            {
                self.transcript_list_state
                    .invalidate_item_measurement(row_index);
            }
        }
        if failed_turn_ids.is_empty() {
            TranscriptTurnDetailApplyResult::Stale
        } else {
            TranscriptTurnDetailApplyResult::Applied
        }
    }

    fn release_transcript_turn_detail_rows(
        &mut self,
        thread_id: &str,
        released: &TranscriptTurnDetailReleaseCounts,
    ) {
        for turn_id in &released.released_turn_ids {
            if let Some(replacement) = self
                .execution_details
                .release_history_turn_detail(thread_id, turn_id)
                && let Some(row_index) = self
                    .transcript_presentation
                    .replace_turn(replacement.index, replacement.turn)
            {
                self.transcript_list_state
                    .invalidate_item_measurement(row_index);
            }
        }
    }

    pub(super) fn prune_transcript_turn_detail_skeletons_for_current_history(&mut self) -> usize {
        let mut protected_turn_ids = self.transcript_history_window.resident_turn_ids();
        protected_turn_ids
            .extend(self.transcript_turn_ids_for_range(self.transcript_list_state.visible_range()));
        self.transcript_turn_detail_cache
            .prune_skeletons_to_protected_turns(protected_turn_ids)
    }
}

fn transcript_turn_detail_retention_range(
    visible_range: std::ops::Range<usize>,
    turn_count: usize,
) -> std::ops::Range<usize> {
    if visible_range.is_empty() {
        return visible_range;
    }
    let start = visible_range
        .start
        .saturating_sub(TRANSCRIPT_TURN_DETAIL_OVERSCAN_ROWS);
    let end = visible_range
        .end
        .saturating_add(TRANSCRIPT_TURN_DETAIL_OVERSCAN_ROWS)
        .min(turn_count)
        .max(start.min(turn_count));
    start.min(turn_count)..end
}

fn clamp_transcript_turn_detail_range(
    range: std::ops::Range<usize>,
    turn_count: usize,
) -> std::ops::Range<usize> {
    let start = range.start.min(turn_count);
    let end = range.end.min(turn_count).max(start);
    start..end
}

fn unique_turn_ids(turn_ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    turn_ids
        .into_iter()
        .filter(|turn_id| seen.insert(turn_id.clone()))
        .collect()
}

fn skipped_transcript_turn_detail_event(
    ticket: &TranscriptTurnDetailLoadTicket,
) -> TranscriptDetailLoadEvent {
    let mut event = TranscriptDetailLoadEvent {
        sequence: 0,
        cursor_present: ticket
            .page_locator()
            .and_then(|locator| locator.cursor())
            .is_some(),
        requested_limit: ticket.page_locator().map(|locator| locator.limit()),
        returned_turn_count: 0,
        applied_turn_count: 0,
        skipped_stale_count: 0,
        total_micros: 0,
        cas_micros: 0,
        response_processing_micros: 0,
        image_source_resolution_micros: 0,
        cache_application_micros: 0,
        outcome: "stale".to_string(),
    };
    event.mark_stale(ticket.coalesced_turn_ids().len().max(1));
    event
}

impl ShellView {
    pub(super) fn begin_transcript_turn_detail_loads_for_current_viewport(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((priority_range, retention_visible_range, order)) =
            self.conversation_surface().map(|surface| {
                let list_state = surface.transcript_list_state();
                let turn_count = surface.transcript_presentation().len();
                let scroll_position = list_state.scroll_position();
                let visible_range = list_state.visible_range();
                let priority_range = match scroll_position {
                    ListScrollPosition::Bottom | ListScrollPosition::VirtualTail { .. } => surface
                        .latest_source_turn_missing_detail_range()
                        .unwrap_or_else(|| visible_range.clone()),
                    _ => visible_range.clone(),
                };
                let order = transcript_turn_detail_viewport_order(
                    scroll_position,
                    priority_range.clone(),
                    turn_count,
                );
                (priority_range, visible_range, order)
            })
        else {
            return false;
        };

        self.begin_transcript_turn_detail_loads_for_viewport(
            priority_range,
            retention_visible_range,
            order,
            window,
            cx,
        )
    }

    pub(super) fn begin_transcript_turn_detail_loads_for_scroll_anchor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((priority_range, retention_visible_range, order)) =
            self.conversation_surface().and_then(|surface| {
                let list_state = surface.transcript_list_state();
                let turn_count = surface.transcript_presentation().len();
                let ListScrollPosition::Content(anchor) = list_state.scroll_position() else {
                    return None;
                };
                if anchor.item_ix >= turn_count {
                    return None;
                }
                let end = anchor.item_ix.saturating_add(1).min(turn_count);
                let anchor_range = anchor.item_ix..end;
                Some((
                    anchor_range.clone(),
                    anchor_range,
                    TranscriptTurnDetailViewportOrder::NewestFirst,
                ))
            })
        else {
            return false;
        };

        self.begin_transcript_turn_detail_loads_for_viewport(
            priority_range,
            retention_visible_range,
            order,
            window,
            cx,
        )
    }

    fn dispatch_next_transcript_turn_detail_ticket(&mut self) -> bool {
        let mut updated = false;

        loop {
            let Some(ticket) = self
                .transcript_turn_detail_task
                .as_mut()
                .and_then(TranscriptTurnDetailTask::pop_pending_ticket)
            else {
                break;
            };

            let should_start = self
                .conversation_surface()
                .is_some_and(|surface| surface.should_start_transcript_turn_detail_ticket(&ticket));
            if !should_start {
                let application_started = Instant::now();
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.skip_transcript_turn_detail_ticket(&ticket);
                }
                let mut diagnostics = skipped_transcript_turn_detail_event(&ticket);
                diagnostics.cache_application_micros =
                    diagnostic_duration_micros(application_started.elapsed());
                diagnostics.total_micros = diagnostics
                    .total_micros
                    .saturating_add(diagnostics.cache_application_micros);
                self.transcript_detail_load_diagnostics.record(diagnostics);
                updated = true;
                continue;
            }

            let started = self
                .transcript_turn_detail_task
                .as_mut()
                .is_some_and(|task| task.start_ticket(ticket.clone()));
            if !started {
                self.transcript_turn_detail_task = None;
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.fail_loading_transcript_turn_details(&ticket);
                }
                warn!(
                    thread_id = ticket.thread_id(),
                    turn_id = ticket.turn_id(),
                    "failed to dispatch transcript turn detail load"
                );
                updated = true;
            } else {
                updated = true;
                break;
            }
        }

        if self
            .transcript_turn_detail_task
            .as_ref()
            .is_some_and(|task| !task.has_active_tickets())
        {
            self.transcript_turn_detail_task = None;
        }

        updated
    }

    pub(super) fn poll_transcript_turn_detail_updates(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        if self.transcript_turn_detail_task.is_none() {
            return false;
        }

        let mut updated = false;
        let poll_started_at = Instant::now();
        let mut processed_updates = 0usize;
        loop {
            if processed_updates >= SHELL_WORKER_POLL_MAX_EVENTS_PER_FRAME
                || poll_started_at.elapsed() >= SHELL_WORKER_POLL_MAX_FRAME_TIME
            {
                return updated;
            }

            let update = match self
                .transcript_turn_detail_task
                .as_ref()
                .map(TranscriptTurnDetailTask::try_recv)
            {
                Some(Ok(update)) => {
                    processed_updates = processed_updates.saturating_add(1);
                    update
                }
                Some(Err(TryRecvError::Empty)) => break,
                Some(Err(TryRecvError::Disconnected)) => {
                    let mut task = self
                        .transcript_turn_detail_task
                        .take()
                        .expect("detail task exists when polling disconnected worker");
                    for ticket in task.take_active_tickets() {
                        if let Some(surface) = self.conversation_surface_mut() {
                            surface.fail_loading_transcript_turn_details(&ticket);
                        }
                    }
                    warn!("transcript turn detail worker stopped before returning all results");
                    updated = true;
                    break;
                }
                None => break,
            };

            match update {
                TranscriptTurnDetailUpdate::DetailsLoaded {
                    ticket,
                    turns,
                    diagnostics,
                } => {
                    let turns_for_resolution = self
                        .conversation_surface()
                        .map(|surface| {
                            surface.transcript_turn_details_for_image_resolution(&ticket, turns)
                        })
                        .unwrap_or_default();
                    if turns_for_resolution.is_empty() {
                        if let Some(task) = self.transcript_turn_detail_task.as_mut() {
                            task.finish_ticket(&ticket);
                        }
                        self.finish_transcript_turn_detail_worker(
                            TranscriptTurnDetailOutcome::Loaded {
                                ticket,
                                turns: Vec::new(),
                                image_resolver: TranscriptImagePathResolver::default(),
                                diagnostics,
                            },
                        );
                        self.dispatch_next_transcript_turn_detail_ticket();
                        if self
                            .transcript_turn_detail_task
                            .as_ref()
                            .is_some_and(|task| !task.has_active_tickets())
                        {
                            self.transcript_turn_detail_task = None;
                        }
                    } else {
                        let resolving =
                            self.transcript_turn_detail_task
                                .as_mut()
                                .is_some_and(|task| {
                                    task.resolve_images_for_loaded_turns(
                                        &ticket,
                                        turns_for_resolution,
                                        diagnostics,
                                    )
                                });
                        if !resolving {
                            if let Some(task) = self.transcript_turn_detail_task.as_mut() {
                                task.finish_ticket(&ticket);
                            }
                            if let Some(surface) = self.conversation_surface_mut() {
                                surface.fail_loading_transcript_turn_details(&ticket);
                            }
                            warn!(
                                thread_id = ticket.thread_id(),
                                turn_id = ticket.turn_id(),
                                "failed to resolve transcript turn detail image sources"
                            );
                            self.dispatch_next_transcript_turn_detail_ticket();
                        }
                    }
                    updated = true;
                }
                TranscriptTurnDetailUpdate::Finished(outcome) => {
                    let ticket = match &outcome {
                        TranscriptTurnDetailOutcome::Loaded { ticket, .. }
                        | TranscriptTurnDetailOutcome::Failed { ticket, .. } => ticket.clone(),
                    };
                    if let TranscriptTurnDetailOutcome::Failed { message, .. } = &outcome {
                        warn!(
                            thread_id = ticket.thread_id(),
                            turn_id = ticket.turn_id(),
                            error = %message,
                            "failed to load transcript turn details"
                        );
                        if let Some(surface) = self.conversation_surface_mut()
                            && surface.selected_thread_id() == Some(ticket.thread_id())
                        {
                            surface.set_notice(SurfaceNotice::new(
                                "Transcript detail load failed",
                                message.clone(),
                            ));
                        }
                    }
                    if let Some(task) = self.transcript_turn_detail_task.as_mut() {
                        task.finish_ticket(&ticket);
                    }
                    self.finish_transcript_turn_detail_worker(outcome);
                    self.dispatch_next_transcript_turn_detail_ticket();
                    if self
                        .transcript_turn_detail_task
                        .as_ref()
                        .is_some_and(|task| !task.has_active_tickets())
                    {
                        self.transcript_turn_detail_task = None;
                    }
                    updated = true;
                }
            }
        }

        updated
    }

    pub(super) fn begin_transcript_turn_detail_loads_for_viewport(
        &mut self,
        priority_range: std::ops::Range<usize>,
        retention_visible_range: std::ops::Range<usize>,
        order: TranscriptTurnDetailViewportOrder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((workspace_id, runtime_mode)) = (match &self.state {
            ShellState::Ready(ready) => Some((
                ready.loaded_workspace.workspace.id().clone(),
                ready.execution_target.runtime_mode().clone(),
            )),
            _ => None,
        }) else {
            return false;
        };

        let Some(connector) = self.backend_client_connector() else {
            return false;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return false;
        };
        let max_requested_tickets = if self
            .transcript_turn_detail_task
            .as_ref()
            .is_some_and(TranscriptTurnDetailTask::has_active_tickets)
        {
            0
        } else {
            TRANSCRIPT_TURN_DETAIL_REQUEST_BATCH_LIMIT
        };
        let Some(schedule) = self.conversation_surface_mut().and_then(|surface| {
            surface.schedule_transcript_turn_details_for_viewport(
                priority_range,
                retention_visible_range,
                order,
                max_requested_tickets,
            )
        }) else {
            return false;
        };
        if schedule.requested_tickets.is_empty() {
            return false;
        }

        for ticket in schedule.requested_tickets {
            if self.transcript_turn_detail_task.is_none() {
                self.transcript_turn_detail_task = Some(spawn_transcript_turn_detail_worker(
                    persistence.clone(),
                    connector.clone(),
                    workspace_id.clone(),
                    runtime_mode.clone(),
                    self.bootstrap.probe_timeout(),
                ));
            }
            let requested = self
                .transcript_turn_detail_task
                .as_mut()
                .is_some_and(|task| task.request(ticket.clone()));
            if !requested {
                self.transcript_turn_detail_task = None;
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.fail_loading_transcript_turn_details(&ticket);
                }
                warn!(
                    thread_id = ticket.thread_id(),
                    turn_id = ticket.turn_id(),
                    "failed to enqueue transcript turn detail load"
                );
            }
        }

        self.dispatch_next_transcript_turn_detail_ticket();
        self.schedule_poll_if_needed(window, cx);
        true
    }
}

fn transcript_turn_detail_viewport_order(
    _scroll_position: ListScrollPosition,
    _visible_range: std::ops::Range<usize>,
    _turn_count: usize,
) -> TranscriptTurnDetailViewportOrder {
    TranscriptTurnDetailViewportOrder::NewestFirst
}
