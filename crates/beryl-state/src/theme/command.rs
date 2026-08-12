use std::{error::Error, fmt, num::NonZeroU64};

use super::{
    InstalledThemeId, InstalledThemeSummary, ThemeDocument, ThemeDocumentIdentity,
    ThemeDraftIdentity, ThemeDraftRevision, ThemeFreshnessError, ThemeManifestIdentity, ThemeName,
    ThemeSettingsIdentity,
};

pub const THEME_REFERENCE_GUARD_MAX_ITEMS: usize = 64;
pub const THEME_RECONCILIATION_MAX_DOCUMENTS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeDocumentDraft {
    identity: ThemeDraftIdentity,
    revision: ThemeDraftRevision,
    binding: ThemeDocumentIdentity,
    document: ThemeDocument,
}

impl ThemeDocumentDraft {
    pub fn new(
        identity: ThemeDraftIdentity,
        revision: ThemeDraftRevision,
        binding: ThemeDocumentIdentity,
        document: ThemeDocument,
    ) -> Result<Self, ThemeCommandError> {
        if document.id().is_some_and(|id| id != binding.theme_id()) {
            return Err(ThemeCommandError::DraftDocumentMismatch);
        }
        Ok(Self {
            identity,
            revision,
            binding,
            document,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> ThemeDraftIdentity {
        self.identity
    }
    #[must_use]
    pub const fn revision(&self) -> ThemeDraftRevision {
        self.revision
    }
    #[must_use]
    pub const fn binding(&self) -> &ThemeDocumentIdentity {
        &self.binding
    }
    #[must_use]
    pub const fn document(&self) -> &ThemeDocument {
        &self.document
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeReferenceSnapshot {
    manifest: ThemeManifestIdentity,
    settings: ThemeSettingsIdentity,
    durable_active: Option<InstalledThemeId>,
    staged_active: Option<InstalledThemeId>,
    draft_bindings: Box<[InstalledThemeId]>,
    operation_bindings: Box<[InstalledThemeId]>,
}

impl ThemeReferenceSnapshot {
    pub fn new(
        manifest: ThemeManifestIdentity,
        settings: ThemeSettingsIdentity,
        durable_active: Option<InstalledThemeId>,
        staged_active: Option<InstalledThemeId>,
        draft_bindings: Vec<InstalledThemeId>,
        operation_bindings: Vec<InstalledThemeId>,
    ) -> Result<Self, ThemeCommandError> {
        if settings.home() != manifest.home() {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleSettings,
            ));
        }
        if draft_bindings.len() > THEME_REFERENCE_GUARD_MAX_ITEMS
            || operation_bindings.len() > THEME_REFERENCE_GUARD_MAX_ITEMS
        {
            return Err(ThemeCommandError::ReferenceSnapshotTooLarge);
        }
        if has_duplicates(&draft_bindings) || has_duplicates(&operation_bindings) {
            return Err(ThemeCommandError::DuplicateReference);
        }
        Ok(Self {
            manifest,
            settings,
            durable_active,
            staged_active,
            draft_bindings: draft_bindings.into_boxed_slice(),
            operation_bindings: operation_bindings.into_boxed_slice(),
        })
    }

    pub fn delete_guard(&self, target: &InstalledThemeId) -> Result<(), ThemeDeleteGuard> {
        if self.durable_active.as_ref() == Some(target) {
            return Err(ThemeDeleteGuard::DurableActive);
        }
        if self.staged_active.as_ref() == Some(target) {
            return Err(ThemeDeleteGuard::SettingsStagedActive);
        }
        if self.draft_bindings.contains(target) {
            return Err(ThemeDeleteGuard::OpenDocumentDraft);
        }
        if self.operation_bindings.contains(target) {
            return Err(ThemeDeleteGuard::RepositoryOperation);
        }
        Ok(())
    }

    #[must_use]
    pub const fn manifest(&self) -> ThemeManifestIdentity {
        self.manifest
    }
    #[must_use]
    pub const fn settings(&self) -> ThemeSettingsIdentity {
        self.settings
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeDeleteGuard {
    DurableActive,
    SettingsStagedActive,
    OpenDocumentDraft,
    RepositoryOperation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstallTheme {
    expected_manifest: ThemeManifestIdentity,
    new_id: InstalledThemeId,
    name: ThemeName,
    document: ThemeDocument,
}

impl InstallTheme {
    pub fn new(
        expected_manifest: ThemeManifestIdentity,
        new_id: InstalledThemeId,
        name: ThemeName,
        document: ThemeDocument,
    ) -> Result<Self, ThemeCommandError> {
        if document.id().is_some_and(|id| id != &new_id) {
            return Err(ThemeCommandError::DraftDocumentMismatch);
        }
        Ok(Self {
            expected_manifest,
            new_id,
            name,
            document,
        })
    }

    #[must_use]
    pub const fn expected_manifest(&self) -> ThemeManifestIdentity {
        self.expected_manifest
    }
    #[must_use]
    pub const fn new_id(&self) -> &InstalledThemeId {
        &self.new_id
    }
    #[must_use]
    pub const fn name(&self) -> &ThemeName {
        &self.name
    }
    #[must_use]
    pub const fn document(&self) -> &ThemeDocument {
        &self.document
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenameTheme {
    expected_manifest: ThemeManifestIdentity,
    expected: InstalledThemeSummary,
    name: ThemeName,
}

impl RenameTheme {
    #[must_use]
    pub fn new(
        expected_manifest: ThemeManifestIdentity,
        expected: InstalledThemeSummary,
        name: ThemeName,
    ) -> Self {
        Self {
            expected_manifest,
            expected,
            name,
        }
    }
    #[must_use]
    pub const fn expected_manifest(&self) -> ThemeManifestIdentity {
        self.expected_manifest
    }
    #[must_use]
    pub const fn expected(&self) -> &InstalledThemeSummary {
        &self.expected
    }
    #[must_use]
    pub const fn target(&self) -> &InstalledThemeId {
        self.expected.id()
    }
    #[must_use]
    pub const fn name(&self) -> &ThemeName {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReorderTheme {
    expected_manifest: ThemeManifestIdentity,
    expected: InstalledThemeSummary,
    new_order: u64,
}

impl ReorderTheme {
    #[must_use]
    pub fn new(
        expected_manifest: ThemeManifestIdentity,
        expected: InstalledThemeSummary,
        new_order: u64,
    ) -> Self {
        Self {
            expected_manifest,
            expected,
            new_order,
        }
    }
    #[must_use]
    pub const fn expected_manifest(&self) -> ThemeManifestIdentity {
        self.expected_manifest
    }
    #[must_use]
    pub const fn expected(&self) -> &InstalledThemeSummary {
        &self.expected
    }
    #[must_use]
    pub const fn target(&self) -> &InstalledThemeId {
        self.expected.id()
    }
    #[must_use]
    pub const fn new_order(&self) -> u64 {
        self.new_order
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteTheme {
    expected_manifest: ThemeManifestIdentity,
    expected: InstalledThemeSummary,
    expected_document: ThemeDocumentIdentity,
    references: ThemeReferenceSnapshot,
}

impl DeleteTheme {
    pub fn new(
        expected_manifest: ThemeManifestIdentity,
        expected: InstalledThemeSummary,
        expected_document: ThemeDocumentIdentity,
        references: ThemeReferenceSnapshot,
    ) -> Result<Self, ThemeCommandError> {
        if references.manifest() != expected_manifest {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleManifest,
            ));
        }
        if expected_document.manifest() != expected_manifest
            || expected_document.theme_id() != expected.id()
        {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleDocument,
            ));
        }
        references
            .delete_guard(expected.id())
            .map_err(ThemeCommandError::DeleteGuard)?;
        Ok(Self {
            expected_manifest,
            expected,
            expected_document,
            references,
        })
    }

    #[must_use]
    pub const fn expected_manifest(&self) -> ThemeManifestIdentity {
        self.expected_manifest
    }
    #[must_use]
    pub const fn expected(&self) -> &InstalledThemeSummary {
        &self.expected
    }
    #[must_use]
    pub const fn target(&self) -> &InstalledThemeId {
        self.expected.id()
    }
    #[must_use]
    pub const fn expected_document(&self) -> &ThemeDocumentIdentity {
        &self.expected_document
    }
    #[must_use]
    pub const fn references(&self) -> &ThemeReferenceSnapshot {
        &self.references
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateTheme {
    target: ThemeDocumentIdentity,
    document: ThemeDocument,
}

impl UpdateTheme {
    pub fn new(
        target: ThemeDocumentIdentity,
        document: ThemeDocument,
    ) -> Result<Self, ThemeCommandError> {
        if document.id().is_some_and(|id| id != target.theme_id()) {
            return Err(ThemeCommandError::DraftDocumentMismatch);
        }
        Ok(Self { target, document })
    }
    #[must_use]
    pub const fn target(&self) -> &ThemeDocumentIdentity {
        &self.target
    }
    #[must_use]
    pub const fn document(&self) -> &ThemeDocument {
        &self.document
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveTheme {
    draft: ThemeDocumentDraft,
}

impl SaveTheme {
    #[must_use]
    pub fn new(draft: ThemeDocumentDraft) -> Self {
        Self { draft }
    }
    #[must_use]
    pub const fn draft(&self) -> &ThemeDocumentDraft {
        &self.draft
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveThemeAs {
    expected_manifest: ThemeManifestIdentity,
    draft: ThemeDocumentDraft,
    new_id: InstalledThemeId,
    name: ThemeName,
}

impl SaveThemeAs {
    pub fn new(
        expected_manifest: ThemeManifestIdentity,
        draft: ThemeDocumentDraft,
        new_id: InstalledThemeId,
        name: ThemeName,
    ) -> Result<Self, ThemeCommandError> {
        if draft.binding().manifest() != expected_manifest {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleManifest,
            ));
        }
        if &new_id == draft.binding().theme_id() {
            return Err(ThemeCommandError::SaveAsReusesBinding);
        }
        Ok(Self {
            expected_manifest,
            draft,
            new_id,
            name,
        })
    }
    #[must_use]
    pub const fn expected_manifest(&self) -> ThemeManifestIdentity {
        self.expected_manifest
    }
    #[must_use]
    pub const fn draft(&self) -> &ThemeDocumentDraft {
        &self.draft
    }
    #[must_use]
    pub const fn new_id(&self) -> &InstalledThemeId {
        &self.new_id
    }
    #[must_use]
    pub const fn name(&self) -> &ThemeName {
        &self.name
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ThemeRepositoryCommand {
    Install(InstallTheme),
    Rename(RenameTheme),
    Delete(DeleteTheme),
    Reorder(ReorderTheme),
    Update(UpdateTheme),
    Save(SaveTheme),
    SaveAs(SaveThemeAs),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRepositoryCommit {
    manifest: Option<ThemeManifestIdentity>,
    affected_documents: Box<[ThemeDocumentIdentity]>,
}

impl ThemeRepositoryCommit {
    pub fn checked(
        manifest: Option<ThemeManifestIdentity>,
        affected_documents: Vec<ThemeDocumentIdentity>,
    ) -> Result<Self, ThemeCommandError> {
        if affected_documents.len() > THEME_RECONCILIATION_MAX_DOCUMENTS {
            return Err(ThemeCommandError::ReconciliationScopeTooLarge);
        }
        if has_duplicate_document_ids(&affected_documents) {
            return Err(ThemeCommandError::DuplicateDocumentIdentity);
        }
        if let Some(manifest) = manifest
            && affected_documents
                .iter()
                .any(|document| document.manifest() != manifest)
        {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleDocument,
            ));
        }
        if manifest.is_none()
            && affected_documents
                .split_first()
                .is_some_and(|(first, rest)| {
                    rest.iter()
                        .any(|document| document.manifest() != first.manifest())
                })
        {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleDocument,
            ));
        }
        Ok(Self {
            manifest,
            affected_documents: affected_documents.into_boxed_slice(),
        })
    }

    #[must_use]
    pub const fn manifest(&self) -> Option<ThemeManifestIdentity> {
        self.manifest
    }
    #[must_use]
    pub fn affected_documents(&self) -> &[ThemeDocumentIdentity] {
        &self.affected_documents
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ThemeNaturalRepositoryIdentity {
    manifest: ThemeManifestIdentity,
}

impl ThemeNaturalRepositoryIdentity {
    pub(crate) fn checked(
        manifest: ThemeManifestIdentity,
        documents: Vec<ThemeDocumentIdentity>,
    ) -> Result<Self, ThemeCommandError> {
        if documents.len() > THEME_RECONCILIATION_MAX_DOCUMENTS {
            return Err(ThemeCommandError::ReconciliationScopeTooLarge);
        }
        if has_duplicate_document_ids(&documents) {
            return Err(ThemeCommandError::DuplicateDocumentIdentity);
        }
        if documents
            .iter()
            .any(|document| document.manifest() != manifest)
        {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleDocument,
            ));
        }
        Ok(Self { manifest })
    }
    #[must_use]
    pub(crate) const fn manifest(&self) -> ThemeManifestIdentity {
        self.manifest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ThemeReconciliationDescriptor {
    operation: NonZeroU64,
}

impl ThemeReconciliationDescriptor {
    pub(crate) fn new(
        operation: NonZeroU64,
        old: ThemeNaturalRepositoryIdentity,
        intended_new: ThemeNaturalRepositoryIdentity,
    ) -> Result<Self, ThemeCommandError> {
        if old.manifest().home() != intended_new.manifest().home() {
            return Err(ThemeCommandError::Freshness(
                ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        let manifest_is_same = intended_new.manifest() == old.manifest();
        let manifest_is_successor = old
            .manifest()
            .generation()
            .checked_next()
            .is_ok_and(|next| next == intended_new.manifest().generation());
        if !manifest_is_same && !manifest_is_successor {
            return Err(ThemeCommandError::NonSuccessorManifest);
        }
        Ok(Self { operation })
    }

    #[must_use]
    pub(crate) const fn operation(&self) -> NonZeroU64 {
        self.operation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeReconciliation {
    ExactOld,
    ExactNew(ThemeRepositoryCommit),
    Collision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeCommandError {
    Freshness(ThemeFreshnessError),
    ReferenceSnapshotTooLarge,
    DuplicateReference,
    DeleteGuard(ThemeDeleteGuard),
    DraftDocumentMismatch,
    SaveAsReusesBinding,
    ReconciliationScopeTooLarge,
    DuplicateDocumentIdentity,
    NonSuccessorManifest,
}

impl fmt::Display for ThemeCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl Error for ThemeCommandError {}

fn has_duplicates(values: &[InstalledThemeId]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[index + 1..].contains(value))
}

fn has_duplicate_document_ids(values: &[ThemeDocumentIdentity]) -> bool {
    values.iter().enumerate().any(|(index, value)| {
        values[index + 1..]
            .iter()
            .any(|candidate| candidate.theme_id() == value.theme_id())
    })
}
