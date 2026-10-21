use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use beryl_model::AssetId;

use super::{ImagePreparationError as Error, MediaResourceLimits, zeroed_bytes};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct RenditionIdentity {
    pub(crate) attachment: u64,
    pub(crate) asset: AssetId,
    pub(crate) extent: [u32; 2],
}

impl RenditionIdentity {
    pub(crate) fn native_key(self) -> gpui::ImageRenditionKey {
        let mut bytes = [0; 64];
        bytes[..8].copy_from_slice(&self.attachment.to_be_bytes());
        bytes[8..40].copy_from_slice(&self.asset.digest());
        bytes[40..48].copy_from_slice(&self.asset.length().get().to_be_bytes());
        bytes[48..52].copy_from_slice(&self.extent[0].to_be_bytes());
        bytes[52..56].copy_from_slice(&self.extent[1].to_be_bytes());
        bytes[56..60].copy_from_slice(&1u32.to_be_bytes());
        bytes[60] = self.asset.version() as u8;
        bytes[61] = 1;
        gpui::ImageRenditionKey(bytes)
    }

    pub(crate) fn output_bytes(self) -> Result<usize, Error> {
        (self.extent[0] as usize)
            .checked_mul(self.extent[1] as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes > 0)
            .ok_or(Error::Capacity)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CpuSurfaceDiagnostics {
    pub(crate) entries: usize,
    pub(crate) bytes: usize,
    pub(crate) evictions: u64,
}

struct SurfaceLedger {
    limits: MediaResourceLimits,
    state: Mutex<CpuSurfaceDiagnostics>,
}

pub(crate) struct CpuSurfaceCache {
    ledger: Arc<SurfaceLedger>,
    cache: Mutex<CacheState>,
}

#[derive(Default)]
struct CacheState {
    clock: u64,
    entries: BTreeMap<RenditionIdentity, CacheEntry>,
}

struct CacheEntry {
    used: u64,
    surface: Arc<CpuSurface>,
}

pub(crate) struct CpuSurface {
    identity: RenditionIdentity,
    pixels: Vec<u8>,
    _reservation: SurfaceReservation,
}

pub(crate) struct CpuSurfaceBuffer {
    identity: RenditionIdentity,
    pixels: Vec<u8>,
    reservation: SurfaceReservation,
}

struct SurfaceReservation {
    ledger: Arc<SurfaceLedger>,
    bytes: usize,
}

impl CpuSurfaceCache {
    pub(crate) fn new(limits: MediaResourceLimits) -> Self {
        Self {
            ledger: Arc::new(SurfaceLedger {
                limits,
                state: Mutex::new(CpuSurfaceDiagnostics::default()),
            }),
            cache: Mutex::new(CacheState::default()),
        }
    }

    pub(crate) fn get(
        &self,
        identity: RenditionIdentity,
    ) -> Result<Option<Arc<CpuSurface>>, Error> {
        let mut cache = self.cache.lock().map_err(|_| Error::Capacity)?;
        cache.clock = cache.clock.checked_add(1).ok_or(Error::Capacity)?;
        let used = cache.clock;
        Ok(cache.entries.get_mut(&identity).map(|entry| {
            entry.used = used;
            entry.surface.clone()
        }))
    }

    pub(crate) fn reserve(&self, identity: RenditionIdentity) -> Result<CpuSurfaceBuffer, Error> {
        let bytes = identity.output_bytes()?;
        let config = self.ledger.limits.configuration();
        if bytes > config.surface_max_bytes || bytes > config.cpu_surface_bytes {
            return Err(Error::Capacity);
        }
        let mut cache = self.cache.lock().map_err(|_| Error::Capacity)?;
        loop {
            let mut ledger = self.ledger.state.lock().map_err(|_| Error::Capacity)?;
            if ledger.entries < config.cpu_surface_entries
                && ledger
                    .bytes
                    .checked_add(bytes)
                    .is_some_and(|total| total <= config.cpu_surface_bytes)
            {
                ledger.entries += 1;
                ledger.bytes += bytes;
                break;
            }
            drop(ledger);
            let victim = cache
                .entries
                .iter()
                .filter(|(_, entry)| Arc::strong_count(&entry.surface) == 1)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| *key)
                .ok_or(Error::Capacity)?;
            cache.entries.remove(&victim);
            let mut ledger = self.ledger.state.lock().map_err(|_| Error::Capacity)?;
            ledger.evictions = ledger.evictions.saturating_add(1);
        }
        drop(cache);
        let reservation = SurfaceReservation {
            ledger: self.ledger.clone(),
            bytes,
        };
        let pixels = zeroed_bytes(bytes)?;
        if pixels.capacity() != bytes {
            return Err(Error::Capacity);
        }
        Ok(CpuSurfaceBuffer {
            identity,
            pixels,
            reservation,
        })
    }

    pub(crate) fn publish(&self, buffer: CpuSurfaceBuffer) -> Result<Arc<CpuSurface>, Error> {
        let mut cache = self.cache.lock().map_err(|_| Error::Capacity)?;
        if cache.entries.contains_key(&buffer.identity) {
            return Err(Error::Capacity);
        }
        cache.clock = cache.clock.checked_add(1).ok_or(Error::Capacity)?;
        let used = cache.clock;
        let surface = Arc::new(CpuSurface {
            identity: buffer.identity,
            pixels: buffer.pixels,
            _reservation: buffer.reservation,
        });
        cache.entries.insert(
            surface.identity,
            CacheEntry {
                used,
                surface: surface.clone(),
            },
        );
        Ok(surface)
    }

    pub(crate) fn retire_attachment(&self, attachment: u64) {
        if let Ok(mut cache) = self.cache.lock() {
            cache
                .entries
                .retain(|identity, _| identity.attachment != attachment);
        }
    }

    pub(crate) fn diagnostics(&self) -> Result<CpuSurfaceDiagnostics, Error> {
        self.ledger
            .state
            .lock()
            .map(|state| *state)
            .map_err(|_| Error::Capacity)
    }
}

impl CpuSurface {
    pub(crate) fn identity(&self) -> RenditionIdentity {
        self.identity
    }
    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl CpuSurfaceBuffer {
    pub(crate) fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }
}

impl Drop for SurfaceReservation {
    fn drop(&mut self) {
        if let Ok(mut ledger) = self.ledger.state.lock() {
            ledger.entries -= 1;
            ledger.bytes -= self.bytes;
        }
    }
}
