use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};

use crate::{
    ProviderItemBuildRecord, ProviderNarrativeSpanRecord, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::{
    FrameSpanError, ProviderFrameStorageValidationError, advance_narrative, narrative_seed,
    source::validate_source_range,
};

pub(crate) fn validate_staged_provider_narrative(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
) -> Result<(), SyndicValidationError> {
    let seed = narrative_seed(build).map_err(storage_error)?;
    let staged = build.staged_narrative();
    let (Some(mut current), Some(staged)) = (seed, staged) else {
        return if seed == staged {
            Ok(())
        } else {
            invalid("provider build narrative presence disagrees")
        };
    };
    if current == staged {
        return Ok(());
    }
    let target = build.target();
    let last_start =
        staged
            .logical_utf8_bytes()
            .checked_sub(1)
            .ok_or(SyndicValidationError::Invariant(
                "provider staged narrative frontier is empty",
            ))?;
    let mut after = None;
    while current != staged {
        let first = ProviderNarrativeSpanKey::new(
            current.content_id(),
            current.generation(),
            after.unwrap_or(current.logical_utf8_bytes()),
        );
        let last =
            ProviderNarrativeSpanKey::new(current.content_id(), current.generation(), last_start);
        let range = after.map_or_else(
            || CursorRange::closed(first, last),
            |_| CursorRange::after(first, last),
        );
        let page = reader.cursor::<ProviderNarrativeSpansCodec>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(256, 65_536).expect("narrative page bounds are nonzero"),
        )?;
        if page.records().is_empty() {
            return invalid("provider staged narrative span is missing");
        }
        for stored in page.records() {
            if current == staged {
                break;
            }
            let key = stored.key();
            let record = *stored.value();
            if key.content_id() != record.content_id()
                || key.generation() != record.generation()
                || key.logical_start() != record.logical_start()
                || record.content_id() != current.content_id()
                || record.generation() != current.generation()
                || record.logical_start() != current.logical_utf8_bytes()
                || record.frame_ordinal() != target.frame().ordinal()
                || record.frame_encoded_digest() != target.frame().encoded_digest()
                || record.source_end() > target.content().summary().encoded_bytes()
            {
                return invalid("provider staged narrative span frontier disagrees");
            }
            let expected = ProviderNarrativeSpanRecord::new(
                record.content_id(),
                record.generation(),
                record.logical_start(),
                record.logical_end(),
                record.frame_ordinal(),
                record.frame_encoded_digest(),
                record.source_start(),
                record.source_end(),
                record.source_digest(),
                current.chain_digest(),
            )
            .map_err(|_| {
                SyndicValidationError::Invariant("provider staged narrative span is invalid")
            })?;
            if expected != record {
                return invalid("provider staged narrative chain disagrees");
            }
            if record.source_end() <= build.staged_encoded_bytes() {
                validate_source_range(
                    reader,
                    build.target().content(),
                    build.staged_encoded_bytes(),
                    record.source_start(),
                    record.source_end(),
                    record.source_digest(),
                )
                .map_err(frame_error)?;
            }
            current = advance_narrative(current, record).map_err(frame_error)?;
            after = Some(record.logical_start());
        }
        if current != staged
            && (current.span_count() >= staged.span_count()
                || current.logical_utf8_bytes() >= staged.logical_utf8_bytes()
                || !page.has_more())
        {
            return invalid("provider staged narrative ended before its build frontier");
        }
    }
    Ok(())
}

fn storage_error(error: ProviderFrameStorageValidationError) -> SyndicValidationError {
    match error {
        ProviderFrameStorageValidationError::Read(source) => SyndicValidationError::Read(source),
        ProviderFrameStorageValidationError::Invariant(message) => {
            SyndicValidationError::Invariant(message)
        }
    }
}

fn frame_error(error: FrameSpanError) -> SyndicValidationError {
    match error {
        FrameSpanError::Read(source) => SyndicValidationError::Read(source),
        FrameSpanError::Invariant(message) => SyndicValidationError::Invariant(message),
    }
}

fn invalid<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
