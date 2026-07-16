use beryl_home_store::HomeStore;

use crate::{
    AbandonActiveBinding, ActivateBinding, ActiveCasBinding, ActiveCasTurnPublicationStatus,
    ActiveCasTurnRecord, BindingPublicationStatus, BindingRecord, BindingState,
    CancelBindingActivation, CasThreadBindingIndexRecord, CasTurnIndexRecord,
    ExecutionSnapshotRecord, InputGateState, PublishActiveCasTurn, PublishStaleBinding,
    PublishUnboundBinding, PublishValidBinding, SyndicReadError, UsableCasBinding, codec::*,
    domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

struct CasThreadReservationPublication<'a> {
    status: BindingPublicationStatus,
    thread: beryl_model::SyndicThreadId,
    cas_thread: &'a beryl_model::CasThreadId,
    revision: beryl_model::BindingRevision,
    stale: bool,
}

impl SyndicStorage {
    /// Reconciles one valid-binding publication through its immutable next revision.
    pub fn valid_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &PublishValidBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let revision = next_binding_revision(request.expected_binding_revision())?;
        let usable = UsableCasBinding::new(
            request.execution().clone(),
            request.cas_thread_id().clone(),
            request.represented_prefix(),
            request.native_turn_count(),
            request.tool_profile(),
            request.lineage(),
        );
        let status = self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            BindingRecord::new(
                request.thread_id(),
                revision,
                request.selected_path(),
                BindingState::valid(usable),
            ),
            limit,
        )?;
        self.classify_cas_thread_reservation(
            store,
            CasThreadReservationPublication {
                status,
                thread: request.thread_id(),
                cas_thread: request.cas_thread_id(),
                revision,
                stale: false,
            },
            limit,
        )
    }

    /// Reconciles one stale-binding publication through its immutable next revision.
    pub fn stale_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &PublishStaleBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let revision = next_binding_revision(request.expected_binding_revision())?;
        let status = self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            BindingRecord::new(
                request.thread_id(),
                revision,
                request.selected_path(),
                BindingState::stale(request.stale().clone()),
            ),
            limit,
        )?;
        self.classify_cas_thread_reservation(
            store,
            CasThreadReservationPublication {
                status,
                thread: request.thread_id(),
                cas_thread: request.stale().cas_thread_id(),
                revision,
                stale: true,
            },
            limit,
        )
    }

    /// Reconciles one atomic active-projection abandonment through its stale binding revision.
    pub fn abandoned_active_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &AbandonActiveBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        self.stale_binding_publication_status(
            store,
            &PublishStaleBinding::new(
                request.thread_id(),
                request.expected_binding_revision(),
                request.selected_path(),
                request.stale().clone(),
            ),
            limit,
        )
    }

    /// Reconciles one unbound-binding publication through its immutable next revision.
    pub fn unbound_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &PublishUnboundBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let revision = next_binding_revision(request.expected_binding_revision())?;
        self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            BindingRecord::new(
                request.thread_id(),
                revision,
                request.selected_path(),
                request.state().clone(),
            ),
            limit,
        )
    }

    /// Reconciles activation through its immutable active binding and execution snapshot.
    pub fn binding_activation_status(
        &self,
        store: &HomeStore,
        request: &ActivateBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let Some(prior) = self.binding(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            limit,
        )?
        else {
            return Ok(BindingPublicationStatus::Collision);
        };
        let BindingState::Valid(usable) = prior.record().state() else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if prior.record().selected_path() != request.selected_path() {
            return Ok(BindingPublicationStatus::Collision);
        }
        let revision = next_binding_revision(request.expected_binding_revision())?;
        let activation_gate_revision =
            request
                .expected_gate_revision()
                .checked_next()
                .map_err(|_| {
                    SyndicReadError::Invariant(
                        "activation reconciliation gate frontier is exhausted",
                    )
                })?;
        let active = ActiveCasBinding::new(
            usable.clone(),
            request.snapshot_id(),
            request.turn_id(),
            activation_gate_revision,
            request.started_at(),
        );
        let expected_binding = BindingRecord::new(
            request.thread_id(),
            revision,
            request.selected_path(),
            BindingState::active(active),
        );
        let expected_snapshot = ExecutionSnapshotRecord::new(
            request.snapshot_id(),
            request.thread_id(),
            revision,
            activation_gate_revision,
            request.turn_id(),
            usable.cas_thread_id().clone(),
            request.selected_path(),
            usable.represented_prefix(),
            usable.native_turn_count(),
            usable.tool_profile(),
            usable.lineage(),
            usable.execution().clone(),
            request.loaded_generation(),
            request.started_at(),
        );
        let owner = self.cas_thread_owner(store, usable.cas_thread_id().clone(), limit)?;
        let membership =
            self.cas_thread_binding_membership(store, usable.cas_thread_id(), revision, limit)?;
        match self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            expected_binding,
            limit,
        )? {
            BindingPublicationStatus::Exact => {
                let snapshot = self.execution_snapshot(store, request.snapshot_id(), limit)?;
                Ok(
                    if snapshot
                        .as_ref()
                        .is_some_and(|stored| stored.record() == &expected_snapshot)
                        && membership.as_ref().is_some_and(|stored| {
                            stored.record()
                                == &expected_membership(
                                    usable.cas_thread_id(),
                                    request.thread_id(),
                                    revision,
                                )
                        })
                        && owner.as_ref().is_some_and(|owner| {
                            owner.record().thread_id() == request.thread_id()
                                && owner.record().first_binding_revision() <= revision
                                && owner.record().latest_binding_revision() >= revision
                                && owner
                                    .record()
                                    .retired_binding_revision()
                                    .is_none_or(|retired| retired > revision)
                        })
                    {
                        BindingPublicationStatus::Exact
                    } else {
                        BindingPublicationStatus::Collision
                    },
                )
            }
            BindingPublicationStatus::Prior => {
                let snapshot = self.execution_snapshot(store, request.snapshot_id(), limit)?;
                let gate = self.input_gate(store, request.thread_id(), limit)?;
                Ok(
                    if snapshot.is_none()
                        && membership.is_none()
                        && owner.as_ref().is_some_and(|owner| {
                            owner.record().thread_id() == request.thread_id()
                                && owner.record().latest_binding_revision()
                                    == request.expected_binding_revision()
                                && owner.record().retired_binding_revision().is_none()
                        })
                        && gate.as_ref().is_some_and(|stored| {
                            stored.record().revision() == request.expected_gate_revision()
                                && stored.record().state()
                                    == &InputGateState::PendingTurn(request.turn_id())
                        })
                    {
                        BindingPublicationStatus::Prior
                    } else {
                        BindingPublicationStatus::Collision
                    },
                )
            }
            BindingPublicationStatus::Collision => Ok(BindingPublicationStatus::Collision),
        }
    }

    /// Reconciles one activation cancellation through its immutable valid successor.
    pub fn cancelled_binding_activation_status(
        &self,
        store: &HomeStore,
        request: &CancelBindingActivation,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let Some(prior) = self.binding(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            limit,
        )?
        else {
            return Ok(BindingPublicationStatus::Collision);
        };
        let BindingState::Active(active) = prior.record().state() else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if prior.record().selected_path() != request.selected_path()
            || active.snapshot_id() != request.snapshot_id()
            || active.turn_id() != request.turn_id()
        {
            return Ok(BindingPublicationStatus::Collision);
        }
        let Some(snapshot) = self.execution_snapshot(store, request.snapshot_id(), limit)? else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if snapshot.record().thread_id() != request.thread_id()
            || snapshot.record().binding_revision() != request.expected_binding_revision()
            || snapshot.record().activation_gate_revision() != request.expected_gate_revision()
            || snapshot.record().active_turn_id() != request.turn_id()
            || self
                .active_cas_turn(store, request.snapshot_id(), limit)?
                .is_some()
        {
            return Ok(BindingPublicationStatus::Collision);
        }

        let revision = next_binding_revision(request.expected_binding_revision())?;
        let status = self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            BindingRecord::new(
                request.thread_id(),
                revision,
                request.selected_path(),
                BindingState::valid(active.usable().clone()),
            ),
            limit,
        )?;
        let status = self.classify_cas_thread_reservation(
            store,
            CasThreadReservationPublication {
                status,
                thread: request.thread_id(),
                cas_thread: active.usable().cas_thread_id(),
                revision,
                stale: false,
            },
            limit,
        )?;
        let gate = self.input_gate(store, request.thread_id(), limit)?;
        let expected_next_gate = request
            .expected_gate_revision()
            .checked_next()
            .map_err(|_| {
                SyndicReadError::Invariant("activation-cancellation gate frontier is exhausted")
            })?;
        Ok(match status {
            BindingPublicationStatus::Exact
                if gate.as_ref().is_some_and(|stored| {
                    stored.record().revision() == expected_next_gate
                        && stored.record().state()
                            == &InputGateState::PendingTurn(request.turn_id())
                        && stored.record().live_count() == 0
                        && stored.record().live_logical_utf8_bytes() == 0
                }) =>
            {
                BindingPublicationStatus::Exact
            }
            BindingPublicationStatus::Prior
                if gate.as_ref().is_some_and(|stored| {
                    stored.record().revision() == request.expected_gate_revision()
                        && matches!(
                            stored.record().state(),
                            InputGateState::AwaitingSteering(pending)
                                if pending.binding_revision()
                                    == request.expected_binding_revision()
                                    && pending.snapshot_id() == request.snapshot_id()
                                    && pending.active_turn_id() == request.turn_id()
                                    && pending.cas_thread_id()
                                        == active.usable().cas_thread_id()
                        )
                        && stored.record().live_count() == 0
                        && stored.record().live_logical_utf8_bytes() == 0
                }) =>
            {
                BindingPublicationStatus::Prior
            }
            _ => BindingPublicationStatus::Collision,
        })
    }

    /// Reconciles the one-way CAS-turn identity and its permanent reverse correlation.
    pub fn active_cas_turn_publication_status(
        &self,
        store: &HomeStore,
        request: &PublishActiveCasTurn,
        limit: SyndicPointReadLimit,
    ) -> Result<ActiveCasTurnPublicationStatus, SyndicReadError> {
        let primary = self.active_cas_turn(store, request.snapshot_id(), limit)?;
        let reverse = self.cas_turn_owner(
            store,
            request.cas_thread_id().clone(),
            request.cas_turn_id().clone(),
            limit,
        )?;
        let Some(primary) = primary else {
            return Ok(if reverse.is_none() {
                ActiveCasTurnPublicationStatus::Absent
            } else {
                ActiveCasTurnPublicationStatus::Collision
            });
        };
        let stored = primary.record();
        let Some(snapshot) = self.execution_snapshot(store, request.snapshot_id(), limit)? else {
            return Ok(ActiveCasTurnPublicationStatus::Collision);
        };
        let post_turn_native_count = snapshot
            .record()
            .represented_base_native_turn_count()
            .checked_next()
            .map_err(|_| {
                SyndicReadError::Invariant(
                    "active CAS-turn reconciliation native count is exhausted",
                )
            })?;
        let expected_primary = ActiveCasTurnRecord::new(
            request.snapshot_id(),
            request.thread_id(),
            stored.turn_id(),
            request.binding_revision(),
            request.cas_thread_id().clone(),
            request.cas_turn_id().clone(),
            request.published_at(),
        );
        let expected_reverse = CasTurnIndexRecord::new(
            request.cas_thread_id().clone(),
            request.cas_turn_id().clone(),
            request.thread_id(),
            stored.turn_id(),
            request.binding_revision(),
            request.snapshot_id(),
            post_turn_native_count,
        );
        Ok(
            if stored == &expected_primary
                && reverse
                    .as_ref()
                    .is_some_and(|record| record.record() == &expected_reverse)
            {
                ActiveCasTurnPublicationStatus::Exact
            } else {
                ActiveCasTurnPublicationStatus::Collision
            },
        )
    }

    fn classify_binding_publication(
        &self,
        store: &HomeStore,
        thread: beryl_model::SyndicThreadId,
        prior_revision: beryl_model::BindingRevision,
        expected: BindingRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        match self.point::<BindingsFamily>(
            store,
            BindingKey {
                thread,
                revision: expected.revision(),
            },
            limit,
        )? {
            Some(stored) => Ok(if stored.record() == &expected {
                BindingPublicationStatus::Exact
            } else {
                BindingPublicationStatus::Collision
            }),
            None => {
                let current = self.current_binding(store, thread, limit)?;
                Ok(
                    if current.as_ref().is_some_and(|current| {
                        current.head().revision() == prior_revision
                            && current.binding().selected_path() == expected.selected_path()
                    }) {
                        BindingPublicationStatus::Prior
                    } else {
                        BindingPublicationStatus::Collision
                    },
                )
            }
        }
    }

    fn classify_cas_thread_reservation(
        &self,
        store: &HomeStore,
        publication: CasThreadReservationPublication<'_>,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let owner = self.cas_thread_owner(store, publication.cas_thread.clone(), limit)?;
        let membership = self.cas_thread_binding_membership(
            store,
            publication.cas_thread,
            publication.revision,
            limit,
        )?;
        Ok(match (publication.status, owner, membership) {
            (BindingPublicationStatus::Collision, _, _) => BindingPublicationStatus::Collision,
            (BindingPublicationStatus::Exact, Some(owner), Some(membership))
                if owner.record().thread_id() == publication.thread
                    && owner.record().first_binding_revision() <= publication.revision
                    && owner.record().latest_binding_revision() >= publication.revision
                    && membership.record()
                        == &expected_membership(
                            publication.cas_thread,
                            publication.thread,
                            publication.revision,
                        )
                    && if publication.stale {
                        owner.record().retired_binding_revision() == Some(publication.revision)
                    } else {
                        owner
                            .record()
                            .retired_binding_revision()
                            .is_none_or(|retired| retired > publication.revision)
                    } =>
            {
                BindingPublicationStatus::Exact
            }
            (BindingPublicationStatus::Prior, None, None) => BindingPublicationStatus::Prior,
            (BindingPublicationStatus::Prior, Some(owner), None)
                if owner.record().thread_id() == publication.thread
                    && owner.record().latest_binding_revision() < publication.revision
                    && owner.record().retired_binding_revision().is_none() =>
            {
                BindingPublicationStatus::Prior
            }
            _ => BindingPublicationStatus::Collision,
        })
    }

    fn cas_thread_binding_membership(
        &self,
        store: &HomeStore,
        cas_thread: &beryl_model::CasThreadId,
        revision: beryl_model::BindingRevision,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<super::SyndicStoredRecord<CasThreadBindingIndexRecord>>, SyndicReadError>
    {
        self.point::<CasThreadBindingIndexFamily>(
            store,
            CasThreadBindingKey::Record(cas_thread.clone(), revision),
            limit,
        )
    }
}

fn expected_membership(
    cas_thread: &beryl_model::CasThreadId,
    thread: beryl_model::SyndicThreadId,
    revision: beryl_model::BindingRevision,
) -> CasThreadBindingIndexRecord {
    CasThreadBindingIndexRecord::new(cas_thread.clone(), thread, revision)
}

fn next_binding_revision(
    revision: beryl_model::BindingRevision,
) -> Result<beryl_model::BindingRevision, SyndicReadError> {
    revision.checked_next().map_err(|_| {
        SyndicReadError::Invariant("binding reconciliation revision frontier is exhausted")
    })
}
