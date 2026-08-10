#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
use beryl_model::BerylHomeId;

use super::ProjectionConnectionService;
use attempt::ConnectionAdoptionState;

mod attempt;
mod disposition;
mod publication;
mod reauthentication;
mod transaction;
mod unpublished;

pub(in crate::cas_projection) use attempt::ServiceAdoptionAttempt;
pub use attempt::{
    AdoptedUnpublishedProjectionConnectionService, PersistentFailureServiceAdoptionError,
    PersistentFailureServiceAdoptionMetadata, PersistentFailureServiceAdoptionReason,
};
pub use publication::{
    RecoveredServicePublicationError, RecoveredServicePublicationMetadata,
    RecoveredServicePublicationReason,
};
pub(in crate::cas_projection) use reauthentication::RecoveredProjectionLaneStagingError;
pub use reauthentication::{
    AdoptedProjectionCandidateReauthenticationLedger,
    CandidateSetConvergedAdoptedProjectionConnectionService, ProjectionCandidateDispositionOutcome,
    ProjectionCandidateId, ProjectionCandidateLedgerAccessError, ProjectionCandidateLedgerMetadata,
    ProjectionCandidateLedgerSealError, ProjectionCandidateLedgerSealFailure,
    ProjectionCandidateLedgerSealReason, ProjectionCandidateMetadata,
    ProjectionCandidateReauthenticationOutcome, ProjectionCandidateReauthenticationReason,
    ProjectionCandidateReauthenticationStatus, RecoveredProjectionCandidateMetadata,
    TerminalAdoptedProjectionConnectionService, TerminalAdoptedProjectionConnectionServiceReason,
};
#[cfg(test)]
pub(in crate::cas_projection) use unpublished::ReplacementResourceFailureForTest;
pub use unpublished::{
    UnpublishedProjectionConnectionService, UnpublishedProjectionConnectionServiceBuildError,
    UnpublishedProjectionConnectionServiceMetadata,
};

#[cfg(test)]
static LATE_AUTHORITY_BEFORE_ADOPTION_COMMIT: OnceLock<Mutex<Option<BerylHomeId>>> =
    OnceLock::new();

#[cfg(test)]
static FAIL_FIRST_INERT_RESERVATION: OnceLock<Mutex<Option<BerylHomeId>>> = OnceLock::new();

#[cfg(test)]
static PAUSE_BEFORE_ADOPTION_COMMIT: OnceLock<
    Mutex<
        Option<(
            BerylHomeId,
            std::sync::mpsc::SyncSender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
> = OnceLock::new();

#[cfg(test)]
static PANIC_AFTER_OLD_INGESTER_JOIN: OnceLock<Mutex<Option<BerylHomeId>>> = OnceLock::new();

#[cfg(test)]
pub(in crate::cas_projection) struct AdoptionCommitPauseForTest {
    reached: std::sync::mpsc::Receiver<()>,
    release: Option<std::sync::mpsc::SyncSender<()>>,
}

#[cfg(test)]
pub(in crate::cas_projection) fn retain_late_authority_before_next_adoption_commit_for_test(
    home_id: BerylHomeId,
) {
    *LATE_AUTHORITY_BEFORE_ADOPTION_COMMIT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("the adoption late-authority test hook is usable") = Some(home_id);
}

#[cfg(test)]
pub(in crate::cas_projection) fn fail_next_first_inert_reservation_for_test(home_id: BerylHomeId) {
    *FAIL_FIRST_INERT_RESERVATION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("the first inert-reservation test hook is usable") = Some(home_id);
}

#[cfg(test)]
pub(in crate::cas_projection) fn pause_next_adoption_before_commit_for_test(
    home_id: BerylHomeId,
) -> AdoptionCommitPauseForTest {
    let (reached_tx, reached_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let previous = PAUSE_BEFORE_ADOPTION_COMMIT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("the adoption precommit-pause test hook is usable")
        .replace((home_id, reached_tx, release_rx));
    assert!(previous.is_none(), "one adoption precommit pause is armed");
    AdoptionCommitPauseForTest {
        reached: reached_rx,
        release: Some(release_tx),
    }
}

#[cfg(test)]
pub(in crate::cas_projection) fn panic_next_adoption_after_old_ingester_join_for_test(
    home_id: BerylHomeId,
) {
    *PANIC_AFTER_OLD_INGESTER_JOIN
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("the post-join adoption-panic test hook is usable") = Some(home_id);
}

#[cfg(test)]
impl AdoptionCommitPauseForTest {
    pub(in crate::cas_projection) fn wait_until_reached(&self) {
        self.reached
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("adoption reaches the precommit test pause");
    }

    pub(in crate::cas_projection) fn release(mut self) {
        self.release
            .take()
            .expect("the adoption precommit pause releases once")
            .send(())
            .expect("the paused adoption remains present");
    }
}

#[cfg(test)]
impl Drop for AdoptionCommitPauseForTest {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}
