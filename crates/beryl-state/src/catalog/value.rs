use std::num::{NonZeroU16, NonZeroU64};

use beryl_model::{
    AdmittedHostPath, Availability, ClaimRevision, RootId, RuntimeId, SyndicThreadId,
    ThreadRevision, WindowId,
};

use crate::RecordRevision;

use super::{CatalogValueError, error::bounded_text};

pub const CATALOG_TITLE_MAX_BYTES: usize = 512;
pub const CATALOG_ENVIRONMENT_LABEL_MAX_BYTES: usize = 256;
pub const CATALOG_NORMALIZED_TITLE_MAX_BYTES: usize = 2 * 1024;
pub const CATALOG_NORMALIZED_ENVIRONMENT_MAX_BYTES: usize = 1024;
pub const CATALOG_NORMALIZED_PATH_MAX_BYTES: usize = 64 * 1024;

const UNTITLED: &str = "Untitled";

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

/// One bounded title candidate and the Syndic revision from which it was derived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogTitleCandidate {
    text: Box<str>,
    source_thread_revision: ThreadRevision,
}

impl CatalogTitleCandidate {
    pub fn new(
        text: impl AsRef<str>,
        source_thread_revision: ThreadRevision,
    ) -> Result<Self, CatalogValueError> {
        Ok(Self {
            text: bounded_text("catalog title", text.as_ref(), CATALOG_TITLE_MAX_BYTES)?,
            source_thread_revision,
        })
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn source_thread_revision(&self) -> ThreadRevision {
        self.source_thread_revision
    }
}

/// Generated and Syndic-history title facts retained for exact display precedence.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogTitleFacts {
    generated: Option<CatalogTitleCandidate>,
    syndic: Option<CatalogTitleCandidate>,
}

impl CatalogTitleFacts {
    #[must_use]
    pub const fn new(
        generated: Option<CatalogTitleCandidate>,
        syndic: Option<CatalogTitleCandidate>,
    ) -> Self {
        Self { generated, syndic }
    }

    #[must_use]
    pub const fn generated(&self) -> Option<&CatalogTitleCandidate> {
        self.generated.as_ref()
    }

    #[must_use]
    pub const fn syndic(&self) -> Option<&CatalogTitleCandidate> {
        self.syndic.as_ref()
    }

    #[must_use]
    pub fn display_title(&self) -> &str {
        self.generated
            .as_ref()
            .or(self.syndic.as_ref())
            .map_or(UNTITLED, CatalogTitleCandidate::text)
    }

    #[must_use]
    pub const fn display_source(&self) -> CatalogTitleSource {
        if self.generated.is_some() {
            CatalogTitleSource::Generated
        } else if self.syndic.is_some() {
            CatalogTitleSource::Syndic
        } else {
            CatalogTitleSource::Untitled
        }
    }
}

/// Which title fact currently wins generated/Syndic/untitled precedence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogTitleSource {
    Generated,
    Syndic,
    Untitled,
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
        top_level_thread_id: SyndicThreadId,
        parent_thread_id: SyndicThreadId,
        depth: NonZeroU16,
    },
}

impl CatalogLineageSummary {
    pub fn descendant(
        top_level_thread_id: SyndicThreadId,
        parent_thread_id: SyndicThreadId,
        depth: u16,
    ) -> Result<Self, CatalogValueError> {
        let depth = NonZeroU16::new(depth).ok_or(CatalogValueError::InvalidLineage(
            "a descendant lineage depth must be nonzero",
        ))?;
        Ok(Self::Descendant {
            top_level_thread_id,
            parent_thread_id,
            depth,
        })
    }

    #[must_use]
    pub const fn top_level_thread_id(self) -> Option<SyndicThreadId> {
        match self {
            Self::TopLevel => None,
            Self::Descendant {
                top_level_thread_id,
                ..
            } => Some(top_level_thread_id),
        }
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
    pub const fn depth(self) -> u16 {
        match self {
            Self::TopLevel => 0,
            Self::Descendant { depth, .. } => depth.get(),
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

/// Caller-admitted normalized lexical fields used by catalog search.
///
/// This boundary deliberately does not normalize Unicode. The caller must pass
/// already normalized, case-folded text; this constructor enforces only the
/// durable v1 size and text-shape contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogSearchFields {
    title: Box<str>,
    environment_label: Box<str>,
    configured_executable_path: Box<str>,
    full_root_path: Box<str>,
}

impl CatalogSearchFields {
    pub fn from_admitted_normalized(
        title: impl AsRef<str>,
        environment_label: impl AsRef<str>,
        configured_executable_path: impl AsRef<str>,
        full_root_path: impl AsRef<str>,
    ) -> Result<Self, CatalogValueError> {
        Ok(Self {
            title: bounded_text(
                "normalized catalog title",
                title.as_ref(),
                CATALOG_NORMALIZED_TITLE_MAX_BYTES,
            )?,
            environment_label: bounded_text(
                "normalized catalog environment label",
                environment_label.as_ref(),
                CATALOG_NORMALIZED_ENVIRONMENT_MAX_BYTES,
            )?,
            configured_executable_path: bounded_text(
                "normalized catalog executable path",
                configured_executable_path.as_ref(),
                CATALOG_NORMALIZED_PATH_MAX_BYTES,
            )?,
            full_root_path: bounded_text(
                "normalized catalog root path",
                full_root_path.as_ref(),
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

/// Exact authoritative record revisions from which one catalog row was built.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogSourceRevisions {
    thread: ThreadRevision,
    thread_metadata: RecordRevision,
    runtime: RecordRevision,
    root: RecordRevision,
    claim: Option<ClaimRevision>,
}

impl CatalogSourceRevisions {
    #[must_use]
    pub const fn new(
        thread: ThreadRevision,
        thread_metadata: RecordRevision,
        runtime: RecordRevision,
        root: RecordRevision,
        claim: Option<ClaimRevision>,
    ) -> Self {
        Self {
            thread,
            thread_metadata,
            runtime,
            root,
            claim,
        }
    }

    #[must_use]
    pub const fn thread(self) -> ThreadRevision {
        self.thread
    }

    #[must_use]
    pub const fn thread_metadata(self) -> RecordRevision {
        self.thread_metadata
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
        if self.thread < previous.thread {
            Some("Syndic thread")
        } else if self.thread_metadata < previous.thread_metadata {
            Some("thread metadata")
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
