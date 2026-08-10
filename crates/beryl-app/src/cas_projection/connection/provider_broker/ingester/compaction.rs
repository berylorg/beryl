use std::time::{SystemTime, UNIX_EPOCH};

use beryl_backend::{
    LoadedThreadStatus, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamRejection, ProviderObservationRoute, ThreadStatusChanged, TurnStarted,
};
use syndic_storage::{
    CasTurnSource, CompactionProviderEvent, CompactionThreadStatus, ProviderCompactionMarkerStager,
    SyndicTimestamp,
};

use super::Ingester;
use crate::cas_projection::connection::router::{
    CompactionControlPermit, CompactionControlPermitError, SourcePublicationFinishError,
    SourcePublicationPermitError,
};

impl Ingester {
    pub(super) fn seal_compaction_marker(
        &mut self,
        route: ProviderObservationRoute,
        permit: crate::cas_projection::connection::router::SourcePublicationPermit,
        marker: ProviderCompactionMarkerStager,
    ) -> (super::BrokerReply, bool) {
        let Some(authority) = permit.compaction() else {
            marker.abandon();
            return self.failed_permit(route, permit);
        };
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire)
            || self.home.home_id() != self.home_id
            || permit.home_generation(&self.home).is_none()
            || permit.cas_thread_id() != route.thread_id()
            || permit.cas_turn_id() != route.turn_id()
            || authority.operation_id().thread_id() != permit.syndic_thread_id()
            || authority.operation_id().provider_turn_id() != authority.provider_turn_id()
        {
            marker.abandon();
            return self.failed_permit(route, permit);
        }
        let marker = match marker.seal() {
            Ok(marker) => marker,
            Err(_) => {
                drop(permit);
                return self.reject(
                    OrderedTurnStreamOperation::ProviderSeal(route),
                    OrderedTurnStreamRejection::SchemaMismatch,
                );
            }
        };
        let source =
            CasTurnSource::new(permit.cas_thread_id().clone(), permit.cas_turn_id().clone());
        let item_id = crate::cas_projection::provider_identity::syndic_item_id(
            permit.syndic_thread_id(),
            authority.provider_turn_id(),
            &source,
            marker.item_id(),
        );
        if self
            .context_compaction
            .publish_provider_event(
                authority,
                CompactionProviderEvent::Marker {
                    item_id,
                    lifecycle: marker.lifecycle(),
                },
                marker.observed_at(),
            )
            .is_err()
        {
            return self.failed_permit(route, permit);
        }
        self.finish_provider_publication(route, permit, None)
    }

    pub(super) fn thread_status_changed(
        &mut self,
        status: ThreadStatusChanged,
    ) -> (super::BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject(
                OrderedTurnStreamOperation::ThreadStatusChanged(status),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let permit = match self
            .router
            .acquire_compaction_thread_status(status.thread_id())
        {
            Ok(permit) => permit,
            Err(CompactionControlPermitError::Unmatched) => return applied(),
            Err(CompactionControlPermitError::Target(invalidation)) => {
                let terminal =
                    self.invalidate_target(invalidation) == super::TargetRouteOutcome::Terminal;
                return (applied().0, terminal);
            }
            Err(CompactionControlPermitError::Router) => {
                return self.reject(
                    OrderedTurnStreamOperation::ThreadStatusChanged(status),
                    OrderedTurnStreamRejection::InvalidControl,
                );
            }
        };
        let durable = match status.status() {
            LoadedThreadStatus::Idle => CompactionThreadStatus::Idle,
            LoadedThreadStatus::SystemError => CompactionThreadStatus::SystemError,
            LoadedThreadStatus::Active { .. } => CompactionThreadStatus::Active,
        };
        self.publish_compaction_control(permit, CompactionProviderEvent::ThreadStatus(durable))
    }

    pub(super) fn turn_started(&mut self, turn: TurnStarted) -> (super::BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject(
                OrderedTurnStreamOperation::TurnStarted(turn),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        match self
            .router
            .acquire_compaction_turn_started(turn.thread_id(), turn.turn_id())
        {
            Ok(permit) => self.publish_compaction_control(
                permit,
                CompactionProviderEvent::TurnStarted(turn.turn_id().clone()),
            ),
            Err(CompactionControlPermitError::Unmatched) => self.activate_ordinary_turn(turn),
            Err(CompactionControlPermitError::Target(invalidation)) => {
                let terminal =
                    self.invalidate_target(invalidation) == super::TargetRouteOutcome::Terminal;
                (applied().0, terminal)
            }
            Err(CompactionControlPermitError::Router) => self.reject(
                OrderedTurnStreamOperation::TurnStarted(turn),
                OrderedTurnStreamRejection::InvalidControl,
            ),
        }
    }

    fn publish_compaction_control(
        &mut self,
        permit: CompactionControlPermit,
        event: CompactionProviderEvent,
    ) -> (super::BrokerReply, bool) {
        let authority = permit.authority();
        if self
            .context_compaction
            .publish_provider_event(authority, event, timestamp_now())
            .is_err()
        {
            if self.exact_persistent_failure() {
                drop(permit);
                return (applied().0, true);
            }
            return match permit.fail() {
                Ok(invalidation) | Err(CompactionControlPermitError::Target(invalidation)) => {
                    let terminal =
                        self.invalidate_target(invalidation) == super::TargetRouteOutcome::Terminal;
                    (applied().0, terminal)
                }
                Err(
                    CompactionControlPermitError::Unmatched | CompactionControlPermitError::Router,
                ) => {
                    self.retire();
                    (applied().0, true)
                }
            };
        }
        match permit.finish() {
            Ok(()) => applied(),
            Err(CompactionControlPermitError::Target(invalidation)) => {
                let terminal =
                    self.invalidate_target(invalidation) == super::TargetRouteOutcome::Terminal;
                (applied().0, terminal)
            }
            Err(CompactionControlPermitError::Unmatched | CompactionControlPermitError::Router) => {
                self.retire();
                (applied().0, true)
            }
        }
    }

    fn activate_ordinary_turn(&mut self, turn: TurnStarted) -> (super::BrokerReply, bool) {
        let permit = match self
            .router
            .acquire_source_publication(turn.thread_id(), turn.turn_id())
        {
            Ok(permit) => permit,
            Err(SourcePublicationPermitError::Unmatched) => return applied(),
            Err(SourcePublicationPermitError::Target(invalidation)) => {
                let terminal =
                    self.invalidate_target(invalidation) == super::TargetRouteOutcome::Terminal;
                return (applied().0, terminal);
            }
            Err(SourcePublicationPermitError::Router) => {
                self.retire();
                return (applied().0, true);
            }
        };
        match self.publish_source_activation(&permit, point_limit()) {
            Ok(_) => {}
            Err(error) if error.authority().is_some() => {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            Err(_) => {
                if self.exact_persistent_failure() {
                    drop(permit);
                    return (applied().0, true);
                }
                return match permit.fail() {
                    Ok(invalidation) | Err(SourcePublicationFinishError::Target(invalidation)) => {
                        let terminal = self.invalidate_target(invalidation)
                            == super::TargetRouteOutcome::Terminal;
                        (applied().0, terminal)
                    }
                    Err(SourcePublicationFinishError::Router) => {
                        self.retire();
                        (applied().0, true)
                    }
                };
            }
        }
        match permit.finish_held() {
            Ok(release) => {
                release.release();
                applied()
            }
            Err(SourcePublicationFinishError::Target(invalidation)) => {
                let terminal =
                    self.invalidate_target(invalidation) == super::TargetRouteOutcome::Terminal;
                (applied().0, terminal)
            }
            Err(SourcePublicationFinishError::Router) => {
                self.retire();
                (applied().0, true)
            }
        }
    }
}

fn applied() -> (super::BrokerReply, bool) {
    (
        super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
        false,
    )
}

fn timestamp_now() -> SyndicTimestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0);
    SyndicTimestamp::from_unix_millis(millis)
}

fn point_limit() -> syndic_storage::SyndicPointReadLimit {
    syndic_storage::SyndicPointReadLimit::new(super::super::PROVIDER_POINT_READ_BYTES)
        .expect("provider broker point-read bound is nonzero")
}
