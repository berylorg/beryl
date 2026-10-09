mod failed_claim_capture;
mod presentation;
mod source;
mod startup;
mod thread_creation;
pub(in crate::main_window::shell::host) use failed_claim_capture::{
    CapturedClaimOperation, RetiringClaimOperation,
};

use super::*;
use crate::composer_host::{
    ComposerHostAutosaveInterval, ComposerHostFlushAdmission as FlushAdmission,
    ComposerHostFlushCapture as FlushCapture, ComposerHostFlushState as FlushState,
};
use crate::main_window::conversation_composer_mount::autosave::{
    current_timestamp, fresh_marker_authority, fresh_piece_operation_id,
};
use crate::main_window::running_threads::activation::{
    RunningThreadActivationCommit as Commit, RunningThreadActivationOutcome as Outcome,
};
use crate::main_window::{
    MainWindowComposerActivationAdvance as ActivationAdvance,
    MainWindowComposerActivationReceipt as Receipt, MainWindowComposerClaimAdvance as ClaimAdvance,
    MainWindowComposerClaimAutosave as Autosave, MainWindowComposerClaimCompletion as Completion,
    MainWindowComposerClaimPreparedPresentation as Presentation,
    MainWindowComposerClaimPublication as Publication,
    MainWindowComposerClaimWidgetWork as WidgetWork,
    MainWindowComposerPublishAdvance as PublishAdvance,
    MainWindowComposerRetirementAdvance as RetirementAdvance,
    MainWindowComposerSelectionIdentity as Selection, MainWindowComposerWidgetRelease as Release,
    MainWindowConversationComposerMount as Mount, MainWindowConversationComposerService as Service,
};
use crate::syndic_transcript::{SyndicTranscriptPanel, TranscriptActivationPlacement};
use crate::transcript_provider::{
    PreparedTranscriptAttachment, TranscriptAttachmentPurpose, TranscriptAttachmentRequest,
    TranscriptProviderReader,
};
use std::sync::{Mutex, atomic::AtomicBool};

#[cfg(test)]
mod owner_diagnostics {
    use super::*;

    #[derive(Clone, Copy)]
    struct Entry {
        key: usize,
        owners: [usize; 3],
    }

    static ENTRIES: Mutex<[Entry; 64]> = Mutex::new(
        [Entry {
            key: 0,
            owners: [0; 3],
        }; 64],
    );
    static UNTRACKED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    pub(super) enum Owner {
        Resume,
        Worker,
        Completion,
    }

    pub(super) struct Witness {
        key: usize,
        owner: usize,
        tracked: bool,
    }

    impl Witness {
        pub(super) fn new(source: &Arc<Mutex<ActivationSource>>, owner: Owner) -> Self {
            let key = Arc::as_ptr(source) as usize;
            let owner = owner as usize;
            let mut entries = ENTRIES.lock().unwrap_or_else(|error| error.into_inner());
            let index = entries
                .iter()
                .position(|entry| entry.key == key)
                .or_else(|| entries.iter().position(|entry| entry.key == 0));
            let tracked = if let Some(index) = index {
                entries[index].key = key;
                entries[index].owners[owner] += 1;
                true
            } else {
                UNTRACKED.fetch_add(1, Ordering::Relaxed);
                false
            };
            Self {
                key,
                owner,
                tracked,
            }
        }
    }

    impl Drop for Witness {
        fn drop(&mut self) {
            if !self.tracked {
                UNTRACKED.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            let mut entries = ENTRIES.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(entry) = entries.iter_mut().find(|entry| entry.key == self.key) {
                entry.owners[self.owner] -= 1;
                if entry.owners == [0; 3] {
                    entry.key = 0;
                }
            }
        }
    }

    pub(super) fn snapshot(source: &Arc<Mutex<ActivationSource>>) -> String {
        let key = Arc::as_ptr(source) as usize;
        let entries = ENTRIES.lock().unwrap_or_else(|error| error.into_inner());
        let owners = entries
            .iter()
            .find(|entry| entry.key == key)
            .map_or([0; 3], |entry| entry.owners);
        format!(
            "resume={}; worker={}; completion={}; registry_untracked={}",
            owners[0],
            owners[1],
            owners[2],
            UNTRACKED.load(Ordering::Relaxed)
        )
    }
}

#[derive(Clone)]
pub(in crate::main_window::shell::host) struct UnviewedRunningActivation {
    creation: bool,
    source: Arc<Mutex<ActivationSource>>,
    active: Arc<AtomicBool>,
    suspended: Arc<AtomicBool>,
    mount: Entity<Mount>,
    panel: Entity<SyndicTranscriptPanel>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    ThreadFence,
    ThreadSave,
    ThreadAcceptSave,
    ThreadPrepare,
    ThreadCommit,
    ThreadReconcile,
    ThreadAdopt,
    ThreadRelease,
    ThreadRestore,
    Begin,
    Install,
    PrepareFence,
    Fence,
    Flush,
    AcceptFlush,
    Commit,
    Reconcile,
    DisposePrior,
    AcceptDisposal,
    BeginFinal,
    Release,
    Complete,
    AcceptRelease,
    PrepareAutosave,
    Revalidate,
    Publish,
    Finalize,
    Retire,
    Detach,
    RestoreAutosave,
    Restore,
    Finished,
}

struct ActivationSource {
    stage: Stage,
    reader: PublishedRunningThreadsReader,
    service: Arc<Service>,
    lease: Arc<crate::window_acquisition::WindowSelectionLease>,
    owner: Option<RunningThreadActivation>,
    creation: Option<thread_creation::ThreadCreationSource>,
    outcome: Option<Outcome>,
    committed: Option<Commit>,
    home: Option<Arc<beryl_home_store::HomeServiceReference>>,
    state: Option<beryl_state::BerylState>,
    syndic: Option<syndic_storage::SyndicStorage>,
    prior: Selection,
    expected: Selection,
    target: beryl_state::WindowClaimSelection,
    receipt: Option<Receipt>,
    presentation: Option<Presentation>,
    pending_selection: Option<Selection>,
    advance: Option<ClaimAdvance>,
    flush: Option<FlushAdmission>,
    flush_settled: bool,
    widget_work: Option<WidgetWork>,
    release: Option<Release>,
    publication: Option<Publication>,
    completed_predecessor: Option<crate::main_window::MainWindowCompletedThreadPredecessorDisposal>,
    ordinary_save: Option<crate::main_window::MainWindowRetiredClaimPredecessorSave>,
    completed_successor: Option<crate::main_window::MainWindowCompletedThreadSuccessorCleanup>,
    completed_successor_progress:
        Option<crate::main_window::MainWindowCompletedThreadSuccessorProgress>,
    autosave: Option<Autosave>,
    settings: Option<(u64, ComposerHostAutosaveInterval)>,
    assets: beryl_state::AssetState,
    marker_seals: crate::composer_marker_seal::DraftMarkerSealService,
    provider: Option<TranscriptProviderReader>,
    transcript: Option<PreparedTranscriptAttachment>,
    request: TranscriptAttachmentRequest,
    transcript_cancel: AtomicBool,
    cancellation: beryl_home_store::CommandCancellation,
    error: Option<String>,
    installed: bool,
    retired: bool,
    gui_published: bool,
    terminal_release_failure: bool,
    transcript_retries: u8,
    source_retry_at: Option<std::time::Instant>,
    #[cfg(all(test, feature = "test-faults"))]
    before_commit: Option<FixtureHook>,
    #[cfg(all(test, feature = "test-faults"))]
    before_save: Option<FixtureHook>,
    #[cfg(all(test, feature = "test-faults"))]
    before_disposal: Option<FixtureHook>,
    #[cfg(all(test, feature = "test-faults"))]
    after_claim_outcome: Option<FixtureOutcomeHook>,
}

#[cfg(all(test, feature = "test-faults"))]
type FixtureHook = Box<dyn FnOnce(&beryl_home_store::CommandCancellation) + Send>;

#[cfg(all(test, feature = "test-faults"))]
type FixtureOutcomeHook = Box<dyn FnOnce(&Outcome) + Send>;

#[cfg(all(test, feature = "test-faults"))]
#[derive(Default)]
pub(in crate::main_window::shell::host) struct RunningActivationFixtureHooks {
    pub(super) commit: Option<FixtureHook>,
    pub(super) save: Option<FixtureHook>,
    pub(super) disposal: Option<FixtureHook>,
    pub(super) outcome: Option<FixtureOutcomeHook>,
}

impl UnviewedRunningActivation {
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fn committed_window_for_test(&self) -> Option<beryl_state::SessionWindowRecord> {
        self.source.try_lock().ok()?.committed_window().cloned()
    }
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fn diagnostics(&self) -> String {
        match self.source.try_lock() {
            Ok(source) => format!(
                "stage={:?} flush={:?} advance={:?} active={} retry={} error={:?} cancelled={} retired={} published={}",
                source.stage,
                source.flush,
                source
                    .advance
                    .as_ref()
                    .map(|advance| match advance.advance {
                        PublishAdvance::Progress(state) => format!("Progress({state:?})"),
                        PublishAdvance::ReconciliationPending => "ReconciliationPending".into(),
                        PublishAdvance::WidgetReleaseRequired(_) => "WidgetReleaseRequired".into(),
                        PublishAdvance::PriorFlushFailed => "PriorFlushFailed".into(),
                        PublishAdvance::Published(_) => "Published".into(),
                    }),
                self.active.load(Ordering::Acquire),
                source
                    .source_retry_at
                    .is_some_and(|at| std::time::Instant::now() < at),
                source.error,
                source.cancellation.is_cancelled(),
                source.retired,
                source.gui_published
            ),
            Err(_) => "source worker active".into(),
        }
    }
    pub(in crate::main_window::shell::host) fn suspend(&self) {
        self.suspended.store(true, Ordering::Release);
    }
    pub(in crate::main_window::shell::host) fn resume(&self) {
        self.suspended.store(false, Ordering::Release);
    }
    pub(in crate::main_window::shell::host) fn is_unavailable(&self) -> bool {
        self.suspended.load(Ordering::Acquire)
    }
    pub(in crate::main_window::shell::host) fn is_drained(&self) -> bool {
        !self.active.load(Ordering::Acquire)
            && self
                .source
                .try_lock()
                .is_ok_and(|source| source.stage == Stage::Finished)
    }
}

struct ActiveJob(Arc<AtomicBool>);
impl Drop for ActiveJob {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn resume_running_activation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(operation) = self.running_threads.activation_operation.clone() else {
            return;
        };
        #[cfg(test)]
        let _resume_owner =
            owner_diagnostics::Witness::new(&operation.source, owner_diagnostics::Owner::Resume);
        if operation.active.load(Ordering::Acquire) {
            return;
        }
        let Ok(mut source) = operation.source.try_lock() else {
            return;
        };
        if operation.suspended.load(Ordering::Acquire) {
            let error = source.error.clone().unwrap_or_else(|| {
                "Thread activation is unavailable; admitted custody is retained".into()
            });
            drop(source);
            self.retain_running_activation_failure(error, window, cx);
            return;
        }
        if matches!(source.stage, Stage::Begin | Stage::ThreadFence)
            && self.running_threads.workers.retained() != 0
        {
            return;
        }
        if !source.reader.current()
            || !self
                .running_threads
                .reader
                .as_ref()
                .is_some_and(|reader| reader.same_publication(&source.reader))
        {
            operation.suspend();
            drop(source);
            self.retain_running_activation_failure("Running threads source retired; activation custody is retained and recovery is unavailable".into(), window, cx);
            return;
        }
        if let Some(error) = source.error.clone() {
            self.retain_running_activation_failure(error, window, cx);
        }
        if source.transcript_retries >= 3 {
            source.transcript_retries = 0;
            source.source_retry_at = Some(std::time::Instant::now() + Duration::from_millis(500));
            drop(source);
            self.schedule_running_activation_wake(
                operation,
                Duration::from_millis(500),
                window,
                cx,
            );
            return;
        }
        if matches!(source.stage, Stage::Revalidate | Stage::Publish)
            && source
                .source_retry_at
                .is_some_and(|at| std::time::Instant::now() < at)
        {
            let delay = source
                .source_retry_at
                .unwrap()
                .saturating_duration_since(std::time::Instant::now());
            drop(source);
            self.schedule_running_activation_wake(operation, delay, window, cx);
            return;
        }
        let result = self.accept_running_activation_stage(&operation, &mut source, window, cx);
        if let Err(error) = result {
            source.fail(error.clone());
            if source.terminal_release_failure {
                operation.suspend();
            }
            self.retain_running_activation_failure(error, window, cx);
        }
        if source.stage == Stage::Finished {
            let result = source.error.take().map_or(Ok(()), Err);
            let success = result.is_ok();
            let reader = source.reader.clone();
            let creation = source.creation.is_some();
            drop(source);
            self.running_threads.activation_operation = None;
            if success {
                if !creation && !self.running_threads.ordinary_activation {
                    self.acknowledge_running_activation(&reader);
                    self.dismiss_running_picker(window, cx);
                }
                window.activate_window();
            }
            self.finish_running_activation(result, window, cx);
            if creation {
                self.finish_thread_confirmation(success, window, cx);
            }
            self.resume_running_thread_reads(window, cx);
            return;
        }
        if !source.source_stage() {
            drop(source);
            self.schedule_running_activation_wake(
                operation,
                Duration::from_millis(100),
                window,
                cx,
            );
            return;
        }
        if source
            .source_retry_at
            .is_some_and(|at| std::time::Instant::now() < at)
        {
            let delay = source
                .source_retry_at
                .unwrap()
                .saturating_duration_since(std::time::Instant::now());
            drop(source);
            self.schedule_running_activation_wake(operation, delay, window, cx);
            return;
        }
        #[cfg(all(test, feature = "test-faults"))]
        let fixture_real_reconciliation = self.running_threads.fixture_real_creation_reconciliation
            && operation.creation
            && source.stage == Stage::ThreadReconcile;
        drop(source);
        if operation.active.swap(true, Ordering::AcqRel) {
            return;
        }
        let worker_source = operation.source.clone();
        #[cfg(test)]
        let worker_owner =
            owner_diagnostics::Witness::new(&worker_source, owner_diagnostics::Owner::Worker);
        let active = ActiveJob(operation.active.clone());
        let suspended = operation.suspended.clone();
        let work = self.running_threads.workers.track(move || {
            #[cfg(test)]
            let _worker_owner = worker_owner;
            let _active = active;
            if suspended.load(Ordering::Acquire) {
                return;
            }
            if let Ok(mut source) = worker_source.lock() {
                if let Err(error) = source.run_source() {
                    if !source.reader.current()
                        || source.terminal_release_failure
                        || source.home.as_ref().is_some_and(|home| {
                            home.health().state() != beryl_home_store::HomeHealthState::Healthy
                        })
                    {
                        suspended.store(true, Ordering::Release);
                        source.error = Some(error);
                    } else {
                        source.fail(error);
                    }
                }
                let retry = source.source_stage()
                    || (source.stage == Stage::AcceptRelease && source.publication.is_none())
                    || (source.stage == Stage::AcceptFlush
                        && source.advance.as_ref().is_some_and(|advance| {
                            matches!(
                                advance.advance,
                                PublishAdvance::ReconciliationPending | PublishAdvance::Progress(_)
                            )
                        }));
                source.source_retry_at =
                    retry.then(|| std::time::Instant::now() + Duration::from_millis(100));
            } else {
                suspended.store(true, Ordering::Release);
            }
        });
        #[cfg(all(test, feature = "test-faults"))]
        let job = if fixture_real_reconciliation {
            self.running_threads.fixture_real_creation_reconciliation = false;
            let (returned, completion) = futures_channel::oneshot::channel();
            let worker = std::thread::spawn(move || {
                work.run();
                let _ = returned.send(());
            });
            cx.background_executor().spawn(async move {
                let _ = completion.await;
                let _ = worker.join();
            })
        } else {
            cx.background_executor().spawn(async move { work.run() })
        };
        #[cfg(not(all(test, feature = "test-faults")))]
        let job = cx.background_executor().spawn(async move { work.run() });
        #[cfg(test)]
        let completion_owner = owner_diagnostics::Witness::new(
            &operation.source,
            owner_diagnostics::Owner::Completion,
        );
        self.running_threads.activation_wake.take();
        self.running_threads.activation_task = Some(cx.spawn_in(window, async move |this, cx| {
            #[cfg(test)]
            let _completion_owner = completion_owner;
            job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.running_threads.activation_task = None;
                if root
                    .running_threads
                    .activation_operation
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(&current.source, &operation.source))
                {
                    root.resume_running_activation(window, cx);
                }
            });
        }));
    }

    fn schedule_running_activation_wake(
        &mut self,
        operation: UnviewedRunningActivation,
        delay: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running_threads.activation_task.is_some() {
            return;
        }
        let suspended = operation.suspended.clone();
        drop(operation);
        let wake = Arc::new(());
        self.running_threads.activation_wake = Some(wake.clone());
        self.running_threads.activation_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |root, window, cx| {
                if !root
                    .running_threads
                    .activation_wake
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &wake))
                {
                    return;
                }
                root.running_threads.activation_wake = None;
                root.running_threads.activation_task = None;
                if suspended.load(Ordering::Acquire)
                    || !root
                        .running_threads
                        .activation_operation
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(&current.suspended, &suspended))
                {
                    return;
                }
                root.resume_running_activation(window, cx);
            });
        }));
    }
}
