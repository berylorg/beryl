use std::sync::atomic::{AtomicUsize, Ordering};

static PAGE_COUNT: AtomicUsize = AtomicUsize::new(0);
static TURN_PAGE_COUNT: AtomicUsize = AtomicUsize::new(0);
static MAX_PAGE_ITEMS: AtomicUsize = AtomicUsize::new(0);
static MAX_PAGE_STORED_BYTES: AtomicUsize = AtomicUsize::new(0);
static CURRENT_BINDING_FIRST_HEAD_BYTES: AtomicUsize = AtomicUsize::new(0);
static CURRENT_BINDING_RECORD_BYTES: AtomicUsize = AtomicUsize::new(0);
static CURRENT_BINDING_SECOND_HEAD_BYTES: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_FRONTIER_ALLOCATION_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_FRONTIER_ALLOCATION_COMPLETIONS: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_FRONTIER_REQUESTED_ITEMS: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_FRONTIER_OBSERVED_CAPACITY: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_TURN_ITEM_READ_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);

/// Test-only bounded validation cursor observations since the last reset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationPageMetrics {
    page_count: usize,
    turn_page_count: usize,
    max_page_items: usize,
    max_page_stored_bytes: usize,
    item_limit: usize,
    byte_limit: usize,
}

/// Test-only component accounting for the latest successful current-binding stability read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurrentBindingReadMetrics {
    first_head_bytes: usize,
    binding_bytes: usize,
    second_head_bytes: usize,
}

/// Test-only observations at the recovery item-frontier allocation and index-read boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryFrontierMetrics {
    allocation_attempts: usize,
    allocation_completions: usize,
    requested_items: usize,
    observed_capacity: usize,
    turn_item_read_attempts: usize,
}

impl RecoveryFrontierMetrics {
    #[must_use]
    pub const fn allocation_attempts(self) -> usize {
        self.allocation_attempts
    }

    #[must_use]
    pub const fn allocation_completions(self) -> usize {
        self.allocation_completions
    }

    #[must_use]
    pub const fn requested_items(self) -> usize {
        self.requested_items
    }

    #[must_use]
    pub const fn observed_capacity(self) -> usize {
        self.observed_capacity
    }

    #[must_use]
    pub const fn turn_item_read_attempts(self) -> usize {
        self.turn_item_read_attempts
    }
}

impl CurrentBindingReadMetrics {
    #[must_use]
    pub const fn first_head_bytes(self) -> usize {
        self.first_head_bytes
    }

    #[must_use]
    pub const fn binding_bytes(self) -> usize {
        self.binding_bytes
    }

    #[must_use]
    pub const fn second_head_bytes(self) -> usize {
        self.second_head_bytes
    }
}

impl ValidationPageMetrics {
    #[must_use]
    pub const fn page_count(self) -> usize {
        self.page_count
    }

    #[must_use]
    pub const fn turn_page_count(self) -> usize {
        self.turn_page_count
    }

    #[must_use]
    pub const fn max_page_items(self) -> usize {
        self.max_page_items
    }

    #[must_use]
    pub const fn max_page_stored_bytes(self) -> usize {
        self.max_page_stored_bytes
    }

    #[must_use]
    pub const fn item_limit(self) -> usize {
        self.item_limit
    }

    #[must_use]
    pub const fn byte_limit(self) -> usize {
        self.byte_limit
    }
}

/// Resets the test-only validation cursor observations.
pub fn reset_validation_page_metrics() {
    PAGE_COUNT.store(0, Ordering::Relaxed);
    TURN_PAGE_COUNT.store(0, Ordering::Relaxed);
    MAX_PAGE_ITEMS.store(0, Ordering::Relaxed);
    MAX_PAGE_STORED_BYTES.store(0, Ordering::Relaxed);
}

/// Returns the test-only validation cursor observations.
#[must_use]
pub fn validation_page_metrics() -> ValidationPageMetrics {
    ValidationPageMetrics {
        page_count: PAGE_COUNT.load(Ordering::Relaxed),
        turn_page_count: TURN_PAGE_COUNT.load(Ordering::Relaxed),
        max_page_items: MAX_PAGE_ITEMS.load(Ordering::Relaxed),
        max_page_stored_bytes: MAX_PAGE_STORED_BYTES.load(Ordering::Relaxed),
        item_limit: crate::validation::VALIDATION_PAGE_ITEMS,
        byte_limit: crate::validation::VALIDATION_PAGE_BYTES,
    }
}

/// Clears the test-only current-binding component accounting.
pub fn reset_current_binding_read_metrics() {
    CURRENT_BINDING_FIRST_HEAD_BYTES.store(0, Ordering::Relaxed);
    CURRENT_BINDING_RECORD_BYTES.store(0, Ordering::Relaxed);
    CURRENT_BINDING_SECOND_HEAD_BYTES.store(0, Ordering::Relaxed);
}

/// Returns component bytes observed by the latest successful current-binding stability read.
#[must_use]
pub fn current_binding_read_metrics() -> CurrentBindingReadMetrics {
    CurrentBindingReadMetrics {
        first_head_bytes: CURRENT_BINDING_FIRST_HEAD_BYTES.load(Ordering::Relaxed),
        binding_bytes: CURRENT_BINDING_RECORD_BYTES.load(Ordering::Relaxed),
        second_head_bytes: CURRENT_BINDING_SECOND_HEAD_BYTES.load(Ordering::Relaxed),
    }
}

/// Clears the test-only recovery item-frontier observations.
pub fn reset_recovery_frontier_metrics() {
    RECOVERY_FRONTIER_ALLOCATION_ATTEMPTS.store(0, Ordering::Relaxed);
    RECOVERY_FRONTIER_ALLOCATION_COMPLETIONS.store(0, Ordering::Relaxed);
    RECOVERY_FRONTIER_REQUESTED_ITEMS.store(0, Ordering::Relaxed);
    RECOVERY_FRONTIER_OBSERVED_CAPACITY.store(0, Ordering::Relaxed);
    RECOVERY_TURN_ITEM_READ_ATTEMPTS.store(0, Ordering::Relaxed);
}

/// Returns observations from the recovery item-frontier path since the last reset.
#[must_use]
pub fn recovery_frontier_metrics() -> RecoveryFrontierMetrics {
    RecoveryFrontierMetrics {
        allocation_attempts: RECOVERY_FRONTIER_ALLOCATION_ATTEMPTS.load(Ordering::Relaxed),
        allocation_completions: RECOVERY_FRONTIER_ALLOCATION_COMPLETIONS.load(Ordering::Relaxed),
        requested_items: RECOVERY_FRONTIER_REQUESTED_ITEMS.load(Ordering::Relaxed),
        observed_capacity: RECOVERY_FRONTIER_OBSERVED_CAPACITY.load(Ordering::Relaxed),
        turn_item_read_attempts: RECOVERY_TURN_ITEM_READ_ATTEMPTS.load(Ordering::Relaxed),
    }
}

pub(crate) fn record_recovery_frontier_allocation_attempt(requested_items: usize) {
    RECOVERY_FRONTIER_ALLOCATION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    RECOVERY_FRONTIER_REQUESTED_ITEMS.store(requested_items, Ordering::Relaxed);
}

pub(crate) fn record_recovery_frontier_allocation_completion(observed_capacity: usize) {
    RECOVERY_FRONTIER_ALLOCATION_COMPLETIONS.fetch_add(1, Ordering::Relaxed);
    RECOVERY_FRONTIER_OBSERVED_CAPACITY.store(observed_capacity, Ordering::Relaxed);
}

pub(crate) fn record_recovery_turn_item_read_attempt() {
    RECOVERY_TURN_ITEM_READ_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_validation_page(family: &'static str, items: usize, stored_bytes: usize) {
    PAGE_COUNT.fetch_add(1, Ordering::Relaxed);
    if family == "turns" {
        TURN_PAGE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    MAX_PAGE_ITEMS.fetch_max(items, Ordering::Relaxed);
    MAX_PAGE_STORED_BYTES.fetch_max(stored_bytes, Ordering::Relaxed);
}

pub(crate) fn record_current_binding_read(
    first_head_bytes: usize,
    binding_bytes: usize,
    second_head_bytes: usize,
) {
    CURRENT_BINDING_FIRST_HEAD_BYTES.store(first_head_bytes, Ordering::Relaxed);
    CURRENT_BINDING_RECORD_BYTES.store(binding_bytes, Ordering::Relaxed);
    CURRENT_BINDING_SECOND_HEAD_BYTES.store(second_head_bytes, Ordering::Relaxed);
}
