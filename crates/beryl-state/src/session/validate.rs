use std::collections::{HashMap, HashSet};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, PointReadLimit,
};
use beryl_model::{SyndicThreadId, WindowId};

use super::error::SessionValidationError;
use super::{
    MAX_RESTORABLE_WINDOWS, MAX_SESSION_CLAIMS, SESSION_HEADER_V1_BYTES, SessionDomain,
    SessionWindowRecord, ThreadClaimRecord,
    codec::{
        ClaimByThreadCodec, ClaimByWindowCodec, HEADER_KEY, SessionHeaderCodec, SessionWindowCodec,
    },
};

const VALIDATION_BYTES: usize = 1024 * 1024;

pub(super) fn validate(
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<(), SessionValidationError> {
    let header =
        reader.point::<SessionHeaderCodec>(&HEADER_KEY, point_limit(SESSION_HEADER_V1_BYTES))?;
    let windows = read_windows(reader)?;
    let by_window = read_claims_by_window(reader)?;
    let by_thread = read_claims_by_thread(reader)?;

    let Some(header) = header else {
        if windows.is_empty() && by_window.is_empty() && by_thread.is_empty() {
            return Ok(());
        }
        return invariant("uninitialized session domain contains non-header records");
    };
    if windows.len() != header.windows.len() {
        return invariant("active session window references do not match stored windows");
    }
    validate_reverse_claims(header.revision, &by_window, &by_thread)?;

    let window_map: HashMap<_, _> = windows
        .iter()
        .map(|window| (window.window_id, window))
        .collect();
    let claim_window_map: HashMap<_, _> = by_window.iter().copied().collect();
    let claim_thread_map: HashMap<_, _> = by_thread.iter().copied().collect();
    let mut selected_threads = HashSet::new();
    let mut threadless = 0_usize;

    for reference in &header.windows {
        let window =
            window_map
                .get(&reference.window_id)
                .ok_or(SessionValidationError::Invariant(
                    "active header references a missing window",
                ))?;
        if window.revision != reference.record_revision {
            return invariant("active header carries the wrong window record revision");
        }
        validate_active_window(
            window,
            &claim_window_map,
            &claim_thread_map,
            &mut selected_threads,
        )?;
        if window.selected_thread.is_none() {
            threadless += 1;
        }
    }

    if threadless != 0 {
        if threadless != 1 || windows.len() != 1 || header.fallback.is_some() {
            return invariant("threadless window is not the sole zero-runtime initial window");
        }
    } else if !windows.is_empty() && header.fallback.is_none() {
        return invariant("runtime-backed session has no remembered fallback target");
    }
    Ok(())
}

fn validate_active_window(
    window: &SessionWindowRecord,
    by_window: &HashMap<WindowId, ThreadClaimRecord>,
    by_thread: &HashMap<SyndicThreadId, ThreadClaimRecord>,
    selected_threads: &mut HashSet<SyndicThreadId>,
) -> Result<(), SessionValidationError> {
    let Some(selection) = window.selected_thread else {
        if window.remembered_target.is_some() || by_window.contains_key(&window.window_id) {
            return invariant("threadless window retains a target or claim");
        }
        return Ok(());
    };
    if window.remembered_target.is_none() {
        return invariant("selected thread has no complete remembered runtime/root pair");
    }
    if !selected_threads.insert(selection.thread_id) {
        return invariant("one thread is selected by more than one active window");
    }
    let claim = by_window
        .get(&window.window_id)
        .ok_or(SessionValidationError::Invariant(
            "selected window has no forward claim",
        ))?;
    if claim.selection() != selection
        || claim.window_id != window.window_id
        || by_thread.get(&selection.thread_id) != Some(claim)
    {
        return invariant("selected window and two-way claim disagree");
    }
    Ok(())
}

fn validate_reverse_claims(
    session_revision: beryl_model::SessionRevision,
    by_window: &[(WindowId, ThreadClaimRecord)],
    by_thread: &[(SyndicThreadId, ThreadClaimRecord)],
) -> Result<(), SessionValidationError> {
    if by_window.len() != by_thread.len() {
        return invariant("reverse claim families contain different record counts");
    }
    let threads: HashMap<_, _> = by_thread.iter().copied().collect();
    for (window_id, claim) in by_window {
        if *window_id != claim.window_id {
            return invariant("claim-by-window key disagrees with its value");
        }
        if claim.generation > session_revision {
            return invariant("claim generation is newer than the active session");
        }
        if threads.get(&claim.thread_id) != Some(claim) {
            return invariant("claim reverse copies are missing or disagree");
        }
    }
    for (thread_id, claim) in by_thread {
        if *thread_id != claim.thread_id {
            return invariant("claim-by-thread key disagrees with its value");
        }
    }
    Ok(())
}

fn read_windows(
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<Vec<SessionWindowRecord>, SessionValidationError> {
    let page = reader.cursor::<SessionWindowCodec>(
        &CursorRange::closed(
            WindowId::from_bytes([0; 16]),
            WindowId::from_bytes([u8::MAX; 16]),
        ),
        CursorDirection::Forward,
        limits(MAX_RESTORABLE_WINDOWS + 1),
    )?;
    if page.has_more() || page.records().len() > MAX_RESTORABLE_WINDOWS {
        return invariant("session contains more than 256 window records");
    }
    let mut windows = Vec::with_capacity(page.records().len());
    for record in page.into_records() {
        let (key, value) = record.into_parts();
        if key != value.window_id {
            return invariant("window record key disagrees with its identity");
        }
        windows.push(value);
    }
    Ok(windows)
}

fn read_claims_by_window(
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<Vec<(WindowId, ThreadClaimRecord)>, SessionValidationError> {
    let page = reader.cursor::<ClaimByWindowCodec>(
        &CursorRange::closed(
            WindowId::from_bytes([0; 16]),
            WindowId::from_bytes([u8::MAX; 16]),
        ),
        CursorDirection::Forward,
        limits(MAX_SESSION_CLAIMS + 1),
    )?;
    if page.has_more() || page.records().len() > MAX_SESSION_CLAIMS {
        return invariant("session claim-by-window family exceeds its bounded generation budget");
    }
    Ok(page
        .into_records()
        .into_iter()
        .map(|record| record.into_parts())
        .collect())
}

fn read_claims_by_thread(
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<Vec<(SyndicThreadId, ThreadClaimRecord)>, SessionValidationError> {
    let page = reader.cursor::<ClaimByThreadCodec>(
        &CursorRange::closed(
            SyndicThreadId::from_bytes([0; 16]),
            SyndicThreadId::from_bytes([u8::MAX; 16]),
        ),
        CursorDirection::Forward,
        limits(MAX_SESSION_CLAIMS + 1),
    )?;
    if page.has_more() || page.records().len() > MAX_SESSION_CLAIMS {
        return invariant("session claim-by-thread family exceeds its bounded generation budget");
    }
    Ok(page
        .into_records()
        .into_iter()
        .map(|record| record.into_parts())
        .collect())
}

fn point_limit(payload: usize) -> PointReadLimit {
    PointReadLimit::new(payload + 4).expect("fixed session point limit is nonzero")
}

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, VALIDATION_BYTES).expect("session validation limits are nonzero")
}

fn invariant<T>(message: &'static str) -> Result<T, SessionValidationError> {
    Err(SessionValidationError::Invariant(message))
}
