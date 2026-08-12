use std::{
    fmt,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, DomainRevision};
use sha2::{Digest, Sha256};

use super::ThemeIdentityError;

static NEXT_THEME_SERVICE_INSTANCE: AtomicU64 = AtomicU64::new(1);

macro_rules! revision_type {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU64);

        impl $name {
            pub const INITIAL: Self = Self(NonZeroU64::MIN);

            pub const fn new(value: NonZeroU64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0.get()
            }

            pub fn checked_next(self) -> Result<Self, ThemeIdentityError> {
                self.0
                    .checked_add(1)
                    .map(Self)
                    .ok_or(ThemeIdentityError::RevisionExhausted($label))
            }
        }
    };
}

revision_type!(ThemeManifestGeneration, "theme manifest generation");
revision_type!(ThemeDocumentRevision, "theme document observation revision");
revision_type!(ThemeDraftRevision, "theme draft revision");

/// One fresh typed theme-service instance bound to an exact home generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeHomeIdentity {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_instance: NonZeroU64,
}

impl ThemeHomeIdentity {
    pub(crate) fn fresh(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> Result<Self, ThemeIdentityError> {
        let raw = NEXT_THEME_SERVICE_INSTANCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| ThemeIdentityError::ServiceInstanceExhausted)?;
        let service_instance =
            NonZeroU64::new(raw).ok_or(ThemeIdentityError::ServiceInstanceExhausted)?;
        Ok(Self {
            home_id,
            home_generation,
            service_instance,
        })
    }

    #[must_use]
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }
}

/// Exact immutable manifest identity owning membership, names, and order only.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeManifestIdentity {
    home: ThemeHomeIdentity,
    generation: ThemeManifestGeneration,
    content: ThemeManifestContentIdentity,
}

impl ThemeManifestIdentity {
    pub(crate) const fn new(home: ThemeHomeIdentity, generation: ThemeManifestGeneration) -> Self {
        Self {
            home,
            generation,
            content: ThemeManifestContentIdentity::Absent,
        }
    }

    pub(crate) const fn observed(
        home: ThemeHomeIdentity,
        generation: ThemeManifestGeneration,
        byte_length: u64,
        digest: ThemeDocumentDigest,
    ) -> Self {
        Self {
            home,
            generation,
            content: ThemeManifestContentIdentity::Present {
                byte_length,
                digest,
            },
        }
    }

    #[must_use]
    pub const fn home(self) -> ThemeHomeIdentity {
        self.home
    }

    #[must_use]
    pub const fn generation(self) -> ThemeManifestGeneration {
        self.generation
    }

    #[must_use]
    pub const fn content(self) -> ThemeManifestContentIdentity {
        self.content
    }
}

/// Exact physical manifest observation; absence is the initialized empty repository state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThemeManifestContentIdentity {
    Absent,
    Present {
        byte_length: u64,
        digest: ThemeDocumentDigest,
    },
}

/// Validated stable identity of one installed theme.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InstalledThemeId(Box<str>);

impl InstalledThemeId {
    pub const MAX_BYTES: usize = 64;

    pub fn new(value: impl AsRef<str>) -> Result<Self, ThemeIdentityError> {
        let value = value.as_ref();
        let edge_ok = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if value.is_empty()
            || value.len() > Self::MAX_BYTES
            || !value.bytes().all(|byte| edge_ok(byte) || byte == b'-')
            || !value.as_bytes().first().copied().is_some_and(edge_ok)
            || !value.as_bytes().last().copied().is_some_and(edge_ok)
        {
            return Err(ThemeIdentityError::InvalidInstalledThemeId);
        }
        Ok(Self(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstalledThemeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Exact identity of one canonical document within a repository generation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThemeDocumentIdentity {
    manifest: ThemeManifestIdentity,
    theme_id: InstalledThemeId,
    revision: ThemeDocumentRevision,
    byte_length: u64,
    digest: ThemeDocumentDigest,
}

impl ThemeDocumentIdentity {
    pub fn new(
        manifest: ThemeManifestIdentity,
        theme_id: InstalledThemeId,
        revision: ThemeDocumentRevision,
        byte_length: u64,
        digest: ThemeDocumentDigest,
    ) -> Self {
        Self {
            manifest,
            theme_id,
            revision,
            byte_length,
            digest,
        }
    }

    #[must_use]
    pub const fn manifest(&self) -> ThemeManifestIdentity {
        self.manifest
    }
    #[must_use]
    pub const fn theme_id(&self) -> &InstalledThemeId {
        &self.theme_id
    }
    #[must_use]
    pub const fn revision(&self) -> ThemeDocumentRevision {
        self.revision
    }
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }
    #[must_use]
    pub const fn digest(&self) -> ThemeDocumentDigest {
        self.digest
    }
}

/// SHA-256 identity of the exact observed installed-file bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeDocumentDigest([u8; 32]);

impl ThemeDocumentDigest {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Exact feature-owned draft identity; it is never a Settings identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeDraftIdentity {
    instance: NonZeroU64,
}

impl ThemeDraftIdentity {
    pub fn new(instance: NonZeroU64) -> Self {
        Self { instance }
    }
}

/// Exact durable Settings snapshot identity used by active-theme preparation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeSettingsIdentity {
    home: ThemeHomeIdentity,
    domain_revision: DomainRevision,
    active_record_revision: Option<crate::RecordRevision>,
}

impl ThemeSettingsIdentity {
    pub const fn new(
        home: ThemeHomeIdentity,
        domain_revision: DomainRevision,
        active_record_revision: Option<crate::RecordRevision>,
    ) -> Self {
        Self {
            home,
            domain_revision,
            active_record_revision,
        }
    }
    #[must_use]
    pub const fn home(self) -> ThemeHomeIdentity {
        self.home
    }
    #[must_use]
    pub const fn domain_revision(self) -> DomainRevision {
        self.domain_revision
    }
    #[must_use]
    pub const fn active_record_revision(self) -> Option<crate::RecordRevision> {
        self.active_record_revision
    }
}
