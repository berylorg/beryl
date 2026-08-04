use super::*;

/// One bounded transcript projection payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionPayload {
    Empty,
    InlineMarkdown {
        block_id: MarkdownBlockId,
        block_kind: MarkdownBlockKind,
        span_ordinal: u64,
        source_range: ProjectionSourceRange,
        source: Box<str>,
    },
    ResourceReference {
        block_id: MarkdownBlockId,
        block_kind: MarkdownBlockKind,
        source_range: ProjectionSourceRange,
        resource_id: SyndicResourceId,
        preview: Box<str>,
    },
    ImageMarker {
        atom_ordinal: ComposerAtomOrdinal,
        marker_ordinal: InputMarkerOrdinal,
        source_offset: u64,
        marker: ComposerImageMarker,
    },
}

impl ProjectionPayload {
    #[must_use]
    pub const fn empty() -> Self {
        Self::Empty
    }

    pub fn inline_markdown(
        block_id: MarkdownBlockId,
        block_kind: MarkdownBlockKind,
        span_ordinal: u64,
        source_range: ProjectionSourceRange,
        source: impl AsRef<str>,
    ) -> Result<Self, SyndicRecordError> {
        if span_ordinal == 0 {
            return Err(SyndicRecordError::ZeroValue {
                kind: "Markdown span ordinal",
            });
        }
        let source = validate_text(
            "inline Markdown projection",
            source.as_ref(),
            MAX_INLINE_TEXT_BYTES,
            false,
        )?;
        if source_range.len() != source.len() as u64 {
            return Err(SyndicRecordError::ProjectionSourceLengthMismatch {
                range_bytes: source_range.len(),
                source_bytes: source.len() as u64,
            });
        }
        Ok(Self::InlineMarkdown {
            block_id,
            block_kind: block_kind.validate()?,
            span_ordinal,
            source_range,
            source,
        })
    }

    pub fn resource_reference(
        block_id: MarkdownBlockId,
        block_kind: MarkdownBlockKind,
        source_range: ProjectionSourceRange,
        resource_id: SyndicResourceId,
        preview: impl AsRef<str>,
    ) -> Result<Self, SyndicRecordError> {
        let block_kind = block_kind.validate()?;
        if !matches!(
            block_kind,
            MarkdownBlockKind::FencedCode | MarkdownBlockKind::Table
        ) {
            return Err(SyndicRecordError::InvalidProjectionResourceKind);
        }
        let preview_max = match block_kind {
            MarkdownBlockKind::FencedCode => MARKDOWN_CODE_PREVIEW_MAX_BYTES,
            MarkdownBlockKind::Table => MARKDOWN_TABLE_PREVIEW_MAX_BYTES,
            _ => unreachable!("resource kind was checked above"),
        };
        Ok(Self::ResourceReference {
            block_id,
            block_kind,
            source_range,
            resource_id,
            preview: validate_text(
                "projection resource preview",
                preview.as_ref(),
                preview_max,
                true,
            )?,
        })
    }

    #[must_use]
    pub const fn image_marker(
        atom_ordinal: ComposerAtomOrdinal,
        marker_ordinal: InputMarkerOrdinal,
        source_offset: u64,
        marker: ComposerImageMarker,
    ) -> Self {
        Self::ImageMarker {
            atom_ordinal,
            marker_ordinal,
            source_offset,
            marker,
        }
    }

    #[must_use]
    pub const fn block_id(&self) -> Option<MarkdownBlockId> {
        match self {
            Self::Empty => None,
            Self::InlineMarkdown { block_id, .. } | Self::ResourceReference { block_id, .. } => {
                Some(*block_id)
            }
            Self::ImageMarker { .. } => None,
        }
    }

    #[must_use]
    pub const fn block_kind(&self) -> Option<MarkdownBlockKind> {
        match self {
            Self::Empty => None,
            Self::InlineMarkdown { block_kind, .. }
            | Self::ResourceReference { block_kind, .. } => Some(*block_kind),
            Self::ImageMarker { .. } => None,
        }
    }

    #[must_use]
    pub const fn source_range(&self) -> Option<ProjectionSourceRange> {
        match self {
            Self::Empty => None,
            Self::InlineMarkdown { source_range, .. }
            | Self::ResourceReference { source_range, .. } => Some(*source_range),
            Self::ImageMarker { .. } => None,
        }
    }

    #[must_use]
    pub const fn inline_source(&self) -> Option<&str> {
        match self {
            Self::Empty => None,
            Self::InlineMarkdown { source, .. } => Some(source),
            Self::ResourceReference { .. } | Self::ImageMarker { .. } => None,
        }
    }

    #[must_use]
    pub const fn resource_id(&self) -> Option<SyndicResourceId> {
        match self {
            Self::Empty | Self::InlineMarkdown { .. } => None,
            Self::ResourceReference { resource_id, .. } => Some(*resource_id),
            Self::ImageMarker { .. } => None,
        }
    }

    #[must_use]
    pub const fn image_marker_value(
        &self,
    ) -> Option<(
        ComposerAtomOrdinal,
        InputMarkerOrdinal,
        u64,
        ComposerImageMarker,
    )> {
        match self {
            Self::ImageMarker {
                atom_ordinal,
                marker_ordinal,
                source_offset,
                marker,
            } => Some((*atom_ordinal, *marker_ordinal, *source_offset, *marker)),
            Self::Empty | Self::InlineMarkdown { .. } | Self::ResourceReference { .. } => None,
        }
    }
}
