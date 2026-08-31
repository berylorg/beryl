use std::{error::Error, fmt};

use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::DomainRevision;

use crate::codec::DraftImageLabelProtectionHeadsFamily;
use crate::domain::SyndicDomain;
use crate::mutation::{point, required};
use crate::{DraftImageLabelProtectionHeadV1, SyndicMutationError, SyndicReadError, SyndicStorage};

use super::super::super::{
    DraftEditorCandidateSessionLifecycleV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftEditorCandidateSessionRecordKeyV1, DraftEditorCandidateSessionRecordV1,
    DraftEditorCandidateSessionsCodec, DraftEditorCandidateSessionsFamily,
    DraftMarkerOrderCommitmentsFamily, DraftMarkerOrderRecordKeyV1, DraftMarkerOrderRecordKindV1,
    DraftPieceBuildRootsV1, DraftPiecePrepareErrorV1, DraftPieceRootReferenceV1,
    DraftPieceRootsFamily, draft_piece_root_reference_is_locally_exact_v1, point_limit,
    validate_marker_order_root_record, validate_position,
};
use super::super::{
    DraftEditHistoryFrontierKeyV1, DraftEditHistoryFrontiersCodec, DraftEditHistoryFrontiersFamily,
    DraftEditHistoryRetentionErrorV1, DraftEditHistoryTransitionsCodec,
    DraftEditHistoryTransitionsFamily, append_historical_draft_edit_history_with_retention_v1,
    authenticate_draft_edit_history_frontier_v1,
};
use super::{codec::*, model::*};

#[derive(Clone)]
pub struct PreparedDraftHistoricalRootAdoptionV1 {
    request: DraftHistoricalRootAdoptionRequestV1,
    request_bytes: Vec<u8>,
    source_session: super::super::super::DraftEditorCandidateSessionV1,
    source_history: super::super::DraftEditHistoryFrontierV1,
    selected_transition: super::super::DraftEditHistoryTransitionV1,
    target_root: super::super::super::DraftPieceRootRecordV1,
}

#[derive(Clone)]
pub enum DraftHistoricalRootSelectionV1 {
    Prepared(PreparedDraftHistoricalRootAdoptionV1),
    Unavailable,
}

impl PreparedDraftHistoricalRootAdoptionV1 {
    pub(crate) const fn request(&self) -> DraftHistoricalRootAdoptionRequestV1 {
        self.request
    }
}

#[derive(Debug)]
pub enum DraftHistoricalRootAdoptionPrepareErrorV1 {
    Read(SyndicReadError),
    InvalidRequest,
    InvalidPosition(DraftPiecePrepareErrorV1),
}

impl fmt::Display for DraftHistoricalRootAdoptionPrepareErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "historical-root adoption read failed: {error}"),
            Self::InvalidRequest => {
                formatter.write_str("historical-root adoption request is invalid")
            }
            Self::InvalidPosition(error) => write!(
                formatter,
                "historical-root adoption position is invalid: {error}"
            ),
        }
    }
}

impl Error for DraftHistoricalRootAdoptionPrepareErrorV1 {}

impl From<SyndicReadError> for DraftHistoricalRootAdoptionPrepareErrorV1 {
    fn from(value: SyndicReadError) -> Self {
        Self::Read(value)
    }
}

#[derive(Clone)]
struct AdoptMutation {
    prepared: PreparedDraftHistoricalRootAdoptionV1,
}

#[derive(Clone, Copy)]
enum TerminalKind {
    Rejected,
    Cancelled,
    Error(DraftHistoricalRootAdoptionErrorReasonV1),
}

#[derive(Clone)]
struct TerminalMutation {
    prepared: PreparedDraftHistoricalRootAdoptionV1,
    kind: TerminalKind,
}

impl SyndicStorage {
    pub fn prepare_draft_historical_root_selection(
        &self,
        store: &HomeStore,
        intent: DraftHistoricalRootSelectionIntentV1,
    ) -> Result<DraftHistoricalRootSelectionV1, DraftHistoricalRootAdoptionPrepareErrorV1> {
        let activation = intent.activation();
        let source_session = match self.draft_editor_candidate_session(
            store,
            activation.draft_id(),
            activation.session_id(),
        )? {
            DraftEditorCandidateSessionReadOutcomeV1::Active(session)
                if super::super::super::DraftEditorCandidateActivationBindingV1::from_head(
                    &session,
                ) == activation
                    && session.active_operation().is_none() =>
            {
                session
            }
            _ => return Err(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest),
        };
        let source_history = self
            .point::<DraftEditHistoryFrontiersFamily>(
                store,
                activation.history().key(),
                point_limit(),
            )?
            .filter(|history| history.reference() == activation.history())
            .ok_or(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest)?;
        if source_session.newest_history() != source_history.reference()
            || source_session.newest_root() != source_history.reference().root()
            || !super::super::draft_edit_history_frontier_is_authenticated_v1(
                self,
                store,
                &source_history,
            )?
        {
            return Err(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest);
        }
        let selected_reference = match intent.direction() {
            DraftHistoricalRootDirectionV1::Undo => source_history.undo_head(),
            DraftHistoricalRootDirectionV1::Redo => source_history.redo_head(),
        };
        let Some(selected_reference) = selected_reference else {
            return Ok(DraftHistoricalRootSelectionV1::Unavailable);
        };
        let selected_transition = self
            .point::<DraftEditHistoryTransitionsFamily>(
                store,
                selected_reference.key(),
                point_limit(),
            )?
            .filter(|transition| transition.reference() == selected_reference)
            .ok_or(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest)?;
        let request = DraftHistoricalRootAdoptionRequestV1::new(
            activation.draft_id(),
            activation.session_id(),
            intent.operation_id(),
            source_history.reference(),
            selected_transition.reference(),
            intent.direction(),
            selected_transition.predecessor_root(),
            selected_transition.before_caret(),
            selected_transition.before_selection(),
        );
        self.prepare_draft_historical_root_adoption(store, request)
            .map(DraftHistoricalRootSelectionV1::Prepared)
    }

    pub(crate) fn prepare_draft_historical_root_adoption(
        &self,
        store: &HomeStore,
        request: DraftHistoricalRootAdoptionRequestV1,
    ) -> Result<PreparedDraftHistoricalRootAdoptionV1, DraftHistoricalRootAdoptionPrepareErrorV1>
    {
        let key = request.key();
        if request.source_history().key().draft_id() != key.draft_id()
            || request.source_history().key().session_id() != Some(key.session_id())
            || request.selected_transition().key().draft_id() != key.draft_id()
            || request.target_root().key().draft_id() != key.draft_id()
        {
            return Err(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest);
        }
        let source_session =
            match self.draft_editor_candidate_session(store, key.draft_id(), key.session_id())? {
                DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
                _ => return Err(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest),
            };
        let source_history = self
            .point::<DraftEditHistoryFrontiersFamily>(
                store,
                request.source_history().key(),
                point_limit(),
            )?
            .filter(|value| value.reference() == request.source_history())
            .ok_or(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest)?;
        let selected_transition = self
            .point::<DraftEditHistoryTransitionsFamily>(
                store,
                request.selected_transition().key(),
                point_limit(),
            )?
            .filter(|value| value.reference() == request.selected_transition())
            .ok_or(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest)?;
        let target_root = self
            .point::<DraftPieceRootsFamily>(store, request.target_root().key(), point_limit())?
            .filter(|value| value.reference() == request.target_root())
            .ok_or(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest)?;
        let protection = self
            .point::<DraftImageLabelProtectionHeadsFamily>(
                store,
                source_session.thread_id(),
                point_limit(),
            )?
            .ok_or(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest)?;
        if source_session.active_operation().is_some()
            || source_session.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
            || source_history.reference().root() != selected_transition.successor_root()
            || selected_transition.predecessor_root() != request.target_root()
            || selected_transition.before_caret() != request.caret()
            || selected_transition.before_selection() != request.selection()
            || match request.direction() {
                DraftHistoricalRootDirectionV1::Undo => source_history.undo_head(),
                DraftHistoricalRootDirectionV1::Redo => source_history.redo_head(),
            } != Some(request.selected_transition())
            || !super::super::draft_edit_history_frontier_is_authenticated_v1(
                self,
                store,
                &source_history,
            )?
            || !historical_marker_root_is_exact_in_store(self, store, target_root.reference())?
            || !historical_protection_contains_target(
                protection,
                source_session.thread_id(),
                target_root.reference(),
            )
        {
            return Err(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidRequest);
        }
        validate_position(self, store, request.target_root(), request.caret())
            .map_err(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidPosition)?;
        validate_position(self, store, request.target_root(), request.selection())
            .map_err(DraftHistoricalRootAdoptionPrepareErrorV1::InvalidPosition)?;
        Ok(PreparedDraftHistoricalRootAdoptionV1 {
            request,
            request_bytes: canonical_historical_root_adoption_request_bytes(request),
            source_session,
            source_history,
            selected_transition,
            target_root,
        })
    }

    pub fn adopt_draft_historical_root(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftHistoricalRootAdoptionV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, AdoptMutation { prepared })
    }

    pub fn reject_draft_historical_root_adoption(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftHistoricalRootAdoptionV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                prepared,
                kind: TerminalKind::Rejected,
            },
        )
    }

    pub fn cancel_draft_historical_root_adoption(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftHistoricalRootAdoptionV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                prepared,
                kind: TerminalKind::Cancelled,
            },
        )
    }

    pub fn error_draft_historical_root_adoption(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftHistoricalRootAdoptionV1,
        reason: DraftHistoricalRootAdoptionErrorReasonV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                prepared,
                kind: TerminalKind::Error(reason),
            },
        )
    }
}

impl DomainMutation<SyndicDomain> for AdoptMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedAdoptMutation>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if let Some(settlement) =
            point::<DraftHistoricalRootAdoptionsFamily>(reader, &self.prepared.request.key())?
        {
            if settlement.request_bytes() == self.prepared.request_bytes
                && settlement_is_exact(reader, &settlement)?
            {
                return Ok(None);
            }
            return Err(SyndicMutationError::IdentityCollision);
        }
        let current = current_session(reader, self.prepared.request.key())?;
        if current.active_operation().is_some()
            || (current == self.prepared.source_session
                && !prepared_closure_is_exact(reader, &self.prepared)?)
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let source_is_current = current == self.prepared.source_session
            && current.active_operation().is_none()
            && current.newest_history() == self.prepared.request.source_history()
            && current.newest_root() == self.prepared.source_history.reference().root();
        let mut outcome = DraftHistoricalRootAdoptionSettlementOutcomeV1::Conflict;
        let mut successor_transition = None;
        let mut successor_history = None;
        let mut successor_candidate = None;
        if source_is_current {
            if point::<super::super::super::DraftPieceSettlementsFamily>(
                reader,
                &super::super::super::DraftPieceSettlementKeyV1::new(
                    self.prepared.request.key().draft_id(),
                    self.prepared.request.key().session_id(),
                    self.prepared.request.key().operation_id(),
                ),
            )?
            .is_some()
            {
                outcome = DraftHistoricalRootAdoptionSettlementOutcomeV1::Error(
                    DraftHistoricalRootAdoptionErrorReasonV1::OccupiedIdentity,
                );
            } else {
                let append_history = if matches!(
                    self.prepared.source_history.reference().key(),
                    DraftEditHistoryFrontierKeyV1::Publication { .. }
                ) {
                    self.prepared
                        .source_history
                        .fork_session(current.session_id())
                        .ok_or(SyndicMutationError::IdentityCollision)?
                } else {
                    self.prepared.source_history.clone()
                };
                match append_historical_draft_edit_history_with_retention_v1(
                    reader,
                    &append_history,
                    self.prepared.request.selected_transition(),
                    self.prepared.request.direction().transition_kind(),
                    current
                        .newest_candidate_generation()
                        .checked_add(1)
                        .ok_or(SyndicMutationError::IdentityCollision)?,
                    self.prepared.request.target_root(),
                    self.prepared.request.caret(),
                    self.prepared.request.selection(),
                    self.prepared.request.key().operation_id(),
                ) {
                    Ok((transition, history)) => {
                        if point::<DraftEditHistoryTransitionsFamily>(reader, &transition.key())?
                            .is_some()
                        {
                            outcome = DraftHistoricalRootAdoptionSettlementOutcomeV1::Error(
                                DraftHistoricalRootAdoptionErrorReasonV1::OccupiedIdentity,
                            );
                        } else {
                            let candidate = current
                                .adopted_without_custody(
                                    self.prepared.request.target_root(),
                                    history.reference(),
                                )
                                .ok_or(SyndicMutationError::IdentityCollision)?;
                            outcome = DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed;
                            successor_transition = Some(transition);
                            successor_history = Some(history);
                            successor_candidate = Some(candidate);
                        }
                    }
                    Err(DraftEditHistoryRetentionErrorV1::CapacityUnavailable) => {
                        outcome = DraftHistoricalRootAdoptionSettlementOutcomeV1::Error(
                            DraftHistoricalRootAdoptionErrorReasonV1::HistoryCapacityUnavailable,
                        )
                    }
                    Err(DraftEditHistoryRetentionErrorV1::Invalid) => {
                        return Err(SyndicMutationError::IdentityCollision);
                    }
                }
            }
        }
        let settlement = DraftHistoricalRootAdoptionV1::new(
            self.prepared.request,
            self.prepared.request_bytes,
            Box::new(self.prepared.source_history),
            Box::new(self.prepared.selected_transition),
            Box::new(self.prepared.target_root),
            outcome,
            successor_transition.clone().map(Box::new),
            successor_history.clone().map(Box::new),
            successor_candidate.clone().map(Box::new),
        );
        Ok(Some(PreparedAdoptMutation {
            settlement,
            successor_transition,
            successor_history,
            successor_candidate,
        }))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftHistoricalRootAdoptionsCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryTransitionsCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            if let Some(transition) = prepared.successor_transition {
                mutations
                    .put::<DraftEditHistoryTransitionsCodec>(&transition.key(), &transition)?;
            }
            if let Some(history) = prepared.successor_history {
                mutations
                    .put::<DraftEditHistoryFrontiersCodec>(&history.reference().key(), &history)?;
            }
            if let Some(candidate) = prepared.successor_candidate {
                mutations.put::<DraftEditorCandidateSessionsCodec>(
                    &DraftEditorCandidateSessionRecordKeyV1::head(
                        candidate.draft_id(),
                        candidate.session_id(),
                    ),
                    &DraftEditorCandidateSessionRecordV1::Head(candidate),
                )?;
            }
            mutations.put::<DraftHistoricalRootAdoptionsCodec>(
                &prepared.settlement.key(),
                &prepared.settlement,
            )?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for TerminalMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<DraftHistoricalRootAdoptionV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if let Some(settlement) =
            point::<DraftHistoricalRootAdoptionsFamily>(reader, &self.prepared.request.key())?
        {
            if settlement.request_bytes() == self.prepared.request_bytes
                && settlement_is_exact(reader, &settlement)?
            {
                return Ok(None);
            }
            return Err(SyndicMutationError::IdentityCollision);
        }
        let current = current_session(reader, self.prepared.request.key())?;
        if current.active_operation().is_some()
            || (current == self.prepared.source_session
                && !prepared_closure_is_exact(reader, &self.prepared)?)
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let outcome = match self.kind {
            TerminalKind::Rejected => DraftHistoricalRootAdoptionSettlementOutcomeV1::Rejected,
            TerminalKind::Cancelled => DraftHistoricalRootAdoptionSettlementOutcomeV1::Cancelled,
            TerminalKind::Error(reason) => {
                DraftHistoricalRootAdoptionSettlementOutcomeV1::Error(reason)
            }
        };
        Ok(Some(DraftHistoricalRootAdoptionV1::new(
            self.prepared.request,
            self.prepared.request_bytes,
            Box::new(self.prepared.source_history),
            Box::new(self.prepared.selected_transition),
            Box::new(self.prepared.target_root),
            outcome,
            None,
            None,
            None,
        )))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftHistoricalRootAdoptionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(settlement) = prepared {
            mutations.put::<DraftHistoricalRootAdoptionsCodec>(&settlement.key(), &settlement)?;
        }
        Ok(())
    }
}

struct PreparedAdoptMutation {
    settlement: DraftHistoricalRootAdoptionV1,
    successor_transition: Option<super::super::DraftEditHistoryTransitionV1>,
    successor_history: Option<super::super::DraftEditHistoryFrontierV1>,
    successor_candidate: Option<super::super::super::DraftEditorCandidateSessionV1>,
}

fn prepared_closure_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    prepared: &PreparedDraftHistoricalRootAdoptionV1,
) -> Result<bool, SyndicMutationError> {
    let protection = required::<DraftImageLabelProtectionHeadsFamily>(
        reader,
        &prepared.source_session.thread_id(),
    )?;
    Ok(point::<DraftEditHistoryFrontiersFamily>(
        reader,
        &prepared.source_history.reference().key(),
    )?
    .as_ref()
        == Some(&prepared.source_history)
        && authenticate_draft_edit_history_frontier_v1(reader, &prepared.source_history).is_ok()
        && point::<DraftEditHistoryTransitionsFamily>(reader, &prepared.selected_transition.key())?
            .as_ref()
            == Some(&prepared.selected_transition)
        && point::<DraftPieceRootsFamily>(reader, &prepared.target_root.reference().key())?
            .as_ref()
            == Some(&prepared.target_root)
        && historical_marker_root_is_exact_in_reader(reader, prepared.target_root.reference())?
        && historical_protection_contains_target(
            protection,
            prepared.source_session.thread_id(),
            prepared.target_root.reference(),
        ))
}

pub(super) fn historical_marker_root_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
) -> Result<bool, SyndicReadError> {
    if !draft_piece_root_reference_is_locally_exact_v1(root) {
        return Ok(false);
    }
    let Some(root_id) = root.marker_order_root() else {
        return Ok(root.marker_commitment().marker_count() == 0);
    };
    let key = DraftMarkerOrderRecordKeyV1::new(
        root.key().draft_id(),
        DraftMarkerOrderRecordKindV1::Internal,
        root_id,
    );
    let record = storage.point::<DraftMarkerOrderCommitmentsFamily>(store, key, point_limit())?;
    Ok(record.is_some_and(|record| {
        record.key() == key
            && validate_marker_order_root_record(record, DraftPieceBuildRootsV1::from_root(root))
                .is_ok()
    }))
}

fn historical_marker_root_is_exact_in_reader(
    reader: &DomainReader<'_, SyndicDomain>,
    root: DraftPieceRootReferenceV1,
) -> Result<bool, SyndicMutationError> {
    if !draft_piece_root_reference_is_locally_exact_v1(root) {
        return Ok(false);
    }
    let Some(root_id) = root.marker_order_root() else {
        return Ok(root.marker_commitment().marker_count() == 0);
    };
    let key = DraftMarkerOrderRecordKeyV1::new(
        root.key().draft_id(),
        DraftMarkerOrderRecordKindV1::Internal,
        root_id,
    );
    let record = point::<DraftMarkerOrderCommitmentsFamily>(reader, &key)?;
    Ok(record.is_some_and(|record| {
        record.key() == key
            && validate_marker_order_root_record(record, DraftPieceBuildRootsV1::from_root(root))
                .is_ok()
    }))
}

fn historical_protection_contains_target(
    protection: DraftImageLabelProtectionHeadV1,
    thread_id: beryl_model::SyndicThreadId,
    root: DraftPieceRootReferenceV1,
) -> bool {
    protection.is_exact()
        && protection.thread_id() == thread_id
        && root
            .marker_commitment()
            .maximum_image_label()
            .is_none_or(|maximum| protection.protected_maximum().contains(maximum))
}

fn current_session(
    reader: &DomainReader<'_, SyndicDomain>,
    key: DraftHistoricalRootAdoptionKeyV1,
) -> Result<super::super::super::DraftEditorCandidateSessionV1, SyndicMutationError> {
    match required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::head(key.draft_id(), key.session_id()),
    )? {
        DraftEditorCandidateSessionRecordV1::Head(session)
            if session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active =>
        {
            Ok(session)
        }
        _ => Err(SyndicMutationError::IdentityCollision),
    }
}

fn settlement_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    settlement: &DraftHistoricalRootAdoptionV1,
) -> Result<bool, SyndicMutationError> {
    if !settlement.is_locally_valid()
        || authenticate_draft_edit_history_frontier_v1(reader, settlement.source_history()).is_err()
        || match settlement.request().direction() {
            DraftHistoricalRootDirectionV1::Undo => settlement.source_history().undo_head(),
            DraftHistoricalRootDirectionV1::Redo => settlement.source_history().redo_head(),
        } != Some(settlement.selected_transition().reference())
        || point::<DraftEditHistoryTransitionsFamily>(
            reader,
            &settlement.selected_transition().key(),
        )?
        .as_ref()
            != Some(settlement.selected_transition())
        || point::<DraftPieceRootsFamily>(reader, &settlement.target_root().reference().key())?
            .as_ref()
            != Some(settlement.target_root())
        || !historical_marker_root_is_exact_in_reader(reader, settlement.target_root().reference())?
    {
        return Ok(false);
    }
    if settlement.outcome() != DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed {
        return Ok(true);
    }
    let (Some(transition), Some(history), Some(_candidate)) = (
        settlement.successor_transition(),
        settlement.successor_history(),
        settlement.successor_candidate(),
    ) else {
        return Ok(false);
    };
    Ok(
        point::<DraftEditHistoryTransitionsFamily>(reader, &transition.key())?.as_ref()
            == Some(transition)
            && point::<DraftEditHistoryFrontiersFamily>(reader, &history.reference().key())?
                .as_ref()
                == Some(history)
            && authenticate_draft_edit_history_frontier_v1(reader, history).is_ok(),
    )
}

pub(crate) fn historical_candidate_session_is_exact(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &super::super::super::DraftEditorCandidateSessionV1,
    operation_id: super::super::super::DraftPieceOperationIdV1,
) -> Result<bool, SyndicMutationError> {
    let key =
        DraftHistoricalRootAdoptionKeyV1::new(head.draft_id(), head.session_id(), operation_id);
    let Some(settlement) = point::<DraftHistoricalRootAdoptionsFamily>(reader, &key)? else {
        return Ok(false);
    };
    Ok(
        settlement.outcome() == DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed
            && settlement.successor_candidate().is_some_and(|candidate| {
                super::super::super::session::adopted_head_matches_current(candidate, head)
            })
            && settlement_is_exact(reader, &settlement)?,
    )
}

pub(crate) fn historical_candidate_session_is_exact_in_store(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &super::super::super::DraftEditorCandidateSessionV1,
    operation_id: super::super::super::DraftPieceOperationIdV1,
) -> Result<bool, SyndicReadError> {
    let key =
        DraftHistoricalRootAdoptionKeyV1::new(head.draft_id(), head.session_id(), operation_id);
    let Some(settlement) =
        storage.point::<DraftHistoricalRootAdoptionsFamily>(store, key, point_limit())?
    else {
        return Ok(false);
    };
    let Some(transition) = settlement.successor_transition() else {
        return Ok(false);
    };
    let Some(history) = settlement.successor_history() else {
        return Ok(false);
    };
    Ok(
        settlement.outcome() == DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed
            && settlement.successor_candidate().is_some_and(|candidate| {
                super::super::super::session::adopted_head_matches_current(candidate, head)
            })
            && storage
                .point::<DraftEditHistoryTransitionsFamily>(store, transition.key(), point_limit())?
                .as_ref()
                == Some(transition)
            && storage
                .point::<DraftEditHistoryFrontiersFamily>(
                    store,
                    history.reference().key(),
                    point_limit(),
                )?
                .as_ref()
                == Some(history),
    )
}
