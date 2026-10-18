use super::*;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub(crate) struct IndexedReadGate {
    revision: crate::syndic_transcript::ProviderRevision,
    kind: IndexedReadKind,
    entered: AtomicBool,
    refused: AtomicBool,
    released: Mutex<bool>,
    wake: Condvar,
}

enum IndexedReadKind {
    RecoveryRange,
    CodeRange { start: u64, end: u64 },
    Source,
    Display,
}

impl IndexedReadGate {
    pub(crate) fn new(revision: crate::syndic_transcript::ProviderRevision) -> Arc<Self> {
        Self::with_kind(revision, IndexedReadKind::RecoveryRange)
    }

    pub(crate) fn source(revision: crate::syndic_transcript::ProviderRevision) -> Arc<Self> {
        Self::with_kind(revision, IndexedReadKind::Source)
    }

    pub(crate) fn code_range(
        revision: crate::syndic_transcript::ProviderRevision,
        lines: Range<u64>,
    ) -> Arc<Self> {
        Self::with_kind(
            revision,
            IndexedReadKind::CodeRange {
                start: lines.start,
                end: lines.end,
            },
        )
    }

    pub(crate) fn display(revision: crate::syndic_transcript::ProviderRevision) -> Arc<Self> {
        Self::with_kind(revision, IndexedReadKind::Display)
    }

    fn with_kind(
        revision: crate::syndic_transcript::ProviderRevision,
        kind: IndexedReadKind,
    ) -> Arc<Self> {
        Arc::new(Self {
            revision,
            kind,
            entered: AtomicBool::new(false),
            refused: AtomicBool::new(false),
            released: Mutex::new(false),
            wake: Condvar::new(),
        })
    }

    pub(super) fn matches(&self, value: &IndexedResourceValue) -> bool {
        match self.kind {
            IndexedReadKind::Source => matches!(value, IndexedResourceValue::Source(_)),
            IndexedReadKind::Display => matches!(value, IndexedResourceValue::Display(_)),
            IndexedReadKind::CodeRange { start, end } => matches!(value, IndexedResourceValue::Range(range)
                if matches!(range.cursor.request, syndic_storage::indexed_resource::Request::Code { lines } if lines.start == start && lines.end == end)
                    && range.request_complete),
            IndexedReadKind::RecoveryRange => matches!(value, IndexedResourceValue::Range(range)
                if matches!(range.cursor.request, syndic_storage::indexed_resource::Request::Code { lines } if lines.start == 902 && lines.end == 918)
                    && range.request_complete),
        }
    }

    pub(crate) fn entered(&self) -> bool {
        self.entered.load(Ordering::Acquire)
    }

    pub(crate) fn refused(&self) -> bool {
        self.refused.load(Ordering::Acquire)
    }

    pub(crate) fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }

    pub(super) fn hold(&self, revision: crate::syndic_transcript::ProviderRevision) -> bool {
        if revision != self.revision {
            return false;
        }
        let released = self.released.lock().unwrap();
        self.entered.store(true, Ordering::Release);
        let (released, timeout) = self
            .wake
            .wait_timeout_while(released, Duration::from_secs(10), |released| !*released)
            .unwrap();
        assert!(
            *released && !timeout.timed_out(),
            "indexed read gate timed out"
        );
        true
    }

    pub(super) fn observe(&self, result: &Result<(), TranscriptAttachmentError>) {
        self.refused.store(
            matches!(
                result,
                Err(TranscriptAttachmentError::Stale | TranscriptAttachmentError::Cancelled)
            ),
            Ordering::Release,
        );
    }
}

impl TranscriptProviderReader {
    pub(crate) fn set_indexed_read_gate_for_test(&mut self, gate: Arc<IndexedReadGate>) {
        self.indexed_read_gate = Some(gate);
    }
}
