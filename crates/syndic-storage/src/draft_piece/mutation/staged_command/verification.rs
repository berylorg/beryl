use crate::codec::{DraftImageLabelProtectionHeadsFamily, Family};
use crate::draft_piece::staging::{
    draft_mutation_staging_head_is_locally_exact, draft_mutation_staging_receipt_is_locally_exact,
    staging_finish_receipt_is_exact,
};

use super::*;

mod history;
mod references;

pub(super) struct OutcomeReader<'a> {
    storage: &'a SyndicStorage,
    store: &'a HomeStore,
    work: StagedDraftPieceVerificationWorkV1,
}

impl<'a> OutcomeReader<'a> {
    pub(super) fn new(storage: &'a SyndicStorage, store: &'a HomeStore) -> Self {
        Self {
            storage,
            store,
            work: StagedDraftPieceVerificationWorkV1::default(),
        }
    }

    pub(super) fn work(&self) -> StagedDraftPieceVerificationWorkV1 {
        self.work
    }

    fn point<F: Family>(
        &mut self,
        key: F::Key,
    ) -> Result<Option<F::Value>, StagedDraftPieceOutcomeErrorV1> {
        let reads = self
            .work
            .attempted_reads
            .checked_add(1)
            .ok_or(StagedDraftPieceOutcomeErrorV1::VerificationLimit)?;
        let bytes = self
            .work
            .charged_encoded_value_bytes
            .checked_add(DRAFT_PIECE_PAGE_MAX_BYTES)
            .ok_or(StagedDraftPieceOutcomeErrorV1::VerificationLimit)?;
        if reads > STAGED_DRAFT_PIECE_OUTCOME_MAX_READS
            || bytes > STAGED_DRAFT_PIECE_OUTCOME_MAX_ENCODED_VALUE_BYTES
        {
            return Err(StagedDraftPieceOutcomeErrorV1::VerificationLimit);
        }
        self.work = StagedDraftPieceVerificationWorkV1 {
            attempted_reads: reads,
            charged_encoded_value_bytes: bytes,
        };
        self.storage
            .point::<F>(self.store, key, point_limit())
            .map_err(Into::into)
    }

    fn required<F: Family>(
        &mut self,
        key: F::Key,
        label: &'static str,
    ) -> Result<F::Value, StagedDraftPieceOutcomeErrorV1> {
        self.point::<F>(key)?
            .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(label))
    }
}

#[derive(PartialEq)]
struct Anchors {
    staging: Option<DraftMutationStagingHeadV1>,
    build: Option<DraftPieceBuildRecordV1>,
    session: Option<DraftEditorCandidateSessionRecordV1>,
    admission: Option<DraftMarkerAdmissionHeadV1>,
    capacity: Option<DraftMarkerAdmissionCapacityV1>,
    protection: Option<crate::DraftImageLabelProtectionHeadV1>,
    history: Option<DraftEditHistoryFrontierV1>,
}

impl Anchors {
    fn read(
        reader: &mut OutcomeReader<'_>,
        selected: &CapturedState,
    ) -> Result<Self, StagedDraftPieceOutcomeErrorV1> {
        let identity = selected.staging.identity();
        let (admission, capacity, protection) = match selected.admission() {
            Some(admission) => (
                reader.point::<DraftMarkerAdmissionHeadsFamily>(admission.binding().owner())?,
                reader.point::<DraftMarkerAdmissionCapacityFamily>(
                    DraftMarkerAdmissionCapacityKeyV1,
                )?,
                reader.point::<DraftImageLabelProtectionHeadsFamily>(
                    admission.binding().protection().thread_id(),
                )?,
            ),
            None => (None, None, None),
        };
        Ok(Self {
            staging: reader.point::<DraftMutationStagingHeadsFamily>(identity)?,
            build: reader.point::<DraftPieceBuildsFamily>(DraftPieceSettlementKeyV1::new(
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            ))?,
            session: reader.point::<DraftEditorCandidateSessionsFamily>(
                DraftEditorCandidateSessionRecordKeyV1::head(
                    identity.draft_id(),
                    identity.session_id(),
                ),
            )?,
            admission,
            capacity,
            protection,
            history: reader.point::<DraftEditHistoryFrontiersFamily>(
                selected.session.newest_history().key(),
            )?,
        })
    }

    fn matches(&self, selected: &CapturedState) -> bool {
        self.staging.as_ref() == Some(&selected.staging)
            && self.build.as_ref() == selected.build.as_ref()
            && self.session.as_ref()
                == Some(&DraftEditorCandidateSessionRecordV1::Head(
                    selected.session.clone(),
                ))
            && self.history.as_ref().is_some_and(|history| {
                history.reference() == selected.session.newest_history()
                    && history.is_locally_valid()
            })
    }
}

pub(super) fn verify(
    reader: &mut OutcomeReader<'_>,
    capture: &CapturedCommand,
    side: SelectedSide,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let selected = match side {
        SelectedSide::Source => &capture.source,
        SelectedSide::Target => &capture.target,
    };
    let before = Anchors::read(reader, selected)?;
    let verification = if before.matches(selected) {
        verify_references(reader, selected, &before)
    } else {
        Err(StagedDraftPieceOutcomeErrorV1::StaleEndpoint)
    };
    let after = Anchors::read(reader, selected)?;
    if before != after {
        return Err(StagedDraftPieceOutcomeErrorV1::ConcurrentChange);
    }
    verification
}

pub(super) fn verify_cleanup(
    reader: &mut OutcomeReader<'_>,
    selected: &CapturedState,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let before = Anchors::read(reader, selected)?;
    let verification = verify_cleanup_references(reader, selected, &before);
    let after = Anchors::read(reader, selected)?;
    if before != after {
        return Err(StagedDraftPieceOutcomeErrorV1::ConcurrentChange);
    }
    verification
}

fn verify_cleanup_references(
    reader: &mut OutcomeReader<'_>,
    selected: &CapturedState,
    anchors: &Anchors,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let settlement =
        selected
            .settlement
            .as_ref()
            .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                "cleanup settlement capture",
            ))?;
    let admission = selected
        .admission()
        .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
            "cleanup admission capture",
        ))?;
    let binding = admission.binding();
    let protection =
        anchors
            .protection
            .as_ref()
            .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                "cleanup protection",
            ))?;
    if anchors.staging.as_ref() != Some(&selected.staging)
        || anchors.build.as_ref() != selected.build.as_ref()
        || anchors.admission.is_some()
        || anchors.capacity.is_none()
        || !admission.is_empty()
        || binding.home_generation().get() != reader.storage.home_generation.get()
        || !protection.is_exact()
        || protection.thread_id() != binding.protection().thread_id()
        || protection.revision() < binding.protection().revision()
        || protection.protected_maximum() < binding.protection().protected_maximum()
        || binding
            .allocation_range()
            .is_some_and(|range| !protection.protected_maximum().contains(range.last()))
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "cleanup owner closure",
        ));
    }
    let stored =
        reader.required::<DraftPieceSettlementsFamily>(settlement.key(), "cleanup settlement")?;
    if &stored != settlement
        || !settlement_closure_is_exact(settlement)
        || !settlement_terminal_build_is_exact(settlement, selected.build.as_ref())
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "cleanup terminal closure",
        ));
    }
    verify_staging(reader, selected)?;
    references::build_endpoint(
        reader,
        selected
            .build
            .as_ref()
            .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                "cleanup terminal build",
            ))?,
    )?;
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "cleanup committed adoption",
        ));
    };
    references::combined_root(reader, adoption.adopted_root().reference())?;
    references::combined_root(reader, adoption.predecessor_history().reference().root())?;
    let transition = reader.required::<DraftEditHistoryTransitionsFamily>(
        adoption.transition().key(),
        "cleanup adopted transition",
    )?;
    if &transition != adoption.transition() {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "cleanup transition bytes",
        ));
    }
    Ok(())
}

fn verify_references(
    reader: &mut OutcomeReader<'_>,
    selected: &CapturedState,
    anchors: &Anchors,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    verify_staging(reader, selected)?;
    if let Some(build) = &selected.build {
        references::build_endpoint(reader, build)?;
        if selected.settlement.is_none()
            && selected.session.active_operation() != Some(&custody_for(build))
        {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "building session custody",
            ));
        }
    } else if !matches!(
        selected.staging.lifecycle(),
        DraftMutationStagingLifecycleV1::Finished(_)
    ) {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant("missing build"));
    }
    let history = anchors
        .history
        .as_ref()
        .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
            "selected history",
        ))?;
    references::combined_root(reader, selected.session.newest_root())?;
    if history.reference().root() != selected.session.newest_root() {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "selected root/history pair",
        ));
    }
    let identity = selected.staging.identity();
    let key = DraftPieceSettlementKeyV1::new(
        identity.draft_id(),
        identity.session_id(),
        identity.operation_id().as_piece_operation(),
    );
    let settlement = reader.point::<DraftPieceSettlementsFamily>(key)?;
    if settlement.as_ref() != selected.settlement.as_ref() {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "settlement selection",
        ));
    }
    if let Some(settlement) = &selected.settlement {
        if !settlement_closure_is_exact(settlement)
            || !settlement_terminal_build_is_exact(settlement, selected.build.as_ref())
        {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "terminal build closure",
            ));
        }
        if selected.session.active_operation().is_some() {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "terminal session custody",
            ));
        }
        history::terminal(reader, selected, settlement, history)?;
    }
    verify_writer(reader, selected, anchors)?;
    verify_absence(reader, selected)
}

fn verify_staging(
    reader: &mut OutcomeReader<'_>,
    selected: &CapturedState,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let head = &selected.staging;
    if !draft_mutation_staging_head_is_locally_exact(head) {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant("staging head"));
    }
    let key = DraftMutationStagingProgressReceiptKeyV1::new(
        head.identity(),
        head.receipt().transition_ordinal(),
    )
    .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
        "staging receipt key",
    ))?;
    let receipt = reader.required::<DraftMutationStagingProgressFamily>(key, "staging receipt")?;
    if !draft_mutation_staging_receipt_is_locally_exact(&receipt)
        || !staging_finish_receipt_is_exact(&receipt)
        || receipt.digest() != head.receipt().digest()
        || receipt.after_head_digest() != head.digest()
        || receipt.after_source() != head.source()
        || receipt.after_proposal() != head.proposal()
        || receipt.after_lifecycle() != head.lifecycle()
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "staging receipt closure",
        ));
    }
    let Some(build) = &selected.build else {
        return if receipt.command() == DraftMutationStagingCommandKindV1::Finish {
            Ok(())
        } else {
            Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "finished staging source",
            ))
        };
    };
    let continuation =
        build
            .durable_continuation()
            .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                "durable continuation",
            ))?;
    let finished = continuation.finished();
    let DraftMutationStagingLifecycleV1::Building(initial_endpoint) = head.lifecycle() else {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "staging transfer lifecycle",
        ));
    };
    if receipt.command() != DraftMutationStagingCommandKindV1::Transfer
        || receipt.build_endpoint() != Some(initial_endpoint)
        || receipt.prior() != Some(finished.receipt())
        || receipt.before_head_digest() != Some(finished.head_digest())
        || finished.identity() != head.identity()
        || finished.source() != head.source()
        || finished.proposal() != head.proposal()
        || initial_endpoint.key().transition_ordinal()
            > build.progress_receipt().key().transition_ordinal()
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "finished staging reference",
        ));
    }
    let key = DraftMutationStagingProgressReceiptKeyV1::new(
        head.identity(),
        finished.receipt().transition_ordinal(),
    )
    .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
        "finish receipt key",
    ))?;
    let prior = reader.required::<DraftMutationStagingProgressFamily>(key, "finish receipt")?;
    if !draft_mutation_staging_receipt_is_locally_exact(&prior)
        || !staging_finish_receipt_is_exact(&prior)
        || prior.digest() != finished.receipt().digest()
        || prior.after_head_digest() != finished.head_digest()
        || prior.after_lifecycle()
            != receipt
                .before_lifecycle()
                .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "transfer predecessor lifecycle",
                ))?
        || prior.after_source() != finished.source()
        || prior.after_proposal() != finished.proposal()
        || prior.command() != DraftMutationStagingCommandKindV1::Finish
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "finished staging receipt",
        ));
    }
    Ok(())
}

fn verify_writer(
    reader: &mut OutcomeReader<'_>,
    selected: &CapturedState,
    anchors: &Anchors,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let Some(admission) = selected.admission() else {
        return Ok(());
    };
    let binding = admission.binding();
    if binding.home_generation().get() != reader.storage.home_generation.get() {
        return Err(StagedDraftPieceOutcomeErrorV1::Unavailable);
    }
    let head = anchors
        .admission
        .as_ref()
        .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant("writer head"))?;
    let capacity = anchors
        .capacity
        .as_ref()
        .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant("writer capacity"))?;
    let protection =
        anchors
            .protection
            .as_ref()
            .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                "writer protection",
            ))?;
    if head.owner() != binding.owner()
        || head.home_generation() != binding.home_generation()
        || head.occurrence_commitment() != binding.occurrence_commitment()
        || capacity.charge().checked_sub(head.charge()).is_none()
        || !protection.is_exact()
        || protection.thread_id() != binding.protection().thread_id()
        || protection.revision() < binding.protection().revision()
        || protection.protected_maximum() < binding.protection().protected_maximum()
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant("writer binding"));
    }
    if let Some(settlement) = &selected.settlement {
        if matches!(
            settlement.outcome(),
            DraftPieceSettlementOutcomeV1::Committed { .. }
        ) {
            if !admission.is_empty()
                || head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Settled
                || head.target_root().count() != 0
                || head.unassigned_count() != 0
                || head.remaining_builder_count() != 0
                || head.charge().associations() != 0
                || head.cleanup_cursor().is_some()
                || binding
                    .allocation_range()
                    .is_some_and(|range| !protection.protected_maximum().contains(range.last()))
            {
                return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "settled writer closure",
                ));
            }
        } else {
            if !cleanup::captured_terminal_admission_is_exact(selected, admission) {
                return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "terminal writer capture",
                ));
            }
            let (expected_head, expected_receipt) = selected.terminal_admission.as_ref().ok_or(
                StagedDraftPieceOutcomeErrorV1::Invariant("terminal writer evidence"),
            )?;
            let command =
                staging_terminal_command(binding.owner(), settlement.terminal_receipt().digest());
            let receipt = reader.required::<DraftMarkerAdmissionReceiptsFamily>(
                DraftMarkerAdmissionReceiptKeyV1::new(binding.owner(), command),
                "terminal admission receipt",
            )?;
            if head != expected_head || &receipt != expected_receipt {
                return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "terminal writer receipt",
                ));
            }
        }
    } else {
        let lifecycle = if selected.build.is_some() {
            DraftMarkerAdmissionLifecycleV1::Building
        } else {
            DraftMarkerAdmissionLifecycleV1::Staging
        };
        if head.lifecycle() != lifecycle
            || head.target_root() != admission.target_root()
            || head.remaining_builder_count() != admission.remaining_count()
        {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "active writer closure",
            ));
        }
        references::admission_root(reader, binding.owner(), head.source_root())?;
        references::admission_root(reader, binding.owner(), head.target_root())?;
    }
    Ok(())
}

fn verify_absence(
    reader: &mut OutcomeReader<'_>,
    selected: &CapturedState,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let identity = selected.staging.identity();
    let (ordinal, fragment_count, staged_count) = match selected.build.as_ref() {
        Some(build) => (
            build
                .progress_receipt()
                .key()
                .transition_ordinal()
                .checked_add(1)
                .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "receipt ordinal overflow",
                ))?,
            build.fragment_count(),
            build.staged_fragment_count(),
        ),
        None => (1, 0, 0),
    };
    let next = DraftPieceBuildProgressReceiptKeyV1::new(
        identity.draft_id(),
        identity.session_id(),
        identity.operation_id().as_piece_operation(),
        ordinal,
    );
    if reader
        .point::<DraftPieceBuildProgressFamily>(next)?
        .is_some()
    {
        return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
            "receipt ahead of selected endpoint",
        ));
    }
    if selected.settlement.is_none() && staged_count < fragment_count {
        let next = DraftPieceBuildFragmentKeyV1::new(
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
            staged_count
                .checked_add(1)
                .ok_or(StagedDraftPieceOutcomeErrorV1::Invariant(
                    "fragment ordinal overflow",
                ))?,
        );
        if reader
            .point::<DraftPieceBuildFragmentsFamily>(next)?
            .is_some()
        {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "fragment ahead of selected endpoint",
            ));
        }
    }
    if selected.settlement.is_none() {
        let key = DraftPieceRootKeyV1::editor_candidate(
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        );
        if reader.point::<DraftPieceRootsFamily>(key)?.is_some() {
            return Err(StagedDraftPieceOutcomeErrorV1::Invariant(
                "candidate ahead of settlement",
            ));
        }
    }
    Ok(())
}
