use std::sync::atomic::{AtomicUsize, Ordering};

static PAGE_COUNT: AtomicUsize = AtomicUsize::new(0);
static TURN_PAGE_COUNT: AtomicUsize = AtomicUsize::new(0);
static MAX_PAGE_ITEMS: AtomicUsize = AtomicUsize::new(0);
static MAX_PAGE_STORED_BYTES: AtomicUsize = AtomicUsize::new(0);
static CURRENT_BINDING_FIRST_HEAD_READS: AtomicUsize = AtomicUsize::new(0);
static CURRENT_BINDING_RECORD_READS: AtomicUsize = AtomicUsize::new(0);
static CURRENT_BINDING_SECOND_HEAD_READS: AtomicUsize = AtomicUsize::new(0);
static DELIVERING_STEERING_POINT_READS: AtomicUsize = AtomicUsize::new(0);
static READY_STEERING_POINT_READS: AtomicUsize = AtomicUsize::new(0);
static SYNDIC_POINT_READS: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_MAX_RESIDENT_TURNS: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_MAX_RESIDENT_ITEMS: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_TURN_ITEM_READ_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_CURSOR_PAGE_COUNT: AtomicUsize = AtomicUsize::new(0);
static RECOVERY_MAX_CURSOR_PAGE_BYTES: AtomicUsize = AtomicUsize::new(0);

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

/// Test-only component observations for the latest successful current-binding stability read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurrentBindingReadMetrics {
    first_head_reads: usize,
    binding_reads: usize,
    second_head_reads: usize,
}

/// Test-only point-read observations for one delivering-steering composite read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveringSteeringReadMetrics {
    point_reads: usize,
}

/// Test-only point-read observations for one ready-steering composite read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadySteeringReadMetrics {
    point_reads: usize,
}

/// Test-only logical replay observations and bounded dependency-state high-water marks.
///
/// Cursor page residency is owned and evidenced by the caller's `beryl_stream::PagePool`; these
/// counters only observe the valid bytes written into those pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryResidencyMetrics {
    max_resident_turns: usize,
    max_resident_items: usize,
    turn_item_read_attempts: usize,
    cursor_page_count: usize,
    max_cursor_page_bytes: usize,
}

impl RecoveryResidencyMetrics {
    #[must_use]
    pub const fn max_resident_turns(self) -> usize {
        self.max_resident_turns
    }

    #[must_use]
    pub const fn max_resident_items(self) -> usize {
        self.max_resident_items
    }

    #[must_use]
    pub const fn turn_item_read_attempts(self) -> usize {
        self.turn_item_read_attempts
    }

    #[must_use]
    pub const fn cursor_page_count(self) -> usize {
        self.cursor_page_count
    }

    #[must_use]
    pub const fn max_cursor_page_bytes(self) -> usize {
        self.max_cursor_page_bytes
    }
}

impl CurrentBindingReadMetrics {
    #[must_use]
    pub const fn first_head_reads(self) -> usize {
        self.first_head_reads
    }

    #[must_use]
    pub const fn binding_reads(self) -> usize {
        self.binding_reads
    }

    #[must_use]
    pub const fn second_head_reads(self) -> usize {
        self.second_head_reads
    }
}

impl DeliveringSteeringReadMetrics {
    /// Returns the exact number of constituent point reads since the last reset.
    #[must_use]
    pub const fn point_reads(self) -> usize {
        self.point_reads
    }
}

impl ReadySteeringReadMetrics {
    /// Returns the exact number of constituent point reads since the last reset.
    #[must_use]
    pub const fn point_reads(self) -> usize {
        self.point_reads
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

/// Clears the test-only current-binding component observations.
pub fn reset_current_binding_read_metrics() {
    CURRENT_BINDING_FIRST_HEAD_READS.store(0, Ordering::Relaxed);
    CURRENT_BINDING_RECORD_READS.store(0, Ordering::Relaxed);
    CURRENT_BINDING_SECOND_HEAD_READS.store(0, Ordering::Relaxed);
}

/// Returns components observed by the latest successful current-binding stability read.
#[must_use]
pub fn current_binding_read_metrics() -> CurrentBindingReadMetrics {
    CurrentBindingReadMetrics {
        first_head_reads: CURRENT_BINDING_FIRST_HEAD_READS.load(Ordering::Relaxed),
        binding_reads: CURRENT_BINDING_RECORD_READS.load(Ordering::Relaxed),
        second_head_reads: CURRENT_BINDING_SECOND_HEAD_READS.load(Ordering::Relaxed),
    }
}

/// Clears the test-only delivering-steering constituent read count.
pub fn reset_delivering_steering_read_metrics() {
    DELIVERING_STEERING_POINT_READS.store(0, Ordering::Relaxed);
}

/// Returns point reads observed through the delivering-steering boundary.
#[must_use]
pub fn delivering_steering_read_metrics() -> DeliveringSteeringReadMetrics {
    DeliveringSteeringReadMetrics {
        point_reads: DELIVERING_STEERING_POINT_READS.load(Ordering::Relaxed),
    }
}

/// Clears the test-only ready-steering constituent read count.
pub fn reset_ready_steering_read_metrics() {
    READY_STEERING_POINT_READS.store(0, Ordering::Relaxed);
}

/// Returns point reads observed through the ready-steering boundary.
#[must_use]
pub fn ready_steering_read_metrics() -> ReadySteeringReadMetrics {
    ReadySteeringReadMetrics {
        point_reads: READY_STEERING_POINT_READS.load(Ordering::Relaxed),
    }
}

pub fn reset_syndic_point_read_count() {
    SYNDIC_POINT_READS.store(0, Ordering::Relaxed);
}

#[must_use]
pub fn syndic_point_read_count() -> usize {
    SYNDIC_POINT_READS.load(Ordering::Relaxed)
}

/// Clears the test-only recovery replay observations.
pub fn reset_recovery_residency_metrics() {
    RECOVERY_MAX_RESIDENT_TURNS.store(0, Ordering::Relaxed);
    RECOVERY_MAX_RESIDENT_ITEMS.store(0, Ordering::Relaxed);
    RECOVERY_TURN_ITEM_READ_ATTEMPTS.store(0, Ordering::Relaxed);
    RECOVERY_CURSOR_PAGE_COUNT.store(0, Ordering::Relaxed);
    RECOVERY_MAX_CURSOR_PAGE_BYTES.store(0, Ordering::Relaxed);
}

/// Returns logical replay observations and bounded dependency-state high-water marks.
#[must_use]
pub fn recovery_residency_metrics() -> RecoveryResidencyMetrics {
    RecoveryResidencyMetrics {
        max_resident_turns: RECOVERY_MAX_RESIDENT_TURNS.load(Ordering::Relaxed),
        max_resident_items: RECOVERY_MAX_RESIDENT_ITEMS.load(Ordering::Relaxed),
        turn_item_read_attempts: RECOVERY_TURN_ITEM_READ_ATTEMPTS.load(Ordering::Relaxed),
        cursor_page_count: RECOVERY_CURSOR_PAGE_COUNT.load(Ordering::Relaxed),
        max_cursor_page_bytes: RECOVERY_MAX_CURSOR_PAGE_BYTES.load(Ordering::Relaxed),
    }
}

pub(crate) fn record_recovery_resident_state(turns: usize, items: usize) {
    RECOVERY_MAX_RESIDENT_TURNS.fetch_max(turns, Ordering::Relaxed);
    RECOVERY_MAX_RESIDENT_ITEMS.fetch_max(items, Ordering::Relaxed);
}

pub(crate) fn record_recovery_turn_item_read_attempt() {
    RECOVERY_TURN_ITEM_READ_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_recovery_cursor_page(bytes: usize) {
    RECOVERY_CURSOR_PAGE_COUNT.fetch_add(1, Ordering::Relaxed);
    RECOVERY_MAX_CURSOR_PAGE_BYTES.fetch_max(bytes, Ordering::Relaxed);
}

pub(crate) fn record_validation_page(family: &'static str, items: usize, stored_bytes: usize) {
    PAGE_COUNT.fetch_add(1, Ordering::Relaxed);
    if family == "turns" {
        TURN_PAGE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    MAX_PAGE_ITEMS.fetch_max(items, Ordering::Relaxed);
    MAX_PAGE_STORED_BYTES.fetch_max(stored_bytes, Ordering::Relaxed);
}

pub(crate) fn record_current_binding_read() {
    CURRENT_BINDING_FIRST_HEAD_READS.store(1, Ordering::Relaxed);
    CURRENT_BINDING_RECORD_READS.store(1, Ordering::Relaxed);
    CURRENT_BINDING_SECOND_HEAD_READS.store(1, Ordering::Relaxed);
}

pub(crate) fn record_delivering_steering_point_read() {
    DELIVERING_STEERING_POINT_READS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_ready_steering_point_read() {
    READY_STEERING_POINT_READS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_syndic_point_read() {
    SYNDIC_POINT_READS.fetch_add(1, Ordering::Relaxed);
}
