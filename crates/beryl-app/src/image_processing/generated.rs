mod header;
mod scheduler;
mod coordinator;
mod source;
mod publication;
#[cfg(feature = "test-faults")]
pub mod qualification;

pub(crate) use coordinator::{GeneratedMediaCoordinator, GeneratedMediaCoordinatorError};

#[cfg(test)]
#[path = "../../tests/unit/generated_media.rs"]
mod tests;
