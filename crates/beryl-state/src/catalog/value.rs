use std::num::NonZeroU64;

use beryl_model::{
    AdmittedHostPath, Availability, ClaimRevision, ProjectionRevision, RootId, RuntimeId,
    SyndicPathDigest, SyndicThreadId, WindowId,
};

use crate::RecordRevision;

use super::{CatalogValueError, error::bounded_text, normalization::normalize};

pub const CATALOG_TITLE_MAX_BYTES: usize = 512;
pub const CATALOG_HISTORY_TITLE_MAX_SCALARS: usize = 80;
pub const CATALOG_ENVIRONMENT_LABEL_MAX_BYTES: usize = 256;
pub const CATALOG_NORMALIZED_TITLE_MAX_BYTES: usize = 2 * 1024;
pub const CATALOG_NORMALIZED_ENVIRONMENT_MAX_BYTES: usize = 1024;
pub const CATALOG_NORMALIZED_PATH_MAX_BYTES: usize = 64 * 1024;

/// Package-local monotonic revision of one compact catalog row.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogRevision(NonZeroU64);

impl CatalogRevision {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    pub fn new(value: u64) -> Result<Self, CatalogValueError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(CatalogValueError::ZeroCatalogRevision)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub(super) fn checked_next(self) -> Result<Self, CatalogValueError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(CatalogValueError::CatalogRevisionExhausted)
    }
}

/// The already-resolved title source copied from one Syndic catalog summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogTitleSource {
    Generated,
    HistoryDerived,
    Absent,
}

/// One bounded title whose precedence was already resolved by Syndic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogResolvedTitle {
    text: Option<Box<str>>,
    source: CatalogTitleSource,
}

impl CatalogResolvedTitle {
    pub fn generated(text: impl AsRef<str>) -> Result<Self, CatalogValueError> {
        let text = bounded_text(
            "generated catalog title",
            text.as_ref(),
            CATALOG_TITLE_MAX_BYTES,
        )?;
        if !text.chars().any(char::is_alphanumeric) {
            return Err(CatalogValueError::MissingAlphanumeric {
                kind: "generated catalog title",
            });
        }
        Ok(Self {
            text: Some(text),
            source: CatalogTitleSource::Generated,
        })
    }

    pub fn history_derived(text: impl AsRef<str>) -> Result<Self, CatalogValueError> {
        let text = text.as_ref();
        if text.is_empty() {
            return Err(CatalogValueError::Empty {
                kind: "history-derived catalog title",
            });
        }
        if text.len() > CATALOG_TITLE_MAX_BYTES {
            return Err(CatalogValueError::TooLong {
                kind: "history-derived catalog title",
                maximum: CATALOG_TITLE_MAX_BYTES,
                actual: text.len(),
            });
        }
        let scalar_count = text.chars().count();
        if scalar_count > CATALOG_HISTORY_TITLE_MAX_SCALARS {
            return Err(CatalogValueError::TooManyScalars {
                kind: "history-derived catalog title",
                maximum: CATALOG_HISTORY_TITLE_MAX_SCALARS,
                actual: scalar_count,
            });
        }
        if text.trim_end() != text {
            return Err(CatalogValueError::TrailingWhitespace {
                kind: "history-derived catalog title",
            });
        }
        if let Some((index, _)) = text.char_indices().find(|(_, value)| value.is_control()) {
            return Err(CatalogValueError::ControlCharacter {
                kind: "history-derived catalog title",
                index,
            });
        }
        Ok(Self {
            text: Some(text.into()),
            source: CatalogTitleSource::HistoryDerived,
        })
    }

    #[must_use]
    pub const fn absent() -> Self {
        Self {
            text: None,
            source: CatalogTitleSource::Absent,
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    #[must_use]
    pub const fn source(&self) -> CatalogTitleSource {
        self.source
    }
}

/// Runtime and root availability copied into one compact projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogAvailabilitySummary {
    runtime: Availability,
    root: Availability,
}

impl CatalogAvailabilitySummary {
    #[must_use]
    pub const fn new(runtime: Availability, root: Availability) -> Self {
        Self { runtime, root }
    }

    #[must_use]
    pub const fn runtime(self) -> Availability {
        self.runtime
    }

    #[must_use]
    pub const fn root(self) -> Availability {
        self.root
    }
}

/// Exact compact runtime/root scope needed to render and filter a thread row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogExecutionSummary {
    runtime_id: RuntimeId,
    root_id: RootId,
    environment_label: Box<str>,
    configured_executable_path: AdmittedHostPath,
    full_root_path: AdmittedHostPath,
    availability: CatalogAvailabilitySummary,
}

impl CatalogExecutionSummary {
    pub fn new(
        runtime_id: RuntimeId,
        root_id: RootId,
        environment_label: impl AsRef<str>,
        configured_executable_path: AdmittedHostPath,
        full_root_path: AdmittedHostPath,
        availability: CatalogAvailabilitySummary,
    ) -> Result<Self, CatalogValueError> {
        Ok(Self {
            runtime_id,
            root_id,
            environment_label: bounded_text(
                "catalog environment label",
                environment_label.as_ref(),
                CATALOG_ENVIRONMENT_LABEL_MAX_BYTES,
            )?,
            configured_executable_path,
            full_root_path,
            availability,
        })
    }

    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    #[must_use]
    pub const fn root_id(&self) -> RootId {
        self.root_id
    }

    #[must_use]
    pub fn environment_label(&self) -> &str {
        &self.environment_label
    }

    #[must_use]
    pub const fn configured_executable_path(&self) -> &AdmittedHostPath {
        &self.configured_executable_path
    }

    #[must_use]
    pub const fn full_root_path(&self) -> &AdmittedHostPath {
        &self.full_root_path
    }

    #[must_use]
    pub const fn availability(&self) -> CatalogAvailabilitySummary {
        self.availability
    }
}

/// Durable claim lifecycle represented by a compact row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogClaimKind {
    Active,
    Restoring,
}

/// Current claim fact used to distinguish unclaimed, current, and open-elsewhere rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogClaimSummary {
    Unclaimed,
    Claimed {
        window_id: WindowId,
        kind: CatalogClaimKind,
    },
}

impl CatalogClaimSummary {
    #[must_use]
    pub const fn claimed(window_id: WindowId, kind: CatalogClaimKind) -> Self {
        Self::Claimed { window_id, kind }
    }

    #[must_use]
    pub const fn window_id(self) -> Option<WindowId> {
        match self {
            Self::Unclaimed => None,
            Self::Claimed { window_id, .. } => Some(window_id),
        }
    }

    #[must_use]
    pub const fn kind(self) -> Option<CatalogClaimKind> {
        match self {
            Self::Unclaimed => None,
            Self::Claimed { kind, .. } => Some(kind),
        }
    }
}

/// Bounded lineage facts; the full turn graph is intentionally absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogLineageSummary {
    TopLevel,
    Descendant {
        parent_thread_id: SyndicThreadId,
        depth: NonZeroU64,
        path_digest: SyndicPathDigest,
    },
}

impl CatalogLineageSummary {
    pub fn descendant(
        parent_thread_id: SyndicThreadId,
        depth: u64,
        path_digest: SyndicPathDigest,
    ) -> Result<Self, CatalogValueError> {
        let depth = NonZeroU64::new(depth).ok_or(CatalogValueError::InvalidLineage(
            "a descendant lineage depth must be nonzero",
        ))?;
        Ok(Self::Descendant {
            parent_thread_id,
            depth,
            path_digest,
        })
    }

    #[must_use]
    pub const fn parent_thread_id(self) -> Option<SyndicThreadId> {
        match self {
            Self::TopLevel => None,
            Self::Descendant {
                parent_thread_id, ..
            } => Some(parent_thread_id),
        }
    }

    #[must_use]
    pub const fn depth(self) -> u64 {
        match self {
            Self::TopLevel => 0,
            Self::Descendant { depth, .. } => depth.get(),
        }
    }

    #[must_use]
    pub const fn path_digest(self) -> Option<SyndicPathDigest> {
        match self {
            Self::TopLevel => None,
            Self::Descendant { path_digest, .. } => Some(path_digest),
        }
    }
}

/// Automatic branch-discussion archive presentation copied into the catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogArchiveSummary {
    Ordinary,
    BranchDiscussionOpen,
    BranchDiscussionArchived,
}

/// Catalog-owned normalized lexical fields used by catalog search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogSearchFields {
    title: Box<str>,
    environment_label: Box<str>,
    configured_executable_path: Box<str>,
    full_root_path: Box<str>,
}

impl CatalogSearchFields {
    pub(super) fn from_visible(
        title: &CatalogResolvedTitle,
        execution: &CatalogExecutionSummary,
    ) -> Result<Self, CatalogValueError> {
        Ok(Self {
            title: normalize(
                "normalized catalog title",
                title.text().unwrap_or_default(),
                CATALOG_NORMALIZED_TITLE_MAX_BYTES,
            )?,
            environment_label: normalize(
                "normalized catalog environment label",
                execution.environment_label(),
                CATALOG_NORMALIZED_ENVIRONMENT_MAX_BYTES,
            )?,
            configured_executable_path: normalize(
                "normalized catalog executable path",
                execution.configured_executable_path().as_str(),
                CATALOG_NORMALIZED_PATH_MAX_BYTES,
            )?,
            full_root_path: normalize(
                "normalized catalog root path",
                execution.full_root_path().as_str(),
                CATALOG_NORMALIZED_PATH_MAX_BYTES,
            )?,
        })
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn environment_label(&self) -> &str {
        &self.environment_label
    }

    #[must_use]
    pub fn configured_executable_path(&self) -> &str {
        &self.configured_executable_path
    }

    #[must_use]
    pub fn full_root_path(&self) -> &str {
        &self.full_root_path
    }
}

/// Exact Syndic-summary and Beryl-record fences from which one catalog row was built.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogSourceRevisions {
    syndic_summary: ProjectionRevision,
    runtime: RecordRevision,
    root: RecordRevision,
    claim: Option<ClaimRevision>,
}

impl CatalogSourceRevisions {
    #[must_use]
    pub const fn new(
        syndic_summary: ProjectionRevision,
        runtime: RecordRevision,
        root: RecordRevision,
        claim: Option<ClaimRevision>,
    ) -> Self {
        Self {
            syndic_summary,
            runtime,
            root,
            claim,
        }
    }

    #[must_use]
    pub const fn syndic_summary(self) -> ProjectionRevision {
        self.syndic_summary
    }

    #[must_use]
    pub const fn runtime(self) -> RecordRevision {
        self.runtime
    }

    #[must_use]
    pub const fn root(self) -> RecordRevision {
        self.root
    }

    #[must_use]
    pub const fn claim(self) -> Option<ClaimRevision> {
        self.claim
    }

    pub(super) fn regression_from(self, previous: Self) -> Option<&'static str> {
        if self.syndic_summary < previous.syndic_summary {
            Some("Syndic catalog summary")
        } else if self.runtime < previous.runtime {
            Some("runtime")
        } else if self.root < previous.root {
            Some("root")
        } else {
            None
        }
    }
}

/// Whether the compact projection agrees with its current authoritative sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogFreshness {
    Current,
    Stale,
}

/// Expected row state used by a publish contribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogRowExpectation {
    Missing,
    Revision(CatalogRevision),
}
