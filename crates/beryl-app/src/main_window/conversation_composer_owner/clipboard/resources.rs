use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerPasteResources {
    queue_items: NonZeroUsize,
    sidecar_page_bytes: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MainWindowComposerPasteResourceError {
    #[error("paste admission queue capacity must be positive")]
    ZeroQueueCapacity,
    #[error("paste sidecar page size must be positive")]
    ZeroSidecarPage,
    #[error("paste resource capacity exceeds the addressable allocation domain")]
    CapacityOverflow,
}

impl MainWindowComposerPasteResources {
    pub fn new(
        queue_items: usize,
        sidecar_page_bytes: usize,
    ) -> Result<Self, MainWindowComposerPasteResourceError> {
        let queue_items = NonZeroUsize::new(queue_items)
            .ok_or(MainWindowComposerPasteResourceError::ZeroQueueCapacity)?;
        let sidecar_page_bytes = NonZeroUsize::new(sidecar_page_bytes)
            .ok_or(MainWindowComposerPasteResourceError::ZeroSidecarPage)?;
        if queue_items
            .get()
            .checked_mul(sidecar_page_bytes.get())
            .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return Err(MainWindowComposerPasteResourceError::CapacityOverflow);
        }
        Ok(Self {
            queue_items,
            sidecar_page_bytes,
        })
    }

    pub fn queue_items(self) -> NonZeroUsize {
        self.queue_items
    }
    pub fn sidecar_page_bytes(self) -> NonZeroUsize {
        self.sidecar_page_bytes
    }
}

impl Default for MainWindowComposerPasteResources {
    fn default() -> Self {
        Self::new(1, 4096).expect("valid default paste resources")
    }
}

#[derive(Clone)]
pub(in super::super) struct ComposerPasteQueue {
    occupied: Arc<AtomicUsize>,
    resources: MainWindowComposerPasteResources,
}

impl ComposerPasteQueue {
    const RETIRED: usize = 1usize << (usize::BITS - 1);
    pub(super) fn new(resources: MainWindowComposerPasteResources) -> Self {
        Self {
            occupied: Arc::new(AtomicUsize::new(0)),
            resources,
        }
    }

    pub(in super::super) fn resources(&self) -> MainWindowComposerPasteResources {
        self.resources
    }

    pub(super) fn reserve(&self) -> Option<PasteQueuePermit> {
        self.occupied
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                if used < self.resources.queue_items.get() {
                    used.checked_add(1)
                } else {
                    None
                }
            })
            .ok()?;
        Some(PasteQueuePermit(self.occupied.clone()))
    }

    pub(super) fn retire(&self) {
        self.occupied.fetch_or(Self::RETIRED, Ordering::AcqRel);
    }
}

pub(super) struct PasteQueuePermit(Arc<AtomicUsize>);

impl Drop for PasteQueuePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
