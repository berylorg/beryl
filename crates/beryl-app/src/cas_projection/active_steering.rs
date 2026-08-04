//! One bounded exact active-steering delivery attempt.

mod model;
mod outcome;
mod predispatch;
mod publication;
mod settle;
mod target;
#[cfg(test)]
mod test_support;
mod worker;

pub(in crate::cas_projection) use model::{
    ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome, ActiveSteeringPreparationFailure,
    ActiveSteeringRetryCause, ActiveSteeringUnknownCause,
};
pub(in crate::cas_projection) use target::ActiveSteeringTarget;
#[cfg(test)]
pub(in crate::cas_projection) use test_support::{DeliveryPause, pause_delivery_if_requested};
#[cfg(test)]
pub(in crate::cas_projection) use worker::deliver;
pub(in crate::cas_projection) use worker::deliver_prepared;

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/active_steering_delivery.rs"
    ));
}
