use std::sync::atomic::{AtomicBool, Ordering};

use tracing::info;

const TARGET: &str = "beryl_app::memory_milestones";

static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, Default)]
pub(crate) struct MemoryMilestone {
    milestone: &'static str,
    workspace_id: Option<String>,
    runtime: Option<String>,
    thread_id: Option<String>,
    backend_pid: Option<u32>,
    turn_count: Option<usize>,
    item_count: Option<usize>,
    generated_image_count: Option<usize>,
    retained_state: RetainedStateSnapshot,
    note: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RetainedStateSnapshot {
    pub(crate) retained_payload_bytes_lower_bound: Option<usize>,
    pub(crate) loaded_transcript_turns: Option<usize>,
    pub(crate) loaded_transcript_items: Option<usize>,
    pub(crate) loaded_transcript_text_bytes: Option<usize>,
    pub(crate) transcript_user_fragments: Option<usize>,
    pub(crate) transcript_backend_input_records: Option<usize>,
    pub(crate) transcript_narrative_entries: Option<usize>,
    pub(crate) released_transcript_placeholders: Option<usize>,
    pub(crate) presentation_rows: Option<usize>,
    pub(crate) presentation_items: Option<usize>,
    pub(crate) presentation_text_bytes: Option<usize>,
    pub(crate) presentation_range_rows: Option<usize>,
    pub(crate) history_pages: Option<usize>,
    pub(crate) history_resident_pages: Option<usize>,
    pub(crate) history_released_pages: Option<usize>,
    pub(crate) markdown_cache_entries: Option<usize>,
    pub(crate) markdown_cache_pending_entries: Option<usize>,
    pub(crate) markdown_source_bytes: Option<usize>,
    pub(crate) markdown_blocks: Option<usize>,
    pub(crate) markdown_inlines: Option<usize>,
    pub(crate) markdown_media_requests: Option<usize>,
    pub(crate) media_cache_entries: Option<usize>,
    pub(crate) media_cache_pending_entries: Option<usize>,
    pub(crate) media_cache_loaded_entries: Option<usize>,
    pub(crate) media_cache_loaded_image_bytes: Option<usize>,
    pub(crate) media_cache_decoded_image_bytes_estimate: Option<usize>,
    pub(crate) media_cache_thumbnail_count: Option<usize>,
    pub(crate) activity_records: Option<usize>,
    pub(crate) activity_rows: Option<usize>,
    pub(crate) activity_visible_thread_indexes: Option<usize>,
    pub(crate) graph_nodes: Option<usize>,
    pub(crate) graph_soft_links: Option<usize>,
    pub(crate) graph_thread_refs: Option<usize>,
    pub(crate) graph_committed_nodes: Option<usize>,
    pub(crate) graph_committed_soft_links: Option<usize>,
    pub(crate) graph_committed_thread_refs: Option<usize>,
    pub(crate) graph_columns: Option<usize>,
    pub(crate) graph_pending_optimistic_mutations: Option<usize>,
    pub(crate) graph_queued_commits: Option<usize>,
    pub(crate) inventory_groups: Option<usize>,
    pub(crate) inventory_threads: Option<usize>,
    pub(crate) known_threads: Option<usize>,
    pub(crate) backend_work_receivers: Option<usize>,
    pub(crate) backend_event_queue_estimate: Option<usize>,
    pub(crate) backend_client_connection_estimate: Option<usize>,
    pub(crate) turn_steering_receivers: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct ProcessMemorySnapshot {
    pid: u32,
    private_bytes: u64,
    working_set_bytes: u64,
    pagefile_usage_bytes: u64,
}

impl MemoryMilestone {
    pub(crate) fn new(milestone: &'static str) -> Self {
        Self {
            milestone,
            ..Self::default()
        }
    }

    pub(crate) fn workspace_id(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    pub(crate) fn runtime(mut self, runtime: impl Into<String>) -> Self {
        self.runtime = Some(runtime.into());
        self
    }

    pub(crate) fn thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    pub(crate) fn backend_pid(mut self, backend_pid: Option<u32>) -> Self {
        self.backend_pid = backend_pid;
        self
    }

    pub(crate) fn turn_count(mut self, turn_count: usize) -> Self {
        self.turn_count = Some(turn_count);
        self
    }

    pub(crate) fn history_counts(
        mut self,
        turn_count: usize,
        item_count: usize,
        generated_image_count: usize,
    ) -> Self {
        self.turn_count = Some(turn_count);
        self.item_count = Some(item_count);
        self.generated_image_count = Some(generated_image_count);
        self
    }

    pub(crate) fn retained_state(mut self, retained_state: RetainedStateSnapshot) -> Self {
        self.retained_state = retained_state;
        self
    }

    pub(crate) fn retained_state_if_enabled(
        mut self,
        retained_state: impl FnOnce() -> RetainedStateSnapshot,
    ) -> Self {
        if enabled() {
            self.retained_state = retained_state();
        }
        self
    }

    pub(crate) fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub(crate) fn log(self) {
        if !enabled() {
            return;
        }

        let workspace_id = self.workspace_id.unwrap_or_default();
        let runtime = self.runtime.unwrap_or_default();
        let thread_id = self.thread_id.unwrap_or_default();
        let backend_pid = self
            .backend_pid
            .map(|value| value.to_string())
            .unwrap_or_default();
        let turn_count = self
            .turn_count
            .map(|value| value.to_string())
            .unwrap_or_default();
        let item_count = self
            .item_count
            .map(|value| value.to_string())
            .unwrap_or_default();
        let generated_image_count = self
            .generated_image_count
            .map(|value| value.to_string())
            .unwrap_or_default();
        let retained_state = self.retained_state;
        let retained_payload_bytes_lower_bound =
            optional_usize(retained_state.retained_payload_bytes_lower_bound);
        let loaded_transcript_turns = optional_usize(retained_state.loaded_transcript_turns);
        let loaded_transcript_items = optional_usize(retained_state.loaded_transcript_items);
        let loaded_transcript_text_bytes =
            optional_usize(retained_state.loaded_transcript_text_bytes);
        let transcript_user_fragments = optional_usize(retained_state.transcript_user_fragments);
        let transcript_backend_input_records =
            optional_usize(retained_state.transcript_backend_input_records);
        let transcript_narrative_entries =
            optional_usize(retained_state.transcript_narrative_entries);
        let released_transcript_placeholders =
            optional_usize(retained_state.released_transcript_placeholders);
        let presentation_rows = optional_usize(retained_state.presentation_rows);
        let presentation_items = optional_usize(retained_state.presentation_items);
        let presentation_text_bytes = optional_usize(retained_state.presentation_text_bytes);
        let presentation_range_rows = optional_usize(retained_state.presentation_range_rows);
        let history_pages = optional_usize(retained_state.history_pages);
        let history_resident_pages = optional_usize(retained_state.history_resident_pages);
        let history_released_pages = optional_usize(retained_state.history_released_pages);
        let markdown_cache_entries = optional_usize(retained_state.markdown_cache_entries);
        let markdown_cache_pending_entries =
            optional_usize(retained_state.markdown_cache_pending_entries);
        let markdown_source_bytes = optional_usize(retained_state.markdown_source_bytes);
        let markdown_blocks = optional_usize(retained_state.markdown_blocks);
        let markdown_inlines = optional_usize(retained_state.markdown_inlines);
        let markdown_media_requests = optional_usize(retained_state.markdown_media_requests);
        let media_cache_entries = optional_usize(retained_state.media_cache_entries);
        let media_cache_pending_entries =
            optional_usize(retained_state.media_cache_pending_entries);
        let media_cache_loaded_entries = optional_usize(retained_state.media_cache_loaded_entries);
        let media_cache_loaded_image_bytes =
            optional_usize(retained_state.media_cache_loaded_image_bytes);
        let media_cache_decoded_image_bytes_estimate =
            optional_usize(retained_state.media_cache_decoded_image_bytes_estimate);
        let media_cache_thumbnail_count =
            optional_usize(retained_state.media_cache_thumbnail_count);
        let activity_records = optional_usize(retained_state.activity_records);
        let activity_rows = optional_usize(retained_state.activity_rows);
        let activity_visible_thread_indexes =
            optional_usize(retained_state.activity_visible_thread_indexes);
        let graph_nodes = optional_usize(retained_state.graph_nodes);
        let graph_soft_links = optional_usize(retained_state.graph_soft_links);
        let graph_thread_refs = optional_usize(retained_state.graph_thread_refs);
        let graph_committed_nodes = optional_usize(retained_state.graph_committed_nodes);
        let graph_committed_soft_links = optional_usize(retained_state.graph_committed_soft_links);
        let graph_committed_thread_refs =
            optional_usize(retained_state.graph_committed_thread_refs);
        let graph_columns = optional_usize(retained_state.graph_columns);
        let graph_pending_optimistic_mutations =
            optional_usize(retained_state.graph_pending_optimistic_mutations);
        let graph_queued_commits = optional_usize(retained_state.graph_queued_commits);
        let inventory_groups = optional_usize(retained_state.inventory_groups);
        let inventory_threads = optional_usize(retained_state.inventory_threads);
        let known_threads = optional_usize(retained_state.known_threads);
        let backend_work_receivers = optional_usize(retained_state.backend_work_receivers);
        let backend_event_queue_estimate =
            optional_usize(retained_state.backend_event_queue_estimate);
        let backend_client_connection_estimate =
            optional_usize(retained_state.backend_client_connection_estimate);
        let turn_steering_receivers = optional_usize(retained_state.turn_steering_receivers);
        let note = self.note.unwrap_or_default();

        match current_process_memory() {
            Ok(snapshot) => {
                info!(
                    target: TARGET,
                    milestone = self.milestone,
                    pid = snapshot.pid,
                    private_bytes = snapshot.private_bytes,
                    working_set_bytes = snapshot.working_set_bytes,
                    pagefile_usage_bytes = snapshot.pagefile_usage_bytes,
                    workspace_id = %workspace_id,
                    runtime = %runtime,
                    thread_id = %thread_id,
                    backend_pid = %backend_pid,
                    turn_count = %turn_count,
                    item_count = %item_count,
                    generated_image_count = %generated_image_count,
                    retained_payload_bytes_lower_bound = %retained_payload_bytes_lower_bound,
                    loaded_transcript_turns = %loaded_transcript_turns,
                    loaded_transcript_items = %loaded_transcript_items,
                    loaded_transcript_text_bytes = %loaded_transcript_text_bytes,
                    transcript_user_fragments = %transcript_user_fragments,
                    transcript_backend_input_records = %transcript_backend_input_records,
                    transcript_narrative_entries = %transcript_narrative_entries,
                    released_transcript_placeholders = %released_transcript_placeholders,
                    presentation_rows = %presentation_rows,
                    presentation_items = %presentation_items,
                    presentation_text_bytes = %presentation_text_bytes,
                    presentation_range_rows = %presentation_range_rows,
                    history_pages = %history_pages,
                    history_resident_pages = %history_resident_pages,
                    history_released_pages = %history_released_pages,
                    markdown_cache_entries = %markdown_cache_entries,
                    markdown_cache_pending_entries = %markdown_cache_pending_entries,
                    markdown_source_bytes = %markdown_source_bytes,
                    markdown_blocks = %markdown_blocks,
                    markdown_inlines = %markdown_inlines,
                    markdown_media_requests = %markdown_media_requests,
                    media_cache_entries = %media_cache_entries,
                    media_cache_pending_entries = %media_cache_pending_entries,
                    media_cache_loaded_entries = %media_cache_loaded_entries,
                    media_cache_loaded_image_bytes = %media_cache_loaded_image_bytes,
                    media_cache_decoded_image_bytes_estimate = %media_cache_decoded_image_bytes_estimate,
                    media_cache_thumbnail_count = %media_cache_thumbnail_count,
                    activity_records = %activity_records,
                    activity_rows = %activity_rows,
                    activity_visible_thread_indexes = %activity_visible_thread_indexes,
                    graph_nodes = %graph_nodes,
                    graph_soft_links = %graph_soft_links,
                    graph_thread_refs = %graph_thread_refs,
                    graph_committed_nodes = %graph_committed_nodes,
                    graph_committed_soft_links = %graph_committed_soft_links,
                    graph_committed_thread_refs = %graph_committed_thread_refs,
                    graph_columns = %graph_columns,
                    graph_pending_optimistic_mutations = %graph_pending_optimistic_mutations,
                    graph_queued_commits = %graph_queued_commits,
                    inventory_groups = %inventory_groups,
                    inventory_threads = %inventory_threads,
                    known_threads = %known_threads,
                    backend_work_receivers = %backend_work_receivers,
                    backend_event_queue_estimate = %backend_event_queue_estimate,
                    backend_client_connection_estimate = %backend_client_connection_estimate,
                    turn_steering_receivers = %turn_steering_receivers,
                    note = %note,
                    "memory milestone"
                );
            }
            Err(error) => {
                info!(
                    target: TARGET,
                    milestone = self.milestone,
                    pid = std::process::id(),
                    memory_counters_available = false,
                    error = %error,
                    workspace_id = %workspace_id,
                    runtime = %runtime,
                    thread_id = %thread_id,
                    backend_pid = %backend_pid,
                    turn_count = %turn_count,
                    item_count = %item_count,
                    generated_image_count = %generated_image_count,
                    retained_payload_bytes_lower_bound = %retained_payload_bytes_lower_bound,
                    loaded_transcript_turns = %loaded_transcript_turns,
                    loaded_transcript_items = %loaded_transcript_items,
                    loaded_transcript_text_bytes = %loaded_transcript_text_bytes,
                    transcript_user_fragments = %transcript_user_fragments,
                    transcript_backend_input_records = %transcript_backend_input_records,
                    transcript_narrative_entries = %transcript_narrative_entries,
                    released_transcript_placeholders = %released_transcript_placeholders,
                    presentation_rows = %presentation_rows,
                    presentation_items = %presentation_items,
                    presentation_text_bytes = %presentation_text_bytes,
                    presentation_range_rows = %presentation_range_rows,
                    history_pages = %history_pages,
                    history_resident_pages = %history_resident_pages,
                    history_released_pages = %history_released_pages,
                    markdown_cache_entries = %markdown_cache_entries,
                    markdown_cache_pending_entries = %markdown_cache_pending_entries,
                    markdown_source_bytes = %markdown_source_bytes,
                    markdown_blocks = %markdown_blocks,
                    markdown_inlines = %markdown_inlines,
                    markdown_media_requests = %markdown_media_requests,
                    media_cache_entries = %media_cache_entries,
                    media_cache_pending_entries = %media_cache_pending_entries,
                    media_cache_loaded_entries = %media_cache_loaded_entries,
                    media_cache_loaded_image_bytes = %media_cache_loaded_image_bytes,
                    media_cache_decoded_image_bytes_estimate = %media_cache_decoded_image_bytes_estimate,
                    media_cache_thumbnail_count = %media_cache_thumbnail_count,
                    activity_records = %activity_records,
                    activity_rows = %activity_rows,
                    activity_visible_thread_indexes = %activity_visible_thread_indexes,
                    graph_nodes = %graph_nodes,
                    graph_soft_links = %graph_soft_links,
                    graph_thread_refs = %graph_thread_refs,
                    graph_committed_nodes = %graph_committed_nodes,
                    graph_committed_soft_links = %graph_committed_soft_links,
                    graph_committed_thread_refs = %graph_committed_thread_refs,
                    graph_columns = %graph_columns,
                    graph_pending_optimistic_mutations = %graph_pending_optimistic_mutations,
                    graph_queued_commits = %graph_queued_commits,
                    inventory_groups = %inventory_groups,
                    inventory_threads = %inventory_threads,
                    known_threads = %known_threads,
                    backend_work_receivers = %backend_work_receivers,
                    backend_event_queue_estimate = %backend_event_queue_estimate,
                    backend_client_connection_estimate = %backend_client_connection_estimate,
                    turn_steering_receivers = %turn_steering_receivers,
                    note = %note,
                    "memory milestone unavailable"
                );
            }
        }
    }
}

pub(crate) fn configure(enabled: bool) {
    ENABLED.store(enabled, Ordering::Release);
}

pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

fn optional_usize(value: Option<usize>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn current_process_memory() -> Result<ProcessMemorySnapshot, &'static str> {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..PROCESS_MEMORY_COUNTERS_EX::default()
    };
    let cb = counters.cb;

    unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            cb,
        )
        .map_err(|_| "GetProcessMemoryInfo failed")?;
    }

    Ok(ProcessMemorySnapshot {
        pid: std::process::id(),
        private_bytes: counters.PrivateUsage as u64,
        working_set_bytes: counters.WorkingSetSize as u64,
        pagefile_usage_bytes: counters.PagefileUsage as u64,
    })
}

#[cfg(not(target_os = "windows"))]
fn current_process_memory() -> Result<ProcessMemorySnapshot, &'static str> {
    Err("process memory counters are only implemented on Windows")
}
