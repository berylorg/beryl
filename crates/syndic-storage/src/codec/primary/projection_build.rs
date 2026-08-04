use crate::*;

use super::super::{CodecError, parts::*};

pub(super) fn encode_item_projection_build(
    value: &ItemProjectionBuildRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, value.item_id());
    enc_item_projection_generation(&mut e, value.generation());
    enc_projection_rev(&mut e, value.revision());
    enc_projection_format(&mut e, value.format());
    enc_projection_rev(&mut e, value.source_item_revision());
    enc_projection_text_source(&mut e, value.source());
    e.u64(value.source_bytes());
    e.u64(value.projection_count());
    e.u64(value.resource_count());
    e.fixed32(&value.output_digest());
    encode_item_build_phase(&mut e, value.phase());
    Ok(e.finish())
}

pub(super) fn decode_item_projection_build(
    bytes: &[u8],
) -> Result<ItemProjectionBuildRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ItemProjectionBuildRecord::new(
        dec_item(&mut d)?,
        dec_item_projection_generation(&mut d)?,
        dec_projection_rev(&mut d)?,
        dec_projection_format(&mut d)?,
        dec_projection_rev(&mut d)?,
        dec_projection_text_source(&mut d)?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.fixed32()?,
        decode_item_build_phase(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn encode_item_build_phase(e: &mut Encoder, phase: &ItemProjectionBuildPhase) {
    match phase {
        ItemProjectionBuildPhase::Parsing(checkpoint) => {
            e.u8(0);
            encode_markdown_checkpoint(e, checkpoint);
        }
        ItemProjectionBuildPhase::Superseded(checkpoint) => {
            e.u8(1);
            encode_markdown_checkpoint(e, checkpoint);
        }
    }
}

fn decode_item_build_phase(d: &mut Decoder<'_>) -> Result<ItemProjectionBuildPhase, CodecError> {
    match d.u8()? {
        0 => Ok(ItemProjectionBuildPhase::Parsing(
            decode_markdown_checkpoint(d)?,
        )),
        1 => Ok(ItemProjectionBuildPhase::Superseded(
            decode_markdown_checkpoint(d)?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "item projection build phase",
            tag,
        }),
    }
}

pub(super) fn encode_markdown_checkpoint(e: &mut Encoder, value: &MarkdownParserCheckpoint) {
    e.u64(value.consumed_source_bytes());
    e.u64(value.closed_source_bytes());
    enc_projection_text_source_cursor(e, value.source_cursor());
    e.u64(value.line_start());
    e.text(value.line_carry());
    enc_bool(e, value.line_continuation());
    enc_opt(e, value.open_block(), encode_markdown_open_block);
}

pub(super) fn decode_markdown_checkpoint(
    d: &mut Decoder<'_>,
) -> Result<MarkdownParserCheckpoint, CodecError> {
    Ok(MarkdownParserCheckpoint::new(
        d.u64()?,
        d.u64()?,
        dec_projection_text_source_cursor(d)?,
        d.u64()?,
        d.text("Markdown line carry")?.into(),
        dec_bool(d, "Markdown line continuation")?,
        dec_opt(d, "open Markdown block", decode_markdown_open_block)?,
    ))
}

fn encode_markdown_open_block(e: &mut Encoder, value: &MarkdownOpenBlock) {
    match value {
        MarkdownOpenBlock::Paragraph {
            block_start,
            next_span_ordinal,
            buffered_source,
        } => {
            e.u8(0);
            e.u64(*block_start);
            e.u64(*next_span_ordinal);
            e.text(buffered_source);
        }
        MarkdownOpenBlock::FencedCode {
            block_start,
            body_start,
            fence,
            language,
            preview,
            logical_lines,
            body_bytes,
            resource_digest,
        } => {
            e.u8(1);
            e.u64(*block_start);
            e.u64(*body_start);
            e.u8(fence.byte());
            e.u8(fence.length());
            enc_opt(e, language.as_deref(), |e, value| e.text(value));
            e.text(preview);
            e.u64(*logical_lines);
            e.u64(*body_bytes);
            e.fixed32(resource_digest);
        }
        MarkdownOpenBlock::Table {
            block_start,
            header_end,
            columns,
            body_rows,
            preview,
            source_bytes,
            resource_digest,
        } => {
            e.u8(2);
            e.u64(*block_start);
            e.u64(*header_end);
            e.u64(*columns);
            e.u64(*body_rows);
            e.text(preview);
            e.u64(*source_bytes);
            e.fixed32(resource_digest);
        }
        MarkdownOpenBlock::Fallback {
            block_start,
            next_span_ordinal,
            buffered_source,
        } => {
            e.u8(3);
            e.u64(*block_start);
            e.u64(*next_span_ordinal);
            e.text(buffered_source);
        }
    }
}

fn decode_markdown_open_block(d: &mut Decoder<'_>) -> Result<MarkdownOpenBlock, CodecError> {
    match d.u8()? {
        0 => Ok(MarkdownOpenBlock::Paragraph {
            block_start: d.u64()?,
            next_span_ordinal: d.u64()?,
            buffered_source: d.text("Markdown paragraph buffer")?.into(),
        }),
        1 => Ok(MarkdownOpenBlock::FencedCode {
            block_start: d.u64()?,
            body_start: d.u64()?,
            fence: MarkdownFenceMarker::new(d.u8()?, d.u8()?),
            language: dec_opt(d, "code language", |d| Ok(d.text("code language")?.into()))?,
            preview: d.text("code preview")?.into(),
            logical_lines: d.u64()?,
            body_bytes: d.u64()?,
            resource_digest: d.fixed32()?,
        }),
        2 => Ok(MarkdownOpenBlock::Table {
            block_start: d.u64()?,
            header_end: d.u64()?,
            columns: d.u64()?,
            body_rows: d.u64()?,
            preview: d.text("table preview")?.into(),
            source_bytes: d.u64()?,
            resource_digest: d.fixed32()?,
        }),
        3 => Ok(MarkdownOpenBlock::Fallback {
            block_start: d.u64()?,
            next_span_ordinal: d.u64()?,
            buffered_source: d.text("Markdown fallback buffer")?.into(),
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "open Markdown block",
            tag,
        }),
    }
}

pub(super) fn encode_transcript_build(
    value: &TranscriptBuildRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    enc_transcript_generation(&mut e, value.generation());
    enc_projection_rev(&mut e, value.revision());
    enc_thread_rev(&mut e, value.source_thread_revision());
    enc_opt(&mut e, value.committed_tail(), enc_turn);
    enc_path_digest(&mut e, value.selected_path_digest());
    e.u64(value.path_turn_count());
    e.u64(value.entry_count());
    e.fixed32(&value.entry_digest());
    enc_bool(&mut e, value.history_complete());
    encode_transcript_build_phase(&mut e, value.phase());
    Ok(e.finish())
}

pub(super) fn decode_transcript_build(bytes: &[u8]) -> Result<TranscriptBuildRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = TranscriptBuildRecord::new(
        dec_thread(&mut d)?,
        dec_transcript_generation(&mut d)?,
        dec_projection_rev(&mut d)?,
        dec_thread_rev(&mut d)?,
        dec_opt(&mut d, "transcript build tail", dec_turn)?,
        dec_path_digest(&mut d)?,
        d.u64()?,
        d.u64()?,
        d.fixed32()?,
        dec_bool(&mut d, "transcript history completeness")?,
        decode_transcript_build_phase(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn encode_transcript_build_phase(e: &mut Encoder, value: TranscriptBuildPhase) {
    match value {
        TranscriptBuildPhase::Collecting { next_turn } => {
            e.u8(0);
            enc_opt(e, next_turn, enc_turn);
        }
        TranscriptBuildPhase::Publishing {
            next_depth,
            next_item,
            next_projection,
        } => {
            e.u8(1);
            enc_turn_depth(e, next_depth);
            enc_item_ord(e, next_item);
            enc_projection_ord(e, next_projection);
        }
        TranscriptBuildPhase::Complete => e.u8(2),
        TranscriptBuildPhase::Superseded => e.u8(3),
    }
}

fn decode_transcript_build_phase(d: &mut Decoder<'_>) -> Result<TranscriptBuildPhase, CodecError> {
    match d.u8()? {
        0 => Ok(TranscriptBuildPhase::Collecting {
            next_turn: dec_opt(d, "next transcript path turn", dec_turn)?,
        }),
        1 => Ok(TranscriptBuildPhase::Publishing {
            next_depth: dec_turn_depth(d)?,
            next_item: dec_item_ord(d)?,
            next_projection: dec_projection_ord(d)?,
        }),
        2 => Ok(TranscriptBuildPhase::Complete),
        3 => Ok(TranscriptBuildPhase::Superseded),
        tag => Err(CodecError::InvalidTag {
            kind: "transcript build phase",
            tag,
        }),
    }
}

pub(crate) fn encode_transcript_path_turn(
    value: &TranscriptPathTurnRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    enc_transcript_generation(&mut e, value.generation());
    enc_turn_depth(&mut e, value.depth());
    enc_turn(&mut e, value.turn_id());
    enc_path_digest(&mut e, value.turn_path_digest());
    enc_turn_state_rev(&mut e, value.state_revision());
    enc_turn_lifecycle(&mut e, value.lifecycle());
    e.u64(value.source_event_count());
    e.u64(value.item_count());
    e.u64(value.finalized_item_count());
    enc_timestamp(&mut e, value.updated_at());
    Ok(e.finish())
}

pub(crate) fn decode_transcript_path_turn(
    bytes: &[u8],
) -> Result<TranscriptPathTurnRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = TranscriptPathTurnRecord::new(
        dec_thread(&mut d)?,
        dec_transcript_generation(&mut d)?,
        dec_turn_depth(&mut d)?,
        dec_turn(&mut d)?,
        dec_path_digest(&mut d)?,
        dec_turn_state_rev(&mut d)?,
        dec_turn_lifecycle(&mut d)?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}
