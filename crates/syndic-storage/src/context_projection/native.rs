use super::{ContextPreparationKind, read::projection_digest};
use crate::{
    CasLineageProof, ContextNativeCut, ContextProjectionBasis, ContextSourceAnchor,
    NativeProjectionError, NativeProjectionPlan, NativeProjectionRequest,
    NativeProjectionUnavailable, SyndicStorage, SyndicTimestamp, UsableCasBinding,
};
use beryl_home_store::HomeStore;

impl SyndicStorage {
    pub(crate) fn qualify_context_native_plan(
        &self,
        store: &HomeStore,
        request: &NativeProjectionRequest,
        plan: NativeProjectionPlan,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<NativeProjectionPlan, NativeProjectionError> {
        let mut basis = plan.basis();
        let presence = super::presence::ContextPresenceAccess::Live(self, store)
            .classify(request.thread_id(), basis.represented_prefix())?;
        let summary = if presence == super::presence::ContextPresence::Absent {
            crate::ContextSummary::empty()
        } else {
            self.context_summary(
                store,
                request.thread_id(),
                basis.represented_prefix(),
                true,
                cancelled,
            )?
        };
        let projection_digest = projection_digest(
            store.home_id(),
            request.thread_id(),
            basis.represented_prefix(),
            summary,
            request.execution(),
            basis.tool_profile(),
        );
        let source_revision = self.revision(store)?;
        let expected_gate = self
            .input_gate(store, request.thread_id(), super::read::point())?
            .ok_or(NativeProjectionError::Invariant(
                "context target gate missing",
            ))?
            .revision();
        basis.context_classification = Some(super::ContextClassification {
            home: store.home_id(),
            home_generation: self.context_generation,
            thread: request.thread_id(),
            prefix: basis.represented_prefix(),
            selected: basis.selected_path(),
            gate: expected_gate,
            summary,
        });
        if summary.count() == 0 {
            return Ok(match plan {
                NativeProjectionPlan::Current { source, .. } => {
                    NativeProjectionPlan::Current { basis, source }
                }
                NativeProjectionPlan::Resume { source, .. } => {
                    NativeProjectionPlan::Resume { basis, source }
                }
                NativeProjectionPlan::Fork {
                    source,
                    through_turn,
                    native_turn_count,
                    ..
                } => NativeProjectionPlan::Fork {
                    basis,
                    source,
                    through_turn,
                    native_turn_count,
                },
                NativeProjectionPlan::Fresh { .. } => NativeProjectionPlan::Fresh { basis },
                NativeProjectionPlan::Unavailable { source, reason, .. } => {
                    NativeProjectionPlan::Unavailable {
                        basis,
                        source,
                        reason,
                    }
                }
            });
        }
        let make = |kind, source_head_revision| ContextProjectionBasis {
            home: store.home_id(),
            home_generation: self.context_generation,
            thread: request.thread_id(),
            prefix: basis.represented_prefix(),
            summary,
            projection_digest,
            source_revision,
            selected_path: basis.selected_path(),
            expected_gate,
            source_head_revision,
            kind,
        };
        let (source, through, full_fork) = match &plan {
            NativeProjectionPlan::Current { source, .. }
            | NativeProjectionPlan::Resume { source, .. } => (Some(source), false, true),
            NativeProjectionPlan::Fork {
                source,
                through_turn,
                ..
            } => (Some(source), true, through_turn.is_none()),
            _ => (None, false, false),
        };
        let mut eligible = true;
        if let Some(source) = source {
            let source_summary = match source.binding().lineage() {
                CasLineageProof::ContextAware(proof) => {
                    self.validate_context_binding(store, source.thread_id(), source.binding())?;
                    if full_fork {
                        proof.summary()
                    } else {
                        self.context_summary(
                            store,
                            source.thread_id(),
                            basis.represented_prefix(),
                            false,
                            cancelled,
                        )?
                    }
                }
                _ => crate::ContextSummary::empty(),
            };
            let cut = ContextNativeCut {
                prefix: basis.represented_prefix(),
                summary: source_summary,
            };
            let anchor = ContextSourceAnchor {
                thread: source.thread_id(),
                revision: source.binding_revision(),
            };
            let floor = source
                .binding()
                .lineage()
                .recovered_completed_at()
                .unwrap_or(SyndicTimestamp::from_unix_millis(0));
            let mut seed_sequence = None;
            let seed = if source_summary == summary {
                None
            } else if through
                && self
                    .context_thread(store, request.thread_id())?
                    .context_owner_id()
                    .is_some()
            {
                let record = self.context_record_for_execution(store, request.thread_id())?;
                let frame = super::frame::describe(store, request.thread_id(), &record)?;
                if record.envelope().descriptor().source_turn_id()
                    == basis
                        .represented_prefix()
                        .tail()
                        .ok_or(NativeProjectionError::Invariant(
                            "context native prefix is empty",
                        ))?
                    && source_summary
                        .append(frame.entry, frame.utf8_bytes)
                        .map_err(|_| NativeProjectionError::Invariant("context summary overflow"))?
                        == summary
                {
                    seed_sequence = Some((frame.utf8_bytes, frame.sequence_digest));
                    Some(frame.entry)
                } else {
                    eligible = false;
                    None
                }
            } else {
                eligible = false;
                None
            };
            let source_head = self
                .current_binding(store, source.thread_id(), super::read::point())?
                .ok_or(NativeProjectionError::Invariant(
                    "context source binding missing",
                ))?
                .binding()
                .revision();
            basis.context = Some(make(
                ContextPreparationKind::Native {
                    source: anchor,
                    cut,
                    seed,
                    seed_sequence,
                    ordering_floor: floor,
                },
                Some(source_head),
            ));
        } else {
            basis.context = Some(make(ContextPreparationKind::Replay, None));
        }
        if !eligible {
            return Ok(NativeProjectionPlan::Unavailable {
                basis,
                source: source
                    .filter(|source| source.thread_id() == request.thread_id())
                    .cloned(),
                reason: NativeProjectionUnavailable::SourcePrefixMismatch,
            });
        }
        Ok(match plan {
            NativeProjectionPlan::Current { source, .. } => {
                NativeProjectionPlan::Current { basis, source }
            }
            NativeProjectionPlan::Resume { source, .. } => {
                NativeProjectionPlan::Resume { basis, source }
            }
            NativeProjectionPlan::Fork {
                source,
                through_turn,
                native_turn_count,
                ..
            } => NativeProjectionPlan::Fork {
                basis,
                source,
                through_turn,
                native_turn_count,
            },
            NativeProjectionPlan::Unavailable { source, reason, .. } => {
                NativeProjectionPlan::Unavailable {
                    basis,
                    source,
                    reason,
                }
            }
            NativeProjectionPlan::Fresh { .. } => {
                return Err(NativeProjectionError::Invariant(
                    "context prefix cannot be native empty",
                ));
            }
        })
    }

    pub fn validate_context_binding(
        &self,
        store: &HomeStore,
        thread: beryl_model::SyndicThreadId,
        usable: &UsableCasBinding,
    ) -> Result<(), NativeProjectionError> {
        let record = self.context_thread(store, thread)?;
        match (record.context_owner_id(), usable.lineage()) {
            (Some(_), CasLineageProof::Native { .. } | CasLineageProof::RecoveredInjection(_)) => {
                Err(NativeProjectionError::DiscussionContextProjectionRequired)
            }
            (
                None | Some(beryl_model::DiscussionContextOwnerId::SubmittedTurn(_)),
                CasLineageProof::ContextAware(proof),
            ) => {
                if proof.home_id() != store.home_id()
                    || projection_digest(
                        store.home_id(),
                        thread,
                        proof.established_prefix(),
                        proof.summary(),
                        usable.execution(),
                        usable.tool_profile(),
                    ) != proof.projection_digest()
                {
                    return Err(NativeProjectionError::DiscussionContextProjectionRequired);
                }
                Ok(())
            }
            (Some(_), _) => Err(NativeProjectionError::DiscussionContextProjectionRequired),
            (None, _) => {
                let presence = super::presence::ContextPresenceAccess::Live(self, store)
                    .classify(thread, usable.represented_prefix())?;
                if presence == super::presence::ContextPresence::Required
                    || (presence == super::presence::ContextPresence::Unclassified
                        && self
                            .context_summary(
                                store,
                                thread,
                                usable.represented_prefix(),
                                true,
                                &|| false,
                            )?
                            .count()
                            != 0)
                {
                    Err(NativeProjectionError::DiscussionContextProjectionRequired)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn prepare_context_binding_cancellable(
        &self,
        store: &HomeStore,
        thread: beryl_model::SyndicThreadId,
        usable: &UsableCasBinding,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<crate::ContextBindingClassification, NativeProjectionError> {
        if cancelled() {
            return Err(crate::RecoveryProjectionError::Cancelled.into());
        }
        let revision = self.revision(store)?;
        let target = self.context_thread(store, thread)?;
        let binding = self
            .current_binding(store, thread, super::read::point())?
            .ok_or(NativeProjectionError::Invariant(
                "context classification binding missing",
            ))?;
        if !matches!(binding.binding().state(), crate::BindingState::Valid(current) if current == usable)
        {
            return Err(NativeProjectionError::ConcurrentChange);
        }
        if matches!(usable.lineage(), CasLineageProof::ContextAware(_)) {
            self.validate_context_binding(store, thread, usable)?;
        }
        let context = self.prepare_context_classification(
            store,
            thread,
            target.selected_path(),
            usable.represented_prefix(),
            binding.binding().revision(),
            cancelled,
        )?;
        if match usable.lineage() {
            CasLineageProof::ContextAware(proof) => context.summary != proof.summary(),
            _ => context.summary.count() != 0,
        } {
            return Err(NativeProjectionError::DiscussionContextProjectionRequired);
        }
        if cancelled() {
            return Err(crate::RecoveryProjectionError::Cancelled.into());
        }
        if self.revision(store)? != revision {
            return Err(crate::RecoveryProjectionError::ConcurrentChange.into());
        }
        Ok(crate::ContextBindingClassification {
            context,
            binding_revision: binding.binding().revision(),
        })
    }

    pub fn prepare_context_free_publication_cancellable(
        &self,
        store: &HomeStore,
        request: crate::PublishValidBinding,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<crate::PublishValidBinding, NativeProjectionError> {
        if cancelled() {
            return Err(crate::RecoveryProjectionError::Cancelled.into());
        }
        if matches!(request.lineage(), CasLineageProof::ContextAware(_)) {
            return Err(NativeProjectionError::DiscussionContextProjectionRequired);
        }
        let revision = self.revision(store)?;
        let execution = self
            .thread_execution(store, request.thread_id(), super::read::point())?
            .ok_or(NativeProjectionError::Invariant(
                "context publication execution missing",
            ))?;
        if execution.execution() != request.execution() {
            return Err(NativeProjectionError::ConcurrentChange);
        }
        let classified = self.prepare_context_classification(
            store,
            request.thread_id(),
            request.selected_path(),
            request.represented_prefix(),
            request.expected_binding_revision(),
            cancelled,
        )?;
        if classified.summary.count() != 0 {
            return Err(NativeProjectionError::DiscussionContextProjectionRequired);
        }
        if self.revision(store)? != revision {
            return Err(NativeProjectionError::ConcurrentChange);
        }
        Ok(request.with_empty_context_classification(classified))
    }

    fn prepare_context_classification(
        &self,
        store: &HomeStore,
        thread: beryl_model::SyndicThreadId,
        selected: crate::SelectedPathProof,
        prefix: crate::CasRepresentedPrefixProof,
        binding_revision: beryl_model::BindingRevision,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<super::ContextClassification, NativeProjectionError> {
        if cancelled() {
            return Err(crate::RecoveryProjectionError::Cancelled.into());
        }
        let revision = self.revision(store)?;
        let target = self.context_thread(store, thread)?;
        let current = self
            .current_binding(store, thread, super::read::point())?
            .ok_or(NativeProjectionError::Invariant(
                "context classification binding missing",
            ))?;
        if target.selected_path() != selected {
            return Err(NativeProjectionError::StaleSelectedPath);
        }
        if current.binding().revision() != binding_revision {
            return Err(NativeProjectionError::ConcurrentChange);
        }
        if prefix.source_thread_revision() > selected.thread_revision() {
            return Err(NativeProjectionError::StaleSelectedPath);
        }
        if let Some(prefix_id) = prefix.tail() {
            let selected_tail = self
                .turn(
                    store,
                    selected
                        .tail()
                        .ok_or(NativeProjectionError::StaleSelectedPath)?,
                    super::read::point(),
                )?
                .ok_or(NativeProjectionError::Invariant(
                    "context selected tail missing",
                ))?;
            let prefix_tail = self.turn(store, prefix_id, super::read::point())?.ok_or(
                NativeProjectionError::Invariant("context represented tail missing"),
            )?;
            if selected_tail.chain_digest() != selected.digest()
                || prefix_tail.chain_digest() != prefix.digest()
                || prefix_tail.depth() > selected_tail.depth()
                || self
                    .context_turn_at_cancellable(
                        store,
                        &selected_tail,
                        prefix_tail.depth().get(),
                        revision,
                        cancelled,
                    )?
                    .id()
                    != prefix_id
            {
                return Err(NativeProjectionError::StaleSelectedPath);
            }
        } else if prefix.digest() != crate::empty_selected_path_digest() {
            return Err(NativeProjectionError::StaleSelectedPath);
        }
        let presence =
            super::presence::ContextPresenceAccess::Live(self, store).classify(thread, prefix)?;
        let summary = if presence == super::presence::ContextPresence::Absent {
            crate::ContextSummary::empty()
        } else {
            self.context_summary(store, thread, prefix, true, cancelled)?
        };
        let gate = self
            .input_gate(store, thread, super::read::point())?
            .ok_or(NativeProjectionError::Invariant(
                "context classification gate missing",
            ))?;
        if cancelled() {
            return Err(crate::RecoveryProjectionError::Cancelled.into());
        }
        if self.revision(store)? != revision {
            return Err(NativeProjectionError::ConcurrentChange);
        }
        Ok(super::ContextClassification {
            home: store.home_id(),
            home_generation: self.context_generation,
            thread,
            prefix,
            selected,
            gate: gate.revision(),
            summary,
        })
    }
}
