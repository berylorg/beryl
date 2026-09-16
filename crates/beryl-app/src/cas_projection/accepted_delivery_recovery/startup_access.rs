use beryl_home_store::{HomeCandidateRecoveryAccess, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, BindingRevision, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AbandonActiveBinding, AbandonStopOperation, DeliveryRecoveryCase,
    DeliveryRecoveryClassificationError, DeliveryRecoverySource, DeliveryRecoveryStartupCursor,
    DeliveryRecoveryStartupPage, SyndicReadError, SyndicStorage, SyndicTimestamp,
};

use super::{point_limit, startup_page_limits};
use crate::cas_projection::{
    ProjectionCoordinatorError, ProjectionPublicationFailure, live_source, ordinary, publication,
};

#[derive(Clone, Copy)]
pub(super) enum StartupAccess<'a> {
    Ordinary(&'a HomeStore, BerylHomeId, HomeGeneration),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

impl StartupAccess<'_> {
    pub(super) fn page(
        self,
        storage: &SyndicStorage,
        cursor: Option<DeliveryRecoveryStartupCursor>,
    ) -> Result<DeliveryRecoveryStartupPage, SyndicReadError> {
        match self {
            Self::Ordinary(home, ..) => {
                storage.delivery_recovery_startup_page(home, cursor, startup_page_limits())
            }
            Self::Candidate(home) => storage.delivery_recovery_startup_page_candidate(
                home,
                cursor,
                startup_page_limits(),
            ),
        }
    }

    pub(super) fn classify(
        self,
        storage: &SyndicStorage,
        source: &DeliveryRecoverySource,
    ) -> Result<DeliveryRecoveryCase, DeliveryRecoveryClassificationError> {
        match self {
            Self::Ordinary(home, ..) => {
                storage.classify_delivery_recovery(home, source, point_limit())
            }
            Self::Candidate(home) => {
                storage.classify_delivery_recovery_candidate(home, source, point_limit())
            }
        }
    }

    pub(super) fn rebase(
        self,
        storage: &SyndicStorage,
        cursor: DeliveryRecoveryStartupCursor,
    ) -> Result<DeliveryRecoveryStartupCursor, SyndicReadError> {
        match self {
            Self::Ordinary(home, ..) => {
                storage.rebase_delivery_recovery_startup_cursor(home, cursor)
            }
            Self::Candidate(home) => {
                storage.rebase_delivery_recovery_startup_cursor_candidate(home, cursor)
            }
        }
    }

    pub(super) fn abandon_active(
        self,
        storage: &SyndicStorage,
        request: &AbandonActiveBinding,
    ) -> Result<BindingRevision, ProjectionPublicationFailure> {
        match self {
            Self::Ordinary(home, id, generation) => publication::abandon_active_reconciled(
                home,
                id,
                generation,
                storage,
                request,
                point_limit(),
            ),
            Self::Candidate(home) => publication::abandon_active_candidate(home, storage, request),
        }
    }

    pub(super) fn abandon_stop(
        self,
        storage: &SyndicStorage,
        request: &AbandonStopOperation,
    ) -> Result<(), ProjectionPublicationFailure> {
        match self {
            Self::Ordinary(home, id, generation) => publication::abandon_stop_reconciled(
                home,
                id,
                generation,
                storage,
                request,
                point_limit(),
            ),
            Self::Candidate(home) => publication::abandon_stop_candidate(home, storage, request),
        }
    }

    pub(super) fn converge_history(
        self,
        storage: &SyndicStorage,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        minimum: SyndicTimestamp,
    ) -> Result<(), ordinary::OrdinaryTurnExecutionError> {
        match self {
            Self::Ordinary(home, ..) => ordinary::converge_terminal_history(
                home,
                storage,
                thread,
                turn,
                minimum,
                point_limit(),
                None,
            ),
            Self::Candidate(home) => ordinary::converge_terminal_history_candidate(
                home,
                storage,
                thread,
                turn,
                minimum,
                point_limit(),
            ),
        }
    }

    pub(super) fn publish_terminal(
        self,
        storage: &SyndicStorage,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        minimum: SyndicTimestamp,
    ) -> Result<(), ProjectionCoordinatorError> {
        match self {
            Self::Ordinary(home, id, generation) => super::publish_source_less_terminal(
                home, id, generation, storage, thread, turn, minimum,
            ),
            Self::Candidate(home) => {
                live_source::publish_source_less_terminal_candidate(
                    home,
                    storage,
                    thread,
                    turn,
                    minimum,
                    point_limit(),
                )
                .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)?;
                self.converge_history(storage, thread, turn, minimum)
                    .map_err(|_| ProjectionCoordinatorError::AcceptedDeliveryRecoveryPublication)
            }
        }
    }

    pub(super) fn converge_compaction(
        self,
        storage: &SyndicStorage,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
    ) -> Result<(), ProjectionCoordinatorError> {
        match self {
            Self::Ordinary(home, ..) => {
                super::converge_compaction_restart(home, storage, thread, turn)
            }
            Self::Candidate(home) => {
                super::converge_compaction_restart_candidate(home, storage, thread, turn)
            }
        }
    }
}
