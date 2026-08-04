use beryl_model::SyndicContentId;

use crate::{ContentEncoding, ContentPieceOrdinal, ContentReference, ProviderNarrativeReference};

/// One closed exact logical-text source selected for transcript projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionTextSource {
    Composer(ContentReference),
    ProviderNarrative(ProviderNarrativeReference),
}

impl ProjectionTextSource {
    #[must_use]
    pub const fn composer(content: ContentReference) -> Self {
        Self::Composer(content)
    }

    #[must_use]
    pub const fn provider_narrative(narrative: ProviderNarrativeReference) -> Self {
        Self::ProviderNarrative(narrative)
    }

    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        match self {
            Self::Composer(content) => content.id(),
            Self::ProviderNarrative(narrative) => narrative.content_id(),
        }
    }

    #[must_use]
    pub const fn logical_utf8_bytes(self) -> u64 {
        match self {
            Self::Composer(content) => content.summary().logical_utf8_bytes(),
            Self::ProviderNarrative(narrative) => narrative.logical_utf8_bytes(),
        }
    }

    #[must_use]
    pub const fn composer_content(self) -> Option<ContentReference> {
        match self {
            Self::Composer(content) => Some(content),
            Self::ProviderNarrative(_) => None,
        }
    }

    #[must_use]
    pub const fn provider_reference(self) -> Option<ProviderNarrativeReference> {
        match self {
            Self::Composer(_) => None,
            Self::ProviderNarrative(narrative) => Some(narrative),
        }
    }

    #[must_use]
    pub const fn initial_cursor(self) -> ProjectionTextSourceCursor {
        match self {
            Self::Composer(_) => ProjectionTextSourceCursor::Composer(ContentPieceOrdinal::FIRST),
            Self::ProviderNarrative(_) => {
                ProjectionTextSourceCursor::ProviderNarrative { logical_start: 0 }
            }
        }
    }

    /// Structural prefix compatibility; append-only family validation proves the physical prefix.
    #[must_use]
    pub fn can_extend(self, next: Self) -> bool {
        match (self, next) {
            (Self::Composer(previous), Self::Composer(current)) => {
                previous.encoding() == ContentEncoding::ComposerV1 && previous == current
            }
            (Self::ProviderNarrative(previous), Self::ProviderNarrative(current)) => {
                previous.content_id() == current.content_id()
                    && previous.generation().get() == current.generation().get()
                    && previous.span_count() <= current.span_count()
                    && previous.logical_utf8_bytes() <= current.logical_utf8_bytes()
                    && (previous.span_count() != current.span_count()
                        || previous.chain_digest() == current.chain_digest())
            }
            (Self::Composer(_), Self::ProviderNarrative(_))
            | (Self::ProviderNarrative(_), Self::Composer(_)) => false,
        }
    }
}

/// Source-typed physical cursor retained by a bounded Markdown parser checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionTextSourceCursor {
    Composer(ContentPieceOrdinal),
    ProviderNarrative { logical_start: u64 },
}

impl ProjectionTextSourceCursor {
    #[must_use]
    pub const fn is_initial(self) -> bool {
        match self {
            Self::Composer(ordinal) => ordinal.get() == ContentPieceOrdinal::FIRST.get(),
            Self::ProviderNarrative { logical_start } => logical_start == 0,
        }
    }

    #[must_use]
    pub const fn composer_piece(self) -> Option<ContentPieceOrdinal> {
        match self {
            Self::Composer(ordinal) => Some(ordinal),
            Self::ProviderNarrative { .. } => None,
        }
    }

    #[must_use]
    pub const fn provider_logical_start(self) -> Option<u64> {
        match self {
            Self::Composer(_) => None,
            Self::ProviderNarrative { logical_start } => Some(logical_start),
        }
    }
}
