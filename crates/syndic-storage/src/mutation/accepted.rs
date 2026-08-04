use beryl_home_store::{CurrentDomainCommand, MutationContribution};
use beryl_model::{AcceptedInputRevision, DomainRevision, SyndicAcceptedInputId, SyndicThreadId};

use crate::{AcceptedRouteLeafTransitionKind, SteeringTargetProof, SyndicStorage};

mod transition;

#[derive(Clone, Debug, Eq, PartialEq)]
struct AcceptedInputDeliveryTransition {
    thread_id: SyndicThreadId,
    input_id: SyndicAcceptedInputId,
    expected_input_revision: AcceptedInputRevision,
    target: SteeringTargetProof,
}

impl AcceptedInputDeliveryTransition {
    fn new(
        thread_id: SyndicThreadId,
        input_id: SyndicAcceptedInputId,
        expected_input_revision: AcceptedInputRevision,
        target: SteeringTargetProof,
    ) -> Self {
        Self {
            thread_id,
            input_id,
            expected_input_revision,
            target,
        }
    }
}

macro_rules! delivery_transition {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name(AcceptedInputDeliveryTransition);

        impl $name {
            /// Constructs one stable accepted-input delivery transition intent.
            #[must_use]
            pub fn new(
                thread_id: SyndicThreadId,
                input_id: SyndicAcceptedInputId,
                expected_input_revision: AcceptedInputRevision,
                target: SteeringTargetProof,
            ) -> Self {
                Self(AcceptedInputDeliveryTransition::new(
                    thread_id,
                    input_id,
                    expected_input_revision,
                    target,
                ))
            }

            #[must_use]
            pub const fn thread_id(&self) -> SyndicThreadId {
                self.0.thread_id
            }

            #[must_use]
            pub const fn input_id(&self) -> SyndicAcceptedInputId {
                self.0.input_id
            }

            #[must_use]
            pub const fn expected_input_revision(&self) -> AcceptedInputRevision {
                self.0.expected_input_revision
            }

            /// Returns the exact durable CAS steering target authorized by this intent.
            #[must_use]
            pub const fn target(&self) -> &SteeringTargetProof {
                &self.0.target
            }
        }
    };
}

delivery_transition!(
    /// Stable intent for claiming one undispatched accepted steering input for delivery.
    BeginAcceptedInputDelivery
);
delivery_transition!(
    /// Stable intent for returning one proven-not-dispatched attempt to retryable work.
    RetryAcceptedInputDelivery
);
delivery_transition!(
    /// Stable intent for recording one authoritative accepted steering response.
    CompleteAcceptedInputDelivery
);

delivery_transition!(
    /// Stable intent for moving one rejected steering attempt to next-turn work.
    SteeringRejection
);

impl SyndicStorage {
    /// Claims one exact live steering route through the current physical domain revision.
    #[must_use]
    pub fn current_begin_accepted_input_delivery(
        &self,
        request: BeginAcceptedInputDelivery,
    ) -> CurrentDomainCommand {
        self.current_delivery_transition(request.0, AcceptedInputDeliveryTransitionKind::Begin)
    }

    /// Claims one exact live steering route before dispatching its provider request.
    #[must_use]
    pub fn begin_accepted_input_delivery(
        &self,
        expected_domain_revision: DomainRevision,
        request: BeginAcceptedInputDelivery,
    ) -> MutationContribution {
        self.delivery_transition(
            expected_domain_revision,
            request.0,
            AcceptedInputDeliveryTransitionKind::Begin,
        )
    }

    /// Restores retry authority after the exact request was proven not dispatched.
    #[must_use]
    pub fn retry_accepted_input_delivery(
        &self,
        expected_domain_revision: DomainRevision,
        request: RetryAcceptedInputDelivery,
    ) -> MutationContribution {
        self.delivery_transition(
            expected_domain_revision,
            request.0,
            AcceptedInputDeliveryTransitionKind::Retry,
        )
    }

    /// Restores retry authority through the current physical domain revision.
    #[must_use]
    pub fn current_retry_accepted_input_delivery(
        &self,
        request: RetryAcceptedInputDelivery,
    ) -> CurrentDomainCommand {
        self.current_delivery_transition(request.0, AcceptedInputDeliveryTransitionKind::Retry)
    }

    /// Records one authoritative successful steering response.
    #[must_use]
    pub fn complete_accepted_input_delivery(
        &self,
        expected_domain_revision: DomainRevision,
        request: CompleteAcceptedInputDelivery,
    ) -> MutationContribution {
        self.delivery_transition(
            expected_domain_revision,
            request.0,
            AcceptedInputDeliveryTransitionKind::Complete,
        )
    }

    /// Records exact provider acceptance through the current physical domain revision.
    #[must_use]
    pub fn current_complete_accepted_input_delivery(
        &self,
        request: CompleteAcceptedInputDelivery,
    ) -> CurrentDomainCommand {
        self.current_delivery_transition(request.0, AcceptedInputDeliveryTransitionKind::Complete)
    }

    /// Preserves one accepted identity after CAS rejects its exact steering attempt.
    #[must_use]
    pub fn record_steering_rejection(
        &self,
        expected_domain_revision: DomainRevision,
        rejection: SteeringRejection,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            AcceptedInputDeliveryMutation {
                transition: rejection.0,
                kind: AcceptedInputDeliveryTransitionKind::Rejected,
            },
        )
    }

    /// Records one closed steering rejection through the current physical domain revision.
    #[must_use]
    pub fn current_record_steering_rejection(
        &self,
        rejection: SteeringRejection,
    ) -> CurrentDomainCommand {
        self.current_delivery_transition(rejection.0, AcceptedInputDeliveryTransitionKind::Rejected)
    }

    fn delivery_transition(
        &self,
        expected_domain_revision: DomainRevision,
        transition: AcceptedInputDeliveryTransition,
        kind: AcceptedInputDeliveryTransitionKind,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            AcceptedInputDeliveryMutation { transition, kind },
        )
    }

    fn current_delivery_transition(
        &self,
        transition: AcceptedInputDeliveryTransition,
        kind: AcceptedInputDeliveryTransitionKind,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(AcceptedInputDeliveryMutation { transition, kind })
    }
}

#[derive(Clone, Copy)]
enum AcceptedInputDeliveryTransitionKind {
    Begin,
    Retry,
    Complete,
    Rejected,
}

impl AcceptedInputDeliveryTransitionKind {
    const fn persisted(self) -> AcceptedRouteLeafTransitionKind {
        match self {
            Self::Begin => AcceptedRouteLeafTransitionKind::Begin,
            Self::Retry => AcceptedRouteLeafTransitionKind::Retry,
            Self::Complete => AcceptedRouteLeafTransitionKind::Complete,
            Self::Rejected => AcceptedRouteLeafTransitionKind::SteeringRejected,
        }
    }
}

struct AcceptedInputDeliveryMutation {
    transition: AcceptedInputDeliveryTransition,
    kind: AcceptedInputDeliveryTransitionKind,
}
