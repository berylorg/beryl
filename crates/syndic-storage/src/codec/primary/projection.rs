use super::projection_build::{decode_markdown_checkpoint, encode_markdown_checkpoint};
use super::*;

pub(super) fn encode_canonical_item(value: &CanonicalItemRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, value.id());
    enc_turn(&mut e, value.turn_id());
    enc_item_ord(&mut e, value.ordinal());
    enc_projection_rev(&mut e, value.revision());
    enc_opt(&mut e, value.source_event(), enc_source_seq);
    e.u64(value.source_event_count());
    match value.cas_source() {
        Some(source) => {
            e.u8(1);
            enc_cas_item_source(&mut e, source)
        }
        None => e.u8(0),
    }
    enc_provider_item_kind(&mut e, value.provider_kind());
    enc_provider_item_lifecycle(&mut e, value.provider_lifecycle());
    enc_provider_item_disposition(&mut e, value.disposition());
    enc_opt(&mut e, value.assistant_phase(), enc_assistant_phase);
    enc_canonical_payload(&mut e, value.payload());
    Ok(e.finish())
}

pub(super) fn decode_canonical_item(bytes: &[u8]) -> Result<CanonicalItemRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let id = dec_item(&mut d)?;
    let turn = dec_turn(&mut d)?;
    let ordinal = dec_item_ord(&mut d)?;
    let revision = dec_projection_rev(&mut d)?;
    let event = dec_opt(&mut d, "item source event", dec_source_seq)?;
    let event_count = d.u64()?;
    let cas = dec_opt(&mut d, "item CAS source", dec_cas_item_source)?;
    let value = CanonicalItemRecord::with_source_state(
        id,
        turn,
        ordinal,
        revision,
        event,
        event_count,
        cas,
        dec_provider_item_kind(&mut d)?,
        dec_provider_item_lifecycle(&mut d)?,
        dec_provider_item_disposition(&mut d)?,
        dec_opt(&mut d, "canonical assistant phase", dec_assistant_phase)?,
        dec_canonical_payload(&mut d)?,
    )
    .map_err(|source| invalid("canonical item", source))?;
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_transcript_head(
    value: &TranscriptViewHeadRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    enc_transcript_generation(&mut e, value.generation());
    enc_projection_rev(&mut e, value.revision());
    e.u64(value.entry_count());
    enc_opt(&mut e, value.committed_tail(), enc_turn);
    enc_path_digest(&mut e, value.selected_path_digest());
    enc_projection_lifecycle(&mut e, value.lifecycle());
    Ok(e.finish())
}

pub(super) fn decode_transcript_head(bytes: &[u8]) -> Result<TranscriptViewHeadRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = TranscriptViewHeadRecord::new(
        dec_thread(&mut d)?,
        dec_transcript_generation(&mut d)?,
        dec_projection_rev(&mut d)?,
        d.u64()?,
        dec_opt(&mut d, "view tail", dec_turn)?,
        dec_path_digest(&mut d)?,
        dec_projection_lifecycle(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_projection_record(value: &ProjectionRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_projection(&mut e, value.id());
    enc_projection_rev(&mut e, value.revision());
    enc_item(&mut e, value.item_id());
    enc_turn(&mut e, value.turn_id());
    enc_projection_ord(&mut e, value.ordinal());
    encode_projection_payload(&mut e, value.payload());
    Ok(e.finish())
}

pub(super) fn decode_projection_record(bytes: &[u8]) -> Result<ProjectionRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let id = dec_projection(&mut d)?;
    let revision = dec_projection_rev(&mut d)?;
    let item = dec_item(&mut d)?;
    let turn = dec_turn(&mut d)?;
    let ordinal = dec_projection_ord(&mut d)?;
    let value = ProjectionRecord::new(
        id,
        revision,
        item,
        turn,
        ordinal,
        decode_projection_payload(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn encode_projection_payload(e: &mut Encoder, value: &ProjectionPayload) {
    match value {
        ProjectionPayload::Empty => e.u8(3),
        ProjectionPayload::InlineMarkdown {
            block_id,
            block_kind,
            span_ordinal,
            source_range,
            source,
        } => {
            e.u8(0);
            e.fixed32(&block_id.as_bytes());
            enc_markdown_block_kind(e, *block_kind);
            e.u64(*span_ordinal);
            enc_projection_source_range(e, *source_range);
            e.text(source);
        }
        ProjectionPayload::ResourceReference {
            block_id,
            block_kind,
            source_range,
            resource_id,
            preview,
        } => {
            e.u8(1);
            e.fixed32(&block_id.as_bytes());
            enc_markdown_block_kind(e, *block_kind);
            enc_projection_source_range(e, *source_range);
            enc_resource(e, *resource_id);
            e.text(preview);
        }
        ProjectionPayload::ImageMarker {
            atom_ordinal,
            marker_ordinal,
            source_offset,
            marker,
        } => {
            e.u8(2);
            enc_composer_atom_ord(e, *atom_ordinal);
            enc_input_marker_ord(e, *marker_ordinal);
            e.u64(*source_offset);
            enc_marker(e, marker.marker_id());
            enc_image_label(e, marker.label());
            enc_asset_id(e, marker.asset_id());
        }
    }
}

fn decode_projection_payload(d: &mut Decoder<'_>) -> Result<ProjectionPayload, CodecError> {
    match d.u8()? {
        0 => ProjectionPayload::inline_markdown(
            MarkdownBlockId::from_bytes(d.fixed32()?),
            dec_markdown_block_kind(d)?,
            d.u64()?,
            dec_projection_source_range(d, "inline projection source range")?,
            d.text("inline Markdown projection")?,
        )
        .map_err(|source| invalid("projection payload", source)),
        1 => ProjectionPayload::resource_reference(
            MarkdownBlockId::from_bytes(d.fixed32()?),
            dec_markdown_block_kind(d)?,
            dec_projection_source_range(d, "resource projection source range")?,
            dec_resource(d)?,
            d.text("projection resource preview")?,
        )
        .map_err(|source| invalid("projection payload", source)),
        2 => Ok(ProjectionPayload::image_marker(
            dec_composer_atom_ord(d)?,
            dec_input_marker_ord(d)?,
            d.u64()?,
            ResolvedImageMarker::new(dec_marker(d)?, dec_image_label(d)?, dec_asset_id(d)?),
        )),
        3 => Ok(ProjectionPayload::empty()),
        tag => Err(CodecError::InvalidTag {
            kind: "projection payload",
            tag,
        }),
    }
}

pub(super) fn encode_item_projection_head(
    value: &ItemProjectionHeadRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, value.item_id());
    enc_projection_rev(&mut e, value.revision());
    enc_projection_rev(&mut e, value.source_item_revision());
    enc_item_projection_generation(&mut e, value.generation());
    enc_projection_lifecycle(&mut e, value.lifecycle());
    Ok(e.finish())
}

pub(super) fn decode_item_projection_head(
    bytes: &[u8],
) -> Result<ItemProjectionHeadRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ItemProjectionHeadRecord::new(
        dec_item(&mut d)?,
        dec_projection_rev(&mut d)?,
        dec_projection_rev(&mut d)?,
        dec_item_projection_generation(&mut d)?,
        dec_projection_lifecycle(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_item_projection_set(
    value: &ItemProjectionSetRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, value.item_id());
    enc_item_projection_generation(&mut e, value.generation());
    enc_projection_format(&mut e, value.format());
    enc_projection_rev(&mut e, value.source_item_revision());
    enc_content_ref(&mut e, value.source_content());
    e.u64(value.source_bytes());
    e.u64(value.stable_projection_count());
    e.u64(value.stable_resource_count());
    e.fixed32(&value.stable_digest());
    e.u64(value.projection_count());
    e.u64(value.resource_count());
    e.fixed32(&value.digest());
    encode_markdown_checkpoint(&mut e, value.resume_checkpoint());
    enc_bool(&mut e, value.stable_eof_resolved());
    Ok(e.finish())
}

pub(super) fn decode_item_projection_set(
    bytes: &[u8],
) -> Result<ItemProjectionSetRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ItemProjectionSetRecord::new(
        dec_item(&mut d)?,
        dec_item_projection_generation(&mut d)?,
        dec_projection_format(&mut d)?,
        dec_projection_rev(&mut d)?,
        dec_content_ref(&mut d)?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.fixed32()?,
        d.u64()?,
        d.u64()?,
        d.fixed32()?,
        decode_markdown_checkpoint(&mut d)?,
        dec_bool(&mut d, "item projection stable EOF resolution")?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_resource_record(
    value: &ResourceMetadataRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_resource(&mut e, value.id());
    enc_projection_rev(&mut e, value.revision());
    enc_item(&mut e, value.item_id());
    match value.backing() {
        ResourceBacking::CanonicalTextRange { content_id, range } => {
            let projection = value.projection_id().ok_or_else(|| {
                invalid(
                    "resource metadata",
                    SyndicRecordError::InvalidResourceStructure,
                )
            })?;
            let ordinal = value.ordinal().ok_or_else(|| {
                invalid(
                    "resource metadata",
                    SyndicRecordError::InvalidResourceStructure,
                )
            })?;
            let media_type = value.media_type().ok_or_else(|| {
                invalid(
                    "resource metadata",
                    SyndicRecordError::InvalidResourceStructure,
                )
            })?;
            let digest = value.digest().ok_or_else(|| {
                invalid(
                    "resource metadata",
                    SyndicRecordError::InvalidResourceStructure,
                )
            })?;
            e.u8(0);
            enc_projection(&mut e, projection);
            enc_resource_ord(&mut e, ordinal);
            enc_resource_kind(&mut e, value.kind());
            e.text(media_type);
            enc_content(&mut e, content_id);
            enc_projection_source_range(&mut e, range);
            e.fixed32(digest);
            enc_opt(&mut e, value.preview_range(), enc_projection_source_range);
            encode_resource_structure(&mut e, value.structure());
        }
        ResourceBacking::GeneratedMedia(disposition) => match disposition {
            GeneratedMediaResourceDisposition::PendingAsset => e.u8(1),
            GeneratedMediaResourceDisposition::Asset(asset) => {
                e.u8(2);
                enc_asset_id(&mut e, asset);
            }
            GeneratedMediaResourceDisposition::Unavailable(reason) => {
                e.u8(3);
                encode_generated_media_unavailable_reason(&mut e, reason);
            }
        },
    }
    Ok(e.finish())
}

pub(super) fn decode_resource_record(bytes: &[u8]) -> Result<ResourceMetadataRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let id = dec_resource(&mut d)?;
    let revision = dec_projection_rev(&mut d)?;
    let item = dec_item(&mut d)?;
    let value = match d.u8()? {
        0 => {
            let projection = dec_projection(&mut d)?;
            let ordinal = dec_resource_ord(&mut d)?;
            let kind = dec_resource_kind(&mut d)?;
            let media = d.text("resource media type")?.to_owned();
            let content_id = dec_content(&mut d)?;
            let range = dec_projection_source_range(&mut d, "resource backing range")?;
            let digest = d.fixed32()?;
            let preview_range = dec_opt(&mut d, "resource preview range", |d| {
                dec_projection_source_range(d, "resource preview range")
            })?;
            let structure = decode_resource_structure(&mut d)?;
            ResourceMetadataRecord::new(
                id,
                revision,
                projection,
                item,
                ordinal,
                kind,
                media,
                ResourceBacking::CanonicalTextRange { content_id, range },
                digest,
                preview_range,
                structure,
            )
            .map_err(|source| invalid("resource metadata", source))?
        }
        1 => ResourceMetadataRecord::pending_generated_media(id, revision, item),
        2 => ResourceMetadataRecord::generated_media(
            id,
            revision,
            item,
            GeneratedMediaResourceDisposition::Asset(dec_asset_id(&mut d)?),
        ),
        3 => ResourceMetadataRecord::generated_media(
            id,
            revision,
            item,
            GeneratedMediaResourceDisposition::Unavailable(
                decode_generated_media_unavailable_reason(&mut d)?,
            ),
        ),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "resource backing",
                tag,
            });
        }
    };
    d.finish()?;
    Ok(value)
}

fn encode_generated_media_unavailable_reason(
    e: &mut Encoder,
    value: GeneratedMediaUnavailableReason,
) {
    e.u8(match value {
        GeneratedMediaUnavailableReason::Missing => 0,
        GeneratedMediaUnavailableReason::Changed => 1,
        GeneratedMediaUnavailableReason::Oversized => 2,
        GeneratedMediaUnavailableReason::Unsupported => 3,
        GeneratedMediaUnavailableReason::Unreadable => 4,
    });
}

fn decode_generated_media_unavailable_reason(
    d: &mut Decoder<'_>,
) -> Result<GeneratedMediaUnavailableReason, CodecError> {
    match d.u8()? {
        0 => Ok(GeneratedMediaUnavailableReason::Missing),
        1 => Ok(GeneratedMediaUnavailableReason::Changed),
        2 => Ok(GeneratedMediaUnavailableReason::Oversized),
        3 => Ok(GeneratedMediaUnavailableReason::Unsupported),
        4 => Ok(GeneratedMediaUnavailableReason::Unreadable),
        tag => Err(CodecError::InvalidTag {
            kind: "generated media unavailable reason",
            tag,
        }),
    }
}

fn encode_resource_structure(e: &mut Encoder, value: &ResourceStructure) {
    match value {
        ResourceStructure::Code {
            language,
            logical_lines,
        } => {
            e.u8(0);
            enc_opt(e, language.as_deref(), |e, language| e.text(language));
            e.u64(*logical_lines);
        }
        ResourceStructure::Table {
            body_rows,
            columns,
            header_range,
        } => {
            e.u8(1);
            e.u64(*body_rows);
            e.u64(*columns);
            enc_projection_source_range(e, *header_range);
        }
        ResourceStructure::Opaque => e.u8(2),
    }
}

fn decode_resource_structure(d: &mut Decoder<'_>) -> Result<ResourceStructure, CodecError> {
    match d.u8()? {
        0 => {
            let language: Option<Box<str>> =
                dec_opt(d, "code language", |d| Ok(d.text("code language")?.into()))?;
            ResourceStructure::code(language.as_deref(), d.u64()?)
                .map_err(|source| invalid("code resource structure", source))
        }
        1 => ResourceStructure::table(
            d.u64()?,
            d.u64()?,
            dec_projection_source_range(d, "table header range")?,
        )
        .map_err(|source| invalid("table resource structure", source)),
        2 => Ok(ResourceStructure::Opaque),
        tag => Err(CodecError::InvalidTag {
            kind: "resource structure",
            tag,
        }),
    }
}

pub(super) fn encode_history_summary(value: &HistorySummaryRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    enc_thread_rev(&mut e, value.thread_revision());
    enc_opt(&mut e, value.committed_tail(), enc_turn);
    enc_path_digest(&mut e, value.selected_path_digest());
    enc_bool(&mut e, value.complete());
    enc_timestamp(&mut e, value.last_activity_at());
    Ok(e.finish())
}

pub(super) fn decode_history_summary(bytes: &[u8]) -> Result<HistorySummaryRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = HistorySummaryRecord::new(
        dec_thread(&mut d)?,
        dec_thread_rev(&mut d)?,
        dec_opt(&mut d, "summary tail", dec_turn)?,
        dec_path_digest(&mut d)?,
        dec_bool(&mut d, "history completeness")?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}
