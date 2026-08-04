use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use beryl_backend::{
    THREAD_INJECTION_MAX_PAGE_BYTES, ThreadInjectionOutcome, ThreadInjectionSource,
    ThreadInjectionSourceError, ThreadInjectionSourcePage,
};
use beryl_stream::{
    ChannelDiagnostics, FixedChannelObserver, FixedChannelReceiver, FixedChannelSender, PageLease,
    PagePool, PagePoolDiagnostics, PagePoolObserver, ReceiveError, SendError, fixed_channel,
};

use super::{ConnectionCommandOutcome, ProjectionCoordinatorError};

const BROKER_POLL_INTERVAL: Duration = Duration::from_millis(20);

type SourceResult = Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError>;

enum RecoverySourceBrokerEvent {
    NextPage {
        max_utf8_bytes: usize,
        page: PageLease,
    },
    Finished(ConnectionCommandOutcome<ThreadInjectionOutcome>),
}

struct RecoveryReplayCounters {
    logical_pages: AtomicU64,
    logical_items: AtomicU64,
    logical_utf8_bytes: AtomicU64,
    waits: AtomicU64,
    final_capacity: Mutex<Option<RecoveryReplayCapacityDiagnostics>>,
}

#[derive(Clone)]
pub(super) struct RecoveryReplayDiagnostics {
    pages: PagePoolObserver,
    requests: FixedChannelObserver<RecoverySourceBrokerEvent>,
    replies: FixedChannelObserver<SourceResult>,
    counters: Arc<RecoveryReplayCounters>,
}

#[derive(Clone, Copy, Debug)]
pub struct RecoveryReplayCapacityDiagnostics {
    pages: PagePoolDiagnostics,
    requests: ChannelDiagnostics,
    replies: ChannelDiagnostics,
}

#[derive(Clone, Copy, Debug)]
pub struct RecoveryReplayDiagnosticsSnapshot {
    live_capacity: Option<RecoveryReplayCapacityDiagnostics>,
    final_capacity: Option<RecoveryReplayCapacityDiagnostics>,
    released: bool,
    logical_pages: u64,
    logical_items: u64,
    logical_utf8_bytes: u64,
    waits: u64,
}

pub(super) struct RecoveryReplayDiagnosticsSlot {
    current: Mutex<Option<RecoveryReplayDiagnostics>>,
}

#[derive(Clone)]
pub struct RecoveryReplayDiagnosticsObserver {
    slot: Weak<RecoveryReplayDiagnosticsSlot>,
}

pub(in crate::cas_projection) struct PreparedRecoverySource {
    source: RemoteRecoverySource,
    service: RecoverySourceService,
    diagnostics: RecoveryReplayDiagnostics,
}

pub(super) struct RecoverySourceService {
    requests: FixedChannelReceiver<RecoverySourceBrokerEvent>,
    replies: FixedChannelSender<SourceResult>,
    _pages: PagePool,
}

pub(super) struct RemoteRecoverySource {
    requests: FixedChannelSender<RecoverySourceBrokerEvent>,
    replies: FixedChannelReceiver<SourceResult>,
    pages: PagePool,
    diagnostics: RecoveryReplayDiagnostics,
}

pub(super) fn prepare() -> Result<PreparedRecoverySource, ProjectionCoordinatorError> {
    let page_capacity = NonZeroUsize::new(THREAD_INJECTION_MAX_PAGE_BYTES)
        .expect("backend recovery page maximum is nonzero");
    let capacity = NonZeroUsize::MIN;
    let pages = PagePool::new(page_capacity, capacity).map_err(admission)?;
    let (request_sender, request_receiver) = fixed_channel(capacity).map_err(admission)?;
    let (reply_sender, reply_receiver) = fixed_channel(capacity).map_err(admission)?;
    let counters = Arc::new(RecoveryReplayCounters {
        logical_pages: AtomicU64::new(0),
        logical_items: AtomicU64::new(0),
        logical_utf8_bytes: AtomicU64::new(0),
        waits: AtomicU64::new(0),
        final_capacity: Mutex::new(None),
    });
    let diagnostics = RecoveryReplayDiagnostics {
        pages: pages.observer(),
        requests: request_sender.observer(),
        replies: reply_sender.observer(),
        counters,
    };
    let service_pages = pages.clone();
    Ok(PreparedRecoverySource {
        source: RemoteRecoverySource {
            requests: request_sender,
            replies: reply_receiver,
            pages,
            diagnostics: diagnostics.clone(),
        },
        service: RecoverySourceService {
            requests: request_receiver,
            replies: reply_sender,
            _pages: service_pages,
        },
        diagnostics,
    })
}

fn admission(error: impl std::fmt::Display) -> ProjectionCoordinatorError {
    ProjectionCoordinatorError::ProviderBrokerAdmission {
        message: error.to_string(),
    }
}

impl PreparedRecoverySource {
    pub(super) fn diagnostics(&self) -> RecoveryReplayDiagnostics {
        self.diagnostics.clone()
    }

    pub(super) fn into_parts(self) -> (RemoteRecoverySource, RecoverySourceService) {
        (self.source, self.service)
    }
}

impl RecoveryReplayDiagnostics {
    pub(super) fn snapshot(&self) -> RecoveryReplayDiagnosticsSnapshot {
        let live_capacity = self.live_capacity();
        let final_capacity = *self
            .counters
            .final_capacity
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        RecoveryReplayDiagnosticsSnapshot {
            live_capacity,
            final_capacity,
            released: live_capacity.is_none(),
            logical_pages: self.counters.logical_pages.load(Ordering::Relaxed),
            logical_items: self.counters.logical_items.load(Ordering::Relaxed),
            logical_utf8_bytes: self.counters.logical_utf8_bytes.load(Ordering::Relaxed),
            waits: self.counters.waits.load(Ordering::Relaxed),
        }
    }

    fn live_capacity(&self) -> Option<RecoveryReplayCapacityDiagnostics> {
        Some(RecoveryReplayCapacityDiagnostics {
            pages: self.pages.diagnostics()?,
            requests: self.requests.diagnostics()?,
            replies: self.replies.diagnostics()?,
        })
    }

    fn capture_final_capacity(&self) {
        let Some(capacity) = self.live_capacity() else {
            return;
        };
        *self
            .counters
            .final_capacity
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(capacity);
    }

    fn record_page(&self, page: &ThreadInjectionSourcePage) {
        self.counters.logical_pages.fetch_add(1, Ordering::Relaxed);
        self.counters
            .logical_utf8_bytes
            .fetch_add(page.text().len() as u64, Ordering::Relaxed);
        if page.item_terminal() {
            self.counters.logical_items.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_wait(&self) {
        self.counters.waits.fetch_add(1, Ordering::Relaxed);
    }
}

impl RecoveryReplayDiagnosticsSlot {
    pub(super) fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    pub(super) fn observer(self: &Arc<Self>) -> RecoveryReplayDiagnosticsObserver {
        RecoveryReplayDiagnosticsObserver {
            slot: Arc::downgrade(self),
        }
    }

    pub(super) fn publish(&self, diagnostics: RecoveryReplayDiagnostics) {
        *self
            .current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(diagnostics);
    }

    pub(super) fn snapshot(&self) -> Option<RecoveryReplayDiagnosticsSnapshot> {
        self.current
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .map(RecoveryReplayDiagnostics::snapshot)
    }
}

impl RecoveryReplayDiagnosticsObserver {
    /// Reads the current content-free recovery capacity facts without retaining the connection.
    #[must_use]
    pub fn snapshot(&self) -> Option<RecoveryReplayDiagnosticsSnapshot> {
        self.slot.upgrade()?.snapshot()
    }
}

impl RecoveryReplayCapacityDiagnostics {
    #[must_use]
    pub const fn pages(self) -> PagePoolDiagnostics {
        self.pages
    }

    #[must_use]
    pub const fn requests(self) -> ChannelDiagnostics {
        self.requests
    }

    #[must_use]
    pub const fn replies(self) -> ChannelDiagnostics {
        self.replies
    }
}

impl RecoveryReplayDiagnosticsSnapshot {
    #[must_use]
    pub const fn live_capacity(self) -> Option<RecoveryReplayCapacityDiagnostics> {
        self.live_capacity
    }

    #[must_use]
    pub const fn final_capacity(self) -> Option<RecoveryReplayCapacityDiagnostics> {
        self.final_capacity
    }

    #[must_use]
    pub const fn released(self) -> bool {
        self.released
    }

    #[must_use]
    pub const fn logical_pages(self) -> u64 {
        self.logical_pages
    }

    #[must_use]
    pub const fn logical_items(self) -> u64 {
        self.logical_items
    }

    #[must_use]
    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }

    #[must_use]
    pub const fn waits(self) -> u64 {
        self.waits
    }
}

impl ThreadInjectionSource for RemoteRecoverySource {
    fn next_page(
        &mut self,
        max_utf8_bytes: usize,
    ) -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError> {
        if max_utf8_bytes == 0 {
            return Err(ThreadInjectionSourceError::ZeroPageRequest);
        }
        let page = self
            .pages
            .try_lease()
            .map_err(|_| ThreadInjectionSourceError::Unavailable)?;
        self.send_request(RecoverySourceBrokerEvent::NextPage {
            max_utf8_bytes,
            page,
        })?;
        loop {
            match self.replies.receive_timeout(BROKER_POLL_INTERVAL) {
                Ok(Some(result)) => return result,
                Ok(None) => self.diagnostics.record_wait(),
                Err(ReceiveError::Closed | ReceiveError::Empty) => {
                    return Err(ThreadInjectionSourceError::Unavailable);
                }
            }
        }
    }
}

impl RemoteRecoverySource {
    fn send_request(
        &self,
        event: RecoverySourceBrokerEvent,
    ) -> Result<(), ThreadInjectionSourceError> {
        let mut pending = event;
        loop {
            match self.requests.send_timeout(pending, BROKER_POLL_INTERVAL) {
                Ok(()) => return Ok(()),
                Err(SendError::Full(returned) | SendError::Timeout(returned)) => {
                    self.diagnostics.record_wait();
                    pending = returned;
                }
                Err(SendError::Closed(_)) => {
                    return Err(ThreadInjectionSourceError::Unavailable);
                }
            }
        }
    }

    pub(super) fn finish(self, outcome: ConnectionCommandOutcome<ThreadInjectionOutcome>) {
        let _ = self.send_request(RecoverySourceBrokerEvent::Finished(outcome));
    }
}

pub(super) fn service_until_finished(
    service: RecoverySourceService,
    diagnostics: &RecoveryReplayDiagnostics,
    mut next_page: impl FnMut(
        usize,
        PageLease,
    )
        -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError>,
) -> Result<ConnectionCommandOutcome<ThreadInjectionOutcome>, ProjectionCoordinatorError> {
    loop {
        let event = match service.requests.receive_timeout(BROKER_POLL_INTERVAL) {
            Ok(Some(event)) => event,
            Ok(None) => {
                diagnostics.record_wait();
                continue;
            }
            Err(ReceiveError::Closed | ReceiveError::Empty) => {
                return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
            }
        };
        match event {
            RecoverySourceBrokerEvent::NextPage {
                max_utf8_bytes,
                page,
            } => {
                let result = next_page(max_utf8_bytes, page);
                if let Ok(Some(page)) = &result {
                    diagnostics.record_page(page);
                }
                send_reply(&service.replies, diagnostics, result)?;
            }
            RecoverySourceBrokerEvent::Finished(outcome) => {
                diagnostics.capture_final_capacity();
                return Ok(outcome);
            }
        }
    }
}

fn send_reply(
    replies: &FixedChannelSender<SourceResult>,
    diagnostics: &RecoveryReplayDiagnostics,
    result: SourceResult,
) -> Result<(), ProjectionCoordinatorError> {
    let mut pending = result;
    loop {
        match replies.send_timeout(pending, BROKER_POLL_INTERVAL) {
            Ok(()) => return Ok(()),
            Err(SendError::Full(returned) | SendError::Timeout(returned)) => {
                diagnostics.record_wait();
                pending = returned;
            }
            Err(SendError::Closed(_)) => {
                return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
            }
        }
    }
}

#[cfg(test)]
#[path = "recovery_source_broker/tests.rs"]
mod tests;
