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

mod publication;

struct CasThreadReservationPublication<'a> {
    status: BindingPublicationStatus,
    thread: beryl_model::SyndicThreadId,
    cas_thread: &'a beryl_model::CasThreadId,
    revision: beryl_model::BindingRevision,
    stale: bool,
}

impl SyndicStorage {
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
        let BindingState::Valid(usable) = prior.state() else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if !request
            .selected_path()
            .is_compatible_descendant_of(prior.selected_path())
        {
            return Ok(BindingPublicationStatus::Collision);
        }
        let Some(usable) =
            usable.advance_represented_source_revision(request.selected_path().thread_revision())
        else {
            return Ok(BindingPublicationStatus::Collision);
        };
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
                        .is_some_and(|stored| stored == &expected_snapshot)
                        && membership.as_ref().is_some_and(|stored| {
                            stored
                                == &expected_membership(
                                    usable.cas_thread_id(),
                                    request.thread_id(),
                                    revision,
                                )
                        })
                        && owner.as_ref().is_some_and(|owner| {
                            owner.thread_id() == request.thread_id()
                                && owner.first_binding_revision() <= revision
                                && owner.latest_binding_revision() >= revision
                                && owner
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
                            owner.thread_id() == request.thread_id()
                                && owner.latest_binding_revision()
                                    == request.expected_binding_revision()
                                && owner.retired_binding_revision().is_none()
                        })
                        && gate.as_ref().is_some_and(|stored| {
                            stored.revision() == request.expected_gate_revision()
                                && stored.state() == &InputGateState::PendingTurn(request.turn_id())
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
        let BindingState::Active(active) = prior.state() else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if prior.selected_path() != request.selected_path()
            || active.snapshot_id() != request.snapshot_id()
            || active.turn_id() != request.turn_id()
        {
            return Ok(BindingPublicationStatus::Collision);
        }
        let Some(snapshot) = self.execution_snapshot(store, request.snapshot_id(), limit)? else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if snapshot.thread_id() != request.thread_id()
            || snapshot.binding_revision() != request.expected_binding_revision()
            || snapshot.activation_gate_revision() != request.expected_gate_revision()
            || snapshot.active_turn_id() != request.turn_id()
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
                    stored.revision() == expected_next_gate
                        && stored.state() == &InputGateState::PendingTurn(request.turn_id())
                        && stored.live_count() == 0
                        && stored.live_logical_utf8_bytes() == 0
                }) =>
            {
                BindingPublicationStatus::Exact
            }
            BindingPublicationStatus::Prior
                if gate.as_ref().is_some_and(|stored| {
                    stored.revision() == request.expected_gate_revision()
                        && matches!(
                            stored.state(),
                            InputGateState::AwaitingSteering(turn)
                                if *turn == request.turn_id()
                        )
                        && stored.selected_route().is_some()
                        && stored.live_count() == 0
                        && stored.live_logical_utf8_bytes() == 0
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
        let stored = primary;
        let Some(snapshot) = self.execution_snapshot(store, request.snapshot_id(), limit)? else {
            return Ok(ActiveCasTurnPublicationStatus::Collision);
        };
        let post_turn_native_count = snapshot
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
            if stored == expected_primary
                && reverse
                    .as_ref()
                    .is_some_and(|record| record == &expected_reverse)
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
            Some(stored) => Ok(if stored == expected {
                BindingPublicationStatus::Exact
            } else {
                BindingPublicationStatus::Collision
            }),
            None => {
                let current = self.current_binding(store, thread, limit)?;
                Ok(
                    if current.as_ref().is_some_and(|current| {
                        current.head().revision() == prior_revision
                            && expected
                                .selected_path()
                                .is_compatible_descendant_of(current.binding().selected_path())
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
                if owner.thread_id() == publication.thread
                    && owner.first_binding_revision() <= publication.revision
                    && owner.latest_binding_revision() >= publication.revision
                    && membership
                        == expected_membership(
                            publication.cas_thread,
                            publication.thread,
                            publication.revision,
                        )
                    && if publication.stale {
                        owner.retired_binding_revision() == Some(publication.revision)
                    } else {
                        owner
                            .retired_binding_revision()
                            .is_none_or(|retired| retired > publication.revision)
                    } =>
            {
                BindingPublicationStatus::Exact
            }
            (BindingPublicationStatus::Prior, None, None) => BindingPublicationStatus::Prior,
            (BindingPublicationStatus::Prior, Some(owner), None)
                if owner.thread_id() == publication.thread
                    && owner.latest_binding_revision() < publication.revision
                    && owner.retired_binding_revision().is_none() =>
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
    ) -> Result<Option<CasThreadBindingIndexRecord>, SyndicReadError> {
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
