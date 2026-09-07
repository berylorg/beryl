use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(feature = "test-faults")]
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use beryl_home_store::{CommandCancellation, HomeStore};
use gpui::BackgroundExecutor;
use gpui_text_input::{
    RangePrepublicationValidationRequest, RangePrepublicationValidationResponse,
    RangeTextInputRequest,
};

#[cfg(feature = "test-faults")]
use super::prepublication::MainWindowNativeLineagePrepublicationDiagnostics;
use super::prepublication::MainWindowNativeLineagePrepublicationSource;

use super::{MainWindowComposerSelectionIdentity, MainWindowComposerWidgetRelease};
use crate::main_window::MainWindowComposerSlot;

mod close;
mod close_cleanup;
mod native_disposal;

pub(in crate::main_window) enum MainWindowNativeLineageSourceRetentionError {
    CapacityFull { epoch: u64 },
    Failed(String),
}

#[cfg(feature = "test-faults")]
pub enum MainWindowSelectedComposerPreparationTestFault {
    HostUnavailable,
    ResponseKind,
    ResponseBinding(crate::composer_host::ComposerHostBinding),
}

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct MainWindowNativeLineageCleanupTestWitness(
    Arc<Mutex<MainWindowNativeLineageCleanupTestWitnessState>>,
);

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MainWindowNativeLineageCleanupTestWitnessSnapshot {
    pub diagnostics: MainWindowNativeLineagePrepublicationDiagnostics,
    pub driver_alive: bool,
    pub capacity_epoch: u64,
}

#[cfg(feature = "test-faults")]
#[derive(Default)]
struct MainWindowNativeLineageCleanupTestWitnessState {
    snapshot: MainWindowNativeLineageCleanupTestWitnessSnapshot,
}

#[cfg(feature = "test-faults")]
impl MainWindowNativeLineageCleanupTestWitness {
    pub fn snapshot(&self) -> MainWindowNativeLineageCleanupTestWitnessSnapshot {
        self.0.lock().unwrap().snapshot
    }
}

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct MainWindowComposerCutPreparationTestRelease(Arc<Mutex<CutPreparationTestGateState>>);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct MainWindowComposerPendingCompletionTestRelease(
    Arc<Mutex<PendingCompletionTestGateState>>,
);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct MainWindowComposerPendingDispatchTestRelease(Arc<Mutex<PendingCompletionTestGateState>>);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub(super) struct CutPreparationTestGate(Arc<Mutex<CutPreparationTestGateState>>);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub(in crate::main_window) struct PendingCompletionTestGate(
    Arc<Mutex<PendingCompletionTestGateState>>,
);

#[cfg(feature = "test-faults")]
struct CutPreparationTestGateState {
    released: bool,
    waker: Option<Waker>,
}

#[cfg(feature = "test-faults")]
fn install_pending_test_gate(
    slot: &Mutex<Option<PendingCompletionTestGate>>,
) -> Option<MainWindowComposerPendingCompletionTestRelease> {
    let state = Arc::new(Mutex::new(PendingCompletionTestGateState {
        entered: false,
        released: false,
        waker: None,
    }));
    let mut slot = slot.lock().ok()?;
    if slot.is_some() {
        return None;
    }
    *slot = Some(PendingCompletionTestGate(state.clone()));
    Some(MainWindowComposerPendingCompletionTestRelease(state))
}

#[cfg(feature = "test-faults")]
struct PendingCompletionTestGateState {
    entered: bool,
    released: bool,
    waker: Option<Waker>,
}

#[cfg(feature = "test-faults")]
impl MainWindowComposerCutPreparationTestRelease {
    pub fn release(self) {
        let mut state = self.0.lock().unwrap();
        state.released = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

#[cfg(feature = "test-faults")]
impl MainWindowComposerPendingCompletionTestRelease {
    pub fn is_blocked(&self) -> bool {
        self.0.lock().unwrap().entered
    }

    pub fn release(self) {
        let mut state = self.0.lock().unwrap();
        state.released = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

#[cfg(feature = "test-faults")]
impl MainWindowComposerPendingDispatchTestRelease {
    pub fn is_blocked(&self) -> bool {
        self.0.lock().unwrap().entered
    }

    pub fn release(self) {
        let mut state = self.0.lock().unwrap();
        state.released = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

#[cfg(feature = "test-faults")]
impl Future for CutPreparationTestGate {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0.lock().unwrap();
        if state.released {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[cfg(feature = "test-faults")]
impl Future for PendingCompletionTestGate {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0.lock().unwrap();
        state.entered = true;
        if state.released {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub struct MainWindowConversationComposerService {
    pub(super) store: Arc<HomeStore>,
    pub(super) slot: Mutex<MainWindowComposerSlot>,
    window_close: Mutex<Option<crate::main_window::MainWindowConversationComposerCloseTicket>>,
    native_lineage_sources: Mutex<Vec<Arc<MainWindowNativeLineagePrepublicationSource>>>,
    native_lineage_driver_started: AtomicBool,
    native_lineage_capacity_epoch: AtomicU64,
    #[cfg(feature = "test-faults")]
    native_lineage_cleanup_witness: Arc<Mutex<MainWindowNativeLineageCleanupTestWitnessState>>,
    #[cfg(feature = "test-faults")]
    test_cancel_next_mutation_commit: AtomicBool,
    #[cfg(feature = "test-faults")]
    test_cut_preparation_gate: Mutex<Option<CutPreparationTestGate>>,
    #[cfg(feature = "test-faults")]
    test_pending_completion_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_pending_dispatch_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_selected_dispatch_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_selected_page_dispatch_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_native_lineage_seed_validation_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_native_lineage_validation_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_native_lineage_page_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_native_lineage_object_page_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_fail_next_native_lineage_validation: AtomicBool,
    #[cfg(feature = "test-faults")]
    test_fail_next_native_lineage_page: AtomicBool,
    #[cfg(feature = "test-faults")]
    test_fail_next_native_lineage_disposal_begin: AtomicBool,
    #[cfg(feature = "test-faults")]
    test_fail_next_native_lineage_disposal_advance: AtomicBool,
    #[cfg(feature = "test-faults")]
    test_append_impossible_pending_initial_response: AtomicBool,
    #[cfg(feature = "test-faults")]
    test_selected_preparation_fault: Mutex<Option<MainWindowSelectedComposerPreparationTestFault>>,
}

impl MainWindowConversationComposerService {
    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn test_native_lineage_disposal_diagnostics(
        &self,
    ) -> crate::main_window::MainWindowNativeLineageDisposalDiagnostics {
        let Ok(slot) = self.slot.lock() else {
            return crate::main_window::MainWindowNativeLineageDisposalDiagnostics {
                mount_release_present: false,
                mount_contribution_present: false,
                mount_subscription_present: false,
                mount_flush_ticket_present: false,
                mount_flush_capture: None,
                mount_last_disposal_advance: None,
                marker_current_flights: 0,
                marker_driving_flights: 0,
                marker_terminalizing_flights: 0,
                slot_selected: false,
                slot_pending: false,
                slot_disposed: false,
                slot_suspended: false,
                slot_disposal_flushing: false,
                slot_awaiting_widget_release: false,
                host_pending_requests: 0,
                host_settlement_custody: 0,
                host_timers: 0,
                host_barriers: 0,
                host_joined_publications: 0,
                host_publication_ready: false,
            };
        };
        let (selected, pending, disposed, suspended, flushing, awaiting) =
            slot.test_native_lineage_disposal_state();
        let host = slot.selected_host();
        let lifecycle = host.map(|host| host.lifecycle_diagnostics());
        crate::main_window::MainWindowNativeLineageDisposalDiagnostics {
            mount_release_present: false,
            mount_contribution_present: false,
            mount_subscription_present: false,
            mount_flush_ticket_present: false,
            mount_flush_capture: None,
            mount_last_disposal_advance: None,
            marker_current_flights: 0,
            marker_driving_flights: 0,
            marker_terminalizing_flights: 0,
            slot_selected: selected,
            slot_pending: pending,
            slot_disposed: disposed,
            slot_suspended: suspended,
            slot_disposal_flushing: flushing,
            slot_awaiting_widget_release: awaiting,
            host_pending_requests: host.map_or(0, |host| host.pending_request_count()),
            host_settlement_custody: host.map_or(0, |host| host.settlement_custody_in_use()),
            host_timers: lifecycle.map_or(0, |diagnostics| diagnostics.timers()),
            host_barriers: lifecycle.map_or(0, |diagnostics| diagnostics.barriers()),
            host_joined_publications: lifecycle
                .map_or(0, |diagnostics| diagnostics.joined_publications()),
            host_publication_ready: lifecycle
                .is_some_and(|diagnostics| diagnostics.publication_ready()),
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_submission_diagnostics(
        &self,
    ) -> Result<crate::main_window::MainWindowComposerSlotSubmissionTestDiagnostics, String> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .test_submission_diagnostics())
    }

    pub fn new(store: Arc<HomeStore>, slot: MainWindowComposerSlot) -> Self {
        Self {
            store,
            slot: Mutex::new(slot),
            window_close: Mutex::new(None),
            native_lineage_sources: Mutex::new(Vec::with_capacity(2)),
            native_lineage_driver_started: AtomicBool::new(false),
            native_lineage_capacity_epoch: AtomicU64::new(0),
            #[cfg(feature = "test-faults")]
            native_lineage_cleanup_witness: Arc::new(Mutex::new(
                MainWindowNativeLineageCleanupTestWitnessState::default(),
            )),
            #[cfg(feature = "test-faults")]
            test_cancel_next_mutation_commit: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            test_cut_preparation_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_pending_completion_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_pending_dispatch_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_selected_dispatch_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_selected_page_dispatch_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_native_lineage_seed_validation_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_native_lineage_validation_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_native_lineage_page_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_native_lineage_object_page_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_fail_next_native_lineage_validation: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            test_fail_next_native_lineage_page: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            test_fail_next_native_lineage_disposal_begin: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            test_fail_next_native_lineage_disposal_advance: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            test_append_impossible_pending_initial_response: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            test_selected_preparation_fault: Mutex::new(None),
        }
    }

    pub fn selected_identity(&self) -> Option<MainWindowComposerSelectionIdentity> {
        self.slot.lock().ok()?.selected_identity()
    }

    pub(in crate::main_window) fn retain_native_lineage_source(
        self: &Arc<Self>,
        source: Arc<MainWindowNativeLineagePrepublicationSource>,
        executor: BackgroundExecutor,
    ) -> Result<(), MainWindowNativeLineageSourceRetentionError> {
        let mut sources = self.native_lineage_sources.lock().map_err(|_| {
            MainWindowNativeLineageSourceRetentionError::Failed(
                "conversation composer prepublication source lock failed".to_owned(),
            )
        })?;
        let previous_len = sources.len();
        sources.retain(|source| !source.drained());
        if sources.len() != previous_len {
            self.native_lineage_capacity_epoch
                .fetch_add(1, Ordering::AcqRel);
        }
        if sources.iter().any(|current| Arc::ptr_eq(current, &source)) {
            return Ok(());
        }
        if sources.len() == sources.capacity() {
            return Err(MainWindowNativeLineageSourceRetentionError::CapacityFull {
                epoch: self.native_lineage_capacity_epoch.load(Ordering::Acquire),
            });
        }
        sources.push(source);
        drop(sources);
        self.update_native_lineage_cleanup_witness(true);
        self.start_native_lineage_cleanup_driver(executor);
        Ok(())
    }

    pub(in crate::main_window) fn retire_native_lineage_sources(&self) {
        if let Ok(mut sources) = self.native_lineage_sources.lock() {
            let previous_len = sources.len();
            sources.retain(|source| !source.drained());
            if sources.len() != previous_len {
                self.native_lineage_capacity_epoch
                    .fetch_add(1, Ordering::AcqRel);
            }
        }
        self.update_native_lineage_cleanup_witness(
            self.native_lineage_driver_started.load(Ordering::Acquire),
        );
    }

    fn start_native_lineage_cleanup_driver(self: &Arc<Self>, executor: BackgroundExecutor) {
        if self
            .native_lineage_driver_started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let service = self.clone();
        let timer_executor = executor.clone();
        executor
            .spawn(async move {
                loop {
                    timer_executor.timer(Duration::from_millis(50)).await;
                    service.drive_native_lineage_cleanup_sources();
                    let empty = service
                        .native_lineage_sources
                        .lock()
                        .map(|sources| sources.is_empty())
                        .unwrap_or(false);
                    if empty {
                        service
                            .native_lineage_driver_started
                            .store(false, Ordering::Release);
                        service.update_native_lineage_cleanup_witness(false);
                        let still_empty = service
                            .native_lineage_sources
                            .lock()
                            .map(|sources| sources.is_empty())
                            .unwrap_or(false);
                        if still_empty
                            || service
                                .native_lineage_driver_started
                                .swap(true, Ordering::AcqRel)
                        {
                            return;
                        }
                        service.update_native_lineage_cleanup_witness(true);
                    }
                }
            })
            .detach();
    }

    pub(in crate::main_window) fn drive_native_lineage_cleanup_sources(&self) {
        let sources = self
            .native_lineage_sources
            .lock()
            .map(|sources| sources.clone())
            .unwrap_or_default();
        for source in sources {
            source.drive_cleanup(8);
        }
        self.retire_native_lineage_sources();
    }

    pub(in crate::main_window) fn native_lineage_capacity_epoch(&self) -> u64 {
        self.native_lineage_capacity_epoch.load(Ordering::Acquire)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_cleanup_witness(&self) -> MainWindowNativeLineageCleanupTestWitness {
        self.update_native_lineage_cleanup_witness(
            self.native_lineage_driver_started.load(Ordering::Acquire),
        );
        MainWindowNativeLineageCleanupTestWitness(self.native_lineage_cleanup_witness.clone())
    }

    #[cfg(feature = "test-faults")]
    fn update_native_lineage_cleanup_witness(&self, driver_alive: bool) {
        let diagnostics = self.test_native_lineage_cleanup_diagnostics();
        if let Ok(mut witness) = self.native_lineage_cleanup_witness.lock() {
            witness.snapshot = MainWindowNativeLineageCleanupTestWitnessSnapshot {
                diagnostics,
                driver_alive,
                capacity_epoch: self.native_lineage_capacity_epoch(),
            };
        }
    }

    #[cfg(not(feature = "test-faults"))]
    fn update_native_lineage_cleanup_witness(&self, _driver_alive: bool) {}

    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_cleanup_diagnostics(
        &self,
    ) -> MainWindowNativeLineagePrepublicationDiagnostics {
        let sources = self
            .native_lineage_sources
            .lock()
            .map(|sources| sources.clone())
            .unwrap_or_default();
        let mut total = MainWindowNativeLineagePrepublicationDiagnostics::default();
        for source in sources {
            let diagnostics = source.diagnostics();
            total.sources += diagnostics.sources;
            total.owner_active_sources += diagnostics.owner_active_sources;
            total.validation_flights += diagnostics.validation_flights;
            total.page_flights += diagnostics.page_flights;
            total.object_page_flights += diagnostics.object_page_flights;
            total.pending_flights += diagnostics.pending_flights;
            total.terminal_flights += diagnostics.terminal_flights;
            total.delivered_flights += diagnostics.delivered_flights;
            total.cleanup_active += diagnostics.cleanup_active;
            total.cleanup_ready += diagnostics.cleanup_ready;
            total.cleanup_awaiting_acknowledgement += diagnostics.cleanup_awaiting_acknowledgement;
        }
        total
    }

    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_suspension_active(&self) -> bool {
        let slot = self.slot.lock().unwrap();
        slot.selected_identity()
            .is_some_and(|selection| slot.native_lineage_prepublication_active(selection))
    }

    #[cfg(feature = "test-faults")]
    pub fn test_gate_next_native_lineage_seed_validation(
        &self,
    ) -> Option<MainWindowComposerPendingCompletionTestRelease> {
        install_pending_test_gate(&self.test_native_lineage_seed_validation_gate)
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn take_test_native_lineage_seed_validation_gate(
        &self,
    ) -> Option<PendingCompletionTestGate> {
        self.test_native_lineage_seed_validation_gate
            .lock()
            .ok()?
            .take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_gate_next_native_lineage_validation(
        &self,
    ) -> Option<MainWindowComposerPendingCompletionTestRelease> {
        install_pending_test_gate(&self.test_native_lineage_validation_gate)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_gate_next_native_lineage_page(
        &self,
    ) -> Option<MainWindowComposerPendingCompletionTestRelease> {
        install_pending_test_gate(&self.test_native_lineage_page_gate)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_gate_next_native_lineage_object_page(
        &self,
    ) -> Option<MainWindowComposerPendingCompletionTestRelease> {
        install_pending_test_gate(&self.test_native_lineage_object_page_gate)
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn take_test_native_lineage_validation_gate(
        &self,
    ) -> Option<PendingCompletionTestGate> {
        self.test_native_lineage_validation_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn take_test_native_lineage_page_gate(
        &self,
    ) -> Option<PendingCompletionTestGate> {
        self.test_native_lineage_page_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn take_test_native_lineage_object_page_gate(
        &self,
    ) -> Option<PendingCompletionTestGate> {
        self.test_native_lineage_object_page_gate
            .lock()
            .ok()?
            .take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_fail_next_native_lineage_validation(&self) {
        self.test_fail_next_native_lineage_validation
            .store(true, Ordering::Release);
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn take_test_native_lineage_validation_failure(&self) -> bool {
        self.test_fail_next_native_lineage_validation
            .swap(false, Ordering::AcqRel)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_fail_next_native_lineage_page(&self) {
        self.test_fail_next_native_lineage_page
            .store(true, Ordering::Release);
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn take_test_native_lineage_page_failure(&self) -> bool {
        self.test_fail_next_native_lineage_page
            .swap(false, Ordering::AcqRel)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_fail_next_native_lineage_disposal_begin(&self) {
        self.test_fail_next_native_lineage_disposal_begin
            .store(true, Ordering::Release);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_fail_next_native_lineage_disposal_advance(&self) {
        self.test_fail_next_native_lineage_disposal_advance
            .store(true, Ordering::Release);
    }

    pub(in crate::main_window) fn begin_native_lineage_suspension(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        seed: gpui_text_input::RangeRestorationSeed,
    ) -> Result<(), String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_native_lineage_suspension(selection, seed)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn validate_native_lineage_restoration(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        seed: gpui_text_input::RangeRestorationSeed,
    ) -> Result<(), String> {
        let (storage, restoration) = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .prepare_native_lineage_validation(selection, seed)
            .map_err(|error| error.to_string())?;
        storage
            .validate_draft_piece_restoration(&self.store, restoration)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn validate_native_lineage_prepublication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        seed: gpui_text_input::RangeRestorationSeed,
        request: RangePrepublicationValidationRequest,
    ) -> Result<RangePrepublicationValidationResponse, String> {
        if request.binding != seed.binding || request.history != seed.history {
            return Err("composer prepublication validation request is stale".to_owned());
        }
        self.validate_native_lineage_restoration(selection, seed)?;
        let current = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_identity();
        Ok(RangePrepublicationValidationResponse {
            key: request.key,
            binding: request.binding,
            history: request.history,
            current: current == Some(selection),
        })
    }

    pub(in crate::main_window) fn dispatch_native_lineage_prepublication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        request: RangeTextInputRequest,
    ) -> Result<crate::main_window::MainWindowComposerDispatchOutcome, String> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if !slot.native_lineage_prepublication_active(selection) {
            return Err("composer prepublication route is stale".to_owned());
        }
        slot.dispatch_selected_request(
            &self.store,
            selection,
            request,
            Vec::new().into_boxed_slice(),
            &CommandCancellation::new(),
        )
        .map_err(|error| format!("composer prepublication dispatch failed: {error}"))
    }

    pub(in crate::main_window) fn attest_native_lineage_prepublication_current(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        current: gpui_text_input::RangePrepublicationCurrent,
    ) -> Result<gpui_text_input::RangePrepublicationCurrent, String> {
        let slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if !slot.native_lineage_prepublication_active(selection)
            || current.binding != selection.binding().range_binding()
            || current.history != Some(selection.binding().range_history_frontier())
        {
            return Err("composer prepublication adoption authority is stale".to_owned());
        }
        Ok(current)
    }

    pub(in crate::main_window) fn cancel_native_lineage_suspension(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .cancel_native_lineage_suspension(selection)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn complete_native_lineage_restoration(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_native_lineage_restoration(selection)
            .map_err(|error| error.to_string())
    }

    pub fn pending_identity(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Option<MainWindowComposerSelectionIdentity> {
        self.slot.lock().ok()?.pending_identity(receipt)
    }

    pub(in crate::main_window) fn pending_request_is_admitted(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        selection: MainWindowComposerSelectionIdentity,
    ) -> bool {
        self.slot
            .lock()
            .is_ok_and(|slot| slot.pending_request_is_admitted(receipt, selection))
    }

    pub fn pending_receipt(&self) -> Option<super::MainWindowComposerActivationReceipt> {
        self.slot.lock().ok()?.pending_receipt()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_host_request_id(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Option<u64> {
        self.slot.lock().ok()?.test_pending_host_request_id(receipt)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_activation_after_open_fault(
        &self,
        fault: impl FnOnce(&HomeStore, syndic_storage::SyndicStorage) + Send + 'static,
    ) {
        self.slot
            .lock()
            .unwrap()
            .test_arm_activation_after_open_fault(fault);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_append_impossible_pending_initial_response(&self) {
        assert!(
            !self
                .test_append_impossible_pending_initial_response
                .swap(true, Ordering::SeqCst)
        );
    }

    #[cfg(feature = "test-faults")]
    pub fn test_cancel_next_mutation_commit(&self) {
        assert!(
            !self
                .test_cancel_next_mutation_commit
                .swap(true, Ordering::SeqCst)
        );
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_cut_preparation(&self) -> MainWindowComposerCutPreparationTestRelease {
        let state = Arc::new(Mutex::new(CutPreparationTestGateState {
            released: false,
            waker: None,
        }));
        let mut gate = self.test_cut_preparation_gate.lock().unwrap();
        assert!(
            gate.replace(CutPreparationTestGate(state.clone()))
                .is_none()
        );
        MainWindowComposerCutPreparationTestRelease(state)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_cut_preparation_gate(&self) -> Option<CutPreparationTestGate> {
        self.test_cut_preparation_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_mutation_commit_cancellation(&self) -> bool {
        self.test_cancel_next_mutation_commit
            .swap(false, Ordering::SeqCst)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_pending_completion(
        &self,
    ) -> MainWindowComposerPendingCompletionTestRelease {
        let state = Arc::new(Mutex::new(PendingCompletionTestGateState {
            entered: false,
            released: false,
            waker: None,
        }));
        let mut gate = self.test_pending_completion_gate.lock().unwrap();
        assert!(
            gate.replace(PendingCompletionTestGate(state.clone()))
                .is_none()
        );
        MainWindowComposerPendingCompletionTestRelease(state)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_pending_completion_gate(&self) -> Option<PendingCompletionTestGate> {
        self.test_pending_completion_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_pending_dispatch(&self) -> MainWindowComposerPendingDispatchTestRelease {
        let state = Arc::new(Mutex::new(PendingCompletionTestGateState {
            entered: false,
            released: false,
            waker: None,
        }));
        let mut gate = self.test_pending_dispatch_gate.lock().unwrap();
        assert!(
            gate.replace(PendingCompletionTestGate(state.clone()))
                .is_none()
        );
        MainWindowComposerPendingDispatchTestRelease(state)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_pending_dispatch_gate(&self) -> Option<PendingCompletionTestGate> {
        self.test_pending_dispatch_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_selected_dispatch(
        &self,
    ) -> MainWindowComposerPendingDispatchTestRelease {
        let release = install_pending_test_gate(&self.test_selected_dispatch_gate)
            .expect("selected dispatch gate is already installed");
        MainWindowComposerPendingDispatchTestRelease(release.0)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_selected_dispatch_gate(&self) -> Option<PendingCompletionTestGate> {
        self.test_selected_dispatch_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_selected_page_dispatch(
        &self,
    ) -> MainWindowComposerPendingDispatchTestRelease {
        let release = install_pending_test_gate(&self.test_selected_page_dispatch_gate)
            .expect("selected page dispatch gate is already installed");
        MainWindowComposerPendingDispatchTestRelease(release.0)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_selected_page_dispatch_gate(
        &self,
    ) -> Option<PendingCompletionTestGate> {
        self.test_selected_page_dispatch_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_advance_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerPublishAdvance, String> {
        self.advance_publish(receipt)
    }

    pub(in crate::main_window) fn assets(&self) -> Result<beryl_state::AssetState, String> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .assets())
    }

    pub(in crate::main_window) fn begin_submission(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        request: crate::composer_host::ComposerHostSubmissionRequest,
    ) -> Result<crate::composer_host::ComposerHostSubmissionTicket, String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_selected_submission(selection, request)
            .map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn advance_submission(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: crate::composer_host::ComposerHostSubmissionTicket,
        assets: beryl_state::AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        publication_operation_id: syndic_storage::DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        successor_request: &crate::composer_host::ComposerHostActivationRequest,
        successor_retirement_operation_id: syndic_storage::DraftPieceOperationIdV1,
        expected_next_draft: beryl_model::SyndicDraftId,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::main_window::MainWindowComposerSubmissionAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_selected_submission(
                &self.store,
                selection,
                ticket,
                assets,
                marker_seals,
                publication_operation_id,
                marker_authority,
                published_at,
                successor_request,
                successor_retirement_operation_id,
                expected_next_draft,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn complete_submission_successor_after_widget_release(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_submission_successor_after_widget_release(receipt, release)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn selected_autosave_timer(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<crate::composer_host::ComposerHostAutosaveTimer>, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_timer(selection)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn selected_autosave_publication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<crate::composer_host::ComposerHostPublicationTicket>, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_publication(selection)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn autosave_capture_requirement(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<super::MainWindowComposerAutosaveCaptureRequirement, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_capture_requirement(selection)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn publish_autosave_interval(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        settings_generation: u64,
        interval: crate::composer_host::ComposerHostAutosaveInterval,
    ) -> Result<crate::composer_host::ComposerHostAutosaveSettingsCompletion, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .publish_selected_autosave_interval(selection, settings_generation, interval)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn fire_autosave(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        timer: crate::composer_host::ComposerHostAutosaveTimer,
        assets: beryl_state::AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        operation_id: syndic_storage::DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostAutosaveCapture, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .fire_selected_autosave(
                &self.store,
                selection,
                timer,
                assets,
                marker_seals,
                operation_id,
                marker_authority,
                published_at,
                cancellation,
            )
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn advance_autosave(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: crate::composer_host::ComposerHostPublicationTicket,
    ) -> Result<crate::composer_host::ComposerHostAutosaveAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_selected_autosave(&self.store, selection, ticket)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn autosave_publication_ready(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: crate::composer_host::ComposerHostPublicationTicket,
    ) -> Result<bool, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_publication_ready(selection, ticket)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn begin_activation(
        &self,
        claim: beryl_state::WindowClaimSelection,
        request: crate::composer_host::ComposerHostActivationRequest,
        retirement_operation_id: syndic_storage::DraftPieceOperationIdV1,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<super::MainWindowComposerActivationAdvance, String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_activation(
                &self.store,
                claim,
                request,
                retirement_operation_id,
                cancellation,
            )
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn retire_pending(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerRetirementAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .retire_pending(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn release_failed_pending(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerRetirementAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .release_failed_pending(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn begin_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_publish(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn publish_preflight(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .publish_preflight(&self.store, receipt)
            .map_err(|_| "conversation composer publication preflight is stale".to_owned())
    }

    pub(in crate::main_window) fn advance_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerPublishAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_publish(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn capture_flush_publication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: crate::composer_host::ComposerHostFlushTicket,
        assets: beryl_state::AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        operation_id: syndic_storage::DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostFlushCapture, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .capture_activation_flush_publication(
                &self.store,
                selection,
                flush,
                assets,
                marker_seals,
                operation_id,
                marker_authority,
                published_at,
                cancellation,
            )
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn capture_flush_disposal(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: crate::composer_host::ComposerHostFlushTicket,
        operation_id: syndic_storage::DraftPieceOperationIdV1,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostFlushCapture, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .capture_selected_flush_disposal(
                &self.store,
                selection,
                flush,
                operation_id,
                cancellation,
            )
            .map_err(|error| {
                format!("conversation composer flush disposal capture failed: {error}")
            })
    }

    pub(in crate::main_window) fn complete_publish_after_widget_release(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<super::MainWindowComposerPublishAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_publish_after_widget_release(&self.store, receipt, release)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn begin_final_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_final_publish(&self.store, receipt, expected)
            .map_err(|_| "conversation composer final publication fence is stale".to_owned())
    }

    pub(in crate::main_window) fn begin_disposal(
        &self,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.ensure_no_window_close()?;
        #[cfg(feature = "test-faults")]
        if self
            .test_fail_next_native_lineage_disposal_begin
            .swap(false, Ordering::AcqRel)
        {
            return Err("conversation composer disposal admission failed for test".to_owned());
        }
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_disposal(&self.store)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn disposal_preflight(
        &self,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if let Some(receipt) = slot.pending_receipt() {
            match slot
                .retire_pending(&self.store, receipt)
                .map_err(|_| "conversation composer service operation failed".to_owned())?
            {
                super::MainWindowComposerRetirementAdvance::Retired => {}
                super::MainWindowComposerRetirementAdvance::Pending => {
                    return Err("composer disposal is waiting for pending retirement".to_owned());
                }
                super::MainWindowComposerRetirementAdvance::DepartedFreshBoundary => {
                    return Err("composer disposal pending target departed fresh state".to_owned());
                }
            }
        }
        slot.selected_identity()
            .ok_or_else(|| "conversation composer disposal has no selected slot".to_owned())
    }

    pub(in crate::main_window) fn advance_disposal(
        &self,
    ) -> Result<super::MainWindowComposerDisposalAdvance, String> {
        #[cfg(feature = "test-faults")]
        if self
            .test_fail_next_native_lineage_disposal_advance
            .swap(false, Ordering::AcqRel)
        {
            return Err("conversation composer disposal advance failed for test".to_owned());
        }
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_disposal(&self.store)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn complete_disposal_after_widget_release(
        &self,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<super::MainWindowComposerDisposalAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_disposal_after_widget_release(&self.store, release)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(super) fn take_initial_presentation(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Box<[crate::composer_host::ComposerHostResponse]>, String> {
        #[cfg(feature = "test-faults")]
        let fault = self.test_selected_preparation_fault.lock().unwrap().take();
        #[cfg(feature = "test-faults")]
        if matches!(
            fault,
            Some(MainWindowSelectedComposerPreparationTestFault::HostUnavailable)
        ) {
            return Err("selected composer host preparation unavailable".to_owned());
        }
        let responses = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .take_selected_initial_presentation(selection)
            .map(|presentation| presentation.into_responses())
            .map_err(|error| format!("selected composer preparation failed: {error:?}"))?;
        #[cfg(feature = "test-faults")]
        let responses = match fault {
            Some(MainWindowSelectedComposerPreparationTestFault::ResponseKind) => {
                let mut responses = responses.into_vec();
                let response = responses.first().expect("selected test seed");
                let crate::composer_host::ComposerHostResponseValue::CandidateText(candidate) =
                    response.value()
                else {
                    panic!("selected test seed is candidate text");
                };
                responses[0] = crate::composer_host::ComposerHostResponse::new(
                    response.key(),
                    crate::composer_host::ComposerHostResponseValue::HistoricalText(
                        candidate.value().clone(),
                    ),
                );
                responses.into_boxed_slice()
            }
            Some(MainWindowSelectedComposerPreparationTestFault::ResponseBinding(binding)) => {
                let mut responses = responses.into_vec();
                let response = responses.first().expect("selected test seed");
                let key = response.key();
                responses[0] = crate::composer_host::ComposerHostResponse::new(
                    crate::composer_host::ComposerHostRequestKey::new(
                        binding,
                        key.request_id(),
                        key.purpose(),
                    ),
                    response.value().clone(),
                );
                responses.into_boxed_slice()
            }
            _ => responses,
        };
        Ok(responses)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_fail_selected_preparation(
        &self,
        fault: MainWindowSelectedComposerPreparationTestFault,
    ) {
        assert!(
            self.test_selected_preparation_fault
                .lock()
                .unwrap()
                .replace(fault)
                .is_none()
        );
    }

    pub(super) fn take_pending_initial_presentation(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<
        (
            MainWindowComposerSelectionIdentity,
            Box<[crate::composer_host::ComposerHostResponse]>,
        ),
        String,
    > {
        let presentation = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .take_pending_initial_presentation(receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())?;
        let selection = presentation.selection();
        let responses = presentation.into_responses().into_vec();
        #[cfg(feature = "test-faults")]
        let mut responses = responses;
        #[cfg(feature = "test-faults")]
        if self
            .test_append_impossible_pending_initial_response
            .swap(false, Ordering::SeqCst)
        {
            let (key, value) = responses
                .iter()
                .find_map(|response| match response.value() {
                    crate::composer_host::ComposerHostResponseValue::CandidateText(candidate) => {
                        Some((response.key(), candidate.value().clone()))
                    }
                    _ => None,
                })
                .expect("test activation includes candidate text");
            responses.push(crate::composer_host::ComposerHostResponse::new(
                key,
                crate::composer_host::ComposerHostResponseValue::HistoricalText(value),
            ));
        }
        Ok((selection, responses.into_boxed_slice()))
    }

    pub(super) fn release_widget_work(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        requests: Vec<RangeTextInputRequest>,
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .release_selected_widget_work(selection, requests)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }
}
