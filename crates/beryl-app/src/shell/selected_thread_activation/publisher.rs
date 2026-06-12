use beryl_backend::ThreadStatus;
use tracing::debug;

use crate::memory_diagnostics::{self, MemoryMilestone};

use super::super::execution_detail::ExecutionItem;
use super::super::transcript_residency_pins::TranscriptResidencyAdmissionSummary;
use super::super::{ConversationSurfaceState, elapsed_ms, transcript_residency_logging};
use super::{
    PublishedSelectedThreadActivation, SelectedThreadInitialViewportPolicy,
    StagedSelectedThreadActivation,
};

pub(super) struct SelectedThreadPublisher;

impl SelectedThreadPublisher {
    pub(super) fn try_publish(
        surface: &mut ConversationSurfaceState,
    ) -> Option<PublishedSelectedThreadActivation> {
        let staged = surface.staged_selected_thread_activation.as_ref()?;
        if !staged.is_ready_for_publication() {
            log_staged_activation_not_ready(staged);
            return None;
        }

        let staged = surface.staged_selected_thread_activation.take()?;
        Some(Self::publish(surface, staged))
    }

    fn publish(
        surface: &mut ConversationSurfaceState,
        staged: StagedSelectedThreadActivation,
    ) -> PublishedSelectedThreadActivation {
        let summary = staged.thread.summary();
        let source = staged.source;
        let execution_target = staged.execution_target.clone();
        let activated_idle = matches!(staged.thread.status, ThreadStatus::Idle);
        let history_turn_count = staged.thread.turns.len();
        let history_item_count = staged
            .thread
            .turns
            .iter()
            .map(|turn| turn.items.len())
            .sum::<usize>();
        let history_generated_image_count = staged
            .thread
            .turns
            .iter()
            .flat_map(|turn| turn.items.iter())
            .filter(|item| matches!(item, beryl_backend::ThreadItem::ImageGeneration(_)))
            .count();
        let presentability = staged.presentability.summary();
        publish_history_window(
            surface,
            &staged.thread,
            staged.history_window,
            &staged.image_resolver,
            staged.initial_viewport_policy,
        );
        if let Some(metadata) = staged.session_metadata {
            surface.set_thread_session_metadata(metadata);
        }
        debug!(
            thread_id = summary.id.as_str(),
            runtime = execution_target.runtime_mode().display_name(),
            activation_source = ?source,
            presentability_rows = presentability.row_count,
            presentable_rows = presentability.presentable_rows,
            completed_media_pending_rows = presentability.completed_media_pending_rows,
            "published staged selected-thread activation"
        );
        PublishedSelectedThreadActivation {
            summary,
            execution_target,
            source,
            activated_idle,
            history_turn_count,
            history_item_count,
            history_generated_image_count,
        }
    }
}

fn log_staged_activation_not_ready(staged: &StagedSelectedThreadActivation) {
    let presentability = staged.presentability.summary();
    debug!(
        thread_id = staged.thread.summary().id.as_str(),
        presentability_rows = presentability.row_count,
        presentable_rows = presentability.presentable_rows,
        completed_media_pending_rows = presentability.completed_media_pending_rows,
        "selected-thread activation remains staged pending activation-owned readiness"
    );
}

pub(super) fn publish_history_window(
    surface: &mut ConversationSurfaceState,
    thread: &beryl_backend::ThreadInfo,
    mut history_window: super::super::transcript_history::TranscriptHistoryWindow,
    image_resolver: &super::super::execution_detail::TranscriptImagePathResolver,
    initial_viewport_policy: SelectedThreadInitialViewportPolicy,
) {
    let load_started = std::time::Instant::now();
    let thread_id = thread.summary().id;
    match initial_viewport_policy {
        SelectedThreadInitialViewportPolicy::Tail => {}
    }
    if history_window.is_empty() {
        history_window =
            super::super::transcript_history::TranscriptHistoryWindow::from_turns(&thread.turns);
    }
    surface.upsert_selected_thread(thread.summary());
    surface.selected_thread_status = Some(thread.status.clone());
    surface.sync_thread_selector_active_thread();
    surface.composer_image_labels.observe_thread_history(thread);
    surface.composer_image_labels.prepare_thread_history_scan(
        &thread.summary().id,
        history_window.has_older_pages()
            || thread
                .turns
                .iter()
                .any(|turn| turn.items_view != beryl_backend::TurnItemsView::Full),
    );
    let execution_detail_started = std::time::Instant::now();
    surface
        .execution_details
        .load_thread_history_with_image_resolver_and_partial_mode(thread, image_resolver, false);
    let execution_detail_elapsed = execution_detail_started.elapsed();
    surface.hard_stop_targets.clear_all();
    let presentation_started = std::time::Instant::now();
    surface
        .transcript_presentation
        .replace_from_turns(surface.execution_details.turns());
    let presentation_elapsed = presentation_started.elapsed();
    if memory_diagnostics::enabled() {
        log_transcript_projection_update_memory(surface, thread_id.as_str());
    }
    surface.status_line.clear_session_metadata();
    surface.reset_loaded_history_live_scroll();
    surface.transcript_user_scrolled = false;
    surface.transcript_history_window = history_window;
    surface
        .transcript_history_window
        .bind_residency_to_thread(thread_id.as_str());
    surface.transcript_residency_diagnostics = Default::default();
    surface.transcript_reset_generation = surface.transcript_reset_generation.saturating_add(1);
    surface.transcript_content_release_generation = 0;
    surface.transcript_content_release_row_identities.clear();
    surface.transcript_content_release_markdown_keys.clear();
    surface.transcript_content_release_media_keys.clear();
    surface.transcript_residency_controller_facts = None;
    surface.invalidate_transcript_residency_controller();
    surface.invalidated_stream_turns.clear();
    surface.pending_thread_activation = None;
    surface.staged_selected_thread_activation = None;
    surface.clear_transcript_residency_page_admission();
    surface.context_compaction_thread_id = None;
    surface.close_transcript_branch_menu();
    surface.cancel_transcript_edit_mode();
    surface.pending_turn_input_queue = None;
    surface.pending_active_turn_steering_queue = None;
    surface.notices.clear_all();
    surface
        .transcript_list_state
        .reset(surface.transcript_list_item_count());
    let loaded_turns = surface.execution_details.turns().len();
    let admission_summary = TranscriptResidencyAdmissionSummary::from_admitted_turns(
        "initial",
        0..loaded_turns,
        &thread.turns,
    );
    surface.note_transcript_residency_admission(&admission_summary);
    transcript_residency_logging::log_transcript_resident_turns_admitted(
        thread_id.as_str(),
        &admission_summary,
    );
    debug!(
        thread_id = thread_id.as_str(),
        execution_detail_load_history_ms = elapsed_ms(execution_detail_elapsed),
        presentation_replace_from_turns_ms = elapsed_ms(presentation_elapsed),
        surface_load_thread_history_window_ms = elapsed_ms(load_started.elapsed()),
        "loaded thread history window into conversation surface"
    );
}

fn log_transcript_projection_update_memory(surface: &ConversationSurfaceState, thread_id: &str) {
    let turn_count = surface.execution_details.turns().len();
    let item_count = surface
        .execution_details
        .turns()
        .iter()
        .map(|turn| turn.items.len())
        .sum::<usize>();
    let generated_image_count = surface
        .execution_details
        .turns()
        .iter()
        .flat_map(|turn| turn.items.iter())
        .filter(|item| matches!(item, ExecutionItem::GeneratedImage(_)))
        .count();
    MemoryMilestone::new("transcript_projection_update")
        .thread_id(thread_id)
        .history_counts(turn_count, item_count, generated_image_count)
        .retained_state_if_enabled(|| surface.retained_state_snapshot())
        .log();
}
