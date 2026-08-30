use std::collections::BTreeSet;

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainReader, PointReadLimit, ProofCorrelationBytes,
    ProofDomain, ProofProtocolIdentity,
};
use beryl_model::SyndicDraftId;

use crate::{
    ImageLabelOriginOwner, ImageLabelOriginSpanRecord, SyndicStorage,
    codec::{
        AcceptedInputsCodec, AcceptedInputsFamily, CanonicalItemsCodec, CanonicalItemsFamily,
        ImageLabelAuthorityHeadsCodec, ImageLabelOriginSpanKey, ImageLabelOriginSpansCodec,
        ThreadsCodec, TurnsCodec, TurnsFamily,
    },
    domain::SyndicDomain,
    draft_piece::{
        DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionLifecycleV1,
        DraftEditorCandidateSessionRecordKeyV1, DraftEditorCandidateSessionRecordV1,
        DraftEditorCandidateSessionsCodec, DraftEditorCandidateSessionsFamily,
        DraftMarkerIdentityOccurrenceV1, DraftPieceSettlementOutcomeV1, DraftPieceSettlementsCodec,
        DraftPieceSettlementsFamily, marker_identity_lookup, marker_identity_lookup_on_snapshot,
        point_limit, settlement_closure_is_exact,
    },
};

use super::model::{
    DraftMarkerReadinessAcceptedSourceV1, DraftMarkerReadinessSourceErrorV1,
    DraftMarkerReadinessSourceSelectorV1, PAGE_MAX_ASSOCIATIONS, PAGE_MAX_EVIDENCE_BYTES,
    PageProtocol, SourceInput, page_correlation,
};

pub enum NoWitness {}

impl ProofDomain for SyndicDomain {
    type SourceInput = SourceInput;
    type WitnessInput = NoWitness;
    type Error = DraftMarkerReadinessSourceErrorV1;

    fn source_protocol(_: &Self::SourceInput) -> ProofProtocolIdentity {
        ProofProtocolIdentity::of::<PageProtocol>()
    }

    fn expected_source_correlation(input: &Self::SourceInput) -> ProofCorrelationBytes {
        ProofCorrelationBytes::new(input.page.expected)
    }

    fn witness_protocol(input: &Self::WitnessInput) -> ProofProtocolIdentity {
        match *input {}
    }

    fn prove_source(
        input: &Self::SourceInput,
        reader: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        let page = &input.page;
        validate_input_shape(page)?;
        let _page = page.page;
        let destination = session_snapshot(reader, page.owner.draft_id(), page.owner.session_id())?;
        for entry in page.entries.iter() {
            match entry.selector {
                DraftMarkerReadinessSourceSelectorV1::Accepted(source) => {
                    if entry.label != source.label || entry.asset_id != source.asset_id {
                        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
                    }
                    let origin = entry
                        .accepted_origin
                        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
                    validate_accepted_snapshot(reader, source, origin)?;
                }
                _ => {
                    if entry.accepted_origin.is_some() {
                        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
                    }
                    let (thread, occurrence) = resolve_snapshot(reader, entry.selector)?;
                    if thread != destination.thread_id()
                        || occurrence.label() != entry.label
                        || occurrence.asset_id() != entry.asset_id
                    {
                        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
                    }
                }
            }
        }
        Ok(ProofCorrelationBytes::new(page_correlation(
            page.ordinal,
            page.eof,
            &page.entries,
        )))
    }

    fn prove_witness(
        input: &Self::WitnessInput,
        _: &DomainReader<'_, Self>,
    ) -> Result<ProofCorrelationBytes, Self::Error> {
        match *input {}
    }
}

impl DomainCallbackError for DraftMarkerReadinessSourceErrorV1 {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

fn validate_input_shape(
    page: &super::model::SealedDraftMarkerReadinessSourcePageV1,
) -> Result<(), DraftMarkerReadinessSourceErrorV1> {
    if page.entries.len() > PAGE_MAX_ASSOCIATIONS || page.entries.is_empty() {
        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
    }
    let bytes = page
        .entries
        .iter()
        .try_fold(0_usize, |total, entry| {
            total.checked_add(entry.evidence_bytes().len())
        })
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    let mut targets = BTreeSet::new();
    if bytes > PAGE_MAX_EVIDENCE_BYTES
        || page
            .entries
            .iter()
            .any(|entry| !targets.insert(entry.target_marker_id))
        || page.entries.windows(2).any(|entries| {
            entries[0].label > entries[1].label
                || (entries[0].label == entries[1].label
                    && entries[0].evidence_bytes() > entries[1].evidence_bytes())
        })
        || page
            .entries
            .windows(2)
            .any(|entries| entries[0].selector_tag() != entries[1].selector_tag())
    {
        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
    }
    Ok(())
}

pub(super) fn resolve_accepted_preflight(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    source: DraftMarkerReadinessAcceptedSourceV1,
) -> Result<ImageLabelOriginSpanRecord, DraftMarkerReadinessSourceErrorV1> {
    let resolved = storage
        .resolve_image_label_origin_span(store, source.thread_id, source.label, point_limit())
        .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    let span = *resolved.span();
    if span.asset_reference_set() != source.asset_reference_set || !span.contains(source.label) {
        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
    }
    validate_accepted_owner_preflight(storage, store, span)?;
    Ok(span)
}

fn validate_accepted_owner_preflight(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    span: ImageLabelOriginSpanRecord,
) -> Result<(), DraftMarkerReadinessSourceErrorV1> {
    let (thread, proof) = match span.admitted_owner() {
        ImageLabelOriginOwner::AcceptedInput(id) => {
            let input = storage
                .point::<AcceptedInputsFamily>(store, id, point_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            (input.thread_id(), input.asset_reference_set())
        }
        ImageLabelOriginOwner::CanonicalItem(id) => {
            let item = storage
                .point::<CanonicalItemsFamily>(store, id, point_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            let turn = storage
                .point::<TurnsFamily>(store, item.turn_id(), point_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            (
                turn.origin_thread_id(),
                item.presentation().asset_reference_set(),
            )
        }
    };
    accepted_owner_is_exact(span, thread, proof)
}

fn validate_accepted_snapshot(
    reader: &DomainReader<'_, SyndicDomain>,
    source: DraftMarkerReadinessAcceptedSourceV1,
    expected_span: ImageLabelOriginSpanRecord,
) -> Result<(), DraftMarkerReadinessSourceErrorV1> {
    let source_thread = reader
        .point::<ThreadsCodec>(&source.thread_id, source_limit())
        .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    let source_head = reader
        .point::<ImageLabelAuthorityHeadsCodec>(&source.thread_id, source_limit())
        .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    if source_thread.id() != source.thread_id
        || !source_head.is_exact()
        || source_head.thread_id() != source.thread_id
        || !source_head.permanent().contains(source.label)
        || (expected_span.thread_id() != source.thread_id
            && !source_head.inherited().contains(source.label))
    {
        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
    }
    let origin_head = reader
        .point::<ImageLabelAuthorityHeadsCodec>(&expected_span.thread_id(), source_limit())
        .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    let span = reader
        .point::<ImageLabelOriginSpansCodec>(
            &ImageLabelOriginSpanKey {
                thread: expected_span.thread_id(),
                end_label: expected_span.end_label(),
            },
            source_limit(),
        )
        .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    if span != expected_span
        || span.asset_reference_set() != source.asset_reference_set
        || !span.contains(source.label)
        || !origin_head.is_exact()
        || origin_head.thread_id() != span.thread_id()
        || !origin_head.permanent().contains(source.label)
        || origin_head.inherited().contains(source.label)
    {
        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
    }
    let (thread, proof) = match span.admitted_owner() {
        ImageLabelOriginOwner::AcceptedInput(id) => {
            let input = reader
                .point::<AcceptedInputsCodec>(&id, source_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            (input.thread_id(), input.asset_reference_set())
        }
        ImageLabelOriginOwner::CanonicalItem(id) => {
            let item = reader
                .point::<CanonicalItemsCodec>(&id, source_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            let turn = reader
                .point::<TurnsCodec>(&item.turn_id(), source_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            (
                turn.origin_thread_id(),
                item.presentation().asset_reference_set(),
            )
        }
    };
    accepted_owner_is_exact(span, thread, proof)
}

fn accepted_owner_is_exact(
    span: ImageLabelOriginSpanRecord,
    owner_thread: beryl_model::SyndicThreadId,
    proof: Option<beryl_model::SealedAssetReferenceSetProof>,
) -> Result<(), DraftMarkerReadinessSourceErrorV1> {
    if owner_thread != span.thread_id()
        || proof != Some(span.asset_reference_set())
        || span
            .asset_reference_set()
            .sequential()
            .maximum_image_label()
            != Some(span.end_label())
    {
        return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
    }
    Ok(())
}

fn source_limit() -> PointReadLimit {
    PointReadLimit::new(point_limit().max_bytes()).expect("draft-piece point limit is nonzero")
}

pub(super) fn session_preflight(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> Result<crate::draft_piece::DraftEditorCandidateSessionV1, DraftMarkerReadinessSourceErrorV1> {
    let record = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id),
            point_limit(),
        )
        .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    match record {
        DraftEditorCandidateSessionRecordV1::Head(session)
            if session.draft_id() == draft_id
                && session.session_id() == session_id
                && session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active =>
        {
            Ok(session)
        }
        _ => Err(DraftMarkerReadinessSourceErrorV1::Rejected),
    }
}

fn session_snapshot(
    reader: &DomainReader<'_, SyndicDomain>,
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> Result<crate::draft_piece::DraftEditorCandidateSessionV1, DraftMarkerReadinessSourceErrorV1> {
    let record = reader
        .point::<DraftEditorCandidateSessionsCodec>(
            &DraftEditorCandidateSessionRecordKeyV1::head(draft_id, session_id),
            source_limit(),
        )
        .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
        .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
    match record {
        DraftEditorCandidateSessionRecordV1::Head(session)
            if session.draft_id() == draft_id
                && session.session_id() == session_id
                && session.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active =>
        {
            Ok(session)
        }
        _ => Err(DraftMarkerReadinessSourceErrorV1::Rejected),
    }
}

pub(super) fn resolve_preflight(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    selector: DraftMarkerReadinessSourceSelectorV1,
) -> Result<
    (beryl_model::SyndicThreadId, DraftMarkerIdentityOccurrenceV1),
    DraftMarkerReadinessSourceErrorV1,
> {
    match selector {
        DraftMarkerReadinessSourceSelectorV1::Candidate(source) => {
            let session = session_preflight(storage, store, source.draft_id, source.session_id)?;
            if session.newest_candidate_generation() != source.candidate_generation
                || session.newest_root() != source.root
            {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            let occurrence = marker_identity_lookup(storage, store, source.root, source.marker_id)
                .map_err(|_| DraftMarkerReadinessSourceErrorV1::Rejected)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            Ok((session.thread_id(), occurrence))
        }
        DraftMarkerReadinessSourceSelectorV1::Cut(source) => {
            let session = session_preflight(
                storage,
                store,
                source.settlement.draft_id(),
                source.settlement.session_id(),
            )?;
            if session.newest_candidate_generation() != source.successor_generation
                || session.newest_root() != source.successor_root
            {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            let settlement = storage
                .point::<DraftPieceSettlementsFamily>(store, source.settlement, point_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::PreflightRead)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            if !cut_settlement_is_exact(source, &settlement) {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            let occurrence = marker_identity_lookup(
                storage,
                store,
                settlement.predecessor_root(),
                source.marker_id,
            )
            .map_err(|_| DraftMarkerReadinessSourceErrorV1::Rejected)?
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            if marker_identity_lookup(storage, store, source.successor_root, source.marker_id)
                .map_err(|_| DraftMarkerReadinessSourceErrorV1::Rejected)?
                .is_some()
            {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            Ok((session.thread_id(), occurrence))
        }
        DraftMarkerReadinessSourceSelectorV1::Accepted(_) => {
            Err(DraftMarkerReadinessSourceErrorV1::Rejected)
        }
    }
}

fn resolve_snapshot(
    reader: &DomainReader<'_, SyndicDomain>,
    selector: DraftMarkerReadinessSourceSelectorV1,
) -> Result<
    (beryl_model::SyndicThreadId, DraftMarkerIdentityOccurrenceV1),
    DraftMarkerReadinessSourceErrorV1,
> {
    match selector {
        DraftMarkerReadinessSourceSelectorV1::Candidate(source) => {
            let session = session_snapshot(reader, source.draft_id, source.session_id)?;
            if session.newest_candidate_generation() != source.candidate_generation
                || session.newest_root() != source.root
            {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            let occurrence =
                marker_identity_lookup_on_snapshot(reader, source.root, source.marker_id)
                    .map_err(snapshot_error)?
                    .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            Ok((session.thread_id(), occurrence))
        }
        DraftMarkerReadinessSourceSelectorV1::Cut(source) => {
            let session = session_snapshot(
                reader,
                source.settlement.draft_id(),
                source.settlement.session_id(),
            )?;
            if session.newest_candidate_generation() != source.successor_generation
                || session.newest_root() != source.successor_root
            {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            let settlement = reader
                .point::<DraftPieceSettlementsCodec>(&source.settlement, source_limit())
                .map_err(DraftMarkerReadinessSourceErrorV1::Read)?
                .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            if !cut_settlement_is_exact(source, &settlement) {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            let occurrence = marker_identity_lookup_on_snapshot(
                reader,
                settlement.predecessor_root(),
                source.marker_id,
            )
            .map_err(snapshot_error)?
            .ok_or(DraftMarkerReadinessSourceErrorV1::Rejected)?;
            if marker_identity_lookup_on_snapshot(reader, source.successor_root, source.marker_id)
                .map_err(snapshot_error)?
                .is_some()
            {
                return Err(DraftMarkerReadinessSourceErrorV1::Rejected);
            }
            Ok((session.thread_id(), occurrence))
        }
        DraftMarkerReadinessSourceSelectorV1::Accepted(_) => {
            Err(DraftMarkerReadinessSourceErrorV1::Rejected)
        }
    }
}

fn cut_settlement_is_exact(
    source: super::model::DraftMarkerReadinessCutSourceV1,
    settlement: &crate::draft_piece::DraftPieceSettlementV1,
) -> bool {
    settlement.key() == source.settlement
        && settlement_closure_is_exact(settlement)
        && matches!(
            settlement.outcome(),
            DraftPieceSettlementOutcomeV1::Committed {
                candidate_generation,
                successor,
                ..
            } if *candidate_generation == source.successor_generation
                && *successor == source.successor_root
        )
}

fn snapshot_error(
    error: crate::draft_piece::SnapshotMarkerLookupErrorV1,
) -> DraftMarkerReadinessSourceErrorV1 {
    match error {
        crate::draft_piece::SnapshotMarkerLookupErrorV1::Read(error) => {
            DraftMarkerReadinessSourceErrorV1::Read(error)
        }
        crate::draft_piece::SnapshotMarkerLookupErrorV1::Rejected => {
            DraftMarkerReadinessSourceErrorV1::Rejected
        }
    }
}
