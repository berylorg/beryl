use beryl_home_store::MutationContribution;
use beryl_model::{
    AcceptedInputRevision, DomainRevision, InputGateRevision, SyndicAcceptedInputId, SyndicThreadId,
};

use crate::SyndicStorage;

mod transition;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AcceptedInputDeliveryTransition {
    thread_id: SyndicThreadId,
    expected_gate_revision: InputGateRevision,
    input_id: SyndicAcceptedInputId,
    expected_input_revision: AcceptedInputRevision,
}

impl AcceptedInputDeliveryTransition {
    const fn new(
        thread_id: SyndicThreadId,
        expected_gate_revision: InputGateRevision,
        input_id: SyndicAcceptedInputId,
        expected_input_revision: AcceptedInputRevision,
    ) -> Self {
        Self {
            thread_id,
            expected_gate_revision,
            input_id,
            expected_input_revision,
        }
    }
}

macro_rules! delivery_transition {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name(AcceptedInputDeliveryTransition);

        impl $name {
            /// Constructs one exact accepted-input delivery transition request.
            #[must_use]
            pub const fn new(
                thread_id: SyndicThreadId,
                expected_gate_revision: InputGateRevision,
                input_id: SyndicAcceptedInputId,
                expected_input_revision: AcceptedInputRevision,
            ) -> Self {
                Self(AcceptedInputDeliveryTransition::new(
                    thread_id,
                    expected_gate_revision,
                    input_id,
                    expected_input_revision,
                ))
            }

            #[must_use]
            pub const fn thread_id(self) -> SyndicThreadId {
                self.0.thread_id
            }

            #[must_use]
            pub const fn expected_gate_revision(self) -> InputGateRevision {
                self.0.expected_gate_revision
            }

            #[must_use]
            pub const fn input_id(self) -> SyndicAcceptedInputId {
                self.0.input_id
            }

            #[must_use]
            pub const fn expected_input_revision(self) -> AcceptedInputRevision {
                self.0.expected_input_revision
            }
        }
    };
}

delivery_transition!(
    /// Exact revisions for claiming one undispatched accepted steering input for delivery.
    BeginAcceptedInputDelivery
);
delivery_transition!(
    /// Exact revisions for returning one proven-not-dispatched attempt to retryable work.
    RetryAcceptedInputDelivery
);
delivery_transition!(
    /// Exact revisions for recording one authoritative accepted steering response.
    CompleteAcceptedInputDelivery
);

/// Exact revisions for moving one rejected steering attempt to next-turn work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SteeringRejection {
    thread_id: SyndicThreadId,
    expected_gate_revision: InputGateRevision,
    input_id: SyndicAcceptedInputId,
    expected_input_revision: AcceptedInputRevision,
}

impl SteeringRejection {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_gate_revision: InputGateRevision,
        input_id: SyndicAcceptedInputId,
        expected_input_revision: AcceptedInputRevision,
    ) -> Self {
        Self {
            thread_id,
            expected_gate_revision,
            input_id,
            expected_input_revision,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_gate_revision(self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn input_id(self) -> SyndicAcceptedInputId {
        self.input_id
    }

    #[must_use]
    pub const fn expected_input_revision(self) -> AcceptedInputRevision {
        self.expected_input_revision
    }

    const fn transition(self) -> AcceptedInputDeliveryTransition {
        AcceptedInputDeliveryTransition::new(
            self.thread_id,
            self.expected_gate_revision,
            self.input_id,
            self.expected_input_revision,
        )
    }
}

impl SyndicStorage {
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
                transition: rejection.transition(),
                kind: AcceptedInputDeliveryTransitionKind::Rejected,
            },
        )
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
}

#[derive(Clone, Copy)]
enum AcceptedInputDeliveryTransitionKind {
    Begin,
    Retry,
    Complete,
    Rejected,
}

struct AcceptedInputDeliveryMutation {
    transition: AcceptedInputDeliveryTransition,
    kind: AcceptedInputDeliveryTransitionKind,
}
