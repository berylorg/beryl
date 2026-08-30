use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{SessionRevision, SyndicThreadId, WindowId, WindowPlacement};

use crate::RecordRevision;

use crate::session::{
    MAX_RESTORABLE_WINDOWS, RememberedTarget, SessionDomain, SessionExitIntent, SessionHeader,
    SessionMutationError, SessionWindowRecord, ThreadClaimRecord, ThreadClaimState,
    WindowClaimSelection,
    codec::{ClaimByThreadCodec, ClaimByWindowCodec, SessionHeaderCodec, SessionWindowCodec},
};

use super::shared::{
    claim_by_thread, claim_by_window, delete_claim, ensure_claim_expectation, header,
    initial_claim_revision, initial_session_revision, insert_reference, put_claim, put_header,
    put_window, replace_reference, required_claim, required_header, required_window,
};

/// Initialize the sole permitted zero-runtime, threadless main window.
pub struct InitializeThreadlessWindow {
    window_id: WindowId,
    placement: WindowPlacement,
}

pub(crate) struct InitializeThreadlessWindowPrepared {
    header: SessionHeader,
    window: SessionWindowRecord,
}

impl InitializeThreadlessWindow {
    #[must_use]
    pub const fn new(window_id: WindowId, placement: WindowPlacement) -> Self {
        Self {
            window_id,
            placement,
        }
    }
}

impl DomainMutation<SessionDomain> for InitializeThreadlessWindow {
    type Error = SessionMutationError;
    type Prepared = InitializeThreadlessWindowPrepared;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if header(reader)?.is_some() {
            return Err(SessionMutationError::AlreadyInitialized);
        }
        let window = SessionWindowRecord {
            window_id: self.window_id,
            remembered_target: None,
            selected_thread: None,
            placement: self.placement,
            revision: RecordRevision::INITIAL,
        };
        let header = SessionHeader {
            revision: initial_session_revision(),
            exit_intent: SessionExitIntent::Running,
            fallback: None,
            windows: vec![crate::session::SessionWindowReference::new(
                window.window_id,
                window.revision,
            )],
        };
        Ok(InitializeThreadlessWindowPrepared { header, window })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        reservation.reserve_records::<SessionWindowCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        put_window(mutations, &prepared.window)?;
        put_header(mutations, &prepared.header)
    }
}

/// Add one runtime-backed window only after its exclusive thread is known.
pub struct CreateClaimedWindow {
    expected_session_revision: SessionRevision,
    window_id: WindowId,
    target: RememberedTarget,
    thread_id: SyndicThreadId,
    placement: WindowPlacement,
}

pub(crate) struct CreateClaimedWindowPrepared {
    header: SessionHeader,
    window: SessionWindowRecord,
    claim: ThreadClaimRecord,
}

impl CreateClaimedWindow {
    #[must_use]
    pub const fn new(
        expected_session_revision: SessionRevision,
        window_id: WindowId,
        target: RememberedTarget,
        thread_id: SyndicThreadId,
        placement: WindowPlacement,
    ) -> Self {
        Self {
            expected_session_revision,
            window_id,
            target,
            thread_id,
            placement,
        }
    }
}

impl DomainMutation<SessionDomain> for CreateClaimedWindow {
    type Error = SessionMutationError;
    type Prepared = CreateClaimedWindowPrepared;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut header = validate_create(&self, reader)?;
        let next_session = header.revision.checked_next()?;
        let claim = ThreadClaimRecord::new(
            self.window_id,
            self.thread_id,
            next_session,
            ThreadClaimState::Active,
            initial_claim_revision(),
        );
        let window = SessionWindowRecord {
            window_id: self.window_id,
            remembered_target: Some(self.target),
            selected_thread: Some(claim.selection()),
            placement: self.placement,
            revision: RecordRevision::INITIAL,
        };
        insert_reference(&mut header, self.window_id, window.revision)?;
        header.revision = next_session;
        header.fallback = Some(self.target);
        Ok(CreateClaimedWindowPrepared {
            header,
            window,
            claim,
        })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        reservation.reserve_records::<SessionWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByThreadCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        put_window(mutations, &prepared.window)?;
        put_claim(mutations, prepared.claim)?;
        put_header(mutations, &prepared.header)
    }
}

fn validate_create(
    command: &CreateClaimedWindow,
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<SessionHeader, SessionMutationError> {
    let header = required_header(reader, command.expected_session_revision, true)?;
    if header.windows.len() == MAX_RESTORABLE_WINDOWS {
        return Err(SessionMutationError::WindowLimit);
    }
    if !header.windows.is_empty() && header.fallback.is_none() {
        return Err(SessionMutationError::InvalidCurrentState(
            "initial threadless window must establish its claim before another window is created",
        ));
    }
    if header
        .windows
        .iter()
        .any(|reference| reference.window_id == command.window_id)
        || claim_by_window(reader, command.window_id)?.is_some()
    {
        return Err(SessionMutationError::WindowExists {
            window_id: command.window_id,
        });
    }
    if let Some(claim) = claim_by_thread(reader, command.thread_id)? {
        return Err(SessionMutationError::ThreadAlreadyClaimed {
            thread_id: command.thread_id,
            window_id: claim.window_id,
        });
    }
    Ok(header)
}

/// Establish or replace one window claim while releasing the old thread atomically.
pub struct ReplaceWindowClaim {
    expected_session_revision: SessionRevision,
    window_id: WindowId,
    expected_window_revision: RecordRevision,
    expected_claim: Option<WindowClaimSelection>,
    target: RememberedTarget,
    thread_id: SyndicThreadId,
}

impl ReplaceWindowClaim {
    #[must_use]
    pub const fn new(
        expected_session_revision: SessionRevision,
        window_id: WindowId,
        expected_window_revision: RecordRevision,
        expected_claim: Option<WindowClaimSelection>,
        target: RememberedTarget,
        thread_id: SyndicThreadId,
    ) -> Self {
        Self {
            expected_session_revision,
            window_id,
            expected_window_revision,
            expected_claim,
            target,
            thread_id,
        }
    }
}

impl DomainMutation<SessionDomain> for ReplaceWindowClaim {
    type Error = SessionMutationError;
    type Prepared = ReplacePlan;

    fn prepare(
        self,
        reader: &DomainReader<'_, SessionDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        prepare_replace(&self, reader)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SessionHeaderCodec>(1)?;
        reservation.reserve_records::<SessionWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByWindowCodec>(1)?;
        reservation.reserve_records::<ClaimByThreadCodec>(2)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SessionDomain>,
    ) -> Result<(), Self::Error> {
        let ReplacePlan {
            header,
            window,
            old_claim,
            claim,
        } = prepared;

        if let Some(old_claim) = old_claim {
            delete_claim(mutations, old_claim, false)?;
        }
        put_claim(mutations, claim)?;
        put_window(mutations, &window)?;
        put_header(mutations, &header)
    }
}

pub(crate) struct ReplacePlan {
    header: SessionHeader,
    window: SessionWindowRecord,
    old_claim: Option<ThreadClaimRecord>,
    claim: ThreadClaimRecord,
}

fn prepare_replace(
    command: &ReplaceWindowClaim,
    reader: &DomainReader<'_, SessionDomain>,
) -> Result<ReplacePlan, SessionMutationError> {
    let header = required_header(reader, command.expected_session_revision, true)?;
    let window = required_window(
        reader,
        &header,
        command.window_id,
        command.expected_window_revision,
    )?;
    ensure_claim_expectation(
        command.window_id,
        command.expected_claim,
        window.selected_thread,
    )?;
    if window
        .selected_thread
        .is_some_and(|selection| selection.thread_id == command.thread_id)
    {
        return Err(SessionMutationError::SameThreadClaim {
            window_id: command.window_id,
        });
    }
    let old_claim = match window.selected_thread {
        Some(selection) => {
            let claim = required_claim(reader, command.window_id, selection)?;
            if claim.state != ThreadClaimState::Active {
                return Err(SessionMutationError::InvalidCurrentState(
                    "restoring claim must be activated before ordinary replacement",
                ));
            }
            Some(claim)
        }
        None => {
            if claim_by_window(reader, command.window_id)?.is_some() {
                return Err(SessionMutationError::ClaimCopiesDisagree {
                    window_id: command.window_id,
                });
            }
            None
        }
    };
    if let Some(claim) = claim_by_thread(reader, command.thread_id)? {
        return Err(SessionMutationError::ThreadAlreadyClaimed {
            thread_id: command.thread_id,
            window_id: claim.window_id,
        });
    }
    let next_claim_revision = match old_claim {
        Some(claim) => claim.revision.checked_next()?,
        None => initial_claim_revision(),
    };
    let next_session = header.revision.checked_next()?;
    let claim = ThreadClaimRecord::new(
        command.window_id,
        command.thread_id,
        next_session,
        ThreadClaimState::Active,
        next_claim_revision,
    );
    let mut window = window;
    window.remembered_target = Some(command.target);
    window.selected_thread = Some(claim.selection());
    window.revision = window.revision.checked_next()?;
    let mut header = header;
    header.revision = next_session;
    header.fallback = Some(command.target);
    replace_reference(&mut header, command.window_id, window.revision)?;
    Ok(ReplacePlan {
        header,
        window,
        old_claim,
        claim,
    })
}
