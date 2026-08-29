use beryl_backend::{
    CheckedUserMessage, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamRejection, UserMessageEchoLifecycle,
};
use beryl_home_store::CursorReadLimits;
use syndic_storage::{
    CanonicalItemKind, CasItemSource, CasTurnSource, ContentReference, ProviderFrameOrdinalV1,
    ProviderItemFrameV1, ProviderItemLifecycle, ProviderItemObservationV1, ProviderItemV1,
    ProviderLifecycleTimestampMsV1, ProviderSubmittedContentV1, ProviderUserMessageV1,
    SealedProviderFrameReference, SyndicPointReadLimit, SyndicStorage, TurnItemOrdinal,
};

use super::{Ingester, TargetRouteOutcome, activation::SourceActivationError};
use crate::cas_projection::{
    connection::router::{
        SourcePublicationFinishError, SourcePublicationPermit, SourcePublicationPermitError,
        TargetInvalidation,
    },
    live_source::{LiveSourcePublicationError, LiveSourceTarget},
    provider_frame::{self, ProviderFramePublication},
};

const SUBMITTED_ITEM_PAGE_BYTES: usize = 4 * 1024;

struct SubmittedUserFrame {
    item_id: beryl_model::SyndicItemId,
    content: ContentReference,
    prior: Option<SealedProviderFrameReference>,
    ordinal: ProviderFrameOrdinalV1,
}

#[derive(Debug)]
enum CheckedUserPreparationError {
    Authority(crate::cas_projection::LiveCommandAdmissionError),
    Activation(SourceActivationError),
    LiveSource(LiveSourcePublicationError),
    Read(syndic_storage::SyndicReadError),
    Reacquire(beryl_home_store::DomainHandleError),
    Target,
}

impl CheckedUserPreparationError {
    fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Authority(source) => Some(*source),
            Self::Activation(source) => source.authority(),
            _ => None,
        }
    }
}

impl Ingester {
    pub(super) fn checked_user_message(
        &mut self,
        message: CheckedUserMessage,
    ) -> (super::BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject(
                OrderedTurnStreamOperation::CheckedUserMessage(message),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let permit = match self
            .router
            .acquire_source_publication(message.thread_id(), message.turn_id())
        {
            Ok(permit) => permit,
            Err(SourcePublicationPermitError::Unmatched) => {
                return self.checked_user_target_failure(None);
            }
            Err(SourcePublicationPermitError::Target(invalidation)) => {
                return self.checked_user_target_failure(Some(invalidation));
            }
            Err(SourcePublicationPermitError::Router) => {
                return self.reject(
                    OrderedTurnStreamOperation::CheckedUserMessage(message),
                    OrderedTurnStreamRejection::InvalidControl,
                );
            }
        };
        #[cfg(feature = "test-faults")]
        let test_metrics = std::sync::Arc::clone(&self.test_metrics);
        #[cfg(feature = "test-faults")]
        let _publication_activity = test_metrics.begin_checked_user_publication();
        let limit = point_limit();
        let (home_generation, storage) = match self
            .publish_source_activation(&permit, limit)
            .map_err(CheckedUserPreparationError::Activation)
        {
            Ok(authority) => authority,
            Err(error) if error.authority().is_some() => {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            Err(_) => return self.failed_checked_user_permit(permit, message),
        };
        let verification =
            match self
                .live_command()
                .enter_current_home(&self.home, self.home_id, home_generation)
            {
                Ok(verification) => verification,
                Err(_) => {
                    permit.settle_authority_lost();
                    return self.authority_lost_terminal();
                }
            };
        let prepared: Result<(LiveSourceTarget, SubmittedUserFrame), CheckedUserPreparationError> =
            (|| {
                let target = LiveSourceTarget::resolve(
                    &self.home,
                    &storage,
                    permit.syndic_thread_id(),
                    permit.cas_thread_id(),
                    permit.cas_turn_id(),
                    limit,
                )
                .map_err(CheckedUserPreparationError::LiveSource)?;
                #[cfg(feature = "test-faults")]
                {
                    crate::cas_projection::test_faults::pause_checked_user_publication(
                        crate::cas_projection::test_faults::ProviderTestKey::new(
                            self.home_id,
                            std::sync::Arc::as_ptr(&self.cancelled) as usize,
                        ),
                        message.lifecycle(),
                    );
                }
                let frame =
                    self.submitted_user_frame(&message, &permit, &target, &storage, limit)?;
                Ok((target, frame))
            })();
        if verification.settle_after_operation().is_err() {
            permit.settle_authority_lost();
            return self.authority_lost_terminal();
        }
        let (target, frame) = match prepared {
            Ok(prepared) => prepared,
            Err(error) if error.authority().is_some() => {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            Err(_) => return self.failed_checked_user_permit(permit, message),
        };
        if let Err(error) = publish_checked_user_frame(
            &self.home,
            self.home_id,
            home_generation,
            &storage,
            target,
            &message,
            frame,
            limit,
            self.live_command(),
        ) {
            if error.authority().is_some() {
                permit.settle_authority_lost();
                return self.authority_lost_terminal();
            }
            return self.failed_checked_user_permit(permit, message);
        }
        self.finish_checked_user_permit(permit, message)
    }

    fn submitted_user_frame(
        &self,
        message: &CheckedUserMessage,
        permit: &SourcePublicationPermit,
        target: &LiveSourceTarget,
        storage: &SyndicStorage,
        limit: SyndicPointReadLimit,
    ) -> Result<SubmittedUserFrame, CheckedUserPreparationError> {
        match message.lifecycle() {
            UserMessageEchoLifecycle::Started => {
                let turn_id = permit
                    .pending_syndic_turn_id()
                    .ok_or(CheckedUserPreparationError::Target)?;
                if turn_id != target.turn_id() {
                    return Err(CheckedUserPreparationError::Target);
                }
                let limits = CursorReadLimits::new(2, SUBMITTED_ITEM_PAGE_BYTES)
                    .expect("checked-user cursor bounds are nonzero");
                let items = storage
                    .turn_items(&self.home, turn_id, None, limits)
                    .map_err(CheckedUserPreparationError::Read)?;
                if items.has_more() || items.records().len() != 1 {
                    return Err(CheckedUserPreparationError::Target);
                }
                let index = &items.records()[0];
                let item = storage
                    .canonical_item(&self.home, index.item_id(), limit)
                    .map_err(CheckedUserPreparationError::Read)?
                    .ok_or(CheckedUserPreparationError::Target)?;
                let confirmed_items = storage
                    .turn_items(&self.home, turn_id, None, limits)
                    .map_err(CheckedUserPreparationError::Read)?;
                let confirmed_item = storage
                    .canonical_item(&self.home, index.item_id(), limit)
                    .map_err(CheckedUserPreparationError::Read)?;
                if confirmed_items != items || confirmed_item.as_ref() != Some(&item) {
                    return Err(CheckedUserPreparationError::Target);
                }
                let record = &item;
                let content = record
                    .presentation_content()
                    .ok_or(CheckedUserPreparationError::Target)?;
                if index.turn_id() != turn_id
                    || index.ordinal() != TurnItemOrdinal::FIRST
                    || record.id() != index.item_id()
                    || record.turn_id() != turn_id
                    || record.ordinal() != TurnItemOrdinal::FIRST
                    || record.revision() != index.item_revision()
                    || record.kind() != CanonicalItemKind::UserInput
                    || record.source_event().is_some()
                    || record.cas_source().is_some()
                    || record.provider().is_some()
                {
                    return Err(CheckedUserPreparationError::Target);
                }
                Ok(SubmittedUserFrame {
                    item_id: record.id(),
                    content,
                    prior: None,
                    ordinal: ProviderFrameOrdinalV1::FIRST,
                })
            }
            UserMessageEchoLifecycle::Completed => {
                let source = CasItemSource::new(
                    CasTurnSource::new(message.thread_id().clone(), message.turn_id().clone()),
                    message.correlation().item_id().clone(),
                );
                let captured = storage
                    .capture_item(&self.home, &source, limit)
                    .map_err(CheckedUserPreparationError::Read)?
                    .ok_or(CheckedUserPreparationError::Target)?;
                let record = captured.item();
                let content = record
                    .presentation_content()
                    .ok_or(CheckedUserPreparationError::Target)?;
                if record.turn_id() != target.turn_id()
                    || record.kind() != CanonicalItemKind::UserInput
                    || record.provider_lifecycle() != ProviderItemLifecycle::Started
                {
                    return Err(CheckedUserPreparationError::Target);
                }
                Ok(SubmittedUserFrame {
                    item_id: record.id(),
                    content,
                    prior: Some(
                        record
                            .provider()
                            .cloned()
                            .ok_or(CheckedUserPreparationError::Target)?,
                    ),
                    ordinal: ProviderFrameOrdinalV1::new(2)
                        .expect("checked-user completion ordinal is nonzero"),
                })
            }
        }
    }

    fn finish_checked_user_permit(
        &mut self,
        permit: SourcePublicationPermit,
        message: CheckedUserMessage,
    ) -> (super::BrokerReply, bool) {
        match permit.finish() {
            Ok(()) => (
                super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                false,
            ),
            Err(SourcePublicationFinishError::Target(invalidation)) => {
                self.checked_user_target_failure(Some(invalidation))
            }
            Err(SourcePublicationFinishError::Router) => self.reject(
                OrderedTurnStreamOperation::CheckedUserMessage(message),
                OrderedTurnStreamRejection::InvalidControl,
            ),
        }
    }

    fn failed_checked_user_permit(
        &mut self,
        permit: SourcePublicationPermit,
        message: CheckedUserMessage,
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
                self.checked_user_target_failure(Some(invalidation))
            }
            Err(SourcePublicationFinishError::Router) => self.reject(
                OrderedTurnStreamOperation::CheckedUserMessage(message),
                OrderedTurnStreamRejection::InvalidControl,
            ),
        }
    }

    fn checked_user_target_failure(
        &mut self,
        invalidation: Option<TargetInvalidation>,
    ) -> (super::BrokerReply, bool) {
        let outcome = match invalidation {
            Some(invalidation) => self.invalidate_target(invalidation),
            None => TargetRouteOutcome::TargetFailure,
        };
        (
            super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            outcome == TargetRouteOutcome::Terminal,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn publish_checked_user_frame(
    home: &beryl_home_store::HomeStore,
    home_id: beryl_model::BerylHomeId,
    home_generation: beryl_home_store::HomeGeneration,
    storage: &SyndicStorage,
    target: LiveSourceTarget,
    message: &CheckedUserMessage,
    submitted: SubmittedUserFrame,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<(), crate::cas_projection::provider_frame::ProviderFramePublicationError> {
    let item = ProviderItemV1::UserMessage(ProviderUserMessageV1 {
        client_id: None,
        submitted: ProviderSubmittedContentV1 {
            content: submitted.content,
        },
    });
    let observed_at = ProviderLifecycleTimestampMsV1::new(message.timestamp().get());
    let observation = match message.lifecycle() {
        UserMessageEchoLifecycle::Started => {
            ProviderItemObservationV1::Started { observed_at, item }
        }
        UserMessageEchoLifecycle::Completed => {
            ProviderItemObservationV1::Completed { observed_at, item }
        }
    };
    provider_frame::publish_frame(
        home,
        home_id,
        home_generation,
        storage,
        ProviderFramePublication {
            target,
            item_id: submitted.item_id,
            cas_item_id: message.correlation().item_id().clone(),
            frame: ProviderItemFrameV1::new(
                submitted.ordinal,
                message.correlation().item_id().clone(),
                observation,
            ),
            prior: submitted.prior,
        },
        limit,
        command,
    )
    .map(|_| ())
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(super::super::PROVIDER_POINT_READ_BYTES)
        .expect("provider broker point-read bound is nonzero")
}

#[cfg(all(test, feature = "test-faults"))]
pub(super) mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/provider_broker_checked_user.rs"
    ));
}
