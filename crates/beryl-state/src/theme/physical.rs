use std::{
    io::{self, BufRead, Read},
    num::{NonZeroU64, NonZeroUsize},
    sync::{Arc, Mutex},
};

use beryl_home_store::{
    HomeStore, StableThemeFileId, StableThemeFileIdError, ThemeFileIdentity, ThemeFileSelector,
    ThemeOperationLimits, ThemeOperationLimitsError, ThemeRepositoryError, ThemeRepositorySnapshot,
};

use super::{
    InstalledThemeId, THEME_DOCUMENT_MAX_BYTES, THEME_DOCUMENT_RANGE_MAX_BYTES,
    ThemeDocumentDigest, ThemeDocumentIdentity, ThemeIdentityError,
};

const PHYSICAL_IO_BUFFER_BYTES: usize = 64 * 1024;
const PHYSICAL_MAX_STAGED_FILES: usize = 2;
const PHYSICAL_MAX_EVIDENCE_FILES: usize = 4;
const PHYSICAL_MAX_EVIDENCE_BYTES: usize = 512;

/// Checked physical-operation bounds selected by the typed theme service.
///
/// Installed documents use [`Self::document`]. Manifest callers must use
/// [`Self::manifest`] with an explicit nonzero per-operation source ceiling;
/// the logical manifest is streamed and has no package-wide whole-file limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PhysicalThemeLimits {
    operations: ThemeOperationLimits,
    range_bytes: NonZeroUsize,
}

impl PhysicalThemeLimits {
    pub(crate) fn new(
        max_source_bytes: NonZeroU64,
        range_bytes: NonZeroUsize,
    ) -> Result<Self, PhysicalThemeLimitsError> {
        if range_bytes.get() > THEME_DOCUMENT_RANGE_MAX_BYTES {
            return Err(PhysicalThemeLimitsError::RangeTooLarge);
        }
        let operations = ThemeOperationLimits::new(
            max_source_bytes.get(),
            NonZeroUsize::new(PHYSICAL_IO_BUFFER_BYTES)
                .ok_or(PhysicalThemeLimitsError::InvalidOperationLimits)?,
            NonZeroUsize::new(PHYSICAL_MAX_STAGED_FILES)
                .ok_or(PhysicalThemeLimitsError::InvalidOperationLimits)?,
            NonZeroUsize::new(PHYSICAL_MAX_EVIDENCE_FILES)
                .ok_or(PhysicalThemeLimitsError::InvalidOperationLimits)?,
            NonZeroUsize::new(PHYSICAL_MAX_EVIDENCE_BYTES)
                .ok_or(PhysicalThemeLimitsError::InvalidOperationLimits)?,
        )
        .map_err(PhysicalThemeLimitsError::OperationLimits)?;
        Ok(Self {
            operations,
            range_bytes,
        })
    }

    pub(crate) fn document() -> Result<Self, PhysicalThemeLimitsError> {
        let max_source_bytes = NonZeroU64::new(THEME_DOCUMENT_MAX_BYTES as u64)
            .ok_or(PhysicalThemeLimitsError::InvalidOperationLimits)?;
        let range_bytes = NonZeroUsize::new(THEME_DOCUMENT_RANGE_MAX_BYTES)
            .ok_or(PhysicalThemeLimitsError::InvalidOperationLimits)?;
        Self::new(max_source_bytes, range_bytes)
    }

    pub(crate) fn manifest(max_source_bytes: NonZeroU64) -> Result<Self, PhysicalThemeLimitsError> {
        let range_bytes = NonZeroUsize::new(THEME_DOCUMENT_RANGE_MAX_BYTES)
            .ok_or(PhysicalThemeLimitsError::InvalidOperationLimits)?;
        Self::new(max_source_bytes, range_bytes)
    }

    pub(crate) const fn operations(self) -> ThemeOperationLimits {
        self.operations
    }

    pub(crate) const fn range_bytes(self) -> NonZeroUsize {
        self.range_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PhysicalThemeLimitsError {
    RangeTooLarge,
    InvalidOperationLimits,
    OperationLimits(ThemeOperationLimitsError),
}

pub(crate) fn stable_file_id(
    id: &InstalledThemeId,
) -> Result<StableThemeFileId, StableThemeFileIdError> {
    StableThemeFileId::new(id.as_str().to_owned())
}

pub(crate) fn installed_theme_id(
    id: &StableThemeFileId,
) -> Result<InstalledThemeId, ThemeIdentityError> {
    InstalledThemeId::new(id.as_str())
}

pub(crate) const fn physical_file_identity(
    length: u64,
    digest: ThemeDocumentDigest,
) -> ThemeFileIdentity {
    ThemeFileIdentity::new(length, *digest.as_bytes())
}

pub(crate) fn physical_document_identity(identity: &ThemeDocumentIdentity) -> ThemeFileIdentity {
    physical_file_identity(identity.byte_length(), identity.digest())
}

pub(crate) const fn document_identity_parts(
    identity: ThemeFileIdentity,
) -> (u64, ThemeDocumentDigest) {
    (
        identity.length(),
        ThemeDocumentDigest::from_bytes(identity.sha256()),
    )
}

pub(crate) fn repository_snapshot(
    store: &HomeStore,
    limits: PhysicalThemeLimits,
) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
    store.theme_repository_snapshot(limits.operations())
}

pub(crate) fn observe_file(
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    selector: &ThemeFileSelector,
    limits: PhysicalThemeLimits,
) -> Result<ThemeFileIdentity, ThemeRepositoryError> {
    store.observe_theme_file(snapshot, selector, limits.operations())
}

/// Forward-only range reader bound to one store snapshot and exact file identity.
///
/// At most one range of `limits.range_bytes()` is retained. Every refill asks
/// `beryl-home-store` to revalidate the complete exact identity against the same
/// snapshot before returning bytes.
pub(crate) struct PhysicalThemeReader<'store> {
    store: &'store HomeStore,
    snapshot: ThemeRepositorySnapshot,
    selector: ThemeFileSelector,
    expected: ThemeFileIdentity,
    limits: PhysicalThemeLimits,
    next_offset: u64,
    buffer: Vec<u8>,
    buffer_offset: usize,
    eof: bool,
    errors: PhysicalThemeReadErrors,
}

#[derive(Clone)]
pub(crate) struct PhysicalThemeReadErrors(Arc<Mutex<Option<ThemeRepositoryError>>>);

impl PhysicalThemeReadErrors {
    pub(crate) fn take(&self) -> Option<ThemeRepositoryError> {
        self.0.lock().ok()?.take()
    }
}

impl<'store> PhysicalThemeReader<'store> {
    pub(crate) fn new(
        store: &'store HomeStore,
        snapshot: &ThemeRepositorySnapshot,
        selector: ThemeFileSelector,
        expected: ThemeFileIdentity,
        limits: PhysicalThemeLimits,
    ) -> Result<Self, ThemeRepositoryError> {
        if expected.length() > limits.operations().max_source_bytes() {
            return Err(ThemeRepositoryError::LimitExceeded);
        }
        Ok(Self {
            store,
            snapshot: snapshot.clone(),
            selector,
            expected,
            limits,
            next_offset: 0,
            buffer: Vec::new(),
            buffer_offset: 0,
            eof: false,
            errors: PhysicalThemeReadErrors(Arc::new(Mutex::new(None))),
        })
    }

    pub(crate) fn errors(&self) -> PhysicalThemeReadErrors {
        self.errors.clone()
    }

    fn refill(&mut self) -> Result<(), ThemeRepositoryError> {
        if self.buffer_offset < self.buffer.len() || self.eof {
            return Ok(());
        }
        let range = self.store.read_theme_file_range(
            &self.snapshot,
            &self.selector,
            self.expected,
            self.next_offset,
            self.limits.range_bytes(),
            self.limits.operations(),
        )?;
        let byte_count =
            u64::try_from(range.bytes().len()).map_err(|_| ThemeRepositoryError::LimitExceeded)?;
        self.next_offset = self
            .next_offset
            .checked_add(byte_count)
            .ok_or(ThemeRepositoryError::LimitExceeded)?;
        self.buffer.clear();
        self.buffer.extend_from_slice(range.bytes());
        self.buffer_offset = 0;
        self.eof = range.eof();
        Ok(())
    }
}

impl Read for PhysicalThemeReader<'_> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if destination.is_empty() {
            return Ok(0);
        }
        let available = self.fill_buf()?;
        let count = available.len().min(destination.len());
        destination[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl BufRead for PhysicalThemeReader<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if let Err(source) = self.refill() {
            if let Ok(mut error) = self.errors.0.lock() {
                *error = Some(source);
            }
            return Err(io::Error::other("typed theme repository range read failed"));
        }
        Ok(&self.buffer[self.buffer_offset..])
    }

    fn consume(&mut self, amount: usize) {
        self.buffer_offset = self
            .buffer_offset
            .saturating_add(amount)
            .min(self.buffer.len());
    }
}
