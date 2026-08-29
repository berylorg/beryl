use beryl_backend::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSubmitCause,
};

use super::super::{Ingester, TargetRouteOutcome};
use crate::cas_projection::connection::router::{
    SourcePublicationFinishError, SourcePublicationPermit, SourcePublicationPermitError,
    TargetInvalidation,
};

use super::staging::staging_rejection;

impl Ingester {
    pub(in super::super) fn seal(
        &mut self,
        route: beryl_backend::ProviderObservationRoute,
    ) -> (super::super::BrokerReply, bool) {
        if !self.provider_is_active() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderSeal(route),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let permit = match self
            .router
            .acquire_source_publication(route.thread_id(), route.turn_id())
        {
            Ok(permit) => permit,
            Err(SourcePublicationPermitError::Unmatched) => {
                return self.provider_target_failure(route, None);
            }
            Err(SourcePublicationPermitError::Target(invalidation)) => {
                return self.provider_target_failure(route, Some(invalidation));
            }
            Err(SourcePublicationPermitError::Router) => {
                return self.reject(
                    OrderedTurnStreamOperation::ProviderSeal(route),
                    OrderedTurnStreamRejection::InvalidControl,
                );
            }
        };
        let observation = self
            .take_provider()
            .expect("observation identity and stager remain paired");
        let observation = match observation {
            super::super::ActiveObservation::Compaction(marker) => {
                return self.seal_compaction_marker(route, permit, marker);
            }
            super::super::ActiveObservation::Durable(observation) => observation,
        };
        if permit.compaction().is_some() {
            observation.stager.abandon();
            return self.failed_permit(route, permit);
        }
        let (home_generation, storage) =
            match self.publish_source_activation(&permit, point_limit()) {
                Ok(authority) => authority,
                Err(error) if error.authority().is_some() => {
                    permit.settle_authority_lost();
                    return self.authority_lost_terminal();
                }
                Err(_) => {
                    observation.stager.abandon();
                    return self.failed_permit(route, permit);
                }
            };
        if observation.home_generation != home_generation {
            observation.stager.abandon();
            return self.failed_permit(route, permit);
        }
        let mut commit = self.committer(
            observation.identity,
            observation.home_generation,
            &observation.storage,
        );
        let sealed = match observation.stager.seal(&mut commit) {
            Ok(syndic_storage::ProviderObservationSealOutcome::Committed {
                value,
                later_failure: None,
                ..
            }) => value,
            Ok(syndic_storage::ProviderObservationSealOutcome::Indeterminate {
                custody, ..
            }) => {
                custody.install();
                drop(permit);
                return self.reject(
                    OrderedTurnStreamOperation::ProviderSeal(route),
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
            Ok(syndic_storage::ProviderObservationSealOutcome::NotCommitted { .. })
            | Ok(syndic_storage::ProviderObservationSealOutcome::Committed {
                later_failure: Some(_),
                ..
            }) => {
                drop(permit);
                return self.reject(
                    OrderedTurnStreamOperation::ProviderSeal(route),
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
            Err(error) => {
                drop(permit);
                return self.reject(
                    OrderedTurnStreamOperation::ProviderSeal(route),
                    staging_rejection(&error),
                );
            }
        };
        let trailing = syndic_storage::ProviderObservationRoute::new(
            route.thread_id().clone(),
            route.turn_id().clone(),
        );
        let bound = match sealed.bind(permit.admitted_route(), trailing) {
            Ok(bound) => bound,
            Err(_) => return self.failed_permit(route, permit),
        };
        let verification = match self.live_command().enter_current_home(
            &self.home,
            self.home_id,
            self.home_generation,
        ) {
            Ok(verification) => verification,
            Err(_) => {
                bound.abandon();
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
        };
        let prepared = super::super::super::consumer::prepare(
            &self.home,
            &storage,
            permit.syndic_thread_id(),
            bound,
            point_limit(),
            &self.cancelled,
        );
        if verification.settle_after_operation().is_err() {
            permit.settle_authority_lost();
            return self.authority_lost_terminal();
        }
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(_) => {
                let _ = self.live_command().observe_persistent_failure();
                return self.failed_permit(route, permit);
            }
        };
        let publication = super::super::super::consumer::publish(
            &self.home,
            self.home_id,
            home_generation,
            &storage,
            prepared,
            point_limit(),
            &self.cancelled,
            self.live_command(),
        );
        match publication {
            Ok(()) => {}
            Err(error) if error.custody_installed() => {
                drop(permit);
                return self.reject(
                    OrderedTurnStreamOperation::ProviderSeal(route),
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
            Err(error) if error.authority().is_some() => {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            Err(_) => {
                let _ = self.live_command().observe_persistent_failure();
                return self.failed_permit(route, permit);
            }
        };
        self.finish_provider_publication(route, permit)
    }

    pub(in super::super) fn finish_provider_publication(
        &mut self,
        route: beryl_backend::ProviderObservationRoute,
        permit: SourcePublicationPermit,
    ) -> (super::super::BrokerReply, bool) {
        match permit.finish_held() {
            Ok(post_commit) => {
                post_commit.release();
                (
                    super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                    false,
                )
            }
            Err(SourcePublicationFinishError::Target(invalidation)) => {
                self.provider_target_failure(route, Some(invalidation))
            }
            Err(SourcePublicationFinishError::Router) => self.reject(
                OrderedTurnStreamOperation::ProviderSeal(route),
                OrderedTurnStreamRejection::InvalidControl,
            ),
        }
    }

    pub(in super::super) fn failed_permit(
        &mut self,
        route: beryl_backend::ProviderObservationRoute,
        permit: SourcePublicationPermit,
    ) -> (super::super::BrokerReply, bool) {
        if self.exact_persistent_failure() {
            drop(permit);
            return (
                super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                true,
            );
        }
        match permit.fail() {
            Ok(invalidation) | Err(SourcePublicationFinishError::Target(invalidation)) => {
                self.provider_target_failure(route, Some(invalidation))
            }
            Err(SourcePublicationFinishError::Router) => self.reject(
                OrderedTurnStreamOperation::ProviderSeal(route),
                OrderedTurnStreamRejection::InvalidControl,
            ),
        }
    }

    fn provider_target_failure(
        &mut self,
        _route: beryl_backend::ProviderObservationRoute,
        invalidation: Option<TargetInvalidation>,
    ) -> (super::super::BrokerReply, bool) {
        if let Some(observation) = self.take_provider() {
            observation.abandon();
        }
        let outcome = match invalidation {
            Some(invalidation) => self.invalidate_target(invalidation),
            None => TargetRouteOutcome::TargetFailure,
        };
        (
            super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            outcome == TargetRouteOutcome::Terminal,
        )
    }

    pub(in super::super) fn reject(
        &mut self,
        operation: OrderedTurnStreamOperation,
        reason: OrderedTurnStreamRejection,
    ) -> (super::super::BrokerReply, bool) {
        self.abandon_active();
        self.retire();
        (
            super::super::BrokerReply::Rejected(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(reason),
            ),
            true,
        )
    }
}

fn point_limit() -> syndic_storage::SyndicPointReadLimit {
    syndic_storage::SyndicPointReadLimit::new(super::super::super::PROVIDER_POINT_READ_BYTES)
        .expect("provider broker point-read bound is nonzero")
}
