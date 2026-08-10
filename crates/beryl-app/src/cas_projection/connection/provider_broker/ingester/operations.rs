mod publication;
mod staging;

#[cfg(test)]
use beryl_backend::{OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause};
#[cfg(test)]
use staging::staging_rejection;
#[cfg(test)]
use syndic_storage::{ProviderObservationStageBatchError, ProviderObservationStagingError};

#[cfg(test)]
pub(super) mod tests {
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
