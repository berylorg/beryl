use std::sync::{Arc, Mutex};

/// Exact number of operation-scoped reservations retained by one home.
pub(crate) const RECONCILIATION_SCOPE_CAPACITY: usize = 1_024;

#[derive(Debug)]
pub(crate) enum ReconciliationReservationError {
    DescriptorTooLarge { requested: usize, limit: usize },
    Capacity,
}

#[derive(Debug)]
struct LedgerState {
    occupied: Box<[bool]>,
    reserved_bytes: usize,
}

#[derive(Debug)]
struct LedgerInner {
    state: Mutex<LedgerState>,
    descriptor_byte_limit: usize,
    reserved_byte_limit: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ReconciliationLedger {
    inner: Arc<LedgerInner>,
}

impl ReconciliationLedger {
    pub(crate) fn new(descriptor_byte_limit: usize, reserved_byte_limit: usize) -> Self {
        assert!(
            descriptor_byte_limit > 0,
            "reconciliation descriptor byte limit must be nonzero"
        );
        assert!(
            reserved_byte_limit >= descriptor_byte_limit,
            "aggregate reconciliation byte limit must admit one descriptor"
        );
        Self {
            inner: Arc::new(LedgerInner {
                state: Mutex::new(LedgerState {
                    occupied: vec![false; RECONCILIATION_SCOPE_CAPACITY].into_boxed_slice(),
                    reserved_bytes: 0,
                }),
                descriptor_byte_limit,
                reserved_byte_limit,
            }),
        }
    }

    pub(crate) fn reserve(
        &self,
        descriptor_bytes: usize,
    ) -> Result<ReconciliationSlot, ReconciliationReservationError> {
        if descriptor_bytes > self.inner.descriptor_byte_limit {
            return Err(ReconciliationReservationError::DescriptorTooLarge {
                requested: descriptor_bytes,
                limit: self.inner.descriptor_byte_limit,
            });
        }

        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(next_reserved_bytes) = state.reserved_bytes.checked_add(descriptor_bytes) else {
            return Err(ReconciliationReservationError::Capacity);
        };
        if next_reserved_bytes > self.inner.reserved_byte_limit {
            return Err(ReconciliationReservationError::Capacity);
        }
        let Some(index) = state.occupied.iter().position(|occupied| !occupied) else {
            return Err(ReconciliationReservationError::Capacity);
        };
        state.occupied[index] = true;
        state.reserved_bytes = next_reserved_bytes;
        Ok(ReconciliationSlot {
            inner: Arc::clone(&self.inner),
            index,
            charged_bytes: descriptor_bytes,
        })
    }
}

/// One RAII reservation transferred only into an indeterminate descriptor.
pub(crate) struct ReconciliationSlot {
    inner: Arc<LedgerInner>,
    index: usize,
    charged_bytes: usize,
}

impl std::fmt::Debug for ReconciliationSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReconciliationSlot")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl Drop for ReconciliationSlot {
    fn drop(&mut self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert!(state.occupied[self.index]);
        state.occupied[self.index] = false;
        state.reserved_bytes = state
            .reserved_bytes
            .checked_sub(self.charged_bytes)
            .expect("reconciliation reserved-byte accounting underflow");
    }
}
