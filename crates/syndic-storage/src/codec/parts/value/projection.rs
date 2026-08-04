use super::*;

pub(crate) fn enc_projection_lifecycle(e: &mut Encoder, value: crate::ProjectionLifecycle) {
    e.u8(match value {
        crate::ProjectionLifecycle::Current => 0,
        crate::ProjectionLifecycle::Stale => 1,
    });
}

pub(crate) fn dec_projection_lifecycle(
    d: &mut Decoder<'_>,
) -> Result<crate::ProjectionLifecycle, CodecError> {
    match d.u8()? {
        0 => Ok(crate::ProjectionLifecycle::Current),
        1 => Ok(crate::ProjectionLifecycle::Stale),
        tag => Err(CodecError::InvalidTag {
            kind: "projection lifecycle",
            tag,
        }),
    }
}

pub(crate) fn enc_projection_format(e: &mut Encoder, value: crate::ProjectionFormatVersion) {
    e.u8(match value {
        crate::ProjectionFormatVersion::V1 => 1,
    });
}

pub(crate) fn dec_projection_format(
    d: &mut Decoder<'_>,
) -> Result<crate::ProjectionFormatVersion, CodecError> {
    match d.u8()? {
        1 => Ok(crate::ProjectionFormatVersion::V1),
        tag => Err(CodecError::InvalidTag {
            kind: "projection format",
            tag,
        }),
    }
}

pub(crate) fn enc_projection_source_range(e: &mut Encoder, value: crate::ProjectionSourceRange) {
    e.u64(value.start());
    e.u64(value.end());
}

pub(crate) fn dec_projection_source_range(
    d: &mut Decoder<'_>,
    kind: &'static str,
) -> Result<crate::ProjectionSourceRange, CodecError> {
    crate::ProjectionSourceRange::new(d.u64()?, d.u64()?).map_err(|source| invalid(kind, source))
}

pub(crate) fn enc_markdown_block_kind(e: &mut Encoder, value: crate::MarkdownBlockKind) {
    match value {
        crate::MarkdownBlockKind::Paragraph => e.u8(0),
        crate::MarkdownBlockKind::Heading(level) => {
            e.u8(1);
            e.u8(level);
        }
        crate::MarkdownBlockKind::BlockQuote => e.u8(2),
        crate::MarkdownBlockKind::List => e.u8(3),
        crate::MarkdownBlockKind::ThematicBreak => e.u8(4),
        crate::MarkdownBlockKind::FencedCode => e.u8(5),
        crate::MarkdownBlockKind::Table => e.u8(6),
        crate::MarkdownBlockKind::Fallback => e.u8(7),
    }
}

pub(crate) fn dec_markdown_block_kind(
    d: &mut Decoder<'_>,
) -> Result<crate::MarkdownBlockKind, CodecError> {
    let value = match d.u8()? {
        0 => crate::MarkdownBlockKind::Paragraph,
        1 => crate::MarkdownBlockKind::Heading(d.u8()?),
        2 => crate::MarkdownBlockKind::BlockQuote,
        3 => crate::MarkdownBlockKind::List,
        4 => crate::MarkdownBlockKind::ThematicBreak,
        5 => crate::MarkdownBlockKind::FencedCode,
        6 => crate::MarkdownBlockKind::Table,
        7 => crate::MarkdownBlockKind::Fallback,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "Markdown block kind",
                tag,
            });
        }
    };
    value
        .validate()
        .map_err(|source| invalid("Markdown block kind", source))
}
