use serde::{Deserialize, Serialize};

/// Explicit last-observed availability of a durable configured identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Availability {
    /// No authoritative observation has been made for the current generation.
    Unknown,
    /// The identity was available at its owning boundary's current observation.
    Available,
    /// The identity was unavailable for one bounded reason category.
    Unavailable(UnavailableReason),
}

/// Bounded reason categories shared across runtime, root, and catalog facts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum UnavailableReason {
    /// The configured object was not found.
    NotFound,
    /// The configured object exists but access was denied.
    AccessDenied,
    /// The declared host or WSL environment is unavailable.
    EnvironmentUnavailable,
    /// The configured backend process or connection is unavailable.
    BackendUnavailable,
    /// The Beryl-home state gate prevents the operation.
    StoreUnavailable,
    /// Another main window owns the exact thread.
    OpenElsewhere,
    /// The platform or provider cannot support the required operation.
    Unsupported,
    /// The durable identity exists but failed current validation.
    Invalid,
}
