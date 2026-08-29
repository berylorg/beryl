use super::*;

mod descendant;

use descendant::*;

impl PromotionObservation {
    pub(super) fn is_exact(
        &self,
        promotion: &PromoteAcceptedInput,
    ) -> Result<bool, SyndicReadError> {
        let basis = promotion.candidate().basis();
        let Some(successor_digest) = self.successor_digest(promotion)? else {
            return Ok(false);
        };
        Ok(self.input.as_ref() == Some(basis.input())
            && self.order.as_ref() == Some(basis.order())
            && self.source_binding.as_ref() == Some(basis.binding())
            && self.raw_turn_draft.is_none()
            && self.raw_turn_accepted.is_none()
            && self.exact_route_agrees(promotion)?
            && self.exact_projection_agrees(promotion, successor_digest)?)
    }

    fn successor_digest(
        &self,
        promotion: &PromoteAcceptedInput,
    ) -> Result<Option<beryl_model::SyndicPathDigest>, SyndicReadError> {
        let basis = promotion.candidate().basis();
        if !self.source_parent_agrees(basis) {
            return Ok(None);
        }
        let Some(parent) = self.parent_turn.as_ref() else {
            return Ok(None);
        };
        let expected_depth = parent.depth().checked_next().map_err(|_| {
            SyndicReadError::Invariant("accepted-input promotion turn depth is exhausted")
        })?;
        let expected_digest = crate::child_turn_chain_digest(
            promotion.successor_turn_id(),
            parent.id(),
            parent.chain_digest(),
        );
        let Some(expected_ancestor_skip) = self.successor_ancestor_skip else {
            return Ok(None);
        };
        let Some(turn) = self.turn.as_ref() else {
            return Ok(None);
        };
        if turn.id() != promotion.successor_turn_id()
            || turn.origin_thread_id() != basis.thread().id()
            || turn.kind() != TurnKind::OrdinaryUser
            || turn.parent() != ConversationParent::Turn(parent.id())
            || turn.ancestor_skip() != Some(expected_ancestor_skip)
            || turn.depth() != expected_depth
            || turn.chain_digest() != expected_digest
            || turn.submitted_at() != promotion.promoted_at()
        {
            return Ok(None);
        }
        let expected_state = TurnStateRecord::with_capture_frontiers(
            turn.id(),
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            1,
            0,
            1,
            0,
            None,
            promotion.promoted_at(),
        )
        .map_err(|_| {
            SyndicReadError::Invariant("accepted-input promotion turn state cannot be constructed")
        })?;
        let item_revision = ProjectionRevision::new(1).map_err(|_| {
            SyndicReadError::Invariant("accepted-input promotion item revision is invalid")
        })?;
        let expected_item = CanonicalItemRecord::local_user_input(
            promotion.successor_item_id(),
            turn.id(),
            TurnItemOrdinal::FIRST,
            item_revision,
            basis.input().content(),
            basis.input().asset_reference_set(),
        );
        let expected_item_index = TurnItemIndexRecord::new(
            turn.id(),
            TurnItemOrdinal::FIRST,
            expected_item.id(),
            item_revision,
        );
        let expected_child =
            TurnChildIndexRecord::new(parent.id(), turn.id(), expected_depth, expected_digest);
        if self.turn_state.as_ref() != Some(&expected_state)
            || self.item.as_ref() != Some(&expected_item)
            || self.item_index.as_ref() != Some(&expected_item_index)
            || self.child_index.as_ref() != Some(&expected_child)
        {
            return Ok(None);
        }
        Ok(Some(expected_digest))
    }
}

impl PromotionObservation {
    fn exact_projection_agrees(
        &self,
        promotion: &PromoteAcceptedInput,
        successor_digest: beryl_model::SyndicPathDigest,
    ) -> Result<bool, SyndicReadError> {
        let basis = promotion.candidate().basis();
        let thread_revision = basis.thread().revision().checked_next().map_err(|_| {
            SyndicReadError::Invariant("accepted-input promotion thread revision is exhausted")
        })?;
        let selected_path = SelectedPathProof::new(
            Some(promotion.successor_turn_id()),
            thread_revision,
            successor_digest,
        );
        let expected_thread = ThreadRecord::new(
            basis.thread().id(),
            selected_path,
            basis.thread().current_draft_id(),
            basis.thread().lineage(),
            basis.thread().context_owner_id(),
        );
        let expected_draft_index = DraftByThreadRecord::new(
            expected_thread.id(),
            basis.draft_by_thread().draft_id(),
            basis.draft_by_thread().draft_revision(),
            thread_revision,
        );
        let expected_transcript = TranscriptViewHeadRecord::new(
            expected_thread.id(),
            basis
                .transcript_head()
                .generation()
                .checked_next()
                .map_err(|_| {
                    SyndicReadError::Invariant(
                        "accepted-input promotion transcript generation is exhausted",
                    )
                })?,
            basis
                .transcript_head()
                .revision()
                .checked_next()
                .map_err(|_| {
                    SyndicReadError::Invariant(
                        "accepted-input promotion transcript revision is exhausted",
                    )
                })?,
            0,
            Some(promotion.successor_turn_id()),
            successor_digest,
            ProjectionLifecycle::Stale,
        );
        let expected_summary = HistorySummaryRecord::new(
            expected_thread.id(),
            basis.summary().revision().checked_next().map_err(|_| {
                SyndicReadError::Invariant(
                    "accepted-input promotion history-summary revision is exhausted",
                )
            })?,
            thread_revision,
            Some(promotion.successor_turn_id()),
            successor_digest,
            false,
            promotion.promoted_at(),
        );
        let logical_bytes = basis.input().content().summary().logical_utf8_bytes();
        let expected_gate = InputGateRecord::new(
            expected_thread.id(),
            basis.gate().revision().checked_next().map_err(|_| {
                SyndicReadError::Invariant("accepted-input promotion gate revision is exhausted")
            })?,
            InputGateState::PendingTurn(promotion.successor_turn_id()),
            basis.gate().accepted_high_water(),
            basis.gate().route_generation_high_water(),
            None,
            basis.gate().live_steering_count(),
            basis.gate().live_next_turn_count().checked_sub(1).ok_or(
                SyndicReadError::Invariant("accepted-input promotion gate count underflowed"),
            )?,
            basis
                .gate()
                .live_logical_utf8_bytes()
                .checked_sub(logical_bytes)
                .ok_or(SyndicReadError::Invariant(
                    "accepted-input promotion gate bytes underflowed",
                ))?,
        )
        .map_err(|_| {
            SyndicReadError::Invariant(
                "accepted-input promotion gate successor cannot be constructed",
            )
        })?;
        let work_period = expected_activity_work_period(basis)?;
        let source = ActivityQuerySource::new(expected_thread.id(), promotion.successor_turn_id());
        let expected_activity_head = ActivityQueryHeadRecord::new(
            expected_thread.id(),
            work_period,
            Some(source),
            true,
            0,
            basis
                .activity_head()
                .revision()
                .checked_next()
                .map_err(|_| {
                    SyndicReadError::Invariant(
                        "accepted-input promotion activity revision is exhausted",
                    )
                })?,
            1,
            0,
            0,
            0,
            0,
            None,
            ProjectionLifecycle::Current,
        )
        .map_err(|_| {
            SyndicReadError::Invariant(
                "accepted-input promotion activity successor cannot be constructed",
            )
        })?;
        let expected_activity_source = ActivityQuerySourceRecord::new(
            expected_thread.id(),
            work_period,
            source,
            None,
            0,
            true,
            None,
        );
        let binding_revision = basis
            .binding_head()
            .revision()
            .checked_next()
            .map_err(|_| {
                SyndicReadError::Invariant("accepted-input promotion binding revision is exhausted")
            })?;
        let expected_binding = BindingRecord::new(
            expected_thread.id(),
            binding_revision,
            selected_path,
            BindingState::unbound("promoted accepted input awaits an execution projection")
                .map_err(|_| {
                    SyndicReadError::Invariant("accepted-input promotion binding reason is invalid")
                })?,
        );
        let expected_binding_head = BindingHeadRecord::new(
            expected_thread.id(),
            binding_revision,
            BindingLifecycle::Unbound,
            successor_digest,
        );
        let Some(thread) = self.thread.as_ref() else {
            return Ok(false);
        };
        let Some(current_draft) = self.current_draft.as_ref() else {
            return Ok(false);
        };
        let expected_parent_index = match (thread.parent_thread_id(), thread.context_owner_id()) {
            (Some(parent), Some(owner)) => Some(ThreadParentIndexRecord::new(
                parent,
                expected_thread.id(),
                thread.revision(),
                owner,
            )),
            _ => None,
        };
        let Some(admission_count) = accepted_admission_descendant_count(
            thread,
            &expected_thread,
            self.gate.as_ref(),
            &expected_gate,
        ) else {
            return Ok(false);
        };
        let Some(draft_advance) = draft_index_descendant(
            self.draft_index.as_ref(),
            self.current_draft.as_ref(),
            thread,
            &expected_draft_index,
            admission_count,
        ) else {
            return Ok(false);
        };
        Ok(transcript_agrees(
            self.transcript_head.as_ref(),
            self.transcript_build.as_ref(),
            &expected_transcript,
            thread,
            &expected_thread,
        ) && summary_agrees(
            self.summary.as_ref(),
            thread,
            current_draft,
            &expected_summary,
            admission_count,
            draft_advance,
        ) && activity_agrees(
            self.activity_head.as_ref(),
            self.activity_source.as_ref(),
            &expected_activity_head,
            &expected_activity_source,
        ) && self.successor_binding.as_ref() == Some(&expected_binding)
            && self.binding_head.as_ref() == Some(&expected_binding_head)
            && self.thread_parent_index == expected_parent_index)
    }
}

impl PromotionObservation {
    fn exact_route_agrees(
        &self,
        promotion: &PromoteAcceptedInput,
    ) -> Result<bool, SyndicReadError> {
        let basis = promotion.candidate().basis();
        let current = basis.generation();
        let revision = current.revision().checked_next().map_err(|_| {
            SyndicReadError::Invariant("accepted-input promotion route revision is exhausted")
        })?;
        let logical_bytes = basis.input().content().summary().logical_utf8_bytes();
        let next_count =
            current
                .next_turn_count()
                .checked_sub(1)
                .ok_or(SyndicReadError::Invariant(
                    "accepted-input promotion next count underflowed",
                ))?;
        let expected_generation = AcceptedRouteGenerationRecord::new(
            current.thread_id(),
            current.generation(),
            revision,
            current.target().clone(),
            current.first_ordinal(),
            current.last_ordinal(),
            current.input_count(),
            current.ready_retryable_count(),
            current.delivering_count(),
            next_count,
            current
                .terminal_count()
                .checked_add(1)
                .ok_or(SyndicReadError::Invariant(
                    "accepted-input promotion terminal count overflowed",
                ))?,
            current
                .live_logical_utf8_bytes()
                .checked_sub(logical_bytes)
                .ok_or(SyndicReadError::Invariant(
                    "accepted-input promotion route bytes underflowed",
                ))?,
            current.delivering_logical_utf8_bytes(),
        )
        .map_err(|_| {
            SyndicReadError::Invariant(
                "accepted-input promotion route successor cannot be constructed",
            )
        })?;
        let mut expected_leaf = AcceptedRouteLeafRecord::new(
            basis.leaf().input_id(),
            basis.leaf().thread_id(),
            basis.leaf().generation(),
            basis.leaf().ordinal(),
            basis.leaf().revision().checked_next().map_err(|_| {
                SyndicReadError::Invariant("accepted-input promotion leaf revision is exhausted")
            })?,
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Promoted,
        );
        if let Some(transition) = basis.leaf().last_transition() {
            expected_leaf = expected_leaf.with_transition_proof(transition);
        }
        expected_leaf = expected_leaf.with_promotion_proof(promotion.proof());
        let expected_source = (next_count > 0).then(|| {
            AcceptedNextSourceRecord::new(
                current.thread_id(),
                current.generation(),
                revision,
                current
                    .first_ordinal()
                    .expect("promotion candidate generation is nonempty"),
                current
                    .last_ordinal()
                    .expect("promotion candidate generation is nonempty"),
            )
        });
        let expected_head = basis.route_head().map(|head| {
            if head.proof().generation() == current.generation() {
                AcceptedRouteGenerationHeadRecord::new(
                    current.thread_id(),
                    AcceptedRouteHeadProof::new(current.generation(), revision),
                )
            } else {
                *head
            }
        });
        Ok(self.generation.as_ref() == Some(&expected_generation)
            && self.leaf.as_ref() == Some(&expected_leaf)
            && self.source == expected_source
            && self.route_head == expected_head)
    }
}
