use std::path::{Path, PathBuf};
use std::{error::Error, fmt};

use deunicode::deunicode;
use serde::{Deserialize, Serialize};

pub const SCRATCHPAD_WORKSPACE_ID: &str = "scratchpad";
pub const SCRATCHPAD_WORKSPACE_TITLE: &str = "Scratchpad";
pub const UNTITLED_WORKSPACE_TITLE_PREFIX: &str = "Untitled";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeMode {
    HostWindows,
    WslLinux { distro_name: String },
}

impl RuntimeMode {
    pub fn display_name(&self) -> String {
        match self {
            Self::HostWindows => "host-windows".to_string(),
            Self::WslLinux { distro_name } => format!("wsl-linux:{distro_name}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId {
    runtime_mode: RuntimeMode,
    canonical_path: PathBuf,
}

pub type ExecutionTargetId = WorkspaceId;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceMemberId(String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMember {
    id: WorkspaceMemberId,
    runtime_mode: RuntimeMode,
    canonical_path: PathBuf,
    #[serde(default)]
    availability: WorkspaceMemberAvailability,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMemberAvailability {
    #[default]
    Available,
    PathNotFound,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BerylWorkspaceId(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BerylWorkspaceIdError {
    Empty,
    InvalidCharacter { ch: char },
    ReservedFilesystemName { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceMemberIdError {
    Empty,
    InvalidCharacter { ch: char },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BerylWorkspaceTitleError {
    Empty,
    EmptyDerivedSlug,
    InvalidDerivedSlug { source: BerylWorkspaceIdError },
    SlugEquivalentCollision { slug: BerylWorkspaceId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BerylWorkspaceKind {
    Scratchpad,
    Untitled,
    Named,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BerylWorkspaceTitleSource {
    FirstCompletedTurn,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BerylWorkspaceManifest {
    id: BerylWorkspaceId,
    kind: BerylWorkspaceKind,
    title: String,
    #[serde(default)]
    title_source: Option<BerylWorkspaceTitleSource>,
    last_updated_at_millis: u64,
}

impl WorkspaceId {
    pub fn from_parts(runtime_mode: RuntimeMode, canonical_path: impl Into<PathBuf>) -> Self {
        Self {
            runtime_mode,
            canonical_path: canonical_path.into(),
        }
    }

    pub fn host_windows(canonical_path: impl Into<PathBuf>) -> Self {
        Self::from_parts(RuntimeMode::HostWindows, canonical_path)
    }

    pub fn wsl_linux(distro_name: impl Into<String>, canonical_path: impl Into<PathBuf>) -> Self {
        Self::from_parts(
            RuntimeMode::WslLinux {
                distro_name: distro_name.into(),
            },
            canonical_path,
        )
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn host_openable_path(&self, path: &Path) -> PathBuf {
        match &self.runtime_mode {
            RuntimeMode::HostWindows => path.to_path_buf(),
            RuntimeMode::WslLinux { distro_name } => {
                let normalized = path
                    .to_string_lossy()
                    .replace('/', "\\")
                    .trim_start_matches('\\')
                    .to_string();

                if normalized.is_empty() {
                    PathBuf::from(format!(r"\\wsl.localhost\{distro_name}"))
                } else {
                    PathBuf::from(format!(r"\\wsl.localhost\{distro_name}\{normalized}"))
                }
            }
        }
    }

    pub fn display_label(&self) -> String {
        format!(
            "{} {}",
            self.runtime_mode.display_name(),
            self.canonical_path.display()
        )
    }
}

impl WorkspaceMemberId {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspaceMemberIdError> {
        let value = value.into();
        validate_ascii_identifier(&value).map_err(|error| match error {
            IdentifierValidationError::Empty => WorkspaceMemberIdError::Empty,
            IdentifierValidationError::InvalidCharacter { ch } => {
                WorkspaceMemberIdError::InvalidCharacter { ch }
            }
        })?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl WorkspaceMember {
    pub fn new(
        id: WorkspaceMemberId,
        runtime_mode: RuntimeMode,
        canonical_path: impl Into<PathBuf>,
    ) -> Self {
        Self::new_with_availability(
            id,
            runtime_mode,
            canonical_path,
            WorkspaceMemberAvailability::Available,
        )
    }

    pub fn new_with_availability(
        id: WorkspaceMemberId,
        runtime_mode: RuntimeMode,
        canonical_path: impl Into<PathBuf>,
        availability: WorkspaceMemberAvailability,
    ) -> Self {
        Self {
            id,
            runtime_mode,
            canonical_path: canonical_path.into(),
            availability,
        }
    }

    pub fn id(&self) -> &WorkspaceMemberId {
        &self.id
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn execution_target(&self) -> WorkspaceId {
        WorkspaceId::from_parts(self.runtime_mode.clone(), self.canonical_path.clone())
    }

    pub fn availability(&self) -> WorkspaceMemberAvailability {
        self.availability
    }

    pub fn is_available(&self) -> bool {
        self.availability == WorkspaceMemberAvailability::Available
    }

    pub fn mark_available(&mut self) -> bool {
        self.set_availability(WorkspaceMemberAvailability::Available)
    }

    pub fn mark_path_not_found(&mut self) -> bool {
        self.set_availability(WorkspaceMemberAvailability::PathNotFound)
    }

    fn set_availability(&mut self, availability: WorkspaceMemberAvailability) -> bool {
        if self.availability == availability {
            return false;
        }

        self.availability = availability;
        true
    }
}

impl BerylWorkspaceId {
    pub fn new(value: impl Into<String>) -> Result<Self, BerylWorkspaceIdError> {
        let value = value.into();
        validate_beryl_workspace_id(&value)?;
        Ok(Self(value))
    }

    pub fn from_title(title: impl AsRef<str>) -> Result<Self, BerylWorkspaceTitleError> {
        let title = normalize_workspace_title(title.as_ref().to_string())?;
        derive_workspace_slug_from_normalized_title(&title)
    }

    pub fn scratchpad() -> Self {
        Self(SCRATCHPAD_WORKSPACE_ID.to_string())
    }

    pub fn untitled(sequence: u64) -> Self {
        Self(format!("untitled-{sequence}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl BerylWorkspaceManifest {
    pub fn scratchpad(last_updated_at_millis: u64) -> Self {
        Self {
            id: BerylWorkspaceId::scratchpad(),
            kind: BerylWorkspaceKind::Scratchpad,
            title: SCRATCHPAD_WORKSPACE_TITLE.to_string(),
            title_source: Some(BerylWorkspaceTitleSource::Manual),
            last_updated_at_millis,
        }
    }

    pub fn named(
        id: BerylWorkspaceId,
        title: impl Into<String>,
        last_updated_at_millis: u64,
    ) -> Self {
        Self {
            id,
            kind: BerylWorkspaceKind::Named,
            title: title.into(),
            title_source: Some(BerylWorkspaceTitleSource::Manual),
            last_updated_at_millis,
        }
    }

    pub fn untitled(sequence: u64, last_updated_at_millis: u64) -> Self {
        Self {
            id: BerylWorkspaceId::untitled(sequence),
            kind: BerylWorkspaceKind::Untitled,
            title: format!("{UNTITLED_WORKSPACE_TITLE_PREFIX} {sequence}"),
            title_source: None,
            last_updated_at_millis,
        }
    }

    pub fn id(&self) -> &BerylWorkspaceId {
        &self.id
    }

    pub fn kind(&self) -> &BerylWorkspaceKind {
        &self.kind
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn title_source(&self) -> Option<BerylWorkspaceTitleSource> {
        self.title_source
    }

    pub fn last_updated_at_millis(&self) -> u64 {
        self.last_updated_at_millis
    }

    pub fn is_scratchpad(&self) -> bool {
        matches!(self.kind, BerylWorkspaceKind::Scratchpad)
    }

    pub fn is_untitled(&self) -> bool {
        matches!(self.kind, BerylWorkspaceKind::Untitled)
    }

    pub fn set_last_updated_at_millis(&mut self, last_updated_at_millis: u64) {
        self.last_updated_at_millis = last_updated_at_millis;
    }

    pub fn set_generated_title_if_untitled(
        &mut self,
        title: impl Into<String>,
    ) -> Result<bool, BerylWorkspaceTitleError> {
        if !self.is_untitled() {
            return Ok(false);
        }

        let title = normalize_workspace_title(title)?;
        let id = derive_workspace_slug_from_normalized_title(&title)?;
        self.kind = BerylWorkspaceKind::Named;
        self.id = id;
        self.title = title;
        self.title_source = Some(BerylWorkspaceTitleSource::FirstCompletedTurn);
        Ok(true)
    }

    pub fn set_manual_title(
        &mut self,
        title: impl Into<String>,
    ) -> Result<bool, BerylWorkspaceTitleError> {
        let title = normalize_workspace_title(title)?;
        let id = derive_workspace_slug_from_normalized_title(&title)?;
        let title_source = Some(BerylWorkspaceTitleSource::Manual);
        if self.kind == BerylWorkspaceKind::Named
            && self.id == id
            && self.title == title
            && self.title_source == title_source
        {
            return Ok(false);
        }

        self.kind = BerylWorkspaceKind::Named;
        self.id = id;
        self.title = title;
        self.title_source = title_source;
        Ok(true)
    }
}

impl fmt::Display for BerylWorkspaceIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "workspace id must not be empty"),
            Self::InvalidCharacter { ch } => write!(
                f,
                "workspace id contains invalid character {ch:?}; only lowercase ASCII letters, digits, '-' and '_' are allowed"
            ),
            Self::ReservedFilesystemName { name } => {
                write!(
                    f,
                    "workspace id {name:?} is reserved by Windows filesystems"
                )
            }
        }
    }
}

impl Error for BerylWorkspaceIdError {}

impl fmt::Display for WorkspaceMemberIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "workspace member id must not be empty"),
            Self::InvalidCharacter { ch } => write!(
                f,
                "workspace member id contains invalid character {ch:?}; only lowercase ASCII letters, digits, '-' and '_' are allowed"
            ),
        }
    }
}

impl Error for WorkspaceMemberIdError {}

impl fmt::Display for BerylWorkspaceTitleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "workspace title must not be empty"),
            Self::EmptyDerivedSlug => write!(
                f,
                "workspace title must include at least one transliterable letter or digit"
            ),
            Self::InvalidDerivedSlug { source } => {
                write!(
                    f,
                    "workspace title derives an invalid workspace id: {source}"
                )
            }
            Self::SlugEquivalentCollision { slug } => write!(
                f,
                "workspace title is too similar to an existing workspace name with slug {:?}",
                slug.as_str()
            ),
        }
    }
}

impl Error for BerylWorkspaceTitleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidDerivedSlug { source } => Some(source),
            Self::Empty | Self::EmptyDerivedSlug | Self::SlugEquivalentCollision { .. } => None,
        }
    }
}

enum IdentifierValidationError {
    Empty,
    InvalidCharacter { ch: char },
}

pub fn derive_workspace_slug(
    title: impl AsRef<str>,
) -> Result<BerylWorkspaceId, BerylWorkspaceTitleError> {
    BerylWorkspaceId::from_title(title)
}

fn normalize_workspace_title(title: impl Into<String>) -> Result<String, BerylWorkspaceTitleError> {
    let title = title.into().trim().to_string();
    if title.is_empty() {
        return Err(BerylWorkspaceTitleError::Empty);
    }

    Ok(title)
}

fn derive_workspace_slug_from_normalized_title(
    title: &str,
) -> Result<BerylWorkspaceId, BerylWorkspaceTitleError> {
    let ascii_title = deunicode(title);
    let mut slug = String::new();
    let mut previous_separator = false;

    for ch in ascii_title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_separator = false;
        } else if !slug.is_empty() && !previous_separator {
            slug.push('-');
            previous_separator = true;
        }
    }

    if previous_separator {
        slug.pop();
    }

    if slug.is_empty() {
        return Err(BerylWorkspaceTitleError::EmptyDerivedSlug);
    }

    BerylWorkspaceId::new(slug)
        .map_err(|source| BerylWorkspaceTitleError::InvalidDerivedSlug { source })
}

fn validate_beryl_workspace_id(value: &str) -> Result<(), BerylWorkspaceIdError> {
    validate_ascii_identifier(value).map_err(|error| match error {
        IdentifierValidationError::Empty => BerylWorkspaceIdError::Empty,
        IdentifierValidationError::InvalidCharacter { ch } => {
            BerylWorkspaceIdError::InvalidCharacter { ch }
        }
    })?;

    if is_windows_reserved_basename(value) {
        return Err(BerylWorkspaceIdError::ReservedFilesystemName {
            name: value.to_string(),
        });
    }

    Ok(())
}

fn validate_ascii_identifier(value: &str) -> Result<(), IdentifierValidationError> {
    if value.is_empty() {
        return Err(IdentifierValidationError::Empty);
    }

    for ch in value.chars() {
        if !matches!(ch, 'a'..='z' | '0'..='9' | '-' | '_') {
            return Err(IdentifierValidationError::InvalidCharacter { ch });
        }
    }

    Ok(())
}

fn is_windows_reserved_basename(value: &str) -> bool {
    matches!(
        value,
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}
