use std::num::NonZeroU64;
use std::sync::Arc;

use beryl_home_store::{CommandError, CommitReceipt};
use beryl_state::AssetState;
use gpui_text_input::{MutationKey, MutationPass, MutationPassKind};
use syndic_storage::{
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionCommittedUnavailableReasonV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionStorageErrorV1,
    DraftMarkerAdmissionTerminalFlightV1, DraftMarkerLabelAssignmentErrorV1,
    DraftMarkerLabelAssignmentFlightV1, DraftMarkerLabelReadinessPageSubmissionFlightV1,
    DraftMarkerReadinessSourceAssociationV1, DraftMarkerReadinessSourceErrorV1,
    PreparedDraftMutationStagingCommandV1,
};

use super::*;

mod begin;
mod readiness;
mod terminal;

const MAX_ADMISSION_TRANSITIONS: usize = 16;
const MAX_READINESS_ASSOCIATIONS: usize = 32;

pub enum ComposerHostMutationEvidenceRequest {
    Begin {
        begin: MutationBeginRequest,
        pass: MutationPass,
    },
    Page {
        pass: MutationPass,
        page: MutationPage,
        metadata: Box<[ComposerHostImageMarkerMetadata]>,
    },
    Finish {
        pass: MutationPass,
        finish: MutationFinishInput,
    },
    Advance(MutationKey),
    Cancel(MutationKey),
}

impl ComposerHostMutationEvidenceRequest {
    pub const fn key(&self) -> MutationKey {
        match self {
            Self::Begin { begin, .. } => begin.proposal().key(),
            Self::Page { pass, .. } | Self::Finish { pass, .. } => pass.key(),
            Self::Advance(key) | Self::Cancel(key) => *key,
        }
    }
}

pub enum ComposerHostMutationEvidenceOutcome {
    Started(MutationPass),
    PageAccepted(MutationPass),
    Pending(MutationKey),
    Began(MutationKey),
    Refused {
        key: MutationKey,
        failure: Arc<ComposerHostMutationAdmissionFailure>,
    },
    Unavailable {
        key: MutationKey,
        failure: Arc<ComposerHostMutationAdmissionFailure>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ComposerHostMutationAdmissionFailure {
    #[error("composer mutation was cancelled before admission")]
    Cancelled,
    #[error("composer mutation evidence was rejected")]
    Rejected,
    #[error("composer mutation predecessor changed")]
    Conflict,
    #[error("composer marker admission custody is unavailable")]
    Unavailable,
    #[error("composer mutation terminal cleanup is unavailable after {outcome:?}: {refusal:?}")]
    TerminalCleanup {
        outcome: ComposerHostMutationOutcome,
        refusal: Option<syndic_storage::DraftMarkerAdmissionTerminalRefusalV1>,
    },
    #[error("composer marker operation exceeds the fixed admission profile")]
    OperationTooLarge,
    #[error("composer marker admission capacity is temporarily unavailable")]
    CapacityUnavailable,
    #[error("composer marker admission storage failed: {0}")]
    Storage(#[from] DraftMarkerAdmissionStorageErrorV1),
    #[error("composer marker evidence failed: {0}")]
    Source(#[from] DraftMarkerReadinessSourceErrorV1),
    #[error("composer marker assignment failed: {0}")]
    Assignment(#[from] DraftMarkerLabelAssignmentErrorV1),
    #[error("composer marker admission committed but is unavailable: {reason}")]
    CommittedUnavailable {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        reason: DraftMarkerAdmissionCommittedUnavailableReasonV1,
    },
    #[error("composer marker staging admission failed: {0}")]
    Staging(syndic_storage::DraftMutationStagingErrorV1),
}

pub(in crate::composer_host) struct ComposerHostMutationAdmission {
    pub(super) binding: ComposerHostBinding,
    pub(super) begin: MutationBeginRequest,
    pass: MutationPass,
    storage_begin: DraftMutationBeginV1,
    session: DraftEditorCandidateSessionV1,
    assets: AssetState,
    owner: DraftMarkerAdmissionOwnerV1,
    source: WidgetLaneFrontier,
    proposal: WidgetLaneFrontier,
    page: Option<EvidencePage>,
    finish: Option<MutationFinishInput>,
    eof: Option<ReadinessPage>,
    eof_accepted: bool,
    next_ordinal: NonZeroU64,
    next_command: u64,
    assignment_command: Option<DraftMarkerAdmissionCommandIdV1>,
    assignment_flight: Option<DraftMarkerLabelAssignmentFlightV1>,
    assignment_error: Option<DraftMarkerLabelAssignmentErrorV1>,
    prepared_begin: Option<PreparedDraftMutationStagingCommandV1>,
    begin_attempted: bool,
    durable_progress: bool,
    failure: Option<Arc<ComposerHostMutationAdmissionFailure>>,
    terminal_command: Option<DraftMarkerAdmissionCommandIdV1>,
    terminal_flight: Option<DraftMarkerAdmissionTerminalFlightV1>,
    terminal_cleanup: bool,
    cancelling_staging: Option<Box<ComposerHostMutationCoordinator>>,
}

struct EvidencePage {
    page: MutationPage,
    metadata: Box<[ComposerHostImageMarkerMetadata]>,
    next_item: usize,
    frontier: WidgetLaneFrontier,
    readiness: Option<ReadinessPage>,
}

struct ReadinessPage {
    command: DraftMarkerAdmissionCommandIdV1,
    ordinal: NonZeroU64,
    eof: bool,
    source_kind: Option<u8>,
    associations: Box<[DraftMarkerReadinessSourceAssociationV1]>,
    completed: usize,
    flight: Option<DraftMarkerLabelReadinessPageSubmissionFlightV1>,
}

enum AdmissionStep {
    More,
    Pending,
    Unavailable(ComposerHostMutationAdmissionFailure),
}

impl ComposerHostMutationAdmission {
    fn command(&mut self) -> Result<DraftMarkerAdmissionCommandIdV1, ComposerHostError> {
        let serial = self.next_command;
        self.next_command = serial
            .checked_add(1)
            .ok_or(ComposerHostError::MutationMalformed)?;
        let mut bytes = *b"composer\0\0\0\0\0\0\0\0";
        bytes[8..].copy_from_slice(&serial.to_be_bytes());
        Ok(DraftMarkerAdmissionCommandIdV1::from_bytes(bytes))
    }

    fn next_page_ordinal(&mut self) -> Result<(), ComposerHostError> {
        self.next_ordinal = NonZeroU64::new(
            self.next_ordinal
                .get()
                .checked_add(1)
                .ok_or(ComposerHostError::MutationMalformed)?,
        )
        .ok_or(ComposerHostError::MutationMalformed)?;
        Ok(())
    }

    pub(super) fn fail(&mut self, failure: ComposerHostMutationAdmissionFailure) {
        if self.failure.is_none()
            || matches!(
                self.failure.as_deref(),
                Some(ComposerHostMutationAdmissionFailure::Cancelled)
            ) && !matches!(failure, ComposerHostMutationAdmissionFailure::Cancelled)
        {
            self.failure = Some(Arc::new(failure));
        }
    }

    fn unavailable(&self) -> ComposerHostMutationEvidenceOutcome {
        ComposerHostMutationEvidenceOutcome::Unavailable {
            key: self.pass.key(),
            failure: self
                .failure
                .clone()
                .unwrap_or_else(|| Arc::new(ComposerHostMutationAdmissionFailure::Unavailable)),
        }
    }
}

impl SyndicComposerHost {
    pub fn dispatch_mutation_evidence(
        &mut self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        assets: &AssetState,
        request: ComposerHostMutationEvidenceRequest,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostMutationEvidenceOutcome, ComposerHostError> {
        let key = request.key();
        if let ComposerHostMutationEvidenceRequest::Begin { begin, pass } = request {
            if cancellation.is_cancelled() {
                return Ok(ComposerHostMutationEvidenceOutcome::Refused {
                    key,
                    failure: Arc::new(ComposerHostMutationAdmissionFailure::Cancelled),
                });
            }
            return self.begin_mutation_evidence(store, binding, assets, begin, pass);
        }
        let pending = self
            .pending_mutation
            .take()
            .ok_or(ComposerHostError::MutationNotPending)?;
        let ComposerHostPendingMutation::Admission(mut admission) = pending else {
            self.pending_mutation = Some(pending);
            return Err(ComposerHostError::MutationNotPending);
        };
        let result = (|| {
            if admission.binding != binding || admission.pass.key() != key {
                return Err(ComposerHostError::RequestMismatch);
            }
            validate_store(binding, store)?;
            if cancellation.is_cancelled()
                || matches!(request, ComposerHostMutationEvidenceRequest::Cancel(_))
            {
                admission.fail(ComposerHostMutationAdmissionFailure::Cancelled);
            }
            match request {
                ComposerHostMutationEvidenceRequest::Page {
                    pass,
                    page,
                    metadata,
                } => {
                    if pass != admission.pass
                        || admission.finish.is_some()
                        || admission.page.is_some()
                    {
                        return Err(ComposerHostError::MutationMalformed);
                    }
                    translation::validate_marker_metadata_intake(&page, &metadata)?;
                    let frontier = match page.key().lane() {
                        MutationLane::Source => admission.source,
                        MutationLane::Proposal => admission.proposal,
                    };
                    if page.key().key() != key {
                        return Err(ComposerHostError::RequestMismatch);
                    }
                    let frontier = match frontier.prevalidate(&page)? {
                        WidgetPageDisposition::Replay => {
                            return Ok(ComposerHostMutationEvidenceOutcome::PageAccepted(pass));
                        }
                        WidgetPageDisposition::Accepted { frontier, .. } => frontier,
                    };
                    admission.page = Some(EvidencePage {
                        page,
                        metadata,
                        next_item: 0,
                        frontier,
                        readiness: None,
                    });
                }
                ComposerHostMutationEvidenceRequest::Finish { pass, finish } => {
                    if pass != admission.pass
                        || finish.key() != key
                        || admission.page.is_some()
                        || !admission.source.matches_finish(finish.source())
                        || !admission.proposal.matches_finish(finish.proposal())
                        || admission.finish.is_some_and(|retained| retained != finish)
                    {
                        return Err(ComposerHostError::MutationMalformed);
                    }
                    if admission.finish.is_none() {
                        admission.finish = Some(finish);
                        admission.eof = Some(ReadinessPage {
                            command: admission.command()?,
                            ordinal: admission.next_ordinal,
                            eof: true,
                            source_kind: None,
                            associations: Box::new([]),
                            completed: 0,
                            flight: None,
                        });
                    }
                }
                ComposerHostMutationEvidenceRequest::Advance(_)
                | ComposerHostMutationEvidenceRequest::Cancel(_) => {}
                ComposerHostMutationEvidenceRequest::Begin { .. } => unreachable!(),
            }
            self.drive_mutation_admission(store, &mut admission)
        })();
        if self.pending_mutation.is_none()
            && !matches!(
                result,
                Ok(ComposerHostMutationEvidenceOutcome::Refused { .. })
            )
        {
            self.pending_mutation = Some(ComposerHostPendingMutation::Admission(admission));
        }
        result
    }

    fn drive_mutation_admission(
        &mut self,
        store: &HomeStore,
        admission: &mut ComposerHostMutationAdmission,
    ) -> Result<ComposerHostMutationEvidenceOutcome, ComposerHostError> {
        let key = admission.pass.key();
        for _ in 0..MAX_ADMISSION_TRANSITIONS.min(self.mutation_transition_limit()) {
            if admission.failure.is_some() {
                return self.cancel_mutation_admission(store, admission);
            }
            if admission.page.is_some() {
                match self.advance_evidence_page(store, admission)? {
                    AdmissionStep::More => {
                        if admission.page.is_none() {
                            return Ok(ComposerHostMutationEvidenceOutcome::PageAccepted(
                                admission.pass,
                            ));
                        }
                    }
                    AdmissionStep::Pending => {
                        return Ok(ComposerHostMutationEvidenceOutcome::Pending(key));
                    }
                    AdmissionStep::Unavailable(failure) => {
                        return Ok(ComposerHostMutationEvidenceOutcome::Unavailable {
                            key,
                            failure: Arc::new(failure),
                        });
                    }
                }
                continue;
            }
            if admission.finish.is_none() {
                return Ok(ComposerHostMutationEvidenceOutcome::Pending(key));
            }
            if !admission.eof_accepted {
                let mut eof = admission
                    .eof
                    .take()
                    .ok_or(ComposerHostError::MutationMalformed)?;
                let result = self.advance_readiness_page(store, admission, &mut eof);
                let complete = eof.completed == 1;
                admission.eof = Some(eof);
                match result? {
                    AdmissionStep::More if complete => {
                        admission.eof = None;
                        admission.eof_accepted = true;
                    }
                    AdmissionStep::More => {}
                    AdmissionStep::Pending => {
                        return Ok(ComposerHostMutationEvidenceOutcome::Pending(key));
                    }
                    AdmissionStep::Unavailable(failure) => {
                        return Ok(ComposerHostMutationEvidenceOutcome::Unavailable {
                            key,
                            failure: Arc::new(failure),
                        });
                    }
                }
                continue;
            }
            if admission.prepared_begin.is_none() {
                match self.advance_readiness_assignment(store, admission)? {
                    AdmissionStep::More => {}
                    AdmissionStep::Pending => {
                        return Ok(ComposerHostMutationEvidenceOutcome::Pending(key));
                    }
                    AdmissionStep::Unavailable(failure) => {
                        return Ok(ComposerHostMutationEvidenceOutcome::Unavailable {
                            key,
                            failure: Arc::new(failure),
                        });
                    }
                }
                continue;
            }
            return self.admit_evidenced_mutation(store, admission);
        }
        if let Some(error) = admission.assignment_error.take() {
            return Ok(ComposerHostMutationEvidenceOutcome::Unavailable {
                key,
                failure: Arc::new(ComposerHostMutationAdmissionFailure::Assignment(error)),
            });
        }
        Ok(ComposerHostMutationEvidenceOutcome::Pending(key))
    }
}
