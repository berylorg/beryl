use beryl_backend::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSubmitCause, ProviderObservationFragment,
};
use syndic_storage::{
    ProviderCompactionMarkerStager, ProviderObservationStageBatchError, ProviderObservationStager,
    ProviderObservationStagingBytes, ProviderObservationStagingError,
};

use super::super::staging::StageCommitError;
use super::{Ingester, TargetRouteOutcome};
use crate::cas_projection::connection::router::{
    SourcePublicationFinishError, SourcePublicationPermit, SourcePublicationPermitError,
    TargetInvalidation,
};

fn staging_rejection<E>(error: &ProviderObservationStagingError<E>) -> OrderedTurnStreamRejection
where
    E: std::error::Error + Send + Sync + 'static,
{
    match error {
        ProviderObservationStagingError::Validation(_)
        | ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::EmptyFragment
            | ProviderObservationStageBatchError::FragmentTooLarge { .. },
        ) => OrderedTurnStreamRejection::SchemaMismatch,
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::InvalidTransition
            | ProviderObservationStageBatchError::FrontierOverflow
            | ProviderObservationStageBatchError::ReplayMismatch,
        )
        | ProviderObservationStagingError::Record(_)
        | ProviderObservationStagingError::Callback(_) => {
            OrderedTurnStreamRejection::StagingConflict
        }
    }
}

fn staging_authority(
    error: &ProviderObservationStagingError<StageCommitError>,
) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
    match error {
        ProviderObservationStagingError::Callback(source) => source.authority(),
        _ => None,
    }
}

enum ProviderStagingOutcome {
    Rejection(OrderedTurnStreamRejection),
    AuthorityLost,
}

impl Ingester {
    pub(super) fn abandon_provider(
        &mut self,
        reason: beryl_backend::ProviderObservationAbandonReason,
    ) -> (super::BrokerReply, bool) {
        let Some(observation) = self.take_provider() else {
            return self.reject(
                OrderedTurnStreamOperation::ProviderAbandon(reason),
                OrderedTurnStreamRejection::InvalidControl,
            );
        };
        observation.abandon();
        (
            super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            false,
        )
    }

    pub(super) fn begin(
        &mut self,
        begin: beryl_backend::ProviderObservationBegin,
    ) -> (super::BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderBegin(begin),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let translated = super::super::translation::begin(begin);
        if matches!(
            translated,
            syndic_storage::ProviderObservationBegin::Item {
                kind: syndic_storage::ProviderObservationItemKind::ContextCompaction,
                ..
            }
        ) {
            return match ProviderCompactionMarkerStager::begin(translated) {
                Ok(marker) => {
                    self.put_provider(super::ActiveObservation::Compaction(marker));
                    (
                        super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                        false,
                    )
                }
                Err(_) => self.reject(
                    OrderedTurnStreamOperation::ProviderBegin(begin),
                    OrderedTurnStreamRejection::SchemaMismatch,
                ),
            };
        }
        let Some((home_generation, storage)) = self.current_observation_authority() else {
            return self.reject(
                OrderedTurnStreamOperation::ProviderBegin(begin),
                OrderedTurnStreamRejection::StagingConflict,
            );
        };
        let mut bytes = [0_u8; 16];
        if getrandom::fill(&mut bytes).is_err() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderBegin(begin),
                OrderedTurnStreamRejection::StagingConflict,
            );
        }
        let identity = beryl_model::ProviderObservationId::from_bytes(bytes);
        let mut commit = self.committer(identity, home_generation, storage);
        match ProviderObservationStager::begin(identity, translated, &mut commit) {
            Ok(observation) => {
                self.put_provider(super::ActiveObservation::Durable(
                    super::ActiveDurableObservation {
                        stager: observation,
                        identity,
                        home_generation,
                        storage,
                    },
                ));
                (
                    super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                    false,
                )
            }
            Err(error) if staging_authority(&error).is_some() => self.authority_lost_terminal(),
            Err(error) => self.reject(
                OrderedTurnStreamOperation::ProviderBegin(begin),
                staging_rejection(&error),
            ),
        }
    }

    pub(super) fn control(
        &mut self,
        control: beryl_backend::ProviderObservationControl,
    ) -> (super::BrokerReply, bool) {
        let Some(mut observation) = self.take_provider() else {
            return self.reject(
                OrderedTurnStreamOperation::ProviderControl(control),
                OrderedTurnStreamRejection::InvalidControl,
            );
        };
        let translated = super::super::translation::control(control);
        let result = match &mut observation {
            super::ActiveObservation::Durable(observation) => {
                let mut commit = self.committer(
                    observation.identity,
                    observation.home_generation,
                    observation.storage,
                );
                observation
                    .stager
                    .control(translated, &mut commit)
                    .map_err(|error| {
                        if staging_authority(&error).is_some() {
                            ProviderStagingOutcome::AuthorityLost
                        } else {
                            ProviderStagingOutcome::Rejection(staging_rejection(&error))
                        }
                    })
            }
            super::ActiveObservation::Compaction(marker) => {
                marker.control(translated).map_err(|_| {
                    ProviderStagingOutcome::Rejection(OrderedTurnStreamRejection::SchemaMismatch)
                })
            }
        };
        match result {
            Ok(()) => {
                self.put_provider(observation);
                (
                    super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                    false,
                )
            }
            Err(ProviderStagingOutcome::AuthorityLost) => self.authority_lost_terminal(),
            Err(ProviderStagingOutcome::Rejection(rejection)) => {
                observation.abandon();
                self.reject(
                    OrderedTurnStreamOperation::ProviderControl(control),
                    rejection,
                )
            }
        }
    }

    pub(super) fn acquire_page(&mut self) -> (super::BrokerReply, bool) {
        if !self.provider_is_active() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderAcquirePage,
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        match self.pages.try_lease() {
            Ok(page) => (
                super::BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                false,
            ),
            Err(_) => (
                super::BrokerReply::Rejected(
                    OrderedTurnStreamOperation::ProviderAcquirePage,
                    OrderedTurnStreamSubmitCause::CapacityFull,
                ),
                true,
            ),
        }
    }

    pub(super) fn fragment(
        &mut self,
        fragment: ProviderObservationFragment,
    ) -> (super::BrokerReply, bool) {
        if !self.provider_is_active() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderFragment(fragment),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let staged = ProviderObservationStagingBytes::new(
            super::super::translation::context(fragment.context()),
            fragment.bytes(),
        );
        let Ok(staged) = staged else {
            return self.reject(
                OrderedTurnStreamOperation::ProviderFragment(fragment),
                OrderedTurnStreamRejection::SchemaMismatch,
            );
        };
        let mut observation = self
            .take_provider()
            .expect("provider fragment requires one active observation");
        let result = match &mut observation {
            super::ActiveObservation::Durable(observation) => {
                let mut commit = self.committer(
                    observation.identity,
                    observation.home_generation,
                    observation.storage,
                );
                observation
                    .stager
                    .fragment(staged, &mut commit)
                    .map_err(|error| {
                        if staging_authority(&error).is_some() {
                            ProviderStagingOutcome::AuthorityLost
                        } else {
                            ProviderStagingOutcome::Rejection(staging_rejection(&error))
                        }
                    })
            }
            super::ActiveObservation::Compaction(marker) => marker.fragment(staged).map_err(|_| {
                ProviderStagingOutcome::Rejection(OrderedTurnStreamRejection::SchemaMismatch)
            }),
        };
        match result {
            Ok(()) => {
                self.put_provider(observation);
                let mut page = fragment.into_lease();
                page.clear();
                (
                    super::BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                    false,
                )
            }
            Err(ProviderStagingOutcome::AuthorityLost) => self.authority_lost_terminal(),
            Err(ProviderStagingOutcome::Rejection(rejection)) => {
                observation.abandon();
                self.reject(
                    OrderedTurnStreamOperation::ProviderFragment(fragment),
                    rejection,
                )
            }
        }
    }

    pub(super) fn seal(
        &mut self,
        route: beryl_backend::ProviderObservationRoute,
    ) -> (super::BrokerReply, bool) {
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
            super::ActiveObservation::Compaction(marker) => {
                return self.seal_compaction_marker(route, permit, marker);
            }
            super::ActiveObservation::Durable(observation) => observation,
        };
        if permit.compaction().is_some() {
            observation.stager.abandon();
            return self.failed_permit(route, permit);
        }
        let activation = loop {
            let verification = match self.live_command().await_current_or_verification(
                &self.home,
                self.home_id,
                self.home_generation,
            ) {
                Ok(verification) => verification,
                Err(_) => {
                    permit.settle_authority_lost();
                    return self.authority_lost_terminal();
                }
            };
            let attempt = self.publish_source_activation(&permit, point_limit());
            match verification.settle_after_operation() {
                Ok(settlement) if settlement.verified_current() => match attempt {
                    Ok(activation) => break Ok(activation),
                    Err(()) => continue,
                },
                Ok(_) => break attempt,
                Err(_) => {
                    permit.settle_authority_lost();
                    return self.authority_lost_terminal();
                }
            }
        };
        let (home_generation, storage) = match activation {
            Ok(authority) => authority,
            Err(()) => {
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
            observation.storage,
        );
        let sealed = match observation.stager.seal(&mut commit) {
            Ok(sealed) => sealed,
            Err(error) if staging_authority(&error).is_some() => {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            Err(_) => {
                drop(permit);
                return self.reject(
                    OrderedTurnStreamOperation::ProviderSeal(route),
                    OrderedTurnStreamRejection::StagingConflict,
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
        let publication = super::super::consumer::consume(
            &self.home,
            self.home_id,
            home_generation,
            storage,
            permit.syndic_thread_id(),
            bound,
            point_limit(),
            &self.cancelled,
            self.live_command(),
        );
        let published_activity = match publication {
            Ok(effect) => effect
                .into_activity()
                .map(|activity| activity.bind(&permit)),
            Err(error) if error.authority().is_some() => {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            Err(_) => {
                let _ = self.live_command().observe_persistent_failure();
                return self.failed_permit(route, permit);
            }
        };
        self.finish_provider_publication(route, permit, published_activity)
    }

    pub(super) fn finish_provider_publication(
        &mut self,
        route: beryl_backend::ProviderObservationRoute,
        permit: SourcePublicationPermit,
        published_activity: Option<super::super::consumer::BoundPublishedHardStopActivity>,
    ) -> (super::BrokerReply, bool) {
        match permit.finish_held() {
            Ok(post_commit) => {
                if let Some(activity) = published_activity {
                    self.stop_coordinator
                        .record_published_activity(activity.into_published());
                }
                post_commit.release();
                (
                    super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
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

    pub(super) fn failed_permit(
        &mut self,
        route: beryl_backend::ProviderObservationRoute,
        permit: SourcePublicationPermit,
    ) -> (super::BrokerReply, bool) {
        if self.exact_persistent_failure() {
            drop(permit);
            return (
                super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
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
    ) -> (super::BrokerReply, bool) {
        if let Some(observation) = self.take_provider() {
            observation.abandon();
        }
        let outcome = match invalidation {
            Some(invalidation) => self.invalidate_target(invalidation),
            None => TargetRouteOutcome::TargetFailure,
        };
        (
            super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            outcome == TargetRouteOutcome::Terminal,
        )
    }

    pub(super) fn reject(
        &mut self,
        operation: OrderedTurnStreamOperation,
        reason: OrderedTurnStreamRejection,
    ) -> (super::BrokerReply, bool) {
        self.abandon_active();
        self.retire();
        (
            super::BrokerReply::Rejected(operation, OrderedTurnStreamSubmitCause::Rejected(reason)),
            true,
        )
    }
}

fn point_limit() -> syndic_storage::SyndicPointReadLimit {
    syndic_storage::SyndicPointReadLimit::new(super::super::PROVIDER_POINT_READ_BYTES)
        .expect("provider broker point-read bound is nonzero")
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_ingester_operations.rs"
    ));

    mod compaction_marker {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/unit/provider_broker_compaction_marker.rs"
        ));
    }
}
