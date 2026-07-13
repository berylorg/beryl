use std::collections::HashSet;

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainMutation, DomainReader, MutationBuilder,
};
use beryl_model::{SessionRevision, WindowId};

use crate::RecordRevision;

use crate::session::{
    MAX_SESSION_CLAIMS, SessionDomain, SessionExitIntent, SessionHeader, SessionMutationError,
    SessionWindowRecord, ThreadClaimRecord, ThreadClaimState, WindowClaimSelection,
    codec::ClaimByWindowCodec,
};

use super::shared::{
    delete_claim, ensure_claim_expectation, put_claim, put_header, put_window, replace_reference,
    required_claim, required_header, required_window,
};

/// Publish startup restoration, convert active claims, and remove paired stale claims.
pub struct BeginSessionRestore {
    expected_session_revision: SessionRevision,
}

impl BeginSessionRestore {
    #[must_use]
    pub const fn new(expected_session_revision: SessionRevision) -> Self {
        Self {
            expected_session_revision,
        }
    }
}

impl DomainMutation<SessionDomain> for BeginSessionRestore {
    type Error = SessionMutationError;

    fn validate(&self, reader: &DomainReader<'_, SessionDomain>) -> Result<(), Self::Error> {
        prepare_restore(self, reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SessionDomain>,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        let plan = prepare_restore(self, reader)?;
        for stale in plan.stale_claims {
            delete_claim(mutations, stale, true)?;
        }
        for claim in plan.changed_claims {
            put_claim(mutations, claim)?;
        }
        for window in plan.changed_windows {
            put_window(mutations, &window)?;
        }
        put_header(mutations, &plan.header)
    }
}

struct RestorePlan {
    header: SessionHeader,
    changed_windows: Vec<SessionWindowRecord>,
    changed_claims: Vec<ThreadClaimRecord>,
    stale_claims: Vec<ThreadClaimRecord>,
}

fn prepare_restore(
    command: &BeginSessionRestore,
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<RestorePlan, SessionMutationError> {
    let mut header = required_header(reader, command.expected_session_revision, false)?;
    let next_session = header.revision.checked_next()?;
    let mut active_claim_windows = HashSet::new();
    let mut changed_windows = Vec::new();
    let mut changed_claims = Vec::new();

    for reference in header.windows.clone() {
        let mut window = required_window(
            reader,
            &header,
            reference.window_id,
            reference.record_revision,
        )?;
        let Some(selection) = window.selected_thread else {
            continue;
        };
        active_claim_windows.insert(window.window_id);
        let claim = required_claim(reader, window.window_id, selection)?;
        if claim.state == ThreadClaimState::Restoring {
            continue;
        }
        let changed_claim = ThreadClaimRecord::new(
            claim.window_id,
            claim.thread_id,
            next_session,
            ThreadClaimState::Restoring,
            claim.revision.checked_next()?,
        );
        window.selected_thread = Some(changed_claim.selection());
        window.revision = window.revision.checked_next()?;
        replace_reference(&mut header, window.window_id, window.revision)?;
        changed_claims.push(changed_claim);
        changed_windows.push(window);
    }

    let stale_claims = read_claims(reader)?
        .into_iter()
        .filter(|claim| !active_claim_windows.contains(&claim.window_id))
        .collect();
    header.exit_intent = SessionExitIntent::Running;
    header.revision = next_session;
    Ok(RestorePlan {
        header,
        changed_windows,
        changed_claims,
        stale_claims,
    })
}

fn read_claims(
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<Vec<ThreadClaimRecord>, SessionMutationError> {
    let page = reader.cursor::<ClaimByWindowCodec>(
        &CursorRange::closed(
            WindowId::from_bytes([0; 16]),
            WindowId::from_bytes([u8::MAX; 16]),
        ),
        CursorDirection::Forward,
        CursorReadLimits::new(MAX_SESSION_CLAIMS + 1, 1024 * 1024)
            .expect("restore claim limits are nonzero"),
    )?;
    if page.has_more() || page.records().len() > MAX_SESSION_CLAIMS {
        return Err(SessionMutationError::InvalidCurrentState(
            "session claim family exceeds its bounded generation budget",
        ));
    }
    Ok(page
        .into_records()
        .into_iter()
        .map(|record| record.into_parts().1)
        .collect())
}

/// Mark one exact restoring claim active after its window activation succeeds.
pub struct ActivateRestoringClaim {
    expected_session_revision: SessionRevision,
    window_id: WindowId,
    expected_window_revision: RecordRevision,
    expected_claim: WindowClaimSelection,
}

impl ActivateRestoringClaim {
    #[must_use]
    pub const fn new(
        expected_session_revision: SessionRevision,
        window_id: WindowId,
        expected_window_revision: RecordRevision,
        expected_claim: WindowClaimSelection,
    ) -> Self {
        Self {
            expected_session_revision,
            window_id,
            expected_window_revision,
            expected_claim,
        }
    }
}

impl DomainMutation<SessionDomain> for ActivateRestoringClaim {
    type Error = SessionMutationError;

    fn validate(&self, reader: &DomainReader<'_, SessionDomain>) -> Result<(), Self::Error> {
        prepare_activation(self, reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SessionDomain>,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        let (mut header, mut window, old_claim) = prepare_activation(self, reader)?;
        let next_session = header.revision.checked_next()?;
        let claim = ThreadClaimRecord::new(
            old_claim.window_id,
            old_claim.thread_id,
            next_session,
            ThreadClaimState::Active,
            old_claim.revision.checked_next()?,
        );
        window.selected_thread = Some(claim.selection());
        window.revision = window.revision.checked_next()?;
        header.revision = next_session;
        header.fallback = window.remembered_target;
        replace_reference(&mut header, self.window_id, window.revision)?;
        put_claim(mutations, claim)?;
        put_window(mutations, &window)?;
        put_header(mutations, &header)
    }
}

fn prepare_activation(
    command: &ActivateRestoringClaim,
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<(SessionHeader, SessionWindowRecord, ThreadClaimRecord), SessionMutationError> {
    let header = required_header(reader, command.expected_session_revision, true)?;
    let window = required_window(
        reader,
        &header,
        command.window_id,
        command.expected_window_revision,
    )?;
    ensure_claim_expectation(
        command.window_id,
        Some(command.expected_claim),
        window.selected_thread,
    )?;
    let claim = required_claim(reader, command.window_id, command.expected_claim)?;
    if claim.state != ThreadClaimState::Restoring {
        return Err(SessionMutationError::ClaimNotRestoring {
            window_id: command.window_id,
        });
    }
    if window.remembered_target.is_none() {
        return Err(SessionMutationError::InvalidCurrentState(
            "restoring selected window has no remembered runtime/root pair",
        ));
    }
    Ok((header, window, claim))
}
