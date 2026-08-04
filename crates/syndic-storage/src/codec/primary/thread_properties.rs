use super::*;

pub(super) fn encode_thread_execution(
    value: &ThreadExecutionRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    enc_execution(&mut e, value.execution());
    Ok(e.finish())
}

pub(super) fn decode_thread_execution(bytes: &[u8]) -> Result<ThreadExecutionRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ThreadExecutionRecord::new(dec_thread(&mut d)?, dec_execution(&mut d)?);
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_thread_attributes(
    value: &ThreadAttributesRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    e.u64(value.revision().get());
    enc_opt(&mut e, value.generated_title(), enc_generated_title);
    enc_archive(&mut e, value.archive());
    Ok(e.finish())
}

pub(super) fn decode_thread_attributes(bytes: &[u8]) -> Result<ThreadAttributesRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ThreadAttributesRecord::from_parts(
        dec_thread(&mut d)?,
        ThreadAttributesRevision::new(d.u64()?)
            .map_err(|source| invalid("thread-attributes revision", source))?,
        dec_opt(&mut d, "generated title", dec_generated_title)?,
        dec_archive(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn enc_generated_title(e: &mut Encoder, value: &GeneratedThreadTitle) {
    e.text(value.text());
    enc_turn(e, value.source_turn_id());
    enc_content_ref(e, value.source_content());
    enc_path_digest(e, value.source_selected_path_digest());
    enc_thread_rev(e, value.source_thread_revision());
    enc_timestamp(e, value.generated_at());
}

fn dec_generated_title(d: &mut Decoder<'_>) -> Result<GeneratedThreadTitle, CodecError> {
    let text = d.text("generated thread title")?;
    GeneratedThreadTitle::new(
        text,
        dec_turn(d)?,
        dec_content_ref(d)?,
        dec_path_digest(d)?,
        dec_thread_rev(d)?,
        dec_timestamp(d)?,
    )
    .map_err(|source| invalid("generated thread title", source))
}

fn enc_archive(e: &mut Encoder, value: ThreadArchiveState) {
    match value {
        ThreadArchiveState::Ordinary => e.u8(0),
        ThreadArchiveState::BranchDiscussionOpen => e.u8(1),
        ThreadArchiveState::BranchDiscussionArchived {
            handoff_job_id,
            archived_at,
        } => {
            e.u8(2);
            e.fixed16(handoff_job_id.as_bytes());
            enc_timestamp(e, archived_at);
        }
    }
}

fn dec_archive(d: &mut Decoder<'_>) -> Result<ThreadArchiveState, CodecError> {
    match d.u8()? {
        0 => Ok(ThreadArchiveState::Ordinary),
        1 => Ok(ThreadArchiveState::BranchDiscussionOpen),
        2 => Ok(ThreadArchiveState::BranchDiscussionArchived {
            handoff_job_id: JobId::from_bytes(d.fixed16()?),
            archived_at: dec_timestamp(d)?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "thread archive state",
            tag,
        }),
    }
}

pub(super) fn encode_thread_usage(value: &ThreadUsageRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    e.u64(value.revision().get());
    enc_opt(&mut e, value.observation(), enc_usage_observation);
    Ok(e.finish())
}

pub(super) fn decode_thread_usage(bytes: &[u8]) -> Result<ThreadUsageRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ThreadUsageRecord::from_parts(
        dec_thread(&mut d)?,
        ThreadUsageRevision::new(d.u64()?)
            .map_err(|source| invalid("thread-usage revision", source))?,
        dec_opt(&mut d, "thread usage observation", dec_usage_observation)?,
    );
    d.finish()?;
    Ok(value)
}

fn enc_usage_breakdown(e: &mut Encoder, value: ThreadTokenUsageBreakdown) {
    e.u64(value.cached_input_tokens());
    e.u64(value.input_tokens());
    e.u64(value.output_tokens());
    e.u64(value.reasoning_output_tokens());
    e.u64(value.total_tokens());
}

fn dec_usage_breakdown(d: &mut Decoder<'_>) -> Result<ThreadTokenUsageBreakdown, CodecError> {
    Ok(ThreadTokenUsageBreakdown::new(
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
    ))
}

fn enc_usage_observation(e: &mut Encoder, value: &ThreadUsageObservation) {
    enc_usage_breakdown(e, value.last());
    enc_usage_breakdown(e, value.total());
    enc_opt(e, value.model_context_window(), Encoder::u64);
    enc_timestamp(e, value.observed_at());
    enc_execution(e, value.execution());
    enc_binding_rev(e, value.binding_revision());
    enc_external(e, value.cas_thread_id().as_str());
    enc_loaded_generation(e, value.loaded_generation());
    e.u64(value.connection_generation().get());
    e.u64(value.provider_control_ordinal().get());
}

fn dec_usage_observation(d: &mut Decoder<'_>) -> Result<ThreadUsageObservation, CodecError> {
    let last = dec_usage_breakdown(d)?;
    let total = dec_usage_breakdown(d)?;
    let model_context_window = dec_opt(d, "model context window", |decoder| decoder.u64())?;
    ThreadUsageObservation::new(
        last,
        total,
        model_context_window,
        dec_timestamp(d)?,
        dec_execution(d)?,
        dec_binding_rev(d)?,
        dec_cas_thread(d)?,
        dec_loaded_generation(d)?,
        SyndicConnectionGeneration::new(d.u64()?)
            .map_err(|source| invalid("connection generation", source))?,
        ProviderControlOrdinal::new(d.u64()?)
            .map_err(|source| invalid("provider-control ordinal", source))?,
    )
    .map_err(|source| invalid("thread usage observation", source))
}

pub(super) fn encode_thread_catalog_summary(
    value: &ThreadCatalogSummaryRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    enc_projection_rev(&mut e, value.revision());
    enc_opt(&mut e, value.title(), enc_catalog_title);
    enc_execution(&mut e, value.execution());
    enc_archive(&mut e, value.archive());
    enc_timestamp(&mut e, value.last_activity_at());
    e.u8(u8::from(value.complete()));
    enc_opt(&mut e, value.parent_thread_id(), enc_thread);
    enc_thread_lineage_depth(&mut e, value.lineage_depth());
    enc_path_digest(&mut e, value.lineage_digest());
    let sources = value.sources();
    e.u64(sources.attributes_revision().get());
    enc_projection_rev(&mut e, sources.history_summary_revision());
    enc_thread_rev(&mut e, sources.history_thread_revision());
    enc_path_digest(&mut e, sources.history_selected_path_digest());
    enc_thread_rev(&mut e, sources.thread_revision());
    Ok(e.finish())
}

pub(super) fn decode_thread_catalog_summary(
    bytes: &[u8],
) -> Result<ThreadCatalogSummaryRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let thread_id = dec_thread(&mut d)?;
    let revision = dec_projection_rev(&mut d)?;
    let title = dec_opt(&mut d, "catalog title", dec_catalog_title)?;
    let execution = dec_execution(&mut d)?;
    let archive = dec_archive(&mut d)?;
    let last_activity_at = dec_timestamp(&mut d)?;
    let complete = dec_bool(&mut d, "catalog completeness")?;
    let parent_thread_id = dec_opt(&mut d, "catalog parent thread", dec_thread)?;
    let lineage_depth = dec_thread_lineage_depth(&mut d)?;
    let lineage_digest = dec_path_digest(&mut d)?;
    let sources = ThreadCatalogSourceWitnesses::new(
        ThreadAttributesRevision::new(d.u64()?)
            .map_err(|source| invalid("catalog attributes revision", source))?,
        dec_projection_rev(&mut d)?,
        dec_thread_rev(&mut d)?,
        dec_path_digest(&mut d)?,
        dec_thread_rev(&mut d)?,
    );
    let value = ThreadCatalogSummaryRecord::new(
        thread_id,
        revision,
        title,
        execution,
        archive,
        last_activity_at,
        complete,
        parent_thread_id,
        lineage_depth,
        lineage_digest,
        sources,
    );
    d.finish()?;
    Ok(value)
}

fn enc_catalog_title(e: &mut Encoder, value: &ThreadCatalogTitle) {
    e.text(value.text());
    e.u8(match value.source() {
        ThreadCatalogTitleSource::Generated => 0,
        ThreadCatalogTitleSource::HistoryDerived => 1,
    });
}

fn dec_catalog_title(d: &mut Decoder<'_>) -> Result<ThreadCatalogTitle, CodecError> {
    let text = d.text("catalog title")?;
    let source = match d.u8()? {
        0 => ThreadCatalogTitleSource::Generated,
        1 => ThreadCatalogTitleSource::HistoryDerived,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "catalog title source",
                tag,
            });
        }
    };
    ThreadCatalogTitle::new(text, source).map_err(|source| invalid("catalog title", source))
}

fn dec_bool(d: &mut Decoder<'_>, kind: &'static str) -> Result<bool, CodecError> {
    match d.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        tag => Err(CodecError::InvalidTag { kind, tag }),
    }
}
