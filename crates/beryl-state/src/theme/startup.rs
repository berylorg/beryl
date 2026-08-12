use std::{error::Error, fmt};

use super::{
    InstalledThemeId, ResolvedAppearance, ThemeDocumentIdentity, ThemeFreshnessError,
    ThemeHomeIdentity, ThemeManifestIdentity, ThemeSettingsIdentity, builtin_fallback_appearance,
};

/// Fixed internal identity of the immutable non-installed fallback.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct BuiltinFallback;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeAppearanceSource {
    BuiltinFallback(BuiltinFallback),
    Installed(ThemeDocumentIdentity),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedThemeAppearance {
    home: ThemeHomeIdentity,
    settings: ThemeSettingsIdentity,
    source: ThemeAppearanceSource,
    appearance: ResolvedAppearance,
}

impl PreparedThemeAppearance {
    pub fn installed(
        settings: ThemeSettingsIdentity,
        active: &InstalledThemeId,
        document: ThemeDocumentIdentity,
        appearance: ResolvedAppearance,
    ) -> Result<Self, ThemeStartupError> {
        if settings.home() != document.manifest().home() {
            return Err(ThemeStartupError::Freshness(
                ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        if document.theme_id() != active {
            return Err(ThemeStartupError::ActiveIdentityMismatch);
        }
        Ok(Self {
            home: settings.home(),
            settings,
            source: ThemeAppearanceSource::Installed(document),
            appearance,
        })
    }

    #[must_use]
    pub fn fallback(settings: ThemeSettingsIdentity) -> Self {
        Self {
            home: settings.home(),
            settings,
            source: ThemeAppearanceSource::BuiltinFallback(BuiltinFallback),
            appearance: builtin_fallback_appearance(),
        }
    }

    #[must_use]
    pub const fn home(&self) -> ThemeHomeIdentity {
        self.home
    }
    #[must_use]
    pub const fn settings(&self) -> ThemeSettingsIdentity {
        self.settings
    }
    #[must_use]
    pub const fn source(&self) -> &ThemeAppearanceSource {
        &self.source
    }
    #[must_use]
    pub const fn appearance(&self) -> &ResolvedAppearance {
        &self.appearance
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeLoadFailure {
    RepositoryUnavailable,
    ActiveIdentityMissing,
    DocumentMissing,
    DocumentUnreadable,
    DocumentInvalid,
    ApplicationUnavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeStartupOutcome {
    appearance: PreparedThemeAppearance,
    failure: Option<ThemeLoadFailure>,
}

impl ThemeStartupOutcome {
    pub fn evaluate(
        settings: ThemeSettingsIdentity,
        active: Option<&InstalledThemeId>,
        loaded: Result<PreparedThemeAppearance, ThemeLoadFailure>,
    ) -> Result<Self, ThemeStartupError> {
        let Some(active) = active else {
            return Ok(Self {
                appearance: PreparedThemeAppearance::fallback(settings),
                failure: None,
            });
        };
        match loaded {
            Ok(appearance) => {
                if appearance.settings != settings {
                    return Err(ThemeStartupError::Freshness(
                        ThemeFreshnessError::StaleSettings,
                    ));
                }
                match appearance.source() {
                    ThemeAppearanceSource::Installed(document) if document.theme_id() == active => {
                        Ok(Self {
                            appearance,
                            failure: None,
                        })
                    }
                    _ => Err(ThemeStartupError::ActiveIdentityMismatch),
                }
            }
            Err(failure) => Ok(Self {
                appearance: PreparedThemeAppearance::fallback(settings),
                failure: Some(failure),
            }),
        }
    }
    #[must_use]
    pub const fn appearance(&self) -> &PreparedThemeAppearance {
        &self.appearance
    }
    #[must_use]
    pub const fn failure(&self) -> Option<&ThemeLoadFailure> {
        self.failure.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeRefreshInput {
    prior: PreparedThemeAppearance,
    candidate_manifest: ThemeManifestIdentity,
}

impl ThemeRefreshInput {
    pub fn new(
        prior_manifest: ThemeManifestIdentity,
        prior: PreparedThemeAppearance,
        candidate_manifest: ThemeManifestIdentity,
    ) -> Result<Self, ThemeStartupError> {
        if prior_manifest.home() != prior.home() || candidate_manifest.home() != prior.home() {
            return Err(ThemeStartupError::Freshness(
                ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        if candidate_manifest.generation() <= prior_manifest.generation() {
            return Err(ThemeStartupError::Freshness(
                ThemeFreshnessError::StaleManifest,
            ));
        }
        Ok(Self {
            prior,
            candidate_manifest,
        })
    }
    pub fn accept(
        self,
        candidate: PreparedThemeAppearance,
    ) -> Result<PreparedThemeAppearance, ThemeStartupError> {
        if candidate.home() != self.candidate_manifest.home() {
            return Err(ThemeStartupError::Freshness(
                ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        if candidate.settings() != self.prior.settings() {
            return Err(ThemeStartupError::Freshness(
                ThemeFreshnessError::StaleSettings,
            ));
        }
        if matches!(self.prior.source(), ThemeAppearanceSource::Installed(_))
            && matches!(
                candidate.source(),
                ThemeAppearanceSource::BuiltinFallback(_)
            )
        {
            return Err(ThemeStartupError::ActiveIdentityMismatch);
        }
        if let ThemeAppearanceSource::Installed(document) = candidate.source()
            && document.manifest() != self.candidate_manifest
        {
            return Err(ThemeStartupError::Freshness(
                ThemeFreshnessError::StaleDocument,
            ));
        }
        Ok(candidate)
    }
    #[must_use]
    pub fn preserve_prior(self) -> PreparedThemeAppearance {
        self.prior
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeStartupError {
    Freshness(ThemeFreshnessError),
    ActiveIdentityMismatch,
}
impl fmt::Display for ThemeStartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for ThemeStartupError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeLiveEditFailure {
    Missing,
    Unreadable,
    OverLimit,
    InvalidDocument,
    ResolutionFailed,
    Superseded,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ThemeLiveEditOutcome {
    Unchanged {
        coherent: PreparedThemeAppearance,
    },
    Prepared {
        replacement: PreparedThemeAppearance,
    },
    Retained {
        coherent: PreparedThemeAppearance,
        observed: Option<ThemeDocumentIdentity>,
        failure: ThemeLiveEditFailure,
    },
}

impl ThemeLiveEditOutcome {
    pub fn valid(
        prior: PreparedThemeAppearance,
        observed: ThemeDocumentIdentity,
        appearance: ResolvedAppearance,
    ) -> Result<Self, ThemeStartupError> {
        if prior.home() != observed.manifest().home() {
            return Err(ThemeStartupError::Freshness(
                ThemeFreshnessError::StaleOrForeignHome,
            ));
        }
        let active = match prior.source() {
            ThemeAppearanceSource::Installed(current) => {
                if current.manifest() != observed.manifest()
                    || current.theme_id() != observed.theme_id()
                {
                    return Err(ThemeStartupError::Freshness(
                        ThemeFreshnessError::StaleDocument,
                    ));
                }
                if current == &observed {
                    return Ok(Self::Unchanged { coherent: prior });
                }
                current.theme_id()
            }
            ThemeAppearanceSource::BuiltinFallback(_) => {
                return Err(ThemeStartupError::ActiveIdentityMismatch);
            }
        };
        let replacement =
            PreparedThemeAppearance::installed(prior.settings(), active, observed, appearance)?;
        Ok(Self::Prepared { replacement })
    }

    pub fn invalid(
        coherent: PreparedThemeAppearance,
        observed: Option<ThemeDocumentIdentity>,
        failure: ThemeLiveEditFailure,
    ) -> Result<Self, ThemeStartupError> {
        if let Some(observed) = &observed {
            if coherent.home() != observed.manifest().home() {
                return Err(ThemeStartupError::Freshness(
                    ThemeFreshnessError::StaleOrForeignHome,
                ));
            }
            match coherent.source() {
                ThemeAppearanceSource::Installed(current)
                    if current.theme_id() == observed.theme_id() => {}
                _ => return Err(ThemeStartupError::ActiveIdentityMismatch),
            }
        }
        Ok(Self::Retained {
            coherent,
            observed,
            failure,
        })
    }
}
