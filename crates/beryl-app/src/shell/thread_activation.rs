use std::time::{Duration, Instant};

use beryl_backend::{
    ManagedBackendSession, ThreadInfo, ThreadItem, ThreadSessionMetadata, ThreadSessionResponse,
    ThreadSummary, ThreadTurnsListResponse,
};
use beryl_model::workspace::WorkspaceId;
use tracing::debug;

use crate::memory_diagnostics::MemoryMilestone;

use super::thread_selection::thread_rebind_detail;
use super::transcript_history::{
    TranscriptHistoryBackend, TranscriptHistoryWindow, initial_thread_history_page_options,
    loaded_page_from_desc_response,
};

#[derive(Debug)]
pub(crate) struct ExistingThreadActivation {
    pub thread: ThreadInfo,
    pub session_metadata: ThreadSessionMetadata,
    pub history_window: TranscriptHistoryWindow,
}

#[derive(Debug)]
pub(crate) enum ExistingThreadActivationError {
    RequiresRebind { detail: String },
    Failed { message: String },
}

pub(crate) trait ExistingThreadActivationBackend: TranscriptHistoryBackend {
    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error>;
}

impl ExistingThreadActivationBackend for ManagedBackendSession {
    fn resume_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        ManagedBackendSession::resume_thread_metadata(self, thread_id, timeout)
    }
}

pub(crate) fn activate_existing_thread_direct<B>(
    backend: &mut B,
    execution_target: &WorkspaceId,
    thread_id: &str,
    label: &str,
    timeout: Duration,
) -> Result<ExistingThreadActivation, ExistingThreadActivationError>
where
    B: ExistingThreadActivationBackend,
{
    let activation_started = Instant::now();
    let resume_started = Instant::now();
    let response = backend
        .resume_thread_metadata(thread_id, timeout)
        .map_err(|error| ExistingThreadActivationError::RequiresRebind {
            detail: thread_rebind_detail(
                label,
                execution_target,
                &format!("Beryl could not reopen the recorded thread: {error}."),
            ),
        })?;
    debug!(
        thread_id,
        resume_metadata_ms = elapsed_ms(resume_started.elapsed()),
        "resumed existing thread metadata"
    );
    MemoryMilestone::new("thread_activation_metadata_resumed")
        .runtime(execution_target.runtime_mode().display_name())
        .thread_id(thread_id)
        .log();
    let session_metadata = response.metadata();
    let mut thread = response.thread;
    let summary = thread.summary();
    validate_thread_execution_target(&summary, execution_target, label)?;

    let page_options = initial_thread_history_page_options();
    let history_read_started = Instant::now();
    let turns = backend
        .list_thread_turns(&summary.id, &page_options, timeout)
        .map_err(|error| ExistingThreadActivationError::Failed {
            message: format!("Beryl could not load the requested thread history: {error}"),
        })?;
    let history_stats = history_page_stats(&turns);
    MemoryMilestone::new("transcript_page_receipt")
        .runtime(execution_target.runtime_mode().display_name())
        .thread_id(thread_id)
        .history_counts(
            history_stats.turn_count,
            history_stats.item_count,
            history_stats.generated_image_count,
        )
        .log();
    debug!(
        thread_id,
        initial_history_read_ms = elapsed_ms(history_read_started.elapsed()),
        initial_history_turns = history_stats.turn_count,
        initial_history_items = history_stats.item_count,
        initial_history_generated_images = history_stats.generated_image_count,
        "loaded initial existing-thread history page"
    );
    let history_apply_started = Instant::now();
    let history_window = apply_initial_thread_history_page(&mut thread, turns);
    MemoryMilestone::new("transcript_page_applied_worker")
        .runtime(execution_target.runtime_mode().display_name())
        .thread_id(thread_id)
        .history_counts(
            history_stats.turn_count,
            history_stats.item_count,
            history_stats.generated_image_count,
        )
        .log();
    debug!(
        thread_id,
        initial_history_apply_ms = elapsed_ms(history_apply_started.elapsed()),
        worker_activation_total_ms = elapsed_ms(activation_started.elapsed()),
        "applied initial existing-thread history page"
    );

    Ok(ExistingThreadActivation {
        thread,
        session_metadata,
        history_window,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct HistoryPageStats {
    turn_count: usize,
    item_count: usize,
    generated_image_count: usize,
}

fn history_page_stats(page: &ThreadTurnsListResponse) -> HistoryPageStats {
    let mut stats = HistoryPageStats {
        turn_count: page.data.len(),
        ..HistoryPageStats::default()
    };
    for turn in &page.data {
        stats.item_count += turn.items.len();
        stats.generated_image_count += turn
            .items
            .iter()
            .filter(|item| matches!(item, ThreadItem::ImageGeneration(_)))
            .count();
    }
    stats
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(crate) fn apply_initial_thread_history_page(
    thread: &mut ThreadInfo,
    page: ThreadTurnsListResponse,
) -> TranscriptHistoryWindow {
    let page = loaded_page_from_desc_response(page);
    thread.turns = page.turns.clone();
    TranscriptHistoryWindow::from_latest_page(&page)
}

pub(crate) fn validate_thread_execution_target(
    summary: &ThreadSummary,
    execution_target: &WorkspaceId,
    label: &str,
) -> Result<(), ExistingThreadActivationError> {
    let expected = execution_target.canonical_path();
    if summary.cwd == expected {
        return Ok(());
    }

    Err(ExistingThreadActivationError::RequiresRebind {
        detail: thread_rebind_detail(
            label,
            execution_target,
            &format!(
                "The reopened thread records working directory {}, but the expected workspace member is {}.",
                summary.cwd.display(),
                expected.display()
            ),
        ),
    })
}
