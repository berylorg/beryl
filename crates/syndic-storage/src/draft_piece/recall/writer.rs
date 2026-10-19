use super::super::import::read::SourceReader;
use super::super::*;
use super::*;
use crate::{codec::*, domain::SyndicDomain};
use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

pub(super) struct Adopt {
    pub p: PreparedDraftComposerRootReplacementV1,
}
pub(super) struct AdoptWrites {
    settlement: DraftPieceSettlementV1,
    terminal: Option<(DraftPieceBuildRecordV1, DraftPieceBuildProgressReceiptV1)>,
}

impl DomainMutation<SyndicDomain> for Adopt {
    type Error = DraftComposerRecallErrorV1;
    type Prepared = Option<AdoptWrites>;
    fn prepare(self, r: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        let p = self.p;
        if let DraftComposerRootReplacementPurposeV1::AcceptedRecall(basis) = &p.purpose {
            authenticate_basis(r, basis)?;
        }
        if let Some(s) = SourceReader::point::<DraftPieceSettlementsFamily>(r, &p.key())? {
            if s.canonical_header() != p.canonical_header || !recall_settlement_is_exact(&s) {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            let DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) = s.closure() else {
                return Err(DraftComposerRecallErrorV1::Conflict);
            };
            if c.purpose != p.purpose
                || c.adoption.predecessor_session() != p.before.as_ref()
                || c.adoption.adopted_root().reference() != p.root
                || required::<DraftPieceRootsFamily>(r, &p.root.key())?
                    != *c.adoption.adopted_root()
                || required::<DraftEditHistoryTransitionsFamily>(r, &c.adoption.transition().key())?
                    != *c.adoption.transition()
            {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            if p.source.is_some() {
                let build = required::<DraftPieceBuildsFamily>(r, &p.key())?;
                if !settlement_terminal_build_is_exact(&s, Some(&build))
                    || !progress_receipt_matches_build(
                        &required::<DraftPieceBuildProgressFamily>(
                            r,
                            &build.progress_receipt().key(),
                        )?,
                        &build,
                    )
                {
                    return Err(DraftComposerRecallErrorV1::Conflict);
                }
            }
            return Ok(None);
        }
        if required::<DraftEditorCandidateSessionsFamily>(r, &command::head_key(&p.before))?
            != DraftEditorCandidateSessionRecordV1::Head(*p.before.clone())
            || required::<DraftEditHistoryFrontiersFamily>(r, &p.history.reference().key())?
                != *p.history
            || p.before.newest_history() != p.history.reference()
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        require_writer_destination(r, &p.before)?;
        let protection =
            required::<DraftImageLabelProtectionHeadsFamily>(r, &p.before.thread_id())?;
        if !protection.is_exact()
            || p.root
                .marker_commitment()
                .maximum_image_label()
                .is_some_and(|l| !protection.protected_maximum().contains(l))
        {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        if required::<DraftPieceRootsFamily>(r, &p.root.key())?.reference() != p.root {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        let append = if matches!(
            p.history.reference().key(),
            DraftEditHistoryFrontierKeyV1::Session { .. }
        ) {
            *p.history.clone()
        } else {
            let fork = p
                .history
                .fork_session(p.before.session_id())
                .ok_or(DraftComposerRecallErrorV1::Conflict)?;
            if p.history
                .reference()
                .key()
                .sealed_import_operation_id()
                .is_some()
                && !checkpoint::imported_baseline_fork_is_exact_in_transaction(
                    r, &p.history, &fork,
                )?
            {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            fork
        };
        let (transition, history) = history::append_ordinary_draft_edit_history_with_retention_v1(
            r,
            &append,
            p.before
                .newest_candidate_generation()
                .checked_add(1)
                .ok_or(DraftComposerRecallErrorV1::Conflict)?,
            p.root,
            p.header.predecessor_caret(),
            p.header.predecessor_selection(),
            p.header.caret(),
            p.header.selection(),
            p.header.operation_id(),
        )
        .map_err(|e| match e {
            history::DraftEditHistoryRetentionErrorV1::CapacityUnavailable => {
                DraftComposerRecallErrorV1::CapacityUnavailable
            }
            _ => DraftComposerRecallErrorV1::Conflict,
        })?;
        if SourceReader::point::<DraftEditHistoryTransitionsFamily>(r, &transition.key())?.is_some()
        {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        let after = if p.source.is_some() {
            p.before.adopted(p.root, history.reference())
        } else {
            p.before
                .adopted_without_custody(p.root, history.reference())
        }
        .ok_or(DraftComposerRecallErrorV1::Conflict)?;
        let (terminal, receipt_ref) = if let Some(source) = &p.source {
            if required::<DraftPieceBuildsFamily>(r, &p.key())? != **source
                || p.before.active_operation() != Some(&command::custody(source))
                || !recall_build_is_exact(source)
                || source.successor() != Some(p.root)
            {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            let prior =
                required::<DraftPieceBuildProgressFamily>(r, &source.progress_receipt().key())?;
            mutation::authenticate_progress_receipt(r, &prior)?;
            let pair = terminal_transition(source, DraftPieceBuildLifecycleV1::Committed)?;
            let receipt = pair.1.reference();
            (Some(pair), Some(receipt))
        } else {
            if SourceReader::point::<DraftPieceBuildsFamily>(r, &p.key())?.is_some() {
                return Err(DraftComposerRecallErrorV1::Conflict);
            }
            (None, None)
        };
        let key = p.key();
        let adoption = DraftPieceCommittedAdoptionV1::new(
            *p.before,
            after.clone(),
            DraftPieceRootRecordV1::new(p.root),
            *p.history,
            transition,
            history,
        );
        let outcome = DraftPieceSettlementOutcomeV1::Committed {
            candidate_generation: after.newest_candidate_generation(),
            successor: p.root,
            history: after.newest_history(),
            caret: p.header.caret(),
            selection: p.header.selection(),
        };
        let settlement = DraftPieceSettlementV1::new_boxed(
            key,
            canonical_proposal_digest(&p.canonical_header),
            p.header.predecessor_candidate_generation(),
            p.header.predecessor_root(),
            p.header.predecessor_history(),
            0,
            canonical_empty_draft_piece_fragment_chain_v1(),
            p.header.predecessor_caret(),
            p.header.predecessor_selection(),
            p.header.caret(),
            p.header.selection(),
            p.source.as_ref().and_then(|b| b.build_digest()),
            p.canonical_header,
            p.source.map(|b| *b),
            receipt_ref,
            outcome,
            Box::new(DraftPieceSettlementClosureV1::OrdinaryRootReplacement(
                DraftComposerRootReplacementV1 {
                    purpose: p.purpose,
                    adoption,
                },
            )),
        );
        if !recall_settlement_is_exact(&settlement) {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(Some(AdoptWrites {
            settlement,
            terminal,
        }))
    }
    fn reserve_reconciliation(
        &self,
        r: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        r.reserve_records::<DraftPieceSettlementsCodec>(1)?;
        r.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        r.reserve_records::<DraftEditHistoryTransitionsCodec>(1)?;
        r.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        if self.p.source.is_some() {
            r.reserve_records::<DraftPieceBuildsCodec>(1)?;
            r.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        }
        Ok(())
    }
    fn contribute(
        p: Self::Prepared,
        w: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(p) = p {
            let DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) = p.settlement.closure()
            else {
                return Err(DraftComposerRecallErrorV1::Conflict);
            };
            let a = &c.adoption;
            w.put::<DraftEditHistoryTransitionsCodec>(&a.transition().key(), a.transition())?;
            w.put::<DraftEditHistoryFrontiersCodec>(
                &a.adopted_history().reference().key(),
                a.adopted_history(),
            )?;
            if let Some((build, receipt)) = &p.terminal {
                command::put_transition(w, build, receipt, a.adopted_session())?;
            } else {
                w.put::<DraftEditorCandidateSessionsCodec>(
                    &command::head_key(a.adopted_session()),
                    &DraftEditorCandidateSessionRecordV1::Head(a.adopted_session().clone()),
                )?;
            }
            w.put::<DraftPieceSettlementsCodec>(&p.settlement.key(), &p.settlement)?;
        }
        Ok(())
    }
}

pub(crate) fn terminal_transition(
    source: &DraftPieceBuildRecordV1,
    lifecycle: DraftPieceBuildLifecycleV1,
) -> Result<(DraftPieceBuildRecordV1, DraftPieceBuildProgressReceiptV1), DraftComposerRecallErrorV1>
{
    let target = DraftPieceBuildRecordV1::new(
        source.draft_id(),
        source.session_id(),
        source.predecessor_candidate_generation(),
        source.predecessor_root(),
        source.predecessor_history(),
        source.operation_id(),
        source.predecessor_caret(),
        source.predecessor_selection(),
        source.caret(),
        source.selection(),
        0,
        source.fragment_chain(),
        source.canonical_header().to_vec(),
        0,
        source.staged_fragment_chain(),
        source.proposal_digest(),
        source.working_roots(),
        source.base_frontier(),
        source.successor_frontier(),
        source.next_record_ordinal(),
        source.frontier(),
        source.progress_receipt(),
        source.successor(),
        source.build_digest(),
        lifecycle,
    );
    authenticated_build_transition(target, Some(source.progress_receipt()), None)
        .map_err(|_| DraftComposerRecallErrorV1::Conflict)
}
