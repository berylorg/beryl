use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
};

#[derive(Debug, thiserror::Error)]
pub enum TemporaryReadError {
    #[error("invalid temporary read limits")]
    InvalidLimits,
    #[error("temporary read capacity unavailable")]
    Capacity,
    #[error("temporary read range or completion is invalid")]
    Range,
    #[error("temporary read owner is poisoned")]
    Poisoned,
    #[error("temporary read {operation} failed: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct TemporaryReadPoolLimits {
    max_bytes: u64,
    max_sources: usize,
    max_page_bytes: usize,
}
impl TemporaryReadPoolLimits {
    pub fn new(
        max_bytes: u64,
        max_sources: usize,
        max_page_bytes: usize,
    ) -> Result<Self, TemporaryReadError> {
        if max_bytes == 0
            || !(1..=256).contains(&max_sources)
            || max_page_bytes == 0
            || u64::try_from(max_page_bytes).is_err()
        {
            return Err(TemporaryReadError::InvalidLimits);
        }
        Ok(Self {
            max_bytes,
            max_sources,
            max_page_bytes,
        })
    }
    pub const fn max_bytes(self) -> u64 {
        self.max_bytes
    }
    pub const fn max_sources(self) -> usize {
        self.max_sources
    }
    pub const fn max_page_bytes(self) -> usize {
        self.max_page_bytes
    }
}
impl Default for TemporaryReadPoolLimits {
    fn default() -> Self {
        Self {
            max_bytes: 1 << 30,
            max_sources: 256,
            max_page_bytes: 65_536,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TemporaryReadPoolUsage {
    pub reserved_bytes: u64,
    pub sources: usize,
}
#[derive(Clone)]
pub struct TemporaryReadPool {
    inner: Arc<Pool>,
}
struct Pool {
    limits: TemporaryReadPoolLimits,
    usage: Mutex<TemporaryReadPoolUsage>,
    #[cfg(feature = "test-faults")]
    fault: Mutex<Option<TemporaryReadFault>>,
}
#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporaryReadFault {
    Create,
    Write,
    Read,
    ShortRead,
}
impl Pool {
    #[cfg(feature = "test-faults")]
    fn fault(&self, point: TemporaryReadFault) -> Result<(), TemporaryReadError> {
        let mut fault = self
            .fault
            .lock()
            .map_err(|_| TemporaryReadError::Poisoned)?;
        if *fault == Some(point) {
            *fault = None;
            return Err(TemporaryReadError::Io {
                operation: "injected",
                source: io::Error::new(
                    if point == TemporaryReadFault::ShortRead {
                        io::ErrorKind::UnexpectedEof
                    } else {
                        io::ErrorKind::Other
                    },
                    "temporary read fault",
                ),
            });
        }
        Ok(())
    }
}
struct Reservation {
    pool: Arc<Pool>,
    bytes: u64,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let mut usage = self.pool.usage.lock().unwrap_or_else(|e| e.into_inner());
        usage.reserved_bytes -= self.bytes;
        usage.sources -= 1;
    }
}
impl TemporaryReadPool {
    pub fn new(limits: TemporaryReadPoolLimits) -> Self {
        Self {
            inner: Arc::new(Pool {
                limits,
                usage: Mutex::new(TemporaryReadPoolUsage::default()),
                #[cfg(feature = "test-faults")]
                fault: Mutex::new(None),
            }),
        }
    }
    #[cfg(feature = "test-faults")]
    pub fn test_fail_next(&self, fault: TemporaryReadFault) {
        *self.inner.fault.lock().expect("temporary fault owner") = Some(fault);
    }
    pub fn limits(&self) -> TemporaryReadPoolLimits {
        self.inner.limits
    }
    pub fn usage(&self) -> Result<TemporaryReadPoolUsage, TemporaryReadError> {
        self.inner
            .usage
            .lock()
            .map(|u| *u)
            .map_err(|_| TemporaryReadError::Poisoned)
    }
    pub fn begin(&self, bytes: u64) -> Result<TemporaryReadWriter, TemporaryReadError> {
        let reservation = {
            let mut usage = self
                .inner
                .usage
                .lock()
                .map_err(|_| TemporaryReadError::Poisoned)?;
            let total = usage
                .reserved_bytes
                .checked_add(bytes)
                .ok_or(TemporaryReadError::Capacity)?;
            let count = usage
                .sources
                .checked_add(1)
                .ok_or(TemporaryReadError::Capacity)?;
            if total > self.inner.limits.max_bytes || count > self.inner.limits.max_sources {
                return Err(TemporaryReadError::Capacity);
            }
            usage.reserved_bytes = total;
            usage.sources = count;
            Reservation {
                pool: self.inner.clone(),
                bytes,
            }
        };
        #[cfg(feature = "test-faults")]
        self.inner.fault(TemporaryReadFault::Create)?;
        let file = tempfile::tempfile().map_err(|source| TemporaryReadError::Io {
            operation: "create",
            source,
        })?;
        Ok(TemporaryReadWriter {
            file,
            reservation,
            written: 0,
            failed: false,
        })
    }
}

pub struct TemporaryReadWriter {
    file: File,
    reservation: Reservation,
    written: u64,
    failed: bool,
}
impl TemporaryReadWriter {
    pub fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<(), TemporaryReadError> {
        let end = offset
            .checked_add(u64::try_from(bytes.len()).map_err(|_| TemporaryReadError::Range)?)
            .ok_or(TemporaryReadError::Range)?;
        if self.failed
            || offset != self.written
            || end > self.reservation.bytes
            || bytes.len() > self.reservation.pool.limits.max_page_bytes
        {
            return Err(TemporaryReadError::Range);
        }
        #[cfg(feature = "test-faults")]
        if let Err(error) = self.reservation.pool.fault(TemporaryReadFault::Write) {
            self.failed = true;
            return Err(error);
        }
        if let Err(source) = self.file.write_all(bytes) {
            self.failed = true;
            return Err(TemporaryReadError::Io {
                operation: "write",
                source,
            });
        }
        self.written = end;
        Ok(())
    }
    pub fn seal(self) -> Result<TemporaryReadReader, TemporaryReadError> {
        if self.failed || self.written != self.reservation.bytes {
            return Err(TemporaryReadError::Range);
        }
        let length = self
            .file
            .metadata()
            .map_err(|source| TemporaryReadError::Io {
                operation: "seal",
                source,
            })?
            .len();
        if length != self.written {
            return Err(TemporaryReadError::Range);
        }
        Ok(TemporaryReadReader {
            inner: Arc::new(Sealed {
                file: Mutex::new(self.file),
                reservation: self.reservation,
            }),
        })
    }
}
struct Sealed {
    file: Mutex<File>,
    reservation: Reservation,
}
#[derive(Clone)]
pub struct TemporaryReadReader {
    inner: Arc<Sealed>,
}
impl TemporaryReadReader {
    pub fn len(&self) -> u64 {
        self.inner.reservation.bytes
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>, TemporaryReadError> {
        let end = offset
            .checked_add(u64::try_from(length).map_err(|_| TemporaryReadError::Range)?)
            .ok_or(TemporaryReadError::Range)?;
        if end > self.len() || length > self.inner.reservation.pool.limits.max_page_bytes {
            return Err(TemporaryReadError::Range);
        }
        #[cfg(feature = "test-faults")]
        self.inner
            .reservation
            .pool
            .fault(TemporaryReadFault::Read)?;
        #[cfg(feature = "test-faults")]
        self.inner
            .reservation
            .pool
            .fault(TemporaryReadFault::ShortRead)?;
        let mut file = self
            .inner
            .file
            .lock()
            .map_err(|_| TemporaryReadError::Poisoned)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|source| TemporaryReadError::Io {
                operation: "seek",
                source,
            })?;
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes)
            .map_err(|source| TemporaryReadError::Io {
                operation: "read",
                source,
            })?;
        Ok(bytes)
    }
}
