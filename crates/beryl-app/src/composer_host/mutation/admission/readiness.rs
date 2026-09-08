use gpui_text_input::{MutationPageItem, ObjectChange};
use syndic_storage::{
    DraftMarkerLabelAssignmentOutcomeV1 as AssignmentOutcome,
    DraftMarkerLabelAssignmentRefusalV1 as AssignmentRefusal,
    DraftMarkerLabelReadinessDispositionV1, DraftMarkerLabelReadinessPageRequestV1,
    DraftMarkerLabelReadinessPageSubmissionOutcomeV1 as PageOutcome,
    DraftMarkerLabelReadinessPageSubmissionRefusalV1 as PageRefusal,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceSelectorV1 as Selector,
    DraftMarkerReadinessWitnessFactoryV1,
};

use super::*;

impl SyndicComposerHost {
    pub(super) fn advance_evidence_page(
        &mut self,
        store: &HomeStore,
        admission: &mut ComposerHostMutationAdmission,
    ) -> Result<AdmissionStep, ComposerHostError> {
        let mut page = admission
            .page
            .take()
            .ok_or(ComposerHostError::MutationMalformed)?;
        let result = (|| {
            if page.readiness.is_none() {
                let mut associations = Vec::with_capacity(MAX_READINESS_ASSOCIATIONS);
                let mut source_kind = None;
                while page.next_item < page.page.items().len() {
                    let association = if page.page.key().lane() == MutationLane::Proposal {
                        evidence_association(admission, &page, page.next_item)?
                    } else {
                        None
                    };
                    if let Some((association, kind)) = association {
                        if source_kind.is_some_and(|expected| expected != kind)
                            || associations.len() == MAX_READINESS_ASSOCIATIONS
                        {
                            break;
                        }
                        source_kind = Some(kind);
                        associations.push(association);
                    }
                    page.next_item += 1;
                }
                if associations.is_empty() {
                    return Ok((AdmissionStep::More, true));
                }
                page.readiness = Some(ReadinessPage {
                    command: admission.command()?,
                    ordinal: admission.next_ordinal,
                    eof: false,
                    source_kind,
                    associations: associations.into_boxed_slice(),
                    completed: 0,
                    flight: None,
                });
            }
            let readiness = page
                .readiness
                .as_mut()
                .ok_or(ComposerHostError::MutationMalformed)?;
            let step = self.advance_readiness_page(store, admission, readiness)?;
            if readiness.completed == readiness.associations.len() {
                page.readiness = None;
                admission.next_page_ordinal()?;
            }
            let complete = page.readiness.is_none() && page.next_item == page.page.items().len();
            Ok((step, complete))
        })();
        match result {
            Ok((step, true)) if admission.failure.is_none() => {
                match page.page.key().lane() {
                    MutationLane::Source => admission.source = page.frontier,
                    MutationLane::Proposal => admission.proposal = page.frontier,
                }
                Ok(step)
            }
            Ok((step, _)) => {
                admission.page = Some(page);
                Ok(step)
            }
            Err(error) => {
                admission.page = Some(page);
                Err(error)
            }
        }
    }

    pub(super) fn advance_readiness_page(
        &mut self,
        store: &HomeStore,
        admission: &mut ComposerHostMutationAdmission,
        page: &mut ReadinessPage,
    ) -> Result<AdmissionStep, ComposerHostError> {
        let flight = match page.flight.take() {
            Some(flight) => flight,
            None => {
                let factory = match page.source_kind {
                    Some(3) => Some(DraftMarkerReadinessWitnessFactoryV1::fresh(
                        admission
                            .assets
                            .draft_marker_fresh_asset_readiness_witness_factory(),
                    )),
                    Some(2) => Some(DraftMarkerReadinessWitnessFactoryV1::new(
                        admission
                            .assets
                            .draft_marker_label_readiness_witness_factory(),
                    )),
                    _ => None,
                };
                let request = DraftMarkerLabelReadinessPageRequestV1::new(
                    admission.owner,
                    page.command,
                    page.ordinal,
                    page.eof,
                    DraftMarkerLabelReadinessDispositionV1::Allocate,
                    page.associations.clone(),
                    factory,
                );
                let mut attempt = match self
                    .storage
                    .prepare_draft_marker_label_readiness_page(store, request)
                {
                    Ok(attempt) => attempt,
                    Err(error) => {
                        admission.fail(source_failure(error));
                        return Ok(AdmissionStep::More);
                    }
                };
                let command = match attempt.take_command() {
                    Some(command) => command,
                    None => {
                        admission.fail(ComposerHostMutationAdmissionFailure::Rejected);
                        return Ok(AdmissionStep::More);
                    }
                };
                let receipt = match store.compose_proof(command) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        admission.fail(ComposerHostMutationAdmissionFailure::Source(
                            DraftMarkerReadinessSourceErrorV1::Compose(error),
                        ));
                        return Ok(AdmissionStep::More);
                    }
                };
                match attempt.into_submission_flight(store, receipt) {
                    Ok(flight) => flight,
                    Err(error) => {
                        admission.fail(source_failure(error));
                        return Ok(AdmissionStep::More);
                    }
                }
            }
        };
        #[cfg(feature = "test-faults")]
        let flight = match self.mutation_admission_retained_limits {
            Some(limits) => flight.with_retained_limits_for_test(limits),
            None => flight,
        };
        match self
            .storage
            .submit_draft_marker_label_readiness_page(store, flight)
        {
            PageOutcome::Advanced {
                receipt,
                later_failure,
            } => {
                admission.durable_progress = true;
                page.completed += 1;
                if let Some(error) = later_failure {
                    admission.fail(ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                        receipt,
                        later_failure: Some(error),
                        reason: DraftMarkerAdmissionCommittedUnavailableReasonV1::LaterFailure,
                    });
                }
                Ok(AdmissionStep::More)
            }
            PageOutcome::Replayed if page.eof && page.associations.is_empty() => {
                admission.durable_progress = true;
                page.completed = 1;
                Ok(AdmissionStep::More)
            }
            PageOutcome::Replayed | PageOutcome::Collision => {
                admission.fail(ComposerHostMutationAdmissionFailure::Rejected);
                Ok(AdmissionStep::More)
            }
            PageOutcome::Retryable => Ok(AdmissionStep::Pending),
            PageOutcome::ReconciliationPending(flight) => {
                page.flight = Some(flight);
                Ok(AdmissionStep::Pending)
            }
            PageOutcome::Refused(refusal) => {
                admission.fail(match refusal {
                    PageRefusal::OperationTooLarge => {
                        ComposerHostMutationAdmissionFailure::OperationTooLarge
                    }
                    PageRefusal::CapacityUnavailable => {
                        ComposerHostMutationAdmissionFailure::CapacityUnavailable
                    }
                    PageRefusal::Obsolete => ComposerHostMutationAdmissionFailure::Conflict,
                    PageRefusal::Rejected => ComposerHostMutationAdmissionFailure::Rejected,
                    PageRefusal::Unavailable => {
                        return Ok(AdmissionStep::Unavailable(
                            ComposerHostMutationAdmissionFailure::Unavailable,
                        ));
                    }
                });
                Ok(AdmissionStep::More)
            }
            PageOutcome::StorageError(error) => {
                admission.fail(ComposerHostMutationAdmissionFailure::Storage(error));
                Ok(AdmissionStep::More)
            }
            PageOutcome::CommittedUnavailable {
                receipt,
                later_failure,
                reason,
            } => {
                admission.durable_progress = true;
                admission.fail(ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                    receipt,
                    later_failure,
                    reason,
                });
                Ok(AdmissionStep::More)
            }
        }
    }

    pub(super) fn advance_readiness_assignment(
        &mut self,
        store: &HomeStore,
        admission: &mut ComposerHostMutationAdmission,
    ) -> Result<AdmissionStep, ComposerHostError> {
        let command = match admission.assignment_command {
            Some(command) => command,
            None => {
                let command = admission.command()?;
                admission.assignment_command = Some(command);
                command
            }
        };
        let flight = match admission.assignment_flight.take() {
            Some(flight) => flight,
            None => match self.storage.prepare_draft_marker_label_assignment(
                store,
                admission.owner,
                command,
            ) {
                Ok(flight) => flight,
                Err(error) => {
                    admission.fail(assignment_failure(error));
                    return Ok(AdmissionStep::More);
                }
            },
        };
        admission.assignment_error = None;
        match self
            .storage
            .submit_draft_marker_label_assignment(store, flight)
        {
            AssignmentOutcome::Advanced {
                receipt,
                later_failure,
            } => {
                admission.durable_progress = true;
                admission.assignment_command = None;
                if let Some(error) = later_failure {
                    admission.fail(ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                        receipt,
                        later_failure: Some(error),
                        reason: DraftMarkerAdmissionCommittedUnavailableReasonV1::LaterFailure,
                    });
                }
                Ok(AdmissionStep::More)
            }
            AssignmentOutcome::Ready {
                proof,
                receipt,
                later_failure,
            } => {
                admission.durable_progress = true;
                if let Some(error) = later_failure {
                    admission.fail(ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                        receipt,
                        later_failure: Some(error),
                        reason: DraftMarkerAdmissionCommittedUnavailableReasonV1::LaterFailure,
                    });
                    return Ok(AdmissionStep::More);
                }
                match self.storage.prepare_draft_mutation_staging_marker_begin(
                    admission.storage_begin,
                    &admission.session,
                    proof,
                ) {
                    Ok(prepared) => admission.prepared_begin = Some(prepared),
                    Err(error) => {
                        admission.fail(ComposerHostMutationAdmissionFailure::Staging(error))
                    }
                }
                Ok(AdmissionStep::More)
            }
            AssignmentOutcome::CommittedReadinessPending { flight, error } => {
                admission.durable_progress = true;
                admission.assignment_flight = Some(flight);
                admission.assignment_error = Some(error);
                Ok(AdmissionStep::More)
            }
            AssignmentOutcome::ReconciliationPending(flight) => {
                admission.assignment_flight = Some(flight);
                Ok(AdmissionStep::Pending)
            }
            AssignmentOutcome::Retryable => Ok(AdmissionStep::Pending),
            AssignmentOutcome::Refused(refusal) => {
                admission.fail(match refusal {
                    AssignmentRefusal::OperationTooLarge => {
                        ComposerHostMutationAdmissionFailure::OperationTooLarge
                    }
                    AssignmentRefusal::CapacityUnavailable => {
                        ComposerHostMutationAdmissionFailure::CapacityUnavailable
                    }
                    AssignmentRefusal::Obsolete => ComposerHostMutationAdmissionFailure::Conflict,
                    AssignmentRefusal::Rejected => ComposerHostMutationAdmissionFailure::Rejected,
                    AssignmentRefusal::Unavailable => {
                        return Ok(AdmissionStep::Unavailable(
                            ComposerHostMutationAdmissionFailure::Unavailable,
                        ));
                    }
                });
                Ok(AdmissionStep::More)
            }
            AssignmentOutcome::Collision => {
                admission.fail(ComposerHostMutationAdmissionFailure::Rejected);
                Ok(AdmissionStep::More)
            }
            AssignmentOutcome::StorageError(error) => {
                admission.fail(ComposerHostMutationAdmissionFailure::Storage(error));
                Ok(AdmissionStep::More)
            }
            AssignmentOutcome::CommittedUnavailable {
                receipt,
                later_failure,
                reason,
            } => {
                admission.durable_progress = true;
                admission.fail(ComposerHostMutationAdmissionFailure::CommittedUnavailable {
                    receipt,
                    later_failure,
                    reason,
                });
                Ok(AdmissionStep::More)
            }
        }
    }
}

fn evidence_association(
    admission: &ComposerHostMutationAdmission,
    page: &EvidencePage,
    index: usize,
) -> Result<Option<(DraftMarkerReadinessSourceAssociationV1, u8)>, ComposerHostError> {
    let (target, selector) = match page.page.items()[index] {
        MutationPageItem::Object(
            ObjectChange::Insert { object } | ObjectChange::Replace { object, .. },
        ) => {
            let metadata = page
                .metadata
                .iter()
                .find(|metadata| metadata.object_id() == object.id())
                .ok_or(ComposerHostError::MutationMalformed)?;
            (object.id(), metadata.source())
        }
        MutationPageItem::Object(ObjectChange::Move { target, object }) => {
            let candidate = admission.binding.candidate();
            (
                object.id(),
                Selector::Candidate(DraftMarkerReadinessCandidateSourceV1::new(
                    candidate.draft_id(),
                    candidate.session_id(),
                    candidate.candidate_generation(),
                    candidate.root(),
                    super::super::translation::marker_id(target.id()),
                )),
            )
        }
        _ => return Ok(None),
    };
    let kind = selector_kind(selector);
    Ok(Some((
        DraftMarkerReadinessSourceAssociationV1::new(
            super::super::translation::marker_id(target),
            selector,
        ),
        kind,
    )))
}

fn selector_kind(selector: Selector) -> u8 {
    match selector {
        Selector::Candidate(_) => 0,
        Selector::Cut(_) => 1,
        Selector::Accepted(_) => 2,
        Selector::FreshAsset(_) => 3,
    }
}

fn source_failure(
    error: DraftMarkerReadinessSourceErrorV1,
) -> ComposerHostMutationAdmissionFailure {
    match error {
        DraftMarkerReadinessSourceErrorV1::OperationTooLarge => {
            ComposerHostMutationAdmissionFailure::OperationTooLarge
        }
        DraftMarkerReadinessSourceErrorV1::CapacityUnavailable => {
            ComposerHostMutationAdmissionFailure::CapacityUnavailable
        }
        error => ComposerHostMutationAdmissionFailure::Source(error),
    }
}

fn assignment_failure(
    error: DraftMarkerLabelAssignmentErrorV1,
) -> ComposerHostMutationAdmissionFailure {
    match error {
        DraftMarkerLabelAssignmentErrorV1::OperationTooLarge => {
            ComposerHostMutationAdmissionFailure::OperationTooLarge
        }
        DraftMarkerLabelAssignmentErrorV1::CapacityUnavailable => {
            ComposerHostMutationAdmissionFailure::CapacityUnavailable
        }
        error => ComposerHostMutationAdmissionFailure::Assignment(error),
    }
}
