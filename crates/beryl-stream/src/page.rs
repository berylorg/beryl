use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

use thiserror::Error;

pub struct PagePool {
    inner: Arc<PagePoolInner>,
}

impl Clone for PagePool {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct PagePoolInner {
    state: Mutex<PagePoolState>,
    page_capacity: usize,
    page_count: usize,
}

struct PagePoolState {
    available: Vec<Box<[u8]>>,
    leased: usize,
    high_water: usize,
    total_leases: u64,
    exhausted: u64,
    next_generation: u64,
}

impl PagePool {
    pub fn new(
        page_capacity: NonZeroUsize,
        page_count: NonZeroUsize,
    ) -> Result<Self, PagePoolError> {
        page_capacity
            .get()
            .checked_mul(page_count.get())
            .ok_or(PagePoolError::SizeOverflow)?;
        let mut pages = Vec::new();
        pages
            .try_reserve_exact(page_count.get())
            .map_err(|_| PagePoolError::AllocationFailed)?;
        for _ in 0..page_count.get() {
            let mut page = Vec::new();
            page.try_reserve_exact(page_capacity.get())
                .map_err(|_| PagePoolError::AllocationFailed)?;
            page.resize(page_capacity.get(), 0);
            pages.push(page.into_boxed_slice());
        }
        Ok(Self {
            inner: Arc::new(PagePoolInner {
                state: Mutex::new(PagePoolState {
                    available: pages,
                    leased: 0,
                    high_water: 0,
                    total_leases: 0,
                    exhausted: 0,
                    next_generation: 1,
                }),
                page_capacity: page_capacity.get(),
                page_count: page_count.get(),
            }),
        })
    }

    pub fn try_lease(&self) -> Result<PageLease, PagePoolError> {
        let mut state = self.inner.lock();
        if state.available.is_empty() {
            state.exhausted = state.exhausted.saturating_add(1);
            return Err(PagePoolError::Exhausted);
        }
        let generation = match take_generation(&mut state.next_generation) {
            Some(generation) => generation,
            None => {
                state.exhausted = state.exhausted.saturating_add(1);
                return Err(PagePoolError::GenerationExhausted);
            }
        };
        let page = state.available.pop().expect("availability checked");
        state.leased += 1;
        state.high_water = state.high_water.max(state.leased);
        state.total_leases = state.total_leases.saturating_add(1);
        drop(state);
        Ok(PageLease {
            pool: Arc::clone(&self.inner),
            page: Some(page),
            valid_len: 0,
            generation,
        })
    }

    pub fn diagnostics(&self) -> PagePoolDiagnostics {
        let state = self.inner.lock();
        PagePoolDiagnostics {
            page_capacity: self.inner.page_capacity,
            page_count: self.inner.page_count,
            available: state.available.len(),
            leased: state.leased,
            high_water: state.high_water,
            total_leases: state.total_leases,
            exhausted: state.exhausted,
        }
    }

    /// Returns a content-free observer that does not keep the pool or its pages resident.
    #[must_use]
    pub fn observer(&self) -> PagePoolObserver {
        PagePoolObserver {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

fn take_generation(next_generation: &mut u64) -> Option<u64> {
    let generation = *next_generation;
    *next_generation = generation.checked_add(1)?;
    Some(generation)
}

impl PagePoolInner {
    fn lock(&self) -> MutexGuard<'_, PagePoolState> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

pub struct PageLease {
    pool: Arc<PagePoolInner>,
    page: Option<Box<[u8]>>,
    valid_len: usize,
    generation: u64,
}

/// Content-free weak diagnostics access for one fixed page pool.
///
/// Once the pool and every outstanding lease are released, [`Self::diagnostics`] returns `None`
/// instead of retaining the page storage solely for observation.
pub struct PagePoolObserver {
    inner: Weak<PagePoolInner>,
}

impl Clone for PagePoolObserver {
    fn clone(&self) -> Self {
        Self {
            inner: Weak::clone(&self.inner),
        }
    }
}

impl PagePoolObserver {
    /// Returns current pool diagnostics, or `None` after complete pool release.
    #[must_use]
    pub fn diagnostics(&self) -> Option<PagePoolDiagnostics> {
        let inner = self.inner.upgrade()?;
        let state = inner.lock();
        Some(PagePoolDiagnostics {
            page_capacity: inner.page_capacity,
            page_count: inner.page_count,
            available: state.available.len(),
            leased: state.leased,
            high_water: state.high_water,
            total_leases: state.total_leases,
            exhausted: state.exhausted,
        })
    }
}

impl PageLease {
    pub fn capacity(&self) -> usize {
        self.pool.page_capacity
    }

    pub const fn len(&self) -> usize {
        self.valid_len
    }

    pub const fn is_empty(&self) -> bool {
        self.valid_len == 0
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.page.as_deref().expect("live lease owns its page")[..self.valid_len]
    }

    pub fn buffer_mut(&mut self) -> &mut [u8] {
        self.page.as_deref_mut().expect("live lease owns its page")
    }

    pub fn set_len(&mut self, valid_len: usize) -> Result<(), PagePoolError> {
        if valid_len > self.capacity() {
            return Err(PagePoolError::InvalidLength {
                requested: valid_len,
                capacity: self.capacity(),
            });
        }
        self.valid_len = valid_len;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.valid_len = 0;
    }
}

impl Drop for PageLease {
    fn drop(&mut self) {
        let Some(mut page) = self.page.take() else {
            return;
        };
        page.fill(0);
        let mut state = self.pool.lock();
        state.leased -= 1;
        state.available.push(page);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PagePoolDiagnostics {
    pub page_capacity: usize,
    pub page_count: usize,
    pub available: usize,
    pub leased: usize,
    pub high_water: usize,
    pub total_leases: u64,
    pub exhausted: u64,
}

#[derive(Debug, Error)]
pub enum PagePoolError {
    #[error("page pool storage size overflowed")]
    SizeOverflow,
    #[error("page pool storage allocation failed")]
    AllocationFailed,
    #[error("page pool is exhausted")]
    Exhausted,
    #[error("page lease generation is exhausted")]
    GenerationExhausted,
    #[error("valid page length {requested} exceeds page capacity {capacity}")]
    InvalidLength { requested: usize, capacity: usize },
}
