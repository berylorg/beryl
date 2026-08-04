use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};

use crate::{
    ProviderItemBuildRecord, ProviderNarrativeComparisonFrontier, ProviderNarrativeCompletionCheck,
    ProviderNarrativeCompletionState, ProviderNarrativeReference, ProviderNarrativeSpanRecord,
    codec::*, domain::SyndicDomain,
};

use super::ProviderFrameStorageValidationError;

const COMPARISON_PAGE_BYTES: u64 = 65_536;
const COMPARISON_PAGE_SPANS: usize = 256;

/// Advances one fixed-work exact comparison page over completion and selected live bytes.
pub(crate) fn advance_provider_completion_comparison(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
) -> Result<ProviderNarrativeCompletionState, ProviderFrameStorageValidationError> {
    let check = build
        .completion_check()
        .ok_or(invalid("provider completion comparison is missing"))?;
    let ProviderNarrativeCompletionState::Pending(frontier) = check.state() else {
        return Err(invalid(
            "provider completion comparison is already terminal",
        ));
    };
    let prior = build
        .prior()
        .and_then(crate::SealedProviderFrameReference::narrative)
        .ok_or(invalid("provider completion prior narrative is missing"))?;
    require_staged_target(build, prior)?;

    let live_bytes = prior.logical_utf8_bytes();
    let completion_bytes = build.target().frame().logical_utf8_bytes();
    let common_bytes = live_bytes.min(completion_bytes);
    if frontier.compared_utf8_bytes() > common_bytes {
        return Err(invalid(
            "provider completion comparison exceeds the shared narrative prefix",
        ));
    }
    if frontier.compared_utf8_bytes() == common_bytes {
        return if live_bytes == completion_bytes {
            finish_equal(prior, frontier)
        } else {
            Ok(ProviderNarrativeCompletionState::Mismatch {
                utf8_byte_offset: common_bytes,
            })
        };
    }
    let source = check
        .source()
        .ok_or(invalid("nonempty provider completion source is missing"))?;
    compare_page(
        reader,
        build,
        prior,
        source.source_start(),
        common_bytes,
        live_bytes == completion_bytes,
        frontier,
    )
}

/// Replays bounded comparison pages to prove an exact persisted frontier or terminal result.
pub(crate) fn validate_provider_completion_comparison(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
) -> Result<(), ProviderFrameStorageValidationError> {
    let Some(actual) = build.completion_check() else {
        return Ok(());
    };
    let narrative = build
        .target()
        .narrative()
        .ok_or(invalid("provider completion narrative is missing"))?;
    let initial_state = ProviderNarrativeCompletionState::Pending(
        ProviderNarrativeComparisonFrontier::initial(narrative),
    );
    if !build.frame_staged() {
        return if actual.state() == initial_state {
            Ok(())
        } else {
            Err(invalid(
                "provider completion comparison advanced before frame staging",
            ))
        };
    }
    let mut replay = ProviderItemBuildRecord::new(
        build.item_id(),
        build.turn_id(),
        build.source().clone(),
        build.source_event(),
        build.revision(),
        build.prior().cloned(),
        build.target().clone(),
        build.staged_chunk_count(),
        build.staged_encoded_bytes(),
        build.staged_chain_digest(),
        build.staged_narrative(),
        Some(ProviderNarrativeCompletionCheck::new(
            actual.source(),
            initial_state,
        )),
        crate::ProviderItemBuildLifecycle::Staging,
    )
    .map_err(|_| invalid("provider completion replay seed is invalid"))?;
    loop {
        let replay_state = replay
            .completion_check()
            .expect("completion replay retains its check")
            .state();
        if replay_state == actual.state() {
            return Ok(());
        }
        if comparison_has_reached_or_passed(replay_state, actual.state()) {
            return Err(invalid(
                "provider completion persisted comparison state is not reproducible",
            ));
        }
        let next = advance_provider_completion_comparison(reader, &replay)?;
        replay = replay
            .advance_completion(next)
            .map_err(|_| invalid("provider completion replay advance is invalid"))?;
    }
}

fn require_staged_target(
    build: &ProviderItemBuildRecord,
    prior: ProviderNarrativeReference,
) -> Result<(), ProviderFrameStorageValidationError> {
    let summary = build.target().content().summary();
    if build.staged_chunk_count() != summary.chunk_count()
        || build.staged_encoded_bytes() != summary.encoded_bytes()
        || build.staged_chain_digest() != summary.digest()
        || build.staged_narrative() != Some(prior)
        || build.target().narrative() != Some(prior)
    {
        return Err(invalid(
            "provider completion comparison requires the exact staged frame target",
        ));
    }
    Ok(())
}

fn compare_page(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
    narrative: ProviderNarrativeReference,
    completion_source_start: u64,
    comparison_end: u64,
    equal_lengths: bool,
    mut frontier: ProviderNarrativeComparisonFrontier,
) -> Result<ProviderNarrativeCompletionState, ProviderFrameStorageValidationError> {
    if frontier.compared_utf8_bytes() == narrative.logical_utf8_bytes() {
        return finish_equal(narrative, frontier);
    }
    let first = predecessor(reader, narrative, frontier.compared_utf8_bytes())?;
    let last = ProviderNarrativeSpanKey::last_for_generation(
        narrative.content_id(),
        narrative.generation(),
    );
    let page = reader
        .cursor::<ProviderNarrativeSpansCodec>(
            &CursorRange::closed(first, last),
            CursorDirection::Forward,
            CursorReadLimits::new(COMPARISON_PAGE_SPANS, 65_536)
                .expect("comparison page bounds are nonzero"),
        )
        .map_err(ProviderFrameStorageValidationError::Read)?;
    if page.records().is_empty() {
        return Err(invalid("provider completion narrative span is missing"));
    }

    let page_end = frontier
        .compared_utf8_bytes()
        .saturating_add(COMPARISON_PAGE_BYTES)
        .min(comparison_end);
    for stored in page.records() {
        let record = *stored.value();
        validate_record(
            stored.key(),
            record,
            narrative,
            build.staged_encoded_bytes(),
            frontier,
        )?;
        let take_start = frontier.compared_utf8_bytes();
        let take_end = record.logical_end().min(page_end);
        let live_start = record
            .source_start()
            .checked_add(take_start - record.logical_start())
            .ok_or(invalid("provider completion live source offset overflowed"))?;
        let live_end = live_start
            .checked_add(take_end - take_start)
            .ok_or(invalid("provider completion live source range overflowed"))?;
        let completion_start = completion_source_start
            .checked_add(take_start)
            .ok_or(invalid("provider completion source offset overflowed"))?;
        let completion_end = completion_start
            .checked_add(take_end - take_start)
            .ok_or(invalid("provider completion source range overflowed"))?;
        let live = read_range(reader, build, live_start, live_end)?;
        let completion = read_range(reader, build, completion_start, completion_end)?;
        if let Some(local) = live
            .iter()
            .zip(&completion)
            .position(|(left, right)| left != right)
        {
            let local = u64::try_from(local)
                .map_err(|_| invalid("provider completion mismatch offset overflowed"))?;
            return Ok(ProviderNarrativeCompletionState::Mismatch {
                utf8_byte_offset: take_start + local,
            });
        }
        frontier = advance_frontier(frontier, record, take_end)?;
        if take_end == comparison_end {
            return if equal_lengths {
                finish_equal(narrative, frontier)
            } else {
                Ok(ProviderNarrativeCompletionState::Mismatch {
                    utf8_byte_offset: comparison_end,
                })
            };
        }
        if take_end == page_end {
            return Ok(ProviderNarrativeCompletionState::Pending(frontier));
        }
    }
    if page.has_more() && frontier.compared_utf8_bytes() < comparison_end {
        Ok(ProviderNarrativeCompletionState::Pending(frontier))
    } else {
        Err(invalid(
            "provider completion narrative page ended before its bounded frontier",
        ))
    }
}

fn predecessor(
    reader: &DomainReader<'_, SyndicDomain>,
    narrative: ProviderNarrativeReference,
    compared: u64,
) -> Result<ProviderNarrativeSpanKey, ProviderFrameStorageValidationError> {
    let page = reader
        .cursor::<ProviderNarrativeSpansCodec>(
            &CursorRange::closed(
                ProviderNarrativeSpanKey::first_for_generation(
                    narrative.content_id(),
                    narrative.generation(),
                ),
                ProviderNarrativeSpanKey::new(
                    narrative.content_id(),
                    narrative.generation(),
                    compared,
                ),
            ),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, 512).expect("comparison predecessor bounds are nonzero"),
        )
        .map_err(ProviderFrameStorageValidationError::Read)?;
    page.records()
        .first()
        .map(|record| *record.key())
        .ok_or(invalid(
            "provider completion narrative predecessor is missing",
        ))
}

fn validate_record(
    key: &ProviderNarrativeSpanKey,
    record: ProviderNarrativeSpanRecord,
    narrative: ProviderNarrativeReference,
    content_frontier: u64,
    frontier: ProviderNarrativeComparisonFrontier,
) -> Result<(), ProviderFrameStorageValidationError> {
    if key.content_id() != narrative.content_id()
        || key.generation() != narrative.generation()
        || key.logical_start() != record.logical_start()
        || record.content_id() != narrative.content_id()
        || record.generation() != narrative.generation()
        || record.logical_start() > frontier.compared_utf8_bytes()
        || record.logical_end() <= frontier.compared_utf8_bytes()
        || record.source_end() > content_frontier
    {
        return Err(invalid(
            "provider completion narrative span frontier disagrees",
        ));
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
        frontier.verified_chain_digest(),
    )
    .map_err(|_| invalid("provider completion narrative span is invalid"))?;
    if expected != record {
        return Err(invalid("provider completion narrative chain disagrees"));
    }
    Ok(())
}

fn advance_frontier(
    frontier: ProviderNarrativeComparisonFrontier,
    record: ProviderNarrativeSpanRecord,
    compared: u64,
) -> Result<ProviderNarrativeComparisonFrontier, ProviderFrameStorageValidationError> {
    let (span_count, chain_digest) = if compared == record.logical_end() {
        (
            frontier
                .verified_span_count()
                .checked_add(1)
                .ok_or(invalid("provider completion span frontier overflowed"))?,
            record.resulting_chain_digest(),
        )
    } else {
        (
            frontier.verified_span_count(),
            frontier.verified_chain_digest(),
        )
    };
    Ok(ProviderNarrativeComparisonFrontier::from_stored_parts(
        compared,
        span_count,
        chain_digest,
    ))
}

const fn comparison_has_reached_or_passed(
    replay: ProviderNarrativeCompletionState,
    actual: ProviderNarrativeCompletionState,
) -> bool {
    match (replay, actual) {
        (
            ProviderNarrativeCompletionState::Pending(replay),
            ProviderNarrativeCompletionState::Pending(actual),
        ) => replay.compared_utf8_bytes() >= actual.compared_utf8_bytes(),
        (ProviderNarrativeCompletionState::Pending(_), _) => false,
        (_, _) => true,
    }
}

fn finish_equal(
    narrative: ProviderNarrativeReference,
    frontier: ProviderNarrativeComparisonFrontier,
) -> Result<ProviderNarrativeCompletionState, ProviderFrameStorageValidationError> {
    if frontier.compared_utf8_bytes() != narrative.logical_utf8_bytes()
        || frontier.verified_span_count() != narrative.span_count()
        || frontier.verified_chain_digest() != narrative.chain_digest()
    {
        return Err(invalid(
            "provider completion comparison ended before the narrative chain frontier",
        ));
    }
    Ok(ProviderNarrativeCompletionState::Equal)
}

fn read_range(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, ProviderFrameStorageValidationError> {
    super::super::content::read_encoded_range(
        reader,
        build.target().content().id(),
        build.staged_encoded_bytes(),
        start,
        end,
    )
    .map_err(|error| match error {
        crate::error::SyndicValidationError::Read(source) => {
            ProviderFrameStorageValidationError::Read(source)
        }
        crate::error::SyndicValidationError::Invariant(message) => {
            ProviderFrameStorageValidationError::Invariant(message)
        }
    })
}

const fn invalid(message: &'static str) -> ProviderFrameStorageValidationError {
    ProviderFrameStorageValidationError::Invariant(message)
}
