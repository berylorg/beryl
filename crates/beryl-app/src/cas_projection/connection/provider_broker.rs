//! Capacity-one ordered ingestion for feature operations and provider observations.

mod approval;
mod channel;
mod consumer;
mod ingester;
mod loss;
mod staging;
mod steering_result;
mod translation;

#[cfg(test)]
pub(super) use ingester::fail_next_provider_broker_join_for_test;
pub(super) use ingester::{
    PreparedProviderBroker, ProviderBroker, ProviderBrokerControl,
    ProviderBrokerResponseActivationFailure, ProviderBrokerStartToken,
    ProviderBrokerTerminalReceipt, RunningProviderBrokerIngester,
    StartBlockedProviderBrokerIngester,
};
pub(in crate::cas_projection) use ingester::{
    ProviderBrokerAdoptionStopped, ProviderBrokerStopped,
};
pub(in crate::cas_projection) use loss::ActiveBindingLossDisposition;
pub(super) use loss::DetachedActivationAuthority;
pub(in crate::cas_projection) use loss::{ProviderBrokerLossError, ProviderBrokerLossOutcome};
#[cfg(test)]
pub(in crate::cas_projection) use steering_result::CheckedSteeringLifecycle;
pub(in crate::cas_projection) use steering_result::{
    CheckedSteeringLifecycleArmError, CheckedSteeringLifecycleOwner,
    CheckedSteeringLifecycleWaitError,
};
const PROVIDER_PAGE_BYTES: usize = 65_536;
const BROKER_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(20);
const PROVIDER_POINT_READ_BYTES: usize = 1_000_000;
