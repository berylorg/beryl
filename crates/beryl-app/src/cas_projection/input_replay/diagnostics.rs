use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use beryl_backend::StreamedInputSourceError;

mod barrier;

pub use barrier::SourcePageHandoffBarrierController;

/// Request-local, content-free input replay progress diagnostics for lifecycle tests.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct OrdinaryInputReplayDiagnostics {
    state: Arc<DiagnosticsState>,
}

#[derive(Debug, Default)]
struct DiagnosticsState {
    source_request_count: AtomicUsize,
    passes_started: AtomicUsize,
    descriptors_emitted: AtomicUsize,
    text_page_requests: AtomicUsize,
    logical_text_bytes: AtomicU64,
    sidecar_verifications: AtomicUsize,
    page_handoff_barriers: barrier::SourcePageHandoffBarriers,
    source_page_failure: Mutex<Option<ScheduledSourcePageFailure>>,
}

#[derive(Debug)]
struct ScheduledSourcePageFailure {
    target_request: usize,
    error: StreamedInputSourceError,
}

/// One content-free request-local ordinary-input lifecycle snapshot.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrdinaryInputReplayDiagnosticsSnapshot {
    source_request_count: usize,
    passes_started: usize,
    descriptors_emitted: usize,
    text_page_requests: usize,
    logical_text_bytes: u64,
    sidecar_verifications: usize,
}

impl OrdinaryInputReplayDiagnosticsSnapshot {
    /// Returns all serviced source-broker calls, including EOF requests.
    #[must_use]
    pub const fn source_request_count(self) -> usize {
        self.source_request_count
    }

    /// Returns successful replay-pass starts.
    #[must_use]
    pub const fn passes_started(self) -> usize {
        self.passes_started
    }

    /// Returns descriptors emitted across all replay passes.
    #[must_use]
    pub const fn descriptors_emitted(self) -> usize {
        self.descriptors_emitted
    }

    /// Returns attempted text-page requests across all replay passes.
    #[must_use]
    pub const fn text_page_requests(self) -> usize {
        self.text_page_requests
    }

    /// Returns logical text bytes successfully handed to the backend.
    #[must_use]
    pub const fn logical_text_bytes(self) -> u64 {
        self.logical_text_bytes
    }

    /// Returns successful and failed sidecar-verification attempts.
    #[must_use]
    pub const fn sidecar_verifications(self) -> usize {
        self.sidecar_verifications
    }
}

impl OrdinaryInputReplayDiagnostics {
    /// Creates independent request-local diagnostics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads one content-free atomic snapshot.
    #[must_use]
    pub fn snapshot(&self) -> OrdinaryInputReplayDiagnosticsSnapshot {
        OrdinaryInputReplayDiagnosticsSnapshot {
            source_request_count: self.state.source_request_count.load(Ordering::SeqCst),
            passes_started: self.state.passes_started.load(Ordering::SeqCst),
            descriptors_emitted: self.state.descriptors_emitted.load(Ordering::SeqCst),
            text_page_requests: self.state.text_page_requests.load(Ordering::SeqCst),
            logical_text_bytes: self.state.logical_text_bytes.load(Ordering::SeqCst),
            sidecar_verifications: self.state.sidecar_verifications.load(Ordering::SeqCst),
        }
    }

    /// Installs one pause for the selected one-based successful page handoff.
    #[doc(hidden)]
    #[must_use]
    pub fn install_source_page_handoff_barrier(
        &self,
        target_request: usize,
    ) -> SourcePageHandoffBarrierController {
        self.state.page_handoff_barriers.install(target_request)
    }

    /// Installs one failure after the selected one-based durable page read.
    #[doc(hidden)]
    pub fn install_source_page_failure(
        &self,
        target_request: usize,
        error: StreamedInputSourceError,
    ) {
        assert!(
            target_request != 0,
            "source-page failure target is one-based"
        );
        let mut installed = self
            .state
            .source_page_failure
            .lock()
            .expect("source-page failure registry is usable");
        assert!(
            installed.is_none(),
            "ordinary-input diagnostics already own a source-page failure"
        );
        *installed = Some(ScheduledSourcePageFailure {
            target_request,
            error,
        });
    }

    pub(super) fn record_source_request(&self) {
        increment(
            &self.state.source_request_count,
            "ordinary-input source request",
        );
    }

    pub(super) fn record_sidecar_verification(&self) {
        increment(
            &self.state.sidecar_verifications,
            "ordinary-input sidecar verification",
        );
    }

    pub(super) fn record_pass_started(&self) {
        increment(&self.state.passes_started, "ordinary-input replay pass");
    }

    pub(super) fn record_descriptor_emitted(&self) {
        increment(
            &self.state.descriptors_emitted,
            "ordinary-input emitted descriptor",
        );
    }

    pub(super) fn record_text_page_request(&self) -> usize {
        increment(
            &self.state.text_page_requests,
            "ordinary-input text-page request",
        )
    }

    pub(super) fn latest_text_page_request(&self) -> usize {
        self.state.text_page_requests.load(Ordering::SeqCst)
    }

    pub(super) fn record_logical_text_bytes(&self, bytes: usize) {
        let bytes = u64::try_from(bytes).expect("bounded text-page bytes fit u64");
        self.state
            .logical_text_bytes
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(bytes)
            })
            .expect("ordinary-input logical text-byte count overflowed");
    }

    pub(super) fn pause_source_page_handoff(&self, request: usize) {
        self.state.page_handoff_barriers.pause_if_target(request);
    }

    pub(super) fn take_source_page_failure(
        &self,
        request: usize,
    ) -> Option<StreamedInputSourceError> {
        let mut installed = self
            .state
            .source_page_failure
            .lock()
            .expect("source-page failure registry is usable");
        if installed
            .as_ref()
            .is_some_and(|failure| failure.target_request == request)
        {
            installed.take().map(|failure| failure.error)
        } else {
            None
        }
    }
}

fn increment(counter: &AtomicUsize, label: &'static str) -> usize {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            current.checked_add(1)
        })
        .map(|previous| previous + 1)
        .unwrap_or_else(|_| panic!("{label} count overflowed"))
}
