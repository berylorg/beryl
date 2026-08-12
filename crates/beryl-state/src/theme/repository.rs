use std::{error::Error, fmt, num::NonZeroUsize, ops::Range};

use super::{
    InstalledThemeId, ThemeDocumentIdentity, ThemeHomeIdentity, ThemeIdentityError,
    ThemeManifestGeneration, ThemeManifestIdentity,
};

pub const THEME_NAME_MAX_BYTES: usize = 128;
pub const THEME_MANIFEST_PAGE_MAX_ITEMS: usize = 128;
pub const THEME_MANIFEST_PAGE_MAX_DECODED_BYTES: usize = 64 * 1024;
pub const THEME_DOCUMENT_RANGE_MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeName(Box<str>);

impl ThemeName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, ThemeIdentityError> {
        let value = value.as_ref().trim();
        if value.is_empty() || value.len() > THEME_NAME_MAX_BYTES {
            return Err(ThemeIdentityError::InvalidThemeName);
        }
        Ok(Self(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledThemeSummary {
    id: InstalledThemeId,
    name: ThemeName,
    order: u64,
}

/// Exact manifest-bound selection proving one installed membership row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledThemeSelection {
    manifest: ThemeManifestIdentity,
    summary: InstalledThemeSummary,
}

impl InstalledThemeSelection {
    #[must_use]
    pub const fn manifest(&self) -> ThemeManifestIdentity {
        self.manifest
    }
    #[must_use]
    pub const fn summary(&self) -> &InstalledThemeSummary {
        &self.summary
    }
    #[must_use]
    pub const fn id(&self) -> &InstalledThemeId {
        self.summary.id()
    }
}

impl InstalledThemeSummary {
    #[must_use]
    pub fn new(id: InstalledThemeId, name: ThemeName, order: u64) -> Self {
        Self { id, name, order }
    }
    #[must_use]
    pub const fn id(&self) -> &InstalledThemeId {
        &self.id
    }
    #[must_use]
    pub const fn name(&self) -> &ThemeName {
        &self.name
    }
    #[must_use]
    pub const fn order(&self) -> u64 {
        self.order
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeManifestCursor {
    manifest: ThemeManifestIdentity,
    next_order: u64,
}

impl ThemeManifestCursor {
    #[must_use]
    pub const fn first(manifest: ThemeManifestIdentity) -> Self {
        Self {
            manifest,
            next_order: 0,
        }
    }
    #[must_use]
    pub const fn manifest(self) -> ThemeManifestIdentity {
        self.manifest
    }
    #[must_use]
    pub const fn next_order(self) -> u64 {
        self.next_order
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemePageLimits {
    max_items: NonZeroUsize,
    max_decoded_bytes: NonZeroUsize,
}

impl ThemePageLimits {
    pub fn new(
        max_items: NonZeroUsize,
        max_decoded_bytes: NonZeroUsize,
    ) -> Result<Self, ThemePageError> {
        if max_items.get() > THEME_MANIFEST_PAGE_MAX_ITEMS
            || max_decoded_bytes.get() > THEME_MANIFEST_PAGE_MAX_DECODED_BYTES
        {
            return Err(ThemePageError::LimitsTooLarge);
        }
        Ok(Self {
            max_items,
            max_decoded_bytes,
        })
    }
    #[must_use]
    pub const fn max_items(self) -> usize {
        self.max_items.get()
    }
    #[must_use]
    pub const fn max_decoded_bytes(self) -> usize {
        self.max_decoded_bytes.get()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeManifestPage {
    manifest: ThemeManifestIdentity,
    records: Box<[InstalledThemeSummary]>,
    decoded_bytes: usize,
    next: Option<ThemeManifestCursor>,
}

impl ThemeManifestPage {
    pub(crate) fn checked(
        cursor: ThemeManifestCursor,
        records: Vec<InstalledThemeSummary>,
        has_more: bool,
        limits: ThemePageLimits,
    ) -> Result<Self, ThemePageError> {
        if records.len() > limits.max_items() {
            return Err(ThemePageError::LimitExceeded);
        }
        if records.is_empty() && has_more {
            return Err(ThemePageError::EmptyContinuation);
        }
        let manifest = cursor.manifest();
        let mut expected_order = cursor.next_order();
        for record in &records {
            if record.order() != expected_order {
                return Err(ThemePageError::NonContiguousOrder);
            }
            expected_order = expected_order
                .checked_add(1)
                .ok_or(ThemePageError::OrderExhausted)?;
        }
        let decoded_bytes = records
            .iter()
            .try_fold(0usize, |total, record| {
                total
                    .checked_add(record.id().as_str().len())
                    .and_then(|value| value.checked_add(record.name().as_str().len()))
                    .and_then(|value| value.checked_add(std::mem::size_of::<u64>()))
            })
            .ok_or(ThemePageError::DecodedBytesOverflow)?;
        if decoded_bytes > limits.max_decoded_bytes() {
            return Err(ThemePageError::LimitExceeded);
        }
        let next = has_more.then_some(ThemeManifestCursor {
            manifest,
            next_order: expected_order,
        });
        Ok(Self {
            manifest,
            records: records.into_boxed_slice(),
            decoded_bytes,
            next,
        })
    }
    #[must_use]
    pub const fn manifest(&self) -> ThemeManifestIdentity {
        self.manifest
    }
    #[must_use]
    pub fn records(&self) -> &[InstalledThemeSummary] {
        &self.records
    }
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
    #[must_use]
    pub const fn next(&self) -> Option<ThemeManifestCursor> {
        self.next
    }

    #[must_use]
    pub fn selection(&self, index: usize) -> Option<InstalledThemeSelection> {
        self.records
            .get(index)
            .cloned()
            .map(|summary| InstalledThemeSelection {
                manifest: self.manifest,
                summary,
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeDocumentRange {
    identity: ThemeDocumentIdentity,
    offset: u64,
    total_bytes: u64,
    bytes: Box<[u8]>,
}

impl ThemeDocumentRange {
    pub fn checked(
        identity: ThemeDocumentIdentity,
        offset: u64,
        total_bytes: u64,
        bytes: Vec<u8>,
        max_bytes: NonZeroUsize,
    ) -> Result<Self, ThemeRangeError> {
        if max_bytes.get() > THEME_DOCUMENT_RANGE_MAX_BYTES || bytes.len() > max_bytes.get() {
            return Err(ThemeRangeError::LimitExceeded);
        }
        if total_bytes != identity.byte_length() {
            return Err(ThemeRangeError::IdentityLengthMismatch);
        }
        let end = offset
            .checked_add(u64::try_from(bytes.len()).map_err(|_| ThemeRangeError::RangeOverflow)?)
            .ok_or(ThemeRangeError::RangeOverflow)?;
        if end > total_bytes {
            return Err(ThemeRangeError::OutsideDocument);
        }
        if bytes.is_empty() && offset < total_bytes {
            return Err(ThemeRangeError::EmptyNonterminalRange);
        }
        Ok(Self {
            identity,
            offset,
            total_bytes,
            bytes: bytes.into_boxed_slice(),
        })
    }
    #[must_use]
    pub const fn identity(&self) -> &ThemeDocumentIdentity {
        &self.identity
    }
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    #[must_use]
    pub fn range(&self) -> Range<u64> {
        self.offset..self.offset + self.bytes.len() as u64
    }
    #[must_use]
    pub fn is_final(&self) -> bool {
        self.range().end == self.total_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeRepositoryService {
    home: ThemeHomeIdentity,
}

impl ThemeRepositoryService {
    #[must_use]
    pub(crate) const fn new(home: ThemeHomeIdentity) -> Self {
        Self { home }
    }
    #[must_use]
    pub const fn home(self) -> ThemeHomeIdentity {
        self.home
    }
    #[must_use]
    pub const fn manifest(self, generation: ThemeManifestGeneration) -> ThemeManifestIdentity {
        ThemeManifestIdentity::new(self.home, generation)
    }
    pub fn check_manifest(
        self,
        manifest: ThemeManifestIdentity,
    ) -> Result<(), ThemeFreshnessError> {
        if manifest.home() != self.home {
            return Err(ThemeFreshnessError::StaleOrForeignHome);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemePageError {
    LimitsTooLarge,
    LimitExceeded,
    EmptyContinuation,
    NonContiguousOrder,
    OrderExhausted,
    DecodedBytesOverflow,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeRangeError {
    LimitExceeded,
    IdentityLengthMismatch,
    RangeOverflow,
    OutsideDocument,
    EmptyNonterminalRange,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeFreshnessError {
    StaleOrForeignHome,
    StaleManifest,
    StaleDocument,
    StaleDraft,
    StaleSettings,
}

macro_rules! display_debug_error { ($($type:ty),+) => {$ (impl fmt::Display for $type { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") } } impl Error for $type {})+ }; }
display_debug_error!(ThemePageError, ThemeRangeError, ThemeFreshnessError);
