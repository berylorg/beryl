use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Request-shareable, content-free residency accounting for one text read path.
#[derive(Clone, Debug, Default)]
pub struct ContentTextReadResidencyTracker {
    state: Arc<TrackerState>,
}

#[derive(Debug, Default)]
struct TrackerState {
    outputs: ResidencyCounter,
    cached_chunks: ResidencyCounter,
    cursor_pages: ResidencyCounter,
    maximum_output_bytes: AtomicUsize,
    maximum_cached_chunk_bytes: AtomicUsize,
    maximum_cursor_page_bytes: AtomicUsize,
    cursor_page_reads: AtomicUsize,
}

#[derive(Debug, Default)]
struct ResidencyCounter {
    current: AtomicUsize,
    maximum: AtomicUsize,
}

/// Current and maximum ownership for one text-read dependency class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentTextReadResidency {
    current: usize,
    maximum: usize,
}

/// One content-free snapshot of text-read dependency ownership and work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentTextReadResidencySnapshot {
    outputs: ContentTextReadResidency,
    cached_chunks: ContentTextReadResidency,
    cursor_pages: ContentTextReadResidency,
    maximum_output_bytes: usize,
    maximum_cached_chunk_bytes: usize,
    maximum_cursor_page_bytes: usize,
    cursor_page_reads: usize,
}

impl ContentTextReadResidency {
    #[must_use]
    pub const fn current(self) -> usize {
        self.current
    }

    #[must_use]
    pub const fn maximum(self) -> usize {
        self.maximum
    }
}

impl ContentTextReadResidencySnapshot {
    #[must_use]
    pub const fn outputs(self) -> ContentTextReadResidency {
        self.outputs
    }

    #[must_use]
    pub const fn cached_chunks(self) -> ContentTextReadResidency {
        self.cached_chunks
    }

    #[must_use]
    pub const fn cursor_pages(self) -> ContentTextReadResidency {
        self.cursor_pages
    }

    #[must_use]
    pub const fn maximum_output_bytes(self) -> usize {
        self.maximum_output_bytes
    }

    #[must_use]
    pub const fn maximum_cached_chunk_bytes(self) -> usize {
        self.maximum_cached_chunk_bytes
    }

    #[must_use]
    pub const fn maximum_cursor_page_bytes(self) -> usize {
        self.maximum_cursor_page_bytes
    }

    #[must_use]
    pub const fn cursor_page_reads(self) -> usize {
        self.cursor_page_reads
    }
}

impl ContentTextReadResidencyTracker {
    #[must_use]
    pub fn snapshot(&self) -> ContentTextReadResidencySnapshot {
        ContentTextReadResidencySnapshot {
            outputs: self.state.outputs.snapshot(),
            cached_chunks: self.state.cached_chunks.snapshot(),
            cursor_pages: self.state.cursor_pages.snapshot(),
            maximum_output_bytes: self.state.maximum_output_bytes.load(Ordering::SeqCst),
            maximum_cached_chunk_bytes: self
                .state
                .maximum_cached_chunk_bytes
                .load(Ordering::SeqCst),
            maximum_cursor_page_bytes: self.state.maximum_cursor_page_bytes.load(Ordering::SeqCst),
            cursor_page_reads: self.state.cursor_page_reads.load(Ordering::SeqCst),
        }
    }

    pub(crate) fn acquire_output(&self, bytes: usize) -> ContentTextReadResidencyLease {
        self.state
            .maximum_output_bytes
            .fetch_max(bytes, Ordering::SeqCst);
        self.acquire(ResidencyKind::Output)
    }

    pub(crate) fn acquire_cached_chunk(&self, bytes: usize) -> ContentTextReadResidencyLease {
        self.state
            .maximum_cached_chunk_bytes
            .fetch_max(bytes, Ordering::SeqCst);
        self.acquire(ResidencyKind::CachedChunk)
    }

    pub(crate) fn acquire_cursor_page(&self, bytes: usize) -> ContentTextReadResidencyLease {
        self.state
            .maximum_cursor_page_bytes
            .fetch_max(bytes, Ordering::SeqCst);
        increment(&self.state.cursor_page_reads);
        self.acquire(ResidencyKind::CursorPage)
    }

    fn acquire(&self, kind: ResidencyKind) -> ContentTextReadResidencyLease {
        self.counter(kind).acquire();
        ContentTextReadResidencyLease {
            tracker: self.clone(),
            kind,
        }
    }

    fn counter(&self, kind: ResidencyKind) -> &ResidencyCounter {
        match kind {
            ResidencyKind::Output => &self.state.outputs,
            ResidencyKind::CachedChunk => &self.state.cached_chunks,
            ResidencyKind::CursorPage => &self.state.cursor_pages,
        }
    }
}

impl ResidencyCounter {
    fn acquire(&self) {
        let previous = self.current.fetch_add(1, Ordering::SeqCst);
        self.maximum.fetch_max(previous + 1, Ordering::SeqCst);
    }

    fn release(&self) {
        let previous = self.current.fetch_sub(1, Ordering::SeqCst);
        assert!(
            previous != 0,
            "content-text residency released without ownership"
        );
    }

    fn snapshot(&self) -> ContentTextReadResidency {
        ContentTextReadResidency {
            current: self.current.load(Ordering::SeqCst),
            maximum: self.maximum.load(Ordering::SeqCst),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ResidencyKind {
    Output,
    CachedChunk,
    CursorPage,
}

#[derive(Debug)]
pub(crate) struct ContentTextReadResidencyLease {
    tracker: ContentTextReadResidencyTracker,
    kind: ResidencyKind,
}

impl Drop for ContentTextReadResidencyLease {
    fn drop(&mut self) {
        self.tracker.counter(self.kind).release();
    }
}

fn increment(counter: &AtomicUsize) {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            current.checked_add(1)
        })
        .expect("content-text read counter overflowed");
}
