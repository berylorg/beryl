use super::PublishedRunningThreadsReader;
use crate::cas_projection::{ResourceConstructionError, ResourceConstructionStep};
use crate::syndic_transcript::{IndexedResourceValue, ResourceDeliveryResponse};
use crate::transcript_provider::{
    PreparedResourceDelivery, TranscriptAttachmentError, TranscriptProviderReader,
    TranscriptResourceRequest,
};
use beryl_home_store::{CommandOutcome, ReconciliationResolution};
use std::sync::atomic::AtomicBool;
use syndic_storage::indexed_resource as index;

pub(crate) enum RunningResourceConstruction {
    Step(ResourceConstructionStep),
    Failed(ResourceConstructionError),
    Reconciled {
        failure: beryl_home_store::CommandError,
        handle: beryl_home_store::ReconciliationHandle,
        resolution: Result<ReconciliationResolution, beryl_home_store::ReconciliationFailure>,
    },
}

pub(crate) struct PreparedRunningResource {
    pub(crate) prepared: PreparedResourceDelivery,
    pub(crate) retry_pending: bool,
    pub(crate) construction: Option<RunningResourceConstruction>,
}

impl PublishedRunningThreadsReader {
    pub(crate) fn prepare_running_resource(
        &self,
        provider: &TranscriptProviderReader,
        request: TranscriptResourceRequest,
        cancelled: &AtomicBool,
    ) -> Result<PreparedRunningResource, TranscriptAttachmentError> {
        if !self.current() {
            return Err(TranscriptAttachmentError::Stale);
        }
        let prepared = provider.prepare_resource_delivery(request.clone(), cancelled)?;
        let pending = matches!(prepared.response(), ResourceDeliveryResponse::Indexed(response)
            if matches!(response.value, IndexedResourceValue::Pending));
        if !pending {
            return Ok(PreparedRunningResource {
                prepared,
                retry_pending: false,
                construction: None,
            });
        }
        let inner = request
            .demand
            .indexed
            .as_ref()
            .ok_or(TranscriptAttachmentError::Identity)?;
        let key = index::BuildKey {
            resource: index::ResourceKey {
                id: request
                    .demand
                    .resource_id
                    .0
                    .parse()
                    .map_err(|_| TranscriptAttachmentError::Identity)?,
                revision: inner.revision,
            },
            operation: [0; 16],
        };
        drop(prepared);
        let construction = match self.step_resource_construction(key, cancelled) {
            Ok(ResourceConstructionStep::Command(CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            })) => {
                let handle = reconciliation.install_and_handle();
                let resolution = self.home.reconcile(&handle);
                RunningResourceConstruction::Reconciled {
                    failure,
                    handle,
                    resolution,
                }
            }
            Ok(step) => RunningResourceConstruction::Step(step),
            Err(error) => RunningResourceConstruction::Failed(error),
        };
        let retry_pending = matches!(
            &construction,
            RunningResourceConstruction::Step(ResourceConstructionStep::Command(
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            )) | RunningResourceConstruction::Step(ResourceConstructionStep::PeerProgress(_))
                | RunningResourceConstruction::Failed(ResourceConstructionError::Busy)
                | RunningResourceConstruction::Reconciled {
                    resolution: Ok(ReconciliationResolution::ExactNew { .. }
                        | ReconciliationResolution::ExactSuccessor { .. }
                        | ReconciliationResolution::ExactOld),
                    ..
                }
        );
        if !self.current() {
            return Err(TranscriptAttachmentError::Stale);
        }
        let prepared = provider.prepare_resource_delivery(request, cancelled)?;
        Ok(PreparedRunningResource {
            prepared,
            retry_pending,
            construction: Some(construction),
        })
    }
}
