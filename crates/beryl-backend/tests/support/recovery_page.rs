use std::num::NonZeroUsize;

use beryl_backend::THREAD_INJECTION_MAX_PAGE_BYTES;
use beryl_stream::{PageLease, PagePool};

pub fn recovery_page_pool() -> PagePool {
    recovery_page_pool_with_capacity(THREAD_INJECTION_MAX_PAGE_BYTES)
}

pub fn recovery_page_pool_with_capacity(page_capacity: usize) -> PagePool {
    let page_capacity = NonZeroUsize::new(page_capacity).unwrap();
    PagePool::new(page_capacity, nonzero_usize(1)).unwrap()
}

pub fn lease_with_bytes(bytes: &[u8]) -> PageLease {
    let pool = recovery_page_pool();
    lease_from_pool(&pool, bytes)
}

pub fn lease_from_pool(pool: &PagePool, bytes: &[u8]) -> PageLease {
    let mut lease = pool.try_lease().unwrap();
    lease.buffer_mut()[..bytes.len()].copy_from_slice(bytes);
    lease.set_len(bytes.len()).unwrap();
    lease
}

pub fn lease_filled(byte: u8, len: usize) -> PageLease {
    let pool = recovery_page_pool();
    let mut lease = pool.try_lease().unwrap();
    lease.buffer_mut()[..len].fill(byte);
    lease.set_len(len).unwrap();
    lease
}

fn nonzero_usize(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
