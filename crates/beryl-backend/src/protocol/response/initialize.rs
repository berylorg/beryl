use thiserror::Error;

use super::{BoundedResponseTextError, ProtocolIdentity};

pub const REQUIRED_CODEX_APP_SERVER_VERSION: &str = "0.146.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializePlatform {
    HostWindows,
    WslLinux,
}

impl InitializePlatform {
    #[must_use]
    pub fn from_wire_pair(family: &str, os: &str) -> Option<Self> {
        match (family, os) {
            ("windows", "windows") => Some(Self::HostWindows),
            ("unix", "linux") => Some(Self::WslLinux),
            _ => None,
        }
    }

    #[must_use]
    pub const fn family(self) -> &'static str {
        match self {
            Self::HostWindows => "windows",
            Self::WslLinux => "unix",
        }
    }

    #[must_use]
    pub const fn os(self) -> &'static str {
        match self {
            Self::HostWindows => "windows",
            Self::WslLinux => "linux",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializeResponse {
    user_agent_product: ProtocolIdentity,
    platform: InitializePlatform,
}

impl InitializeResponse {
    pub fn try_new(
        user_agent_product: &str,
        platform: InitializePlatform,
    ) -> Result<Self, BoundedResponseTextError> {
        if user_agent_product.chars().any(char::is_whitespace) {
            return Err(BoundedResponseTextError::InvalidUserAgentProduct);
        }
        Ok(Self {
            user_agent_product: ProtocolIdentity::try_new(user_agent_product)?,
            platform,
        })
    }

    #[must_use]
    pub fn user_agent_product(&self) -> &str {
        self.user_agent_product.as_str()
    }

    #[must_use]
    pub const fn platform(&self) -> InitializePlatform {
        self.platform
    }

    /// Validates the managed client-shaped product token against the pinned app-server release.
    pub fn validate_required_app_server_version(&self) -> Result<(), CompatibilityError> {
        validate_required_app_server_version(self.user_agent_product())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendConfigDefaults {
    model: Option<ProtocolIdentity>,
    model_reasoning_effort: Option<ProtocolIdentity>,
    multi_agent_v2_enabled: bool,
    expose_spawn_agent_model_overrides: bool,
}

impl BackendConfigDefaults {
    #[must_use]
    pub const fn new(
        model: Option<ProtocolIdentity>,
        model_reasoning_effort: Option<ProtocolIdentity>,
        multi_agent_v2_enabled: bool,
        expose_spawn_agent_model_overrides: bool,
    ) -> Self {
        Self {
            model,
            model_reasoning_effort,
            multi_agent_v2_enabled,
            expose_spawn_agent_model_overrides,
        }
    }

    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_ref().map(ProtocolIdentity::as_str)
    }

    #[must_use]
    pub fn model_reasoning_effort(&self) -> Option<&str> {
        self.model_reasoning_effort
            .as_ref()
            .map(ProtocolIdentity::as_str)
    }

    #[must_use]
    pub const fn multi_agent_v2_enabled(&self) -> bool {
        self.multi_agent_v2_enabled
    }

    #[must_use]
    pub const fn expose_spawn_agent_model_overrides(&self) -> bool {
        self.expose_spawn_agent_model_overrides
    }

    #[must_use]
    pub const fn proves_spawn_agent_model_overrides(&self) -> bool {
        self.multi_agent_v2_enabled && self.expose_spawn_agent_model_overrides
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConfigReadResponse {
    defaults: BackendConfigDefaults,
}

impl ConfigReadResponse {
    #[must_use]
    pub const fn new(defaults: BackendConfigDefaults) -> Self {
        Self { defaults }
    }

    #[must_use]
    pub const fn defaults(&self) -> &BackendConfigDefaults {
        &self.defaults
    }

    #[must_use]
    pub fn into_defaults(self) -> BackendConfigDefaults {
        self.defaults
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CompatibilityError {
    #[error(
        "backend initialize product did not identify beryl with a semantic version; required exactly {required_version}"
    )]
    AppServerVersionUnrecognized { required_version: &'static str },
    #[error(
        "backend Codex App Server version {actual_major}.{actual_minor}.{actual_patch} does not match required {required_version}"
    )]
    AppServerVersionMismatch {
        required_version: &'static str,
        actual_major: u16,
        actual_minor: u16,
        actual_patch: u16,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CodexAppServerVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl CodexAppServerVersion {
    fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
        let major = parse_version_component(parts.next()?)?;
        let minor = parse_version_component(parts.next()?)?;
        let patch = parse_version_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

fn validate_required_app_server_version(product: &str) -> Result<(), CompatibilityError> {
    let actual = product
        .split_once('/')
        .filter(|(name, _)| *name == "beryl")
        .and_then(|(_, version)| CodexAppServerVersion::parse(version))
        .ok_or(CompatibilityError::AppServerVersionUnrecognized {
            required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
        })?;
    let required = CodexAppServerVersion::parse(REQUIRED_CODEX_APP_SERVER_VERSION)
        .expect("pinned Codex App Server version is valid");
    if actual != required {
        return Err(CompatibilityError::AppServerVersionMismatch {
            required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
            actual_major: actual.major,
            actual_minor: actual.minor,
            actual_patch: actual.patch,
        });
    }
    Ok(())
}

fn parse_version_component(value: &str) -> Option<u16> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse().ok()
}
