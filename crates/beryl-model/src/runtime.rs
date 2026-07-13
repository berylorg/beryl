use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{RootId, RuntimeId};

const MAX_WSL_DISTRIBUTION_BYTES: usize = 256;
const MAX_PATH_BYTES: usize = 32 * 1024;

/// Why a bounded shared value could not be constructed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueError {
    /// The value is empty.
    Empty {
        /// Kind of value being validated.
        kind: &'static str,
    },
    /// The value exceeds its byte budget.
    TooLong {
        /// Kind of value being validated.
        kind: &'static str,
        /// Maximum accepted UTF-8 byte length.
        max_bytes: usize,
        /// Observed UTF-8 byte length.
        actual_bytes: usize,
    },
    /// The value begins or ends with whitespace that would make identity ambiguous.
    SurroundingWhitespace {
        /// Kind of value being validated.
        kind: &'static str,
    },
    /// The value contains a control or otherwise forbidden character.
    InvalidCharacter {
        /// Kind of value being validated.
        kind: &'static str,
        /// UTF-8 byte offset within the value.
        index: usize,
    },
    /// The path is not absolute in its declared syntax.
    NotAbsolute {
        /// Declared path syntax.
        flavor: PathFlavor,
    },
    /// The path syntax cannot belong to the declared runtime environment.
    RuntimePathMismatch,
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { kind } => write!(formatter, "{kind} must not be empty"),
            Self::TooLong {
                kind,
                max_bytes,
                actual_bytes,
            } => write!(
                formatter,
                "{kind} must not exceed {max_bytes} bytes, got {actual_bytes}"
            ),
            Self::SurroundingWhitespace { kind } => {
                write!(formatter, "{kind} must not contain surrounding whitespace")
            }
            Self::InvalidCharacter { kind, index } => {
                write!(
                    formatter,
                    "{kind} contains an invalid character at index {index}"
                )
            }
            Self::NotAbsolute { flavor } => {
                write!(formatter, "path is not absolute for {flavor:?} syntax")
            }
            Self::RuntimePathMismatch => {
                formatter.write_str("path syntax does not match the runtime environment")
            }
        }
    }
}

impl Error for ValueError {}

pub(crate) fn bounded_text(
    kind: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<Box<str>, ValueError> {
    if value.is_empty() {
        return Err(ValueError::Empty { kind });
    }
    if value.len() > max_bytes {
        return Err(ValueError::TooLong {
            kind,
            max_bytes,
            actual_bytes: value.len(),
        });
    }
    if value.trim() != value {
        return Err(ValueError::SurroundingWhitespace { kind });
    }
    if let Some((index, _)) = value
        .char_indices()
        .find(|(_, character)| character.is_control())
    {
        return Err(ValueError::InvalidCharacter { kind, index });
    }
    Ok(value.into())
}

/// Exact configured WSL distribution name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WslDistributionName(Box<str>);

impl WslDistributionName {
    /// Validates one bounded distribution name without probing WSL.
    pub fn new(value: impl AsRef<str>) -> Result<Self, ValueError> {
        let value = value.as_ref();
        let bounded = bounded_text("WSL distribution name", value, MAX_WSL_DISTRIBUTION_BYTES)?;
        if let Some((index, _)) = value
            .char_indices()
            .find(|(_, character)| matches!(character, '/' | '\\' | ':'))
        {
            return Err(ValueError::InvalidCharacter {
                kind: "WSL distribution name",
                index,
            });
        }
        Ok(Self(bounded))
    }

    /// Returns the exact configured name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WslDistributionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for WslDistributionName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// Execution environment derived by the runtime-admission boundary.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RuntimeMode {
    /// The executable runs directly in the Beryl host environment.
    Host,
    /// The executable runs in one exact WSL distribution.
    Wsl(WslDistributionName),
}

impl RuntimeMode {
    /// Constructs the host execution mode.
    #[must_use]
    pub const fn host() -> Self {
        Self::Host
    }

    /// Constructs a validated WSL execution mode without probing WSL.
    pub fn wsl(distribution: impl AsRef<str>) -> Result<Self, ValueError> {
        WslDistributionName::new(distribution).map(Self::Wsl)
    }
}

/// Syntax used by an admitted absolute path.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PathFlavor {
    /// Windows drive, UNC, or extended absolute syntax.
    Windows,
    /// POSIX absolute syntax.
    Posix,
}

fn is_windows_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let unc_absolute = (value.starts_with("\\\\") || value.starts_with("//"))
        && value
            .split(['\\', '/'])
            .filter(|component| !component.is_empty())
            .count()
            >= 2;
    drive_absolute || unc_absolute
}

fn validate_absolute_path(
    kind: &'static str,
    flavor: PathFlavor,
    value: &str,
) -> Result<Box<str>, ValueError> {
    let value = bounded_text(kind, value, MAX_PATH_BYTES)?;
    let absolute = match flavor {
        PathFlavor::Windows => is_windows_absolute(&value),
        PathFlavor::Posix => value.starts_with('/'),
    };
    if !absolute {
        return Err(ValueError::NotAbsolute { flavor });
    }
    Ok(value)
}

/// Canonical host-visible path admitted by a filesystem-owning boundary.
///
/// This value validates bounded absolute syntax only. Its constructor name
/// records that the caller, not this pure crate, proved canonical filesystem
/// identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AdmittedHostPath {
    flavor: PathFlavor,
    value: Box<str>,
}

impl AdmittedHostPath {
    /// Constructs a path after the caller has admitted its canonical identity.
    pub fn from_admitted(flavor: PathFlavor, value: impl AsRef<str>) -> Result<Self, ValueError> {
        Ok(Self {
            flavor,
            value: validate_absolute_path("host path", flavor, value.as_ref())?,
        })
    }

    /// Returns the declared path syntax.
    #[must_use]
    pub const fn flavor(&self) -> PathFlavor {
        self.flavor
    }

    /// Returns the exact admitted spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<'de> Deserialize<'de> for AdmittedHostPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawPath {
            flavor: PathFlavor,
            value: String,
        }

        let raw = RawPath::deserialize(deserializer)?;
        Self::from_admitted(raw.flavor, raw.value).map_err(de::Error::custom)
    }
}

/// Canonical runtime-native path admitted by the runtime/root boundary.
///
/// WSL paths must use POSIX syntax. Host paths retain an explicit flavor so
/// pure values remain portable across Windows and other supported hosts.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RuntimeNativePath {
    mode: RuntimeMode,
    flavor: PathFlavor,
    value: Box<str>,
}

impl RuntimeNativePath {
    /// Constructs a path after its owning boundary proved canonical identity.
    pub fn from_admitted(
        mode: RuntimeMode,
        flavor: PathFlavor,
        value: impl AsRef<str>,
    ) -> Result<Self, ValueError> {
        if matches!(mode, RuntimeMode::Wsl(_)) && flavor != PathFlavor::Posix {
            return Err(ValueError::RuntimePathMismatch);
        }
        Ok(Self {
            mode,
            flavor,
            value: validate_absolute_path("runtime-native path", flavor, value.as_ref())?,
        })
    }

    /// Returns the exact execution environment.
    #[must_use]
    pub const fn mode(&self) -> &RuntimeMode {
        &self.mode
    }

    /// Returns the declared path syntax.
    #[must_use]
    pub const fn flavor(&self) -> PathFlavor {
        self.flavor
    }

    /// Returns the exact admitted spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<'de> Deserialize<'de> for RuntimeNativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawPath {
            mode: RuntimeMode,
            flavor: PathFlavor,
            value: String,
        }

        let raw = RawPath::deserialize(deserializer)?;
        Self::from_admitted(raw.mode, raw.flavor, raw.value).map_err(de::Error::custom)
    }
}

/// Immutable runtime/root execution identity for one Syndic thread.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ExecutionBinding {
    runtime_id: RuntimeId,
    root_id: RootId,
    root_path: RuntimeNativePath,
}

impl ExecutionBinding {
    /// Constructs an exact immutable binding from already admitted values.
    #[must_use]
    pub const fn new(runtime_id: RuntimeId, root_id: RootId, root_path: RuntimeNativePath) -> Self {
        Self {
            runtime_id,
            root_id,
            root_path,
        }
    }

    /// Returns the configured runtime identity.
    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    /// Returns the configured root identity.
    #[must_use]
    pub const fn root_id(&self) -> RootId {
        self.root_id
    }

    /// Returns the exact runtime-native root path retained by the binding.
    #[must_use]
    pub const fn root_path(&self) -> &RuntimeNativePath {
        &self.root_path
    }
}
