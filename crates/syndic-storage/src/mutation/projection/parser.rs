use crate::{
    ComposerAtomOrdinal, InputMarkerOrdinal, MarkdownBlockKind, MarkdownOpenBlock,
    MarkdownParserCheckpoint, ProjectionSourceRange, ProjectionTextSourceCursor, ResourceStructure,
    SyndicMutationError,
};

use super::range::LoadedPiece;

mod output;
mod syntax;

use self::{
    output::{
        append_fallback, append_open_text, append_preview, emit_code, emit_inline, emit_marker,
        finish_boundary, finish_open_block, flush_open_spans, utf8_prefix,
    },
    syntax::{
        is_blank, is_closing_fence, looks_like_table_row, one_line_block_kind, opening_fence,
        single_logical_line, starts_distinct_block, table_columns, table_delimiter_columns,
    },
};

const MAX_STEP_OUTPUTS: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParserOutput {
    Inline {
        block_start: u64,
        kind: MarkdownBlockKind,
        span_ordinal: u64,
        range: ProjectionSourceRange,
    },
    Resource {
        block_start: u64,
        kind: MarkdownBlockKind,
        range: ProjectionSourceRange,
        preview: Box<str>,
        structure: ResourceStructure,
        digest: [u8; 32],
    },
    ImageMarker {
        atom_ordinal: ComposerAtomOrdinal,
        marker_ordinal: InputMarkerOrdinal,
        source_offset: u64,
        marker_id: beryl_model::SyndicDraftMarkerId,
        label: crate::ImageLabelOrdinal,
    },
    Empty,
}

pub(crate) struct ParserStep {
    pub(crate) checkpoint: MarkdownParserCheckpoint,
    pub(crate) outputs: Vec<ParserOutput>,
    pub(crate) finished: bool,
}

struct ParserState {
    consumed: u64,
    closed: u64,
    source_cursor: ProjectionTextSourceCursor,
    line_start: u64,
    line_carry: String,
    line_continuation: bool,
    open: Option<MarkdownOpenBlock>,
}

pub(crate) fn advance(
    checkpoint: &MarkdownParserCheckpoint,
    piece: LoadedPiece,
) -> Result<ParserStep, SyndicMutationError> {
    let mut state = ParserState {
        consumed: checkpoint.consumed_source_bytes(),
        closed: checkpoint.closed_source_bytes(),
        source_cursor: checkpoint.source_cursor(),
        line_start: checkpoint.line_start(),
        line_carry: checkpoint.line_carry().to_owned(),
        line_continuation: checkpoint.line_continuation(),
        open: checkpoint.open_block().cloned(),
    };
    let mut outputs = Vec::new();
    let finished = match piece {
        LoadedPiece::Text {
            logical_end,
            source,
            next_cursor,
        } => {
            let consumed = consume_text(&mut state, &source, &mut outputs)?;
            if consumed == source.len() {
                state.source_cursor = next_cursor;
            }
            if state.consumed > logical_end {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            false
        }
        LoadedPiece::ImageMarker(marker) => {
            let boundary = state.consumed;
            finish_boundary(&mut state, boundary, &mut outputs)?;
            emit_marker(&mut state, marker, &mut outputs)?;
            let Some(next_piece) = state.source_cursor.composer_piece() else {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            };
            state.source_cursor = ProjectionTextSourceCursor::Composer(next_piece.checked_next()?);
            false
        }
        LoadedPiece::End => {
            let boundary = state.consumed;
            finish_boundary(&mut state, boundary, &mut outputs)?;
            if state.consumed == 0 && state.source_cursor.is_initial() && outputs.is_empty() {
                outputs.push(ParserOutput::Empty);
            }
            true
        }
    };
    Ok(ParserStep {
        checkpoint: MarkdownParserCheckpoint::new(
            state.consumed,
            state.closed,
            state.source_cursor,
            state.line_start,
            state.line_carry.into_boxed_str(),
            state.line_continuation,
            state.open,
        ),
        outputs,
        finished,
    })
}

fn consume_text(
    state: &mut ParserState,
    source: &str,
    outputs: &mut Vec<ParserOutput>,
) -> Result<usize, SyndicMutationError> {
    let mut offset = 0_usize;
    while offset < source.len() && outputs.len() < MAX_STEP_OUTPUTS {
        let remaining = &source[offset..];
        let take = remaining
            .as_bytes()
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(remaining.len(), |index| index + 1);
        let segment = &remaining[..take];
        state.line_carry.push_str(segment);
        state.consumed = state
            .consumed
            .checked_add(segment.len() as u64)
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        offset += take;
        spill_line_carry(state, outputs)?;
        if segment.ends_with('\n') {
            let start = state.line_start;
            let continuation = state.line_continuation;
            let line = std::mem::take(&mut state.line_carry);
            state.line_start = state.consumed;
            state.line_continuation = false;
            process_line(state, start, &line, continuation, outputs)?;
        }
    }
    Ok(offset)
}

fn spill_line_carry(
    state: &mut ParserState,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    while state.line_carry.len() > crate::MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES {
        let split = utf8_prefix(&state.line_carry, crate::MARKDOWN_SPAN_MAX_BYTES);
        let fragment = state.line_carry[..split].to_owned();
        state.line_carry.drain(..split);
        let start = state.line_start;
        process_long_line_fragment(state, start, &fragment, outputs)?;
        state.line_start = state
            .line_start
            .checked_add(split as u64)
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        state.line_continuation = true;
    }
    Ok(())
}

fn process_long_line_fragment(
    state: &mut ParserState,
    start: u64,
    fragment: &str,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    match state.open.take() {
        Some(MarkdownOpenBlock::FencedCode {
            block_start,
            body_start,
            fence,
            language,
            mut preview,
            logical_lines,
            body_bytes,
            resource_digest,
        }) => {
            append_preview(
                &mut preview,
                fragment,
                crate::MARKDOWN_CODE_PREVIEW_MAX_BYTES,
            );
            state.open = Some(MarkdownOpenBlock::FencedCode {
                block_start,
                body_start,
                fence,
                language,
                preview,
                logical_lines,
                body_bytes: body_bytes
                    .checked_add(fragment.len() as u64)
                    .ok_or(SyndicMutationError::ProjectionBuildConflict)?,
                resource_digest: crate::projection::advance_resource_content_digest(
                    resource_digest,
                    fragment.as_bytes(),
                ),
            });
        }
        Some(table @ MarkdownOpenBlock::Table { .. }) => {
            state.open = Some(table);
            finish_open_block(state, start, outputs)?;
            append_fallback(state, start, fragment, outputs)?;
        }
        Some(
            block @ (MarkdownOpenBlock::Paragraph { .. } | MarkdownOpenBlock::Fallback { .. }),
        ) => {
            state.open = Some(block);
            append_open_text(state, fragment, outputs)?;
        }
        None => append_fallback(state, start, fragment, outputs)?,
    }
    Ok(())
}

fn process_line(
    state: &mut ParserState,
    start: u64,
    line: &str,
    continuation: bool,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    let end = start
        .checked_add(line.len() as u64)
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let mut reprocess = true;
    while reprocess {
        reprocess = false;
        match state.open.take() {
            Some(MarkdownOpenBlock::FencedCode {
                block_start,
                body_start,
                fence,
                language,
                mut preview,
                mut logical_lines,
                mut body_bytes,
                mut resource_digest,
            }) => {
                if !continuation && is_closing_fence(line, fence) {
                    emit_code(
                        state,
                        block_start,
                        body_start,
                        start,
                        end,
                        language,
                        preview,
                        logical_lines,
                        body_bytes,
                        resource_digest,
                        outputs,
                    )?;
                } else {
                    if logical_lines < crate::MARKDOWN_CODE_PREVIEW_MAX_LINES {
                        append_preview(&mut preview, line, crate::MARKDOWN_CODE_PREVIEW_MAX_BYTES);
                    }
                    body_bytes = body_bytes
                        .checked_add(line.len() as u64)
                        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
                    resource_digest = crate::projection::advance_resource_content_digest(
                        resource_digest,
                        line.as_bytes(),
                    );
                    if line.ends_with('\n') || !line.is_empty() {
                        logical_lines = logical_lines
                            .checked_add(1)
                            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
                    }
                    state.open = Some(MarkdownOpenBlock::FencedCode {
                        block_start,
                        body_start,
                        fence,
                        language,
                        preview,
                        logical_lines,
                        body_bytes,
                        resource_digest,
                    });
                }
            }
            Some(MarkdownOpenBlock::Table {
                block_start,
                header_end,
                columns,
                mut body_rows,
                mut preview,
                mut source_bytes,
                mut resource_digest,
            }) => {
                if continuation || is_blank(line) || !looks_like_table_row(line) {
                    state.open = Some(MarkdownOpenBlock::Table {
                        block_start,
                        header_end,
                        columns,
                        body_rows,
                        preview,
                        source_bytes,
                        resource_digest,
                    });
                    finish_open_block(state, start, outputs)?;
                    reprocess = true;
                } else {
                    if body_rows < crate::MARKDOWN_TABLE_PREVIEW_MAX_BODY_ROWS {
                        append_preview(&mut preview, line, crate::MARKDOWN_TABLE_PREVIEW_MAX_BYTES);
                    }
                    body_rows = body_rows
                        .checked_add(1)
                        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
                    source_bytes = source_bytes
                        .checked_add(line.len() as u64)
                        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
                    resource_digest = crate::projection::advance_resource_content_digest(
                        resource_digest,
                        line.as_bytes(),
                    );
                    state.open = Some(MarkdownOpenBlock::Table {
                        block_start,
                        header_end,
                        columns,
                        body_rows,
                        preview,
                        source_bytes,
                        resource_digest,
                    });
                }
            }
            Some(MarkdownOpenBlock::Paragraph {
                block_start,
                next_span_ordinal,
                buffered_source,
            }) => {
                if !continuation
                    && next_span_ordinal == 1
                    && single_logical_line(&buffered_source)
                    && table_columns(&buffered_source) == table_delimiter_columns(line)
                    && table_delimiter_columns(line).is_some()
                {
                    let columns = table_delimiter_columns(line)
                        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
                    let header_end = block_start + buffered_source.len() as u64;
                    let resource_digest = crate::projection::advance_resource_content_digest(
                        crate::projection::advance_resource_content_digest(
                            crate::projection::resource_content_digest_seed(
                                crate::ResourceKind::Table,
                            ),
                            buffered_source.as_bytes(),
                        ),
                        line.as_bytes(),
                    );
                    let mut preview = buffered_source;
                    append_preview(&mut preview, line, crate::MARKDOWN_TABLE_PREVIEW_MAX_BYTES);
                    state.open = Some(MarkdownOpenBlock::Table {
                        block_start,
                        header_end,
                        columns,
                        body_rows: 0,
                        preview,
                        source_bytes: end - block_start,
                        resource_digest,
                    });
                } else if !continuation && starts_distinct_block(line) {
                    state.open = Some(MarkdownOpenBlock::Paragraph {
                        block_start,
                        next_span_ordinal,
                        buffered_source,
                    });
                    finish_open_block(state, start, outputs)?;
                    reprocess = true;
                } else {
                    state.open = Some(MarkdownOpenBlock::Paragraph {
                        block_start,
                        next_span_ordinal,
                        buffered_source,
                    });
                    append_open_text(state, line, outputs)?;
                    if is_blank(line) {
                        finish_open_block(state, end, outputs)?;
                    }
                }
            }
            Some(MarkdownOpenBlock::Fallback {
                block_start,
                next_span_ordinal,
                buffered_source,
            }) => {
                if !continuation && !is_blank(line) {
                    state.open = Some(MarkdownOpenBlock::Fallback {
                        block_start,
                        next_span_ordinal,
                        buffered_source,
                    });
                    finish_open_block(state, start, outputs)?;
                    reprocess = true;
                } else {
                    state.open = Some(MarkdownOpenBlock::Fallback {
                        block_start,
                        next_span_ordinal,
                        buffered_source,
                    });
                    append_open_text(state, line, outputs)?;
                }
            }
            None => start_line_block(state, start, end, line, outputs)?,
        }
    }
    Ok(())
}

fn start_line_block(
    state: &mut ParserState,
    start: u64,
    end: u64,
    line: &str,
    outputs: &mut Vec<ParserOutput>,
) -> Result<(), SyndicMutationError> {
    if let Some((fence, language)) = opening_fence(line) {
        state.open = Some(MarkdownOpenBlock::FencedCode {
            block_start: start,
            body_start: end,
            fence,
            language,
            preview: Box::<str>::default(),
            logical_lines: 0,
            body_bytes: 0,
            resource_digest: crate::projection::resource_content_digest_seed(
                crate::ResourceKind::Code,
            ),
        });
    } else if let Some(kind) = one_line_block_kind(line) {
        emit_inline(state, start, kind, 1, start, end, outputs)?;
    } else if is_blank(line) {
        append_fallback(state, start, line, outputs)?;
    } else {
        state.open = Some(MarkdownOpenBlock::Paragraph {
            block_start: start,
            next_span_ordinal: 1,
            buffered_source: line.into(),
        });
        flush_open_spans(state, outputs)?;
    }
    Ok(())
}
