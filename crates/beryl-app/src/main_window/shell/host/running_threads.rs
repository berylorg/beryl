use super::*;
use crate::app_services::PublishedRunningThreadsReader;
use crate::cas_projection::{
    ProcessWorkQueryPage, ProcessWorkQueryRecord, ProcessWorkQueryRevision,
    ProjectionCancellationToken,
};
use crate::thread_root_picker::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

mod activation;
pub(crate) use activation::OrdinaryThreadActivationAcceptance;
pub(in crate::main_window::shell::host) use activation::{
    CapturedClaimOperation, RetiringClaimOperation,
};
mod command;
mod lineage;
mod transcript;
pub(super) use command::{render_command, render_picker};

const RUNNING_READ_RESOURCE_CAPACITY: usize = 9;

fn running_resource_capacity_available(retained: usize) -> bool {
    retained.saturating_add(2) <= RUNNING_READ_RESOURCE_CAPACITY
}

struct RunningReadOutput<T> {
    result: T,
    _release: crate::main_window::conversation_composer_mount::worker::ResourceWorker<fn()>,
}

fn release_running_read_output() {}

pub(super) struct RunningThreadsContribution {
    pub(super) reader: Option<PublishedRunningThreadsReader>,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_reader: Option<std::sync::Weak<PublishedRunningThreadsReader>>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_selection_lease: Option<(
        beryl_model::SyndicThreadId,
        Arc<crate::window_acquisition::WindowSelectionLease>,
    )>,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_windows: Vec<WindowHandle<MainWindowShellRoot>>,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_activation_hooks: Option<activation::RunningActivationFixtureHooks>,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_pending_ready_observation: Option<
        Arc<
            std::sync::Mutex<
                Option<(
                    beryl_model::SyndicThreadId,
                    crate::main_window::MainWindowComposerSelectionIdentity,
                    gpui::Pixels,
                )>,
            >,
        >,
    >,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_real_creation_reconciliation: bool,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_creation_release_counts: (usize, usize),
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_creation_page_release_acknowledgements: usize,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_claim_page_release_evidence: Vec<(
        crate::main_window::MainWindowComposerSelectionIdentity,
        u64,
        u64,
        usize,
    )>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_claim_widget_release: Option<(
        crate::main_window::MainWindowClaimRetirementKind,
        crate::main_window::MainWindowComposerWidgetRelease,
    )>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_reject_creation_recovery_mount: bool,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_reject_creation_transcript_mount: bool,
    generation: Arc<AtomicU64>,
    cancellation: ProjectionCancellationToken,
    poll: Option<gpui::Task<()>>,
    workers: crate::main_window::conversation_composer_mount::worker::WorkerLifetime,
    pub(super) reads_suspended: bool,
    pub(super) views_retired: bool,
    read_drain_task: Option<gpui::Task<()>>,
    pub(super) transcript: Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    resource_subscription: Option<gpui::Subscription>,
    pub(super) quote_subscription: Option<gpui::Subscription>,
    pub(super) historical_replacement_subscription: Option<gpui::Subscription>,
    pub(super) image_preview_subscription: Option<gpui::Subscription>,
    pub(super) historical_dimming_key:
        Option<(u64, u64, Option<syndic_storage::ReplacementEditIntent>)>,
    quote_presentation_subscription: Option<gpui::Subscription>,
    pub(super) transcript_provider: Option<crate::transcript_provider::TranscriptProviderReader>,
    transcript_task: Option<gpui::Task<()>>,
    #[cfg(test)]
    fixture_fence_next_adjacent: bool,
    #[cfg(test)]
    fixture_adjacent_fenced: bool,
    transcript_demand: Option<crate::syndic_transcript::AdjacentTranscriptDemand>,
    resource_demand: Option<crate::syndic_transcript::ResourceTranscriptDemand>,
    measured_resource: Option<transcript::PendingMeasuredResource>,
    resource_refresh_due: bool,
    transcript_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub(super) transcript_claim: Option<beryl_state::WindowClaimSelection>,
    transcript_request: u64,
    pub(super) transcript_source:
        Option<crate::transcript_provider::TranscriptAttachmentSourceIdentity>,
    measured_attachment: Option<transcript::PendingMeasuredAttachment>,
    pub(super) recovered_attachment: Option<Result<transcript::PendingRecoveredAttachment, String>>,
    pages: VecDeque<ProcessWorkQueryPage>,
    source_revision: Option<ProcessWorkQueryRevision>,
    count: Option<u64>,
    attention: u64,
    failure: Option<String>,
    focus: gpui::FocusHandle,
    picker: Option<Entity<ThreadRootPicker>>,
    subscription: Option<gpui::Subscription>,
    query: beryl_state::CatalogNormalizedQuery,
    query_revision: u64,
    page_jobs: Vec<(
        PickerPageRequest,
        ProjectionCancellationToken,
        gpui::Task<()>,
    )>,
    pub(super) pending_activation: Option<beryl_model::SyndicThreadId>,
    ordinary_activation: bool,
    pub(super) navigation_history: activation::ThreadNavigationHistory,
    pub(super) selected_title: Option<(
        crate::main_window::MainWindowComposerSelectionIdentity,
        beryl_state::CatalogResolvedTitle,
    )>,
    lineage: lineage::SelectedLineage,
    activation_task: Option<gpui::Task<()>>,
    activation_wake: Option<Arc<()>>,
    activation_operation: Option<activation::UnviewedRunningActivation>,
    activation_cancel: beryl_home_store::CommandCancellation,
    prepared_activation:
        Option<crate::main_window::running_threads::activation::RunningThreadActivation>,
    activation_attention: Vec<crate::lifecycle_attention::LifecycleAttentionToken>,
    failure_notice: Option<crate::main_window::NoticeRecordToken>,
    selection_lease: Option<Arc<crate::window_acquisition::WindowSelectionLease>>,
}

impl Drop for RunningThreadsContribution {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.transcript_cancel.store(true, Ordering::Release);
        if let Some(provider) = &self.transcript_provider {
            provider.retire();
        }
        for (_, cancel, _) in &self.page_jobs {
            cancel.cancel();
        }
        self.lineage.cancel();
    }
}

impl RunningThreadsContribution {
    fn resource_subscription(
        transcript: &Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
        cx: &mut Context<MainWindowShellRoot>,
    ) -> gpui::Subscription {
        cx.subscribe(
            transcript,
            |root, _, _: &crate::syndic_transcript::ResourceViewportDemandChanged, cx| {
                if !root.running_threads.views_retired {
                    cx.notify();
                }
            },
        )
    }

    fn quote_presentation_subscription(
        transcript: &Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
        cx: &mut Context<MainWindowShellRoot>,
    ) -> gpui::Subscription {
        cx.observe(transcript, |root, _, cx| {
            if !root.running_threads.views_retired {
                cx.notify();
            }
        })
    }

    pub(in crate::main_window::shell::host) fn replace_transcript(
        &mut self,
        transcript: Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
        window: &mut Window,
        cx: &mut Context<MainWindowShellRoot>,
    ) {
        let admission = self.workers.admission(RUNNING_READ_RESOURCE_CAPACITY - 2);
        transcript.update(cx, |panel, _| panel.set_code_save_admission(admission));
        self.resource_subscription = Some(Self::resource_subscription(&transcript, cx));
        self.quote_presentation_subscription =
            Some(Self::quote_presentation_subscription(&transcript, cx));
        self.quote_subscription = Some(super::quote::subscribe_quote(&transcript, window, cx));
        self.image_preview_subscription = Some(super::transcript_image_preview::subscribe(&transcript, window, cx));
        self.historical_replacement_subscription = Some(
            super::historical_replacement::subscribe_historical_replacement(
                &transcript,
                window,
                cx,
            ),
        );
        self.transcript = transcript;
        self.historical_dimming_key = None;
    }

    fn resume_read_admission(&mut self) {
        self.reads_suspended = false;
        self.workers.suspend(false);
    }

    pub(super) fn new(cx: &mut Context<MainWindowShellRoot>) -> Self {
        let workers =
            crate::main_window::conversation_composer_mount::worker::WorkerLifetime::default();
        let admission = workers.admission(RUNNING_READ_RESOURCE_CAPACITY - 2);
        let transcript = cx.new(crate::syndic_transcript::SyndicTranscriptPanel::new);
        transcript.update(cx, |panel, _| panel.set_code_save_admission(admission));
        let resource_subscription = Self::resource_subscription(&transcript, cx);
        let quote_presentation_subscription =
            Self::quote_presentation_subscription(&transcript, cx);
        Self {
            reader: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_reader: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_selection_lease: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_windows: Vec::new(),
            #[cfg(all(test, feature = "test-faults"))]
            fixture_activation_hooks: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_pending_ready_observation: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_real_creation_reconciliation: false,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_creation_release_counts: (0, 0),
            #[cfg(all(test, feature = "test-faults"))]
            fixture_creation_page_release_acknowledgements: 0,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_claim_page_release_evidence: Vec::new(),
            #[cfg(all(test, feature = "test-faults"))]
            fixture_claim_widget_release: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_reject_creation_recovery_mount: false,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_reject_creation_transcript_mount: false,
            generation: Arc::new(AtomicU64::new(1)),
            cancellation: ProjectionCancellationToken::new(),
            poll: None,
            workers,
            reads_suspended: false,
            views_retired: false,
            read_drain_task: None,
            transcript,
            resource_subscription: Some(resource_subscription),
            quote_subscription: None,
            historical_replacement_subscription: None,
            image_preview_subscription: None,
            historical_dimming_key: None,
            quote_presentation_subscription: Some(quote_presentation_subscription),
            transcript_provider: None,
            transcript_task: None,
            #[cfg(test)]
            fixture_fence_next_adjacent: false,
            #[cfg(test)]
            fixture_adjacent_fenced: false,
            transcript_demand: None,
            resource_demand: None,
            measured_resource: None,
            resource_refresh_due: false,
            transcript_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            transcript_claim: None,
            transcript_request: 0,
            transcript_source: None,
            measured_attachment: None,
            recovered_attachment: None,
            pages: VecDeque::new(),
            source_revision: None,
            count: None,
            attention: 0,
            failure: None,
            focus: cx.focus_handle(),
            picker: None,
            subscription: None,
            query: beryl_state::CatalogNormalizedQuery::new("").expect("empty catalog query"),
            query_revision: 1,
            page_jobs: Vec::new(),
            pending_activation: None,
            ordinary_activation: false,
            navigation_history: Default::default(),
            selected_title: None,
            lineage: Default::default(),
            activation_task: None,
            activation_wake: None,
            activation_operation: None,
            activation_cancel: beryl_home_store::CommandCancellation::new(),
            prepared_activation: None,
            activation_attention: Vec::new(),
            failure_notice: None,
            selection_lease: None,
        }
    }

    pub(in crate::main_window::shell::host) fn has_activation_custody(&self) -> bool {
        self.pending_activation.is_some()
            || self.activation_operation.is_some()
            || self.activation_task.is_some()
            || self.prepared_activation.is_some()
            || self.selection_lease.is_some()
    }

    fn retire_source(&mut self) -> Result<(), String> {
        if self.has_activation_custody() || self.workers.retained() != 0 {
            return Err("Running threads still owns unsettled read or activation work.".into());
        }
        #[cfg(all(test, feature = "test-faults"))]
        self.fixture_selection_lease.take();
        if self.reader.is_none()
            && self.transcript_provider.is_none()
            && self.source_revision.is_none()
        {
            return Ok(());
        }
        if self
            .transcript_provider
            .as_ref()
            .is_some_and(|provider| !provider.retire())
        {
            return Err("Running threads transcript reads are still settling.".into());
        }
        self.generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                generation.checked_add(1)
            })
            .map_err(|_| "Running threads source generation is exhausted.".to_owned())?;
        if let Some(provider) = self.transcript_provider.take() {
            drop(provider);
        }
        self.transcript_source = None;
        self.lineage.observation = None;
        self.reader = None;
        self.source_revision = None;
        Ok(())
    }
}

impl MainWindowShellRoot {
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_fence_pending_resource_read(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let pending = self.test_pending_resource_read();
        if pending {
            self.ordinary_close_interaction_gated = true;
            self.suspend_running_thread_reads(window, cx);
        }
        pending
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_pending_resource_read(&self) -> bool {
        self.running_threads
            .resource_demand
            .as_ref()
            .is_some_and(|demand| demand.range.is_some() || demand.indexed.is_some())
            && self.running_threads.transcript_task.is_some()
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_fence_pending_adjacent_read(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.running_threads.fixture_adjacent_fenced {
            return true;
        }
        if self
            .running_threads
            .transcript
            .read(cx)
            .adjacent_demand()
            .is_some()
        {
            self.running_threads.fixture_fence_next_adjacent = true;
        }
        self.sync_running_transcript(window, cx);
        let pending = self.running_threads.transcript_demand.is_some()
            && self.running_threads.transcript_task.is_some()
            && !self
                .running_threads
                .transcript_cancel
                .load(Ordering::Acquire)
            && self
                .running_threads
                .transcript
                .read(cx)
                .adjacent_demand()
                .as_ref()
                == self.running_threads.transcript_demand.as_ref();
        if pending {
            self.running_threads.fixture_fence_next_adjacent = false;
            self.running_threads.fixture_adjacent_fenced = true;
            self.ordinary_close_interaction_gated = true;
            self.suspend_running_thread_reads(window, cx);
        }
        self.running_threads.fixture_adjacent_fenced
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_running_thread_reader(
        &mut self,
        reader: &Arc<PublishedRunningThreadsReader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !reader.current() {
            return Err("Running threads source retired".into());
        }
        self.running_threads.fixture_reader = Some(Arc::downgrade(reader));
        self.sync_running_threads(window, cx);
        Ok(())
    }

    pub(crate) fn running_thread_reads_drained(&self) -> bool {
        self.running_threads.workers.retained() == 0
            && self.running_threads.measured_resource.is_none()
            && !self.running_threads.has_activation_custody()
    }

    pub(crate) fn release_suspended_running_thread_sources(&mut self) -> bool {
        if self.running_thread_reads_drained()
            && self.running_threads.reader.is_none()
            && self.running_threads.transcript_provider.is_none()
        {
            self.running_threads.read_drain_task = None;
            return true;
        }
        let released = self.running_threads.reads_suspended
            && self.running_thread_reads_drained()
            && self.running_threads.retire_source().is_ok();
        if released {
            self.running_threads.read_drain_task = None;
        }
        released
    }

    fn running_read_capacity_available(&self) -> bool {
        running_resource_capacity_available(self.running_threads.workers.retained())
    }

    fn cancel_running_page_reads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let jobs = std::mem::take(&mut self.running_threads.page_jobs);
        for (request, cancel, _) in jobs {
            cancel.cancel();
            if let Some(picker) = self.running_threads.picker.clone() {
                picker.update(cx, |picker, picker_cx| {
                    picker.settle_page(PickerPageOutcome::Cancelled(request), window, picker_cx)
                });
            }
        }
    }

    pub(crate) fn suspend_running_thread_reads(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.running_threads.reads_suspended = true;
        self.running_threads.workers.suspend(true);
        self.running_threads
            .transcript
            .update(cx, |panel, cx| panel.fence_code_saves(cx));
        self.thread_navigation.status = [None, None];
        self.running_threads.cancellation.cancel();
        self.running_threads.poll = None;
        self.running_threads
            .transcript_cancel
            .store(true, Ordering::Release);
        self.running_threads.transcript_task = None;
        self.running_threads
            .transcript
            .update(cx, |panel, panel_cx| {
                panel.cancel_narrative_preparations();
                panel_cx.notify();
            });
        self.running_threads.transcript_demand = None;
        self.running_threads.resource_demand = None;
        self.running_threads.resource_refresh_due = false;
        if let Some(pending) = self.running_threads.measured_resource.take() {
            pending.discard(cx);
        }
        if self.running_threads.measured_attachment.take().is_some() {
            self.running_threads
                .transcript
                .update(cx, |panel, _| panel.discard_staged_activation());
        }
        self.cancel_running_page_reads(window, cx);
        self.cancel_lineage_reads(cx);
        if self.running_threads.read_drain_task.is_none() && !self.running_thread_reads_drained() {
            self.running_threads.read_drain_task =
                Some(cx.spawn_in(window, async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                        let drained = this
                            .update_in(cx, |root, window, cx| {
                                root.resume_running_activation(window, cx);
                                if !root.running_thread_reads_drained() {
                                    return false;
                                }
                                root.running_threads.read_drain_task = None;
                                root.resume_running_thread_reads(window, cx);
                                cx.notify();
                                true
                            })
                            .unwrap_or(true);
                        if drained {
                            break;
                        }
                    }
                }));
        }
        cx.notify();
    }

    pub(crate) fn resume_running_thread_reads(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running_threads.views_retired
            || self.startup_interaction_gated()
            || self.shutdown_interaction_gated
            || self.ordinary_close_interaction_gated
            || !self.running_thread_reads_drained()
        {
            return;
        }
        self.running_threads.resume_read_admission();
        self.running_threads.read_drain_task = None;
        self.settle_thread_navigation_history(cx);
        self.sync_running_threads(window, cx);
        if let Some(picker) = self.running_threads.picker.clone() {
            picker.update(cx, |picker, picker_cx| {
                picker.request_initial_page(picker_cx)
            });
        }
    }

    pub(crate) fn retire_running_thread_views(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.running_threads.recovered_attachment = None;
        self.running_threads
            .transcript
            .update(cx, |panel, _| panel.discard_staged_activation());
        self.suspend_running_thread_reads(window, cx);
        self.running_threads.retire_source()?;
        self.running_threads.read_drain_task = None;
        self.running_threads.views_retired = true;
        self.running_threads.resource_subscription = None;
        self.running_threads.quote_subscription = None;
        self.running_threads.historical_replacement_subscription = None;
        self.running_threads.image_preview_subscription = None;
        self.running_threads.quote_presentation_subscription = None;
        if let Some(picker) = self.running_threads.picker.take() {
            picker.update(cx, |picker, picker_cx| picker.dismiss(window, picker_cx));
        }
        self.running_threads.subscription = None;
        self.running_threads.lineage.widget = None;
        self.running_threads.lineage.subscription = None;
        Ok(())
    }

    pub(crate) fn matches_running_claim(
        &self,
        claim: beryl_state::ThreadClaimRecord,
        app: &gpui::App,
    ) -> bool {
        if claim.state() != beryl_state::ThreadClaimState::Active {
            return false;
        }
        let Some(controller) = self.controller.as_ref() else {
            return false;
        };
        if controller.window_id() != claim.window_id()
            || self.startup_interaction_gated()
            || self.shutdown_interaction_gated
            || self.ordinary_close_interaction_gated
        {
            return false;
        }
        let Some((selection, _)) = self.cached_running_selection(app) else {
            return false;
        };
        let selected = selection.claim();
        selected.thread_id() == claim.thread_id()
            && selected.generation() == claim.generation()
            && selected.revision() == claim.revision()
    }
    fn running_threads_enabled(&self) -> bool {
        !self.running_threads.reads_suspended
            && !self.running_threads.views_retired
            && !self.startup_interaction_gated()
            && !self.shutdown_interaction_gated
            && !self.ordinary_close_interaction_gated
            && self
                .running_threads
                .reader
                .as_ref()
                .is_some_and(PublishedRunningThreadsReader::current)
    }

    pub(super) fn sync_running_threads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.resume_running_activation(window, cx);
        if self.running_threads.views_retired {
            return;
        }
        if self.startup_interaction_gated()
            || self.shutdown_interaction_gated
            || self.ordinary_close_interaction_gated
        {
            if !self.running_threads.reads_suspended {
                self.suspend_running_thread_reads(window, cx);
            }
            return;
        }
        if self.running_threads.reads_suspended {
            if !self.running_thread_reads_drained() {
                return;
            }
            self.running_threads.resume_read_admission();
            self.running_threads.read_drain_task = None;
            self.settle_thread_navigation_history(cx);
        }
        #[cfg(target_os = "windows")]
        let reader = crate::running_owner::RunningProcessOwner::mounted_owner(cx)
            .and_then(|owner| owner.upgrade())
            .and_then(|owner| owner.borrow().running_threads_reader());
        #[cfg(not(target_os = "windows"))]
        let reader: Option<PublishedRunningThreadsReader> = None;
        #[cfg(all(test, feature = "test-faults"))]
        let reader = reader.or_else(|| {
            self.running_threads
                .fixture_reader
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .filter(|reader| reader.current())
                .map(|reader| (*reader).clone())
        });
        let same = match (&reader, &self.running_threads.reader) {
            (Some(next), Some(prior)) => next.same_publication(prior),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.suspend_running_thread_reads(window, cx);
            if self.running_threads.retire_source().is_err() {
                return;
            }
            self.running_threads.reader = reader;
            self.running_threads.resume_read_admission();
            self.running_threads.read_drain_task = None;
        }
        if self.running_threads.measured_attachment.is_some()
            || self.running_threads.measured_resource.is_some()
            || self.running_threads.resource_demand.is_some()
            || self
                .running_threads
                .transcript
                .read(cx)
                .narrative_demand()
                .is_some()
            || self
                .running_threads
                .transcript
                .read(cx)
                .resource_demand()
                .is_some()
            || self.running_threads.transcript_demand.is_some()
            || self
                .running_threads
                .transcript
                .read(cx)
                .adjacent_demand()
                .is_some()
            || self.running_threads.transcript_claim
                != self
                    .cached_running_selection(cx)
                    .map(|(selection, _)| selection.claim())
        {
            self.sync_running_transcript(window, cx);
        }
        if !self.running_threads_enabled() {
            return;
        }
        if self.running_threads.poll.is_some() {
            return;
        }
        let Some(reader) = self.running_threads.reader.clone() else {
            return;
        };
        self.running_threads.cancellation = ProjectionCancellationToken::new();
        let cancellation = self.running_threads.cancellation.clone();
        let generation = self.running_threads.generation.clone();
        let expected = generation.load(Ordering::Acquire);
        let executor = cx.background_executor().clone();
        let workers = self.running_threads.workers.clone();
        self.running_threads.poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                if cancellation.is_cancelled() || generation.load(Ordering::Acquire) != expected {
                    break;
                }
                let can_read = this
                    .update_in(cx, |root, _, _| {
                        !root.running_threads.reads_suspended
                            && !root.running_threads.has_activation_custody()
                            && root.running_read_capacity_available()
                    })
                    .unwrap_or(false);
                if !can_read {
                    executor.timer(Duration::from_millis(50)).await;
                    continue;
                }
                let Some((query, query_revision, focused, focused_position, navigation)) = this
                    .update_in(cx, |root, _, cx| {
                        let picker = root
                            .running_threads
                            .picker
                            .as_ref()
                            .map(|picker| picker.read(cx));
                        (
                            root.running_threads.query.clone(),
                            root.running_threads.query_revision,
                            picker
                                .and_then(|picker| picker.focused_key())
                                .and_then(|key| key.0.parse::<beryl_model::SyndicThreadId>().ok()),
                            picker.and_then(|picker| picker.focused_position()),
                            root.controller.as_ref().map(|controller| {
                                (
                                    controller.window_id(),
                                    root.cached_running_selection(cx)
                                        .map(|(selection, _)| selection.claim()),
                                    [
                                        root.running_threads.navigation_history.target(false),
                                        root.running_threads.navigation_history.target(true),
                                    ],
                                )
                            }),
                        )
                    })
                    .ok()
                else {
                    break;
                };
                let source = reader.clone();
                let cancel = cancellation.clone();
                let output_release = workers.track(release_running_read_output as fn());
                let work = workers.track(move || {
                    let result = (|| {
                        let page = source.query(&query, 0, &cancel)?;
                        let focus_position = focused
                            .map(|thread| source.position(page.revision(), &query, thread, &cancel))
                            .transpose()?
                            .flatten();
                        let navigation_status = navigation.map(|(window, prior, targets)| {
                            targets.map(|thread| {
                                thread.and_then(|thread| {
                                    source
                                        .ordinary_thread_unavailability(
                                            window, prior, thread, &cancel,
                                        )
                                        .ok()
                                        .map(|(observation, reason)| {
                                            super::thread_navigation::NavigationAvailability {
                                                thread,
                                                observation,
                                                reason,
                                            }
                                        })
                                })
                            })
                        });
                        Ok::<_, crate::cas_projection::ProcessWorkError>((
                            page,
                            focus_position,
                            navigation_status,
                        ))
                    })();
                    RunningReadOutput {
                        result,
                        _release: output_release,
                    }
                });
                let output = executor.spawn(async move { work.run() }).await;
                let admitted = this
                    .update_in(cx, |root, window, cx| {
                        let RunningReadOutput { _release, result } = output;
                        if root.running_threads.generation.load(Ordering::Acquire) != expected
                            || cancellation.is_cancelled()
                            || !root.running_threads_enabled()
                            || !root.running_threads.reader.as_ref().is_some_and(|current| {
                                current.same_publication(&reader) && current.current()
                            })
                        {
                            return false;
                        }
                        if root.running_threads.query_revision != query_revision {
                            return true;
                        }
                        match result {
                            Ok((page, position, navigation_status)) => {
                                if let (Some((invoking, prior, _)), Some(status)) =
                                    (navigation, navigation_status)
                                {
                                    if root.controller.as_ref().is_some_and(|controller| {
                                        controller.window_id() == invoking
                                    }) && root
                                        .cached_running_selection(cx)
                                        .map(|(selection, _)| selection.claim())
                                        == prior
                                    {
                                        for (index, status) in status.into_iter().enumerate() {
                                            root.thread_navigation.status[index] =
                                                status.filter(|status| {
                                                    root.running_threads
                                                        .navigation_history
                                                        .target(index == 1)
                                                        == Some(status.thread)
                                                        && reader
                                                            .elect(&status.observation, || ())
                                                            .is_ok()
                                                });
                                        }
                                    }
                                }
                                root.running_threads.count = Some(page.total_threads());
                                root.running_threads.attention = page.attention_threads();
                                root.running_threads.failure = None;
                                if root.running_threads.query_revision == query_revision
                                    && root.running_threads.source_revision.as_ref()
                                        != Some(page.revision())
                                {
                                    root.refresh_running_collection(
                                        page,
                                        focused.is_some(),
                                        position,
                                        focused_position,
                                        window,
                                        cx,
                                    );
                                }
                            }
                            Err(error) => root.running_threads.failure = Some(error.to_string()),
                        }
                        root.sync_running_transcript(window, cx);
                        root.sync_selected_lineage(window, cx);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !admitted {
                    break;
                }
                executor.timer(Duration::from_millis(500)).await;
            }
        }));
    }

    fn refresh_running_collection(
        &mut self,
        page: ProcessWorkQueryPage,
        had_focus: bool,
        position: Option<u64>,
        prior_position: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.running_threads.picker.clone() else {
            return;
        };
        if self.running_threads.pending_activation.is_some() {
            return;
        }
        let (Ok(total), Some(revision)) = (
            usize::try_from(page.matched_threads()),
            self.running_threads.query_revision.checked_add(1),
        ) else {
            return;
        };
        self.cancel_running_page_reads(window, cx);
        self.running_threads.pages.clear();
        self.running_threads.source_revision = Some(page.revision().clone());
        self.running_threads.query_revision = revision;
        let key = self.running_collection_key();
        let position = position.and_then(|position| usize::try_from(position).ok());
        picker.update(cx, |picker, picker_cx| {
            picker.replace_collection(key, revision, total, position, window, picker_cx);
            if had_focus && position.is_none() {
                picker.resolve_removed_focus(
                    prior_position
                        .filter(|_| total > 0)
                        .map(|position| position.min(total.saturating_sub(1))),
                    window,
                    picker_cx,
                );
            }
            picker.request_initial_page(picker_cx);
        });
    }

    fn running_collection_key(&self) -> PickerCollectionKey {
        PickerCollectionKey("running-threads".into())
    }

    pub(super) fn open_running_threads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.running_threads_enabled() || self.running_threads.picker.is_some() {
            return;
        }
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let config = ThreadRootPickerConfig {
            title: "Running threads".into(),
            helper: String::new(),
            heading: "RUNNING THREADS".into(),
            empty_text: "No running threads or threads needing attention.".into(),
            search_placeholder: "Search running threads".into(),
            owner_focus: self.running_threads.focus.clone(),
            appearance: Some(controller.appearance().clone()),
            style: ThreadRootPickerStyle::default(),
            scrollbar_style: controller.appearance.scrollbar.clone(),
        };
        self.running_threads.query =
            beryl_state::CatalogNormalizedQuery::new("").expect("empty query");
        self.running_threads.pages.clear();
        self.running_threads.source_revision = None;
        let Some(revision) = self.running_threads.query_revision.checked_add(1) else {
            return;
        };
        self.running_threads.query_revision = revision;
        let key = self.running_collection_key();
        let picker =
            cx.new(|picker_cx| ThreadRootPicker::new(config, key, revision, 0, window, picker_cx));
        self.running_threads.subscription = Some(cx.subscribe_in(
            &picker,
            window,
            |root, _, event: &PickerEvent, window, cx| {
                root.running_picker_event(event.clone(), window, cx);
            },
        ));
        self.running_threads.picker = Some(picker.clone());
        picker.update(cx, |picker, picker_cx| {
            picker.request_initial_page(picker_cx);
            picker.focus_search(window, picker_cx);
        });
        cx.notify();
    }

    fn running_picker_event(
        &mut self,
        event: PickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::QueryChanged {
                collection_key,
                query_revision,
                query,
            } => {
                if collection_key != self.running_collection_key()
                    || query_revision <= self.running_threads.query_revision
                {
                    return;
                }
                self.cancel_running_page_reads(window, cx);
                self.running_threads.pages.clear();
                self.running_threads.source_revision = None;
                self.running_threads.query_revision = query_revision;
                match beryl_state::CatalogNormalizedQuery::new(&query) {
                    Ok(query) => self.running_threads.query = query,
                    Err(error) => self.running_threads.failure = Some(error.to_string()),
                }
            }
            PickerEvent::RequestPage(request) => self.request_running_page(request, window, cx),
            PickerEvent::RequestRuntimePage(_)
            | PickerEvent::SelectionChanged(_)
            | PickerEvent::Command(_) => return,
            PickerEvent::Activate(key) => {
                if !self.running_threads_enabled()
                    || self.running_threads.pending_activation.is_some()
                {
                    if let Some(picker) = &self.running_threads.picker {
                        picker.update(cx, |picker, picker_cx| picker.finish_activation(picker_cx));
                    }
                    return;
                }
                let target = self
                    .running_threads
                    .pages
                    .iter()
                    .flat_map(|page| page.records())
                    .find(|row| row.thread_id.to_string() == key.0)
                    .cloned();
                if let Some(target) = target {
                    self.begin_running_activation(target, window, cx);
                } else if let Some(picker) = &self.running_threads.picker {
                    picker.update(cx, |picker, picker_cx| picker.finish_activation(picker_cx));
                }
            }
            PickerEvent::Dismiss => {
                self.running_threads.activation_cancel.cancel();
                self.cancel_running_page_reads(window, cx);
                self.running_threads.picker = None;
                self.running_threads.subscription = None;
                self.running_threads.pages.clear();
            }
        }
        cx.notify();
    }

    fn request_running_page(
        &mut self,
        request: PickerPageRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.running_threads.picker.clone() else {
            return;
        };
        if request.collection_key != self.running_collection_key()
            || request.query_revision != self.running_threads.query_revision
            || !self.running_threads_enabled()
            || self.running_threads.has_activation_custody()
        {
            picker.update(cx, |picker, picker_cx| {
                picker.settle_page(PickerPageOutcome::Cancelled(request), window, picker_cx)
            });
            return;
        }
        if self
            .running_threads
            .page_jobs
            .iter()
            .any(|(active, _, _)| active == &request)
        {
            return;
        }
        let start = u64::try_from(request.range.start);
        if self.running_threads.page_jobs.len() >= 2
            || !self.running_read_capacity_available()
            || start.is_err()
            || request.range.start >= request.range.end
            || request.range.len() > 32
        {
            picker.update(cx, |picker, picker_cx| {
                picker.settle_page(
                    PickerPageOutcome::Failed {
                        request,
                        message: "The running collection cannot admit this bounded page request."
                            .into(),
                    },
                    window,
                    picker_cx,
                )
            });
            return;
        }
        let Some(reader) = self.running_threads.reader.clone() else {
            return;
        };
        let start = start.expect("checked logical start");
        let query = self.running_threads.query.clone();
        let revision = self.running_threads.source_revision.clone();
        let cancel = ProjectionCancellationToken::new();
        let cancellation = cancel.clone();
        let executor = cx.background_executor().clone();
        let source = reader.clone();
        let output_release = self
            .running_threads
            .workers
            .track(release_running_read_output as fn());
        let work = self.running_threads.workers.track(move || {
            let result = match revision {
                Some(revision) => source.page(&revision, &query, start, &cancellation),
                None => source.query(&query, start, &cancellation),
            };
            RunningReadOutput {
                result,
                _release: output_release,
            }
        });
        let job = executor.spawn(async move { work.run() });
        let pending_request = request.clone();
        let settled_cancel = cancel.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let output = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                let RunningReadOutput { _release, result } = output;
                root.running_threads
                    .page_jobs
                    .retain(|(pending, _, _)| pending != &request);
                let Some(picker) = root.running_threads.picker.clone() else {
                    return;
                };
                if request.collection_key != root.running_collection_key()
                    || request.query_revision != root.running_threads.query_revision
                    || settled_cancel.is_cancelled()
                    || !root.running_threads_enabled()
                    || !reader.current()
                    || !root
                        .running_threads
                        .reader
                        .as_ref()
                        .is_some_and(|current| current.same_publication(&reader))
                {
                    picker.update(cx, |picker, picker_cx| {
                        picker.settle_page(PickerPageOutcome::Cancelled(request), window, picker_cx)
                    });
                    return;
                }
                let outcome = match result {
                    Ok(page) => {
                        if root
                            .running_threads
                            .source_revision
                            .as_ref()
                            .is_some_and(|revision| revision != page.revision())
                        {
                            picker.update(cx, |picker, picker_cx| {
                                picker.settle_page(
                                    PickerPageOutcome::Cancelled(request),
                                    window,
                                    picker_cx,
                                )
                            });
                            return;
                        }
                        match usize::try_from(page.matched_threads()) {
                            Ok(total_count)
                                if page.logical_start() == start
                                    && page.records().len() <= request.range.len()
                                    && ((page.records().is_empty()
                                        && request.range.start >= total_count)
                                        || (!page.records().is_empty()
                                            && request
                                                .range
                                                .start
                                                .checked_add(page.records().len())
                                                .is_some_and(|end| end <= total_count))) =>
                            {
                                let window_id = root
                                    .controller
                                    .as_ref()
                                    .map(MainWindowShellController::window_id);
                                let rows = page
                                    .records()
                                    .iter()
                                    .map(|row| picker_row(row, window_id))
                                    .collect();
                                root.running_threads.source_revision =
                                    Some(page.revision().clone());
                                root.running_threads.count = Some(page.total_threads());
                                root.running_threads.attention = page.attention_threads();
                                root.running_threads.pages.push_back(page);
                                while root.running_threads.pages.len() > 24 {
                                    root.running_threads.pages.pop_front();
                                }
                                PickerPageOutcome::Success(PickerPage {
                                    request: request.clone(),
                                    total_count,
                                    rows,
                                })
                            }
                            _ => PickerPageOutcome::Failed {
                                request: request.clone(),
                                message:
                                    "The running collection returned an unsupported logical range."
                                        .into(),
                            },
                        }
                    }
                    Err(
                        crate::cas_projection::ProcessWorkError::Cancelled
                        | crate::cas_projection::ProcessWorkError::StaleRevision
                        | crate::cas_projection::ProcessWorkError::Closed,
                    ) => PickerPageOutcome::Cancelled(request.clone()),
                    Err(error) => PickerPageOutcome::Failed {
                        request: request.clone(),
                        message: error.to_string(),
                    },
                };
                picker.update(cx, |picker, picker_cx| {
                    picker.settle_page(outcome, window, picker_cx)
                });
                cx.notify();
            });
        });
        self.running_threads
            .page_jobs
            .push((pending_request, cancel, task));
    }
}

fn picker_row(row: &ProcessWorkQueryRecord, window: Option<beryl_model::WindowId>) -> PickerRow {
    let execution = row.catalog.execution();
    let current = row
        .claim
        .is_some_and(|claim| Some(claim.window_id()) == window);
    let elsewhere = row
        .claim
        .is_some_and(|claim| Some(claim.window_id()) != window);
    let secondary = format!(
        "{} · {} · {}{}",
        execution.environment_label(),
        execution.configured_executable_path().as_str(),
        execution.full_root_path().as_str(),
        if elsewhere {
            " · Open in another window"
        } else if current {
            " · Current window"
        } else {
            " · Unviewed"
        }
    );
    let status = if !row.attention.is_empty() {
        "NEEDS ATTENTION"
    } else if row.facts.stopping {
        "STOPPING"
    } else if row.facts.compacting {
        "COMPACTING"
    } else if row.facts.request_handling {
        "HANDLING REQUEST"
    } else if row.facts.terminal_settlement {
        "FINALIZING HISTORY"
    } else if row.facts.preparing {
        "PREPARING"
    } else if row.facts.continuation {
        "CONTINUING"
    } else if row.facts.executing {
        "RUNNING"
    } else {
        "PENDING"
    };
    let primary = row.catalog.title().text().unwrap_or("Untitled thread");
    PickerRow {
        key: PickerRowKey(row.thread_id.to_string()),
        primary: primary.into(),
        tooltip: Some(format!("{primary}\n{secondary}")),
        secondary,
        status: status.into(),
        unavailable_reason: None,
        current,
        activation_pending: false,
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/running_threads_shell.rs"
    ));
}
