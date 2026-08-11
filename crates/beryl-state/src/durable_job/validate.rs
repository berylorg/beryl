use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, PointReadLimit,
};
use beryl_model::{JobId, SyndicThreadId};

use super::{
    codec::{
        DiscussionAttemptIndexCodec, DiscussionAttemptKey, JobRecordCodec, LatestAttemptIndexCodec,
        LiveJobIndexCodec, RequestIdempotencyIndexCodec, RequestIndexKey,
    },
    BranchHandoffJobRecord, DurableJobDomain, DurableJobValidationError,
    LatestBranchHandoffAttempt, ResolutionAttemptOrdinal, ResolutionRequestAdmission,
    BRANCH_HANDOFF_JOB_RECORD_LIMIT,
};

const VALIDATION_PAGE_ITEMS: usize = 64;
const VALIDATION_PAGE_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn validate(
    reader: &DomainReader<'_, DurableJobDomain>,
) -> Result<(), DurableJobValidationError> {
    validate_records(reader)?;
    validate_live_index(reader)?;
    validate_requests(reader)?;
    validate_attempt_order(reader)?;
    validate_latest_index(reader)
}

fn validate_records(
    reader: &DomainReader<'_, DurableJobDomain>,
) -> Result<(), DurableJobValidationError> {
    let mut after = None;
    loop {
        let range = job_range(after);
        let page = reader.cursor::<JobRecordCodec>(
            &range,
            CursorDirection::Forward,
            validation_limits(),
        )?;
        for entry in page.records() {
            let job = entry.value();
            if entry.key() != &job.job_id {
                return invariant("job record key does not match its job identity");
            }
            let request = reader.point::<RequestIdempotencyIndexCodec>(
                &RequestIndexKey::new(job.request.clone()),
                small_point_limit(),
            )?;
            if request != Some(ResolutionRequestAdmission::from_job(job)) {
                return invariant("job request-idempotency index is missing or mismatched");
            }
            let attempt = reader.point::<DiscussionAttemptIndexCodec>(
                &DiscussionAttemptKey::new(job.discussion_thread_id, job.attempt_ordinal),
                small_point_limit(),
            )?;
            if attempt != Some(job.job_id) {
                return invariant("job discussion-attempt index is missing or mismatched");
            }
            let latest = reader
                .point::<LatestAttemptIndexCodec>(&job.discussion_thread_id, small_point_limit())?;
            let Some(latest) = latest else {
                return invariant("job discussion has no latest-attempt index");
            };
            if latest.attempt_ordinal() < job.attempt_ordinal {
                return invariant("job attempt is later than its discussion latest-attempt index");
            }
            if latest.attempt_ordinal() == job.attempt_ordinal && latest.job_id() != job.job_id {
                return invariant("latest attempt ordinal maps to a different job");
            }
            let live = reader.point::<LiveJobIndexCodec>(&job.job_id, job_point_limit())?;
            if job.lifecycle().is_live() {
                if live.as_ref() != Some(job) {
                    return invariant("live job index is missing or differs from its record");
                }
                if latest.job_id() != job.job_id {
                    return invariant("a non-latest discussion attempt remains live");
                }
            } else if live.is_some() {
                return invariant("terminal job remains present in the live-job index");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("bounded job cursor reported more without a record");
        }
    }
}

fn validate_live_index(
    reader: &DomainReader<'_, DurableJobDomain>,
) -> Result<(), DurableJobValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<LiveJobIndexCodec>(
            &job_range(after),
            CursorDirection::Forward,
            validation_limits(),
        )?;
        for entry in page.records() {
            let job = entry.value();
            if entry.key() != &job.job_id || !job.lifecycle().is_live() {
                return invariant("live-job index contains an invalid job entry");
            }
            let authoritative = reader.point::<JobRecordCodec>(entry.key(), job_point_limit())?;
            if authoritative.as_ref() != Some(job) {
                return invariant("live-job index differs from its authoritative record");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("bounded live-job cursor reported more without a record");
        }
    }
}

fn validate_requests(
    reader: &DomainReader<'_, DurableJobDomain>,
) -> Result<(), DurableJobValidationError> {
    let mut after = None;
    loop {
        let range = match after {
            Some(after) => CursorRange::after(after, RequestIndexKey::Upper),
            None => CursorRange::closed(RequestIndexKey::Lower, RequestIndexKey::Upper),
        };
        let page = reader.cursor::<RequestIdempotencyIndexCodec>(
            &range,
            CursorDirection::Forward,
            validation_limits(),
        )?;
        for entry in page.records() {
            let RequestIndexKey::Value(request) = entry.key() else {
                return invariant("request-idempotency index persisted a sentinel key");
            };
            let admission = entry.value();
            let job = required_job(reader, admission.job_id())?;
            if &job.request != request || ResolutionRequestAdmission::from_job(&job) != *admission {
                return invariant("request-idempotency entry differs from its job admission");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| entry.key().clone());
        if after.is_none() {
            return invariant("bounded request cursor reported more without a record");
        }
    }
}

fn validate_attempt_order(
    reader: &DomainReader<'_, DurableJobDomain>,
) -> Result<(), DurableJobValidationError> {
    let mut after = None;
    let mut previous: Option<BranchHandoffJobRecord> = None;
    loop {
        let range = attempt_range(after);
        let page = reader.cursor::<DiscussionAttemptIndexCodec>(
            &range,
            CursorDirection::Forward,
            validation_limits(),
        )?;
        for entry in page.records() {
            let key = *entry.key();
            let job = required_job(reader, *entry.value())?;
            if job.discussion_thread_id != key.discussion_thread_id()
                || job.attempt_ordinal != key.attempt_ordinal()
            {
                return invariant("discussion-attempt entry differs from its job record");
            }

            match previous.as_ref() {
                Some(previous) if previous.discussion_thread_id == job.discussion_thread_id => {
                    if !matches!(
                        previous.lifecycle(),
                        super::BranchHandoffJobLifecycle::TerminalFailed
                    ) {
                        return invariant(
                            "a later discussion attempt follows a non-terminal-failed attempt",
                        );
                    }
                    let expected = previous.attempt_ordinal.checked_next()?;
                    if job.attempt_ordinal != expected {
                        return invariant("discussion attempt ordinals are not contiguous");
                    }
                }
                Some(previous) => {
                    ensure_latest(reader, previous)?;
                    if job.attempt_ordinal != ResolutionAttemptOrdinal::FIRST {
                        return invariant("a discussion's first attempt ordinal is not one");
                    }
                }
                None => {
                    if job.attempt_ordinal != ResolutionAttemptOrdinal::FIRST {
                        return invariant("a discussion's first attempt ordinal is not one");
                    }
                }
            }
            previous = Some(job);
        }
        if !page.has_more() {
            if let Some(previous) = previous.as_ref() {
                ensure_latest(reader, previous)?;
            }
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("bounded attempt cursor reported more without a record");
        }
    }
}

fn validate_latest_index(
    reader: &DomainReader<'_, DurableJobDomain>,
) -> Result<(), DurableJobValidationError> {
    let mut after = None;
    loop {
        let range = thread_range(after);
        let page = reader.cursor::<LatestAttemptIndexCodec>(
            &range,
            CursorDirection::Forward,
            validation_limits(),
        )?;
        for entry in page.records() {
            let latest = entry.value();
            let job = required_job(reader, latest.job_id())?;
            if job.discussion_thread_id != *entry.key()
                || job.attempt_ordinal != latest.attempt_ordinal()
            {
                return invariant("latest-attempt entry differs from its job record");
            }
            let indexed = reader.point::<DiscussionAttemptIndexCodec>(
                &DiscussionAttemptKey::new(*entry.key(), latest.attempt_ordinal()),
                small_point_limit(),
            )?;
            if indexed != Some(latest.job_id()) {
                return invariant("latest attempt is missing from ordered attempt index");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("bounded latest-attempt cursor reported more without a record");
        }
    }
}

fn ensure_latest(
    reader: &DomainReader<'_, DurableJobDomain>,
    job: &BranchHandoffJobRecord,
) -> Result<(), DurableJobValidationError> {
    let latest =
        reader.point::<LatestAttemptIndexCodec>(&job.discussion_thread_id, small_point_limit())?;
    if latest != Some(LatestBranchHandoffAttempt::from_job(job)) {
        return invariant("discussion latest-attempt index does not name its final attempt");
    }
    Ok(())
}

fn required_job(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
) -> Result<BranchHandoffJobRecord, DurableJobValidationError> {
    reader
        .point::<JobRecordCodec>(&job_id, job_point_limit())?
        .ok_or(DurableJobValidationError::Invariant(
            "durable job index references a missing authoritative record",
        ))
}

fn job_range(after: Option<JobId>) -> CursorRange<JobId> {
    let upper = JobId::from_bytes([u8::MAX; 16]);
    match after {
        Some(after) => CursorRange::after(after, upper),
        None => CursorRange::closed(JobId::from_bytes([0; 16]), upper),
    }
}

fn thread_range(after: Option<SyndicThreadId>) -> CursorRange<SyndicThreadId> {
    let upper = SyndicThreadId::from_bytes([u8::MAX; 16]);
    match after {
        Some(after) => CursorRange::after(after, upper),
        None => CursorRange::closed(SyndicThreadId::from_bytes([0; 16]), upper),
    }
}

fn attempt_range(after: Option<DiscussionAttemptKey>) -> CursorRange<DiscussionAttemptKey> {
    let upper = DiscussionAttemptKey::new(
        SyndicThreadId::from_bytes([u8::MAX; 16]),
        ResolutionAttemptOrdinal::new(u64::MAX).expect("maximum attempt ordinal is nonzero"),
    );
    match after {
        Some(after) => CursorRange::after(after, upper),
        None => CursorRange::closed(
            DiscussionAttemptKey::new(
                SyndicThreadId::from_bytes([0; 16]),
                ResolutionAttemptOrdinal::FIRST,
            ),
            upper,
        ),
    }
}

fn validation_limits() -> CursorReadLimits {
    CursorReadLimits::new(VALIDATION_PAGE_ITEMS, VALIDATION_PAGE_BYTES)
        .expect("durable job validation limits are nonzero")
}

fn job_point_limit() -> PointReadLimit {
    PointReadLimit::new(BRANCH_HANDOFF_JOB_RECORD_LIMIT + 4)
        .expect("durable job point limit is nonzero")
}

fn small_point_limit() -> PointReadLimit {
    PointReadLimit::new(128).expect("durable job index point limit is nonzero")
}

fn invariant<T>(message: &'static str) -> Result<T, DurableJobValidationError> {
    Err(DurableJobValidationError::Invariant(message))
}
