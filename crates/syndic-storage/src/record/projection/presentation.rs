use super::*;

/// Closed canonical item classification retained below transcript projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalItemKind {
    UserInput,
    AssistantMessage(AssistantMessagePhase),
    ProviderText(ProviderItemKind),
    Operational(ProviderItemKind),
    Activity(ProviderItemKind),
    GeneratedMedia,
}

/// Closed presentation policy retained independently from exact provider bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalItemPresentation {
    UserInput {
        content: ContentReference,
        asset_reference_set: Option<Box<SealedAssetReferenceSetProof>>,
    },
    Narrative,
    Operational,
    Activity,
    GeneratedMedia {
        resource_id: SyndicResourceId,
    },
}

impl CanonicalItemPresentation {
    #[must_use]
    pub fn user_input(
        content: ContentReference,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
    ) -> Self {
        Self::UserInput {
            content,
            asset_reference_set: asset_reference_set.map(Box::new),
        }
    }

    #[must_use]
    pub const fn content(&self) -> Option<ContentReference> {
        match self {
            Self::UserInput { content, .. } => Some(*content),
            Self::Narrative | Self::Operational | Self::Activity | Self::GeneratedMedia { .. } => {
                None
            }
        }
    }

    #[must_use]
    pub fn asset_reference_set(&self) -> Option<SealedAssetReferenceSetProof> {
        match self {
            Self::UserInput {
                asset_reference_set,
                ..
            } => asset_reference_set.as_deref().copied(),
            Self::Narrative | Self::Operational | Self::Activity | Self::GeneratedMedia { .. } => {
                None
            }
        }
    }

    #[must_use]
    pub const fn resource_id(&self) -> Option<SyndicResourceId> {
        match self {
            Self::GeneratedMedia { resource_id } => Some(*resource_id),
            Self::UserInput { .. } | Self::Narrative | Self::Operational | Self::Activity => None,
        }
    }
}
