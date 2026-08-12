use std::num::NonZeroU64;

use beryl_state::{
    PreparedThemeAppearance, ThemeAppearanceSource, ThemeDocumentDigest, ThemeDocumentIdentity,
    ThemeDraftIdentity, ThemeDraftRevision, ThemeSettingsIdentity,
};

use super::{
    GenerationExhausted, PreviewSequenceExhausted, PreviewSourceError, WindowEpochExhausted,
};

macro_rules! monotonic_identity {
    ($name:ident, $error:ty) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU64);

        impl $name {
            #[must_use]
            pub const fn initial() -> Self {
                Self(NonZeroU64::MIN)
            }

            #[must_use]
            pub const fn from_nonzero(value: NonZeroU64) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn get(self) -> NonZeroU64 {
                self.0
            }

            pub fn checked_next(self) -> Result<Self, $error> {
                let next = self.0.get().checked_add(1).ok_or(<$error>::default())?;
                Ok(Self(NonZeroU64::new(next).ok_or(<$error>::default())?))
            }
        }
    };
}

monotonic_identity!(AppearanceGenerationNumber, GenerationExhausted);
monotonic_identity!(WindowSetEpoch, WindowEpochExhausted);
monotonic_identity!(PreviewSequence, PreviewSequenceExhausted);

/// Bounded process-local identity carried by a preview source.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PreviewSourceIdentity(NonZeroU64);

impl PreviewSourceIdentity {
    pub fn try_new(value: u64) -> Result<Self, PreviewSourceError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(PreviewSourceError::ZeroIdentity)
    }

    #[must_use]
    pub const fn get(self) -> NonZeroU64 {
        self.0
    }
}

/// Feature boundary that originated a preview request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PreviewSource {
    TranscriptCandidate(PreviewSourceIdentity),
    DynamicTool(PreviewSourceIdentity),
}

impl PreviewSource {
    #[must_use]
    pub const fn kind(self) -> PreviewSourceKind {
        match self {
            Self::TranscriptCandidate(_) => PreviewSourceKind::TranscriptCandidate,
            Self::DynamicTool(_) => PreviewSourceKind::DynamicTool,
        }
    }
}

/// Content-free source class suitable for diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PreviewSourceKind {
    TranscriptCandidate,
    DynamicTool,
}

/// Exact candidate authority retained by one bounded preview request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewCandidateIdentity {
    Document(ThemeDocumentIdentity),
    Draft {
        draft: ThemeDraftIdentity,
        revision: ThemeDraftRevision,
    },
    Digest(ThemeDocumentDigest),
}

/// Exact reason and authority for replacing the durable base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurablePublicationIdentity {
    ActiveDocument(ThemeDocumentIdentity),
    RepositoryRefresh(ThemeAppearanceSource),
    Settings {
        draft: ThemeDraftIdentity,
        revision: ThemeDraftRevision,
        committed: ThemeSettingsIdentity,
    },
}

impl DurablePublicationIdentity {
    pub(crate) const fn ends_preview(&self) -> bool {
        matches!(self, Self::Settings { .. })
    }
}

/// Why one immutable generation exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppearancePublication {
    Durable,
    Preview {
        source: PreviewSource,
        candidate: PreviewCandidateIdentity,
        sequence: PreviewSequence,
    },
}

/// One complete finite appearance offered atomically to all eligible adapters.
#[derive(Clone, Debug, PartialEq)]
pub struct AppearanceGeneration {
    number: AppearanceGenerationNumber,
    prepared: PreparedThemeAppearance,
    publication: AppearancePublication,
}

impl AppearanceGeneration {
    pub(crate) fn new(
        number: AppearanceGenerationNumber,
        prepared: PreparedThemeAppearance,
        publication: AppearancePublication,
    ) -> Self {
        Self {
            number,
            prepared,
            publication,
        }
    }

    #[must_use]
    pub const fn number(&self) -> AppearanceGenerationNumber {
        self.number
    }

    #[must_use]
    pub const fn prepared(&self) -> &PreparedThemeAppearance {
        &self.prepared
    }

    #[must_use]
    pub const fn publication(&self) -> &AppearancePublication {
        &self.publication
    }

    #[must_use]
    pub const fn is_preview(&self) -> bool {
        matches!(&self.publication, AppearancePublication::Preview { .. })
    }
}
