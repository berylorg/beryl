use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, HomeProofReceipt, HomeStore, ReconciliationHandle,
    ReconciliationResolution,
};

use crate::{
    SyndicStorage,
    admission_attachment::{
        DraftMarkerAdmissionAttemptReservation, DraftMarkerAdmissionPreparedAttempt,
    },
    draft_piece::{
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadsFamily,
        DraftMarkerAdmissionOwnerV1, DraftMarkerLabelReadinessPageAttemptV1,
        DraftMarkerLabelReadinessProvenPageV1, DraftMarkerReadinessSourceErrorV1,
    },
};

use super::publication::{
    DraftMarkerAdmissionPublicationSeedV1, PublicationFailureClass, classify_not_committed,
};
use super::readiness_source::PageProtocol;

pub enum DraftMarkerLabelReadinessPageSubmissionRefusalV1 {
    Obsolete,
    FinalEvidenceEof,
    Unavailable,
    Rejected,
}

pub enum DraftMarkerLabelReadinessPageSubmissionOutcomeV1 {
    Advanced {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    Retryable,
    Refused(DraftMarkerLabelReadinessPageSubmissionRefusalV1),
    Collision,
    ReconciliationPending(DraftMarkerLabelReadinessPageSubmissionFlightV1),
}

pub struct DraftMarkerLabelReadinessPageSubmissionFlightV1 {
    state: SubmissionState,
}

enum SubmissionState {
    Ready {
        seed: DraftMarkerAdmissionPublicationSeedV1,
        page: DraftMarkerLabelReadinessProvenPageV1,
        reservation: DraftMarkerAdmissionPreparedAttempt,
    },
    Reconciling {
        owner: DraftMarkerAdmissionOwnerV1,
        handle: ReconciliationHandle,
        retry_failed: bool,
        retain_on_exact_old: bool,
        frontier: u64,
    },
}

impl DraftMarkerLabelReadinessPageAttemptV1 {
    pub fn into_submission_flight(
        self,
        store: &HomeStore,
        receipt: HomeProofReceipt<PageProtocol>,
    ) -> Result<DraftMarkerLabelReadinessPageSubmissionFlightV1, DraftMarkerReadinessSourceErrorV1>
    {
        let (page, seed, reservation) = self.consume_for_submission(store, receipt)?;
        Ok(DraftMarkerLabelReadinessPageSubmissionFlightV1 {
            state: SubmissionState::Ready {
                seed,
                page,
                reservation,
            },
        })
    }
}

impl SyndicStorage {
    pub fn submit_draft_marker_label_readiness_page(
        &self,
        store: &HomeStore,
        flight: DraftMarkerLabelReadinessPageSubmissionFlightV1,
    ) -> DraftMarkerLabelReadinessPageSubmissionOutcomeV1 {
        match flight.state {
            SubmissionState::Ready {
                seed,
                page,
                reservation,
            } => self.submit_ready(store, seed, page, reservation),
            SubmissionState::Reconciling {
                owner,
                handle,
                retry_failed,
                retain_on_exact_old,
                frontier,
            } => self.finish_reconciliation(
                store,
                owner,
                handle,
                retry_failed,
                retain_on_exact_old,
                frontier,
            ),
        }
    }

    fn submit_ready(
        &self,
        store: &HomeStore,
        seed: DraftMarkerAdmissionPublicationSeedV1,
        page: DraftMarkerLabelReadinessProvenPageV1,
        reservation: DraftMarkerAdmissionPreparedAttempt,
    ) -> DraftMarkerLabelReadinessPageSubmissionOutcomeV1 {
        let owner = seed.owner();
        let attempt = page.page_identity();
        let frontier = page.sealed_page().ordinal.get();
        if store
            .health()
            .generation()
            .is_none_or(|generation| generation.get() != seed.home_generation().get())
        {
            return DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
            );
        }
        let had_durable_before = match self.point::<DraftMarkerAdmissionHeadsFamily>(
            store,
            owner,
            crate::draft_piece::point_limit(),
        ) {
            Ok(head) => head.is_some(),
            Err(_) => {
                return DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                    DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                );
            }
        };
        let reservation = reservation.disarm();
        let outcome =
            store.execute_current(self.current_publish_draft_marker_admission_v1(seed, page));
        let retain_on_exact_old = had_durable_before || reservation.was_present;
        match outcome {
            CommandOutcome::NotCommitted { evidence } => self.finish_not_committed(
                store,
                owner,
                attempt,
                frontier,
                had_durable_before,
                reservation,
                classify_not_committed(&evidence),
            ),
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => match local_finalization {
                Some(local_finalization) => {
                    if !matches!(
                        store.with_committed_local_finalization(
                            local_finalization,
                            &receipt,
                            &self.handle,
                            |attachment| attachment
                                .finish_submission(owner, attempt, true, false, frontier),
                        ),
                        Ok(Ok(()))
                    ) {
                        return DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                        );
                    }
                    DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                        DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                    )
                }
                None => {
                    let receipt_is_current =
                        matches!(self.committed_revision(store, &receipt), Ok(Some(_)));
                    let released =
                        self.finish_local_attempt(store, owner, attempt, true, false, frontier);
                    if receipt_is_current && released {
                        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced {
                            receipt,
                            later_failure,
                        }
                    } else {
                        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                            DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                        )
                    }
                }
            },
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                let handle = reconciliation.install_and_handle();
                let _ = self.finish_local_attempt(store, owner, attempt, true, true, frontier);
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(
                    DraftMarkerLabelReadinessPageSubmissionFlightV1 {
                        state: SubmissionState::Reconciling {
                            owner,
                            handle,
                            retry_failed: false,
                            retain_on_exact_old,
                            frontier,
                        },
                    },
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_not_committed(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
        frontier: u64,
        had_durable_before: bool,
        reservation: DraftMarkerAdmissionAttemptReservation,
        classification: PublicationFailureClass,
    ) -> DraftMarkerLabelReadinessPageSubmissionOutcomeV1 {
        let retain_operation = had_durable_before || reservation.was_present;
        if !self.finish_local_attempt(store, owner, attempt, retain_operation, false, frontier) {
            return DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
            );
        }
        match classification {
            PublicationFailureClass::Obsolete => {
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                    DraftMarkerLabelReadinessPageSubmissionRefusalV1::Obsolete,
                )
            }
            PublicationFailureClass::FinalEvidenceEof => {
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                    DraftMarkerLabelReadinessPageSubmissionRefusalV1::FinalEvidenceEof,
                )
            }
            PublicationFailureClass::Collision => {
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Collision
            }
            PublicationFailureClass::Refused => {
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                    DraftMarkerLabelReadinessPageSubmissionRefusalV1::Rejected,
                )
            }
            PublicationFailureClass::Retryable => {
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Retryable
            }
        }
    }

    fn finish_reconciliation(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        handle: ReconciliationHandle,
        retry_failed: bool,
        retain_on_exact_old: bool,
        frontier: u64,
    ) -> DraftMarkerLabelReadinessPageSubmissionOutcomeV1 {
        let resolution = if retry_failed {
            store.retry_reconciliation(&handle)
        } else {
            store.reconcile(&handle)
        };
        match resolution {
            Ok(ReconciliationResolution::ExactOld)
                if self.resolve_local_reconciliation(
                    store,
                    owner,
                    retain_on_exact_old,
                    false,
                    frontier,
                ) =>
            {
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Retryable
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                if self.resolve_local_reconciliation(store, owner, true, false, frontier) {
                    DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced {
                        receipt,
                        later_failure: None,
                    }
                } else {
                    DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                        DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                    )
                }
            }
            Ok(ReconciliationResolution::Collision) => {
                if self.resolve_local_reconciliation(store, owner, true, true, frontier) {
                    DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Collision
                } else {
                    DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                        DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                    )
                }
            }
            Ok(ReconciliationResolution::ExactSuccessor { .. }) => {
                if !self.resolve_local_reconciliation(store, owner, true, true, frontier) {
                    return DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                        DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                    );
                }
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                    DraftMarkerLabelReadinessPageSubmissionRefusalV1::Rejected,
                )
            }
            Ok(ReconciliationResolution::ExactOld) => {
                DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Refused(
                    DraftMarkerLabelReadinessPageSubmissionRefusalV1::Unavailable,
                )
            }
            Err(_) => DraftMarkerLabelReadinessPageSubmissionOutcomeV1::ReconciliationPending(
                DraftMarkerLabelReadinessPageSubmissionFlightV1 {
                    state: SubmissionState::Reconciling {
                        owner,
                        handle,
                        retry_failed: true,
                        retain_on_exact_old,
                        frontier,
                    },
                },
            ),
        }
    }

    fn resolve_local_reconciliation(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        retain_operation: bool,
        uncertain_closed: bool,
        frontier: u64,
    ) -> bool {
        matches!(
            self.with_admission_attachment(store, |attachment| {
                attachment.resolve_submission(owner, retain_operation, uncertain_closed, frontier)
            }),
            Ok(Ok(()))
        )
    }

    fn finish_local_attempt(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
        retain_operation: bool,
        uncertain_closed: bool,
        frontier: u64,
    ) -> bool {
        matches!(
            self.with_admission_attachment(store, |attachment| {
                attachment.finish_submission(
                    owner,
                    attempt,
                    retain_operation,
                    uncertain_closed,
                    frontier,
                )
            }),
            Ok(Ok(()))
        )
    }

    fn with_admission_attachment<R>(
        &self,
        store: &HomeStore,
        callback: impl FnOnce(&crate::admission_attachment::DraftMarkerAdmissionAttachment) -> R,
    ) -> Result<R, beryl_home_store::DomainAttachmentAccessError> {
        store.with_domain_attachment(&self.handle.attachment_capability(), callback)
    }
}
