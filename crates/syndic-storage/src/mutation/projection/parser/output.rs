use crate::{
    ContentPieceRecord, MarkdownBlockKind, MarkdownOpenBlock, ProjectionSourceRange,
    ResourceStructure, SyndicMutationError,
};

use super::{ParserOutput, ParserState, process_line};

pub(super) fn append_fallback(
    state: &mut ParserState,
    start: u64,
    source: &str,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    state.open = Some(MarkdownOpenBlock::Fallback {
        block_start: start,
        next_span_ordinal: 1,
        buffered_source: source.into(),
    });
    flush_open_spans(state, outputs)
}

pub(super) fn append_open_text(
    state: &mut ParserState,
    source: &str,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    state.open = match state.open.take() {
        Some(MarkdownOpenBlock::Paragraph {
            block_start,
            next_span_ordinal,
            buffered_source,
        }) => {
            let mut buffer = buffered_source.into_string();
            buffer.push_str(source);
            Some(MarkdownOpenBlock::Paragraph {
                block_start,
                next_span_ordinal,
                buffered_source: buffer.into_boxed_str(),
            })
        }
        Some(MarkdownOpenBlock::Fallback {
            block_start,
            next_span_ordinal,
            buffered_source,
        }) => {
            let mut buffer = buffered_source.into_string();
            buffer.push_str(source);
            Some(MarkdownOpenBlock::Fallback {
                block_start,
                next_span_ordinal,
                buffered_source: buffer.into_boxed_str(),
            })
        }
        _ => return Err(SyndicMutationError::ProjectionBuildConflict),
    };
    flush_open_spans(state, outputs)
}

pub(super) fn flush_open_spans(
    state: &mut ParserState,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    loop {
        let Some(block) = state.open.take() else {
            return Ok(());
        };
        let (block_start, kind, mut next_span, mut buffer) = match block {
            MarkdownOpenBlock::Paragraph {
                block_start,
                next_span_ordinal,
                buffered_source,
            } => (
                block_start,
                MarkdownBlockKind::Paragraph,
                next_span_ordinal,
                buffered_source.into_string(),
            ),
            MarkdownOpenBlock::Fallback {
                block_start,
                next_span_ordinal,
                buffered_source,
            } => (
                block_start,
                MarkdownBlockKind::Fallback,
                next_span_ordinal,
                buffered_source.into_string(),
            ),
            other => {
                state.open = Some(other);
                return Ok(());
            }
        };
        let must_split = if next_span == 1 {
            buffer.len() > crate::MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES
        } else {
            buffer.len() > crate::MARKDOWN_SPAN_MAX_BYTES
        };
        if !must_split {
            state.open = Some(match kind {
                MarkdownBlockKind::Paragraph => MarkdownOpenBlock::Paragraph {
                    block_start,
                    next_span_ordinal: next_span,
                    buffered_source: buffer.into_boxed_str(),
                },
                MarkdownBlockKind::Fallback => MarkdownOpenBlock::Fallback {
                    block_start,
                    next_span_ordinal: next_span,
                    buffered_source: buffer.into_boxed_str(),
                },
                _ => unreachable!("only text-span blocks enter this helper"),
            });
            return Ok(());
        }
        let split = utf8_prefix(&buffer, crate::MARKDOWN_SPAN_MAX_BYTES);
        let end = state
            .closed
            .checked_add(split as u64)
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        emit_inline(
            state,
            block_start,
            kind,
            next_span,
            state.closed,
            end,
            outputs,
        )?;
        buffer.drain(..split);
        next_span = next_span
            .checked_add(1)
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        state.open = Some(match kind {
            MarkdownBlockKind::Paragraph => MarkdownOpenBlock::Paragraph {
                block_start,
                next_span_ordinal: next_span,
                buffered_source: buffer.into_boxed_str(),
            },
            MarkdownBlockKind::Fallback => MarkdownOpenBlock::Fallback {
                block_start,
                next_span_ordinal: next_span,
                buffered_source: buffer.into_boxed_str(),
            },
            _ => unreachable!("only text-span blocks enter this helper"),
        });
    }
}

pub(super) fn finish_boundary(
    state: &mut ParserState,
    boundary: u64,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    if !state.line_carry.is_empty() {
        let start = state.line_start;
        let continuation = state.line_continuation;
        let line = std::mem::take(&mut state.line_carry);
        state.line_start = boundary;
        state.line_continuation = false;
        process_line(state, start, &line, continuation, outputs)?;
    }
    finish_open_block(state, boundary, outputs)
}

pub(super) fn finish_open_block(
    state: &mut ParserState,
    boundary: u64,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    let Some(block) = state.open.take() else {
        state.closed = boundary;
        return Ok(());
    };
    match block {
        MarkdownOpenBlock::Paragraph {
            block_start,
            next_span_ordinal,
            buffered_source,
        } => {
            if !buffered_source.is_empty() {
                let start = if next_span_ordinal == 1 {
                    block_start
                } else {
                    state.closed
                };
                emit_inline(
                    state,
                    block_start,
                    MarkdownBlockKind::Paragraph,
                    next_span_ordinal,
                    start,
                    boundary,
                    outputs,
                )?;
            } else {
                state.closed = boundary;
            }
        }
        MarkdownOpenBlock::Fallback {
            block_start,
            next_span_ordinal,
            buffered_source,
        } => {
            if !buffered_source.is_empty() {
                let start = if next_span_ordinal == 1 {
                    block_start
                } else {
                    state.closed
                };
                emit_inline(
                    state,
                    block_start,
                    MarkdownBlockKind::Fallback,
                    next_span_ordinal,
                    start,
                    boundary,
                    outputs,
                )?;
            } else {
                state.closed = boundary;
            }
        }
        MarkdownOpenBlock::FencedCode {
            block_start,
            body_start,
            language,
            preview,
            logical_lines,
            body_bytes,
            resource_digest,
            ..
        } => emit_code(
            state,
            block_start,
            body_start,
            boundary,
            boundary,
            language,
            preview,
            logical_lines,
            body_bytes,
            resource_digest,
            outputs,
        )?,
        MarkdownOpenBlock::Table {
            block_start,
            header_end,
            columns,
            body_rows,
            preview,
            source_bytes,
            resource_digest,
        } => emit_table(
            state,
            block_start,
            header_end,
            boundary,
            columns,
            body_rows,
            preview,
            source_bytes,
            resource_digest,
            outputs,
        )?,
    }
    Ok(())
}

pub(super) fn emit_inline(
    state: &mut ParserState,
    block_start: u64,
    kind: MarkdownBlockKind,
    span_ordinal: u64,
    start: u64,
    end: u64,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    if start < end {
        outputs.push(ParserOutput::Inline {
            block_start,
            kind,
            span_ordinal,
            range: ProjectionSourceRange::new(start, end)?,
        });
    }
    state.closed = end;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_code(
    state: &mut ParserState,
    block_start: u64,
    body_start: u64,
    body_end: u64,
    block_end: u64,
    language: Option<Box<str>>,
    preview: Box<str>,
    logical_lines: u64,
    body_bytes: u64,
    resource_digest: [u8; 32],
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    if body_bytes > crate::MARKDOWN_CODE_INLINE_MAX_BYTES as u64
        || logical_lines > crate::MARKDOWN_CODE_INLINE_MAX_LINES
    {
        outputs.push(ParserOutput::Resource {
            block_start,
            kind: MarkdownBlockKind::FencedCode,
            range: ProjectionSourceRange::new(body_start, body_end)?,
            preview,
            structure: ResourceStructure::code(language.as_deref(), logical_lines)?,
            digest: resource_digest,
        });
        state.closed = block_end;
        Ok(())
    } else {
        emit_inline(
            state,
            block_start,
            MarkdownBlockKind::FencedCode,
            1,
            block_start,
            block_end,
            outputs,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_table(
    state: &mut ParserState,
    block_start: u64,
    header_end: u64,
    block_end: u64,
    columns: u64,
    body_rows: u64,
    preview: Box<str>,
    source_bytes: u64,
    resource_digest: [u8; 32],
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    if source_bytes > crate::MARKDOWN_TABLE_INLINE_MAX_BYTES as u64
        || body_rows > crate::MARKDOWN_TABLE_INLINE_MAX_BODY_ROWS
        || columns > crate::MARKDOWN_TABLE_INLINE_MAX_COLUMNS
    {
        outputs.push(ParserOutput::Resource {
            block_start,
            kind: MarkdownBlockKind::Table,
            range: ProjectionSourceRange::new(block_start, block_end)?,
            preview,
            structure: ResourceStructure::table(
                body_rows,
                columns,
                ProjectionSourceRange::new(0, header_end - block_start)?,
            )?,
            digest: resource_digest,
        });
        state.closed = block_end;
        Ok(())
    } else {
        emit_inline(
            state,
            block_start,
            MarkdownBlockKind::Table,
            1,
            block_start,
            block_end,
            outputs,
        )
    }
}

pub(super) fn emit_marker(
    state: &mut ParserState,
    marker: ContentPieceRecord,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    let ContentPieceRecord::ImageMarker {
        atom_ordinal,
        marker_ordinal,
        logical_offset,
        marker_id,
        label,
        ..
    } = marker
    else {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    };
    outputs.push(ParserOutput::ImageMarker {
        atom_ordinal,
        marker_ordinal,
        source_offset: logical_offset,
        marker_id,
        label,
    });
    state.closed = logical_offset;
    state.line_start = logical_offset;
    Ok(())
}

pub(super) fn append_preview(preview: &mut Box<str>, source: &str, maximum: usize) {
    if preview.len() >= maximum {
        return;
    }
    let available = maximum - preview.len();
    let take = utf8_prefix(source, available);
    let mut value = std::mem::take(preview).into_string();
    value.push_str(&source[..take]);
    *preview = value.into_boxed_str();
}

pub(super) fn utf8_prefix(source: &str, maximum: usize) -> usize {
    let mut end = source.len().min(maximum);
    while end != 0 && !source.is_char_boundary(end) {
        end -= 1;
    }
    end
}
