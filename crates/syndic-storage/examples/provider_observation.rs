use std::{io, path::PathBuf};

use beryl_home_store::{
    CommandError, CommandOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::ProviderObservationId;
use syndic_storage::{
    ProviderField, ProviderObservationBegin, ProviderObservationControl,
    ProviderObservationItemKind, ProviderObservationItemLifecycle, ProviderObservationStageBatch,
    ProviderObservationStager, ProviderObservationStagingBytes, ProviderScalar,
    ProviderValueContext, SyndicStorage,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("usage: provider_observation <home-path>"))?;
    let mut home = HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))?;
    let syndic = SyndicStorage::register(&mut home)?;
    // The callback returns HomeStore's exact outcome without collapsing it into Result.
    let mut commit = |batch: &ProviderObservationStageBatch| -> CommandOutcome {
        home.execute_current(syndic.current_stage_provider_observation_batch(batch.clone()))
    };

    let identity = ProviderObservationId::from_bytes([7; 16]);
    let mut staging = match ProviderObservationStager::begin(
        identity,
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Completed,
            kind: ProviderObservationItemKind::ContextCompaction,
        },
        &mut commit,
    )? {
        syndic_storage::ProviderObservationStageOutcome::NotCommitted { evidence } => {
            return Err(Box::new(ExampleOutcome::NotCommitted(evidence)));
        }
        syndic_storage::ProviderObservationStageOutcome::Committed {
            value,
            receipt,
            later_failure,
        } => {
            println!("begin committed: {receipt:?}");
            if let Some(failure) = later_failure {
                return Err(Box::new(ExampleOutcome::CommittedLaterFailure(failure)));
            }
            value
        }
        syndic_storage::ProviderObservationStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            return Err(Box::new(ExampleOutcome::Indeterminate(failure)));
        }
    };
    match staging.control(
        ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
            value: ProviderScalar::Unsigned(42),
        },
        &mut commit,
    )? {
        syndic_storage::ProviderObservationStageOutcome::NotCommitted { evidence } => {
            return Err(Box::new(ExampleOutcome::NotCommitted(evidence)));
        }
        syndic_storage::ProviderObservationStageOutcome::Committed {
            receipt,
            later_failure,
            ..
        } => {
            println!("control committed: {receipt:?}");
            if let Some(failure) = later_failure {
                return Err(Box::new(ExampleOutcome::CommittedLaterFailure(failure)));
            }
        }
        syndic_storage::ProviderObservationStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            return Err(Box::new(ExampleOutcome::Indeterminate(failure)));
        }
    }
    let item = ProviderValueContext::Field(ProviderField::ItemId);
    match staging.control(ProviderObservationControl::BeginField(item), &mut commit)? {
        syndic_storage::ProviderObservationStageOutcome::NotCommitted { evidence } => {
            return Err(Box::new(ExampleOutcome::NotCommitted(evidence)));
        }
        syndic_storage::ProviderObservationStageOutcome::Committed {
            receipt,
            later_failure,
            ..
        } => {
            println!("begin field committed: {receipt:?}");
            if let Some(failure) = later_failure {
                return Err(Box::new(ExampleOutcome::CommittedLaterFailure(failure)));
            }
        }
        syndic_storage::ProviderObservationStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            return Err(Box::new(ExampleOutcome::Indeterminate(failure)));
        }
    }
    match staging.fragment(
        ProviderObservationStagingBytes::new(item, b"provider-item")?,
        &mut commit,
    )? {
        syndic_storage::ProviderObservationStageOutcome::NotCommitted { evidence } => {
            return Err(Box::new(ExampleOutcome::NotCommitted(evidence)));
        }
        syndic_storage::ProviderObservationStageOutcome::Committed {
            receipt,
            later_failure,
            ..
        } => {
            println!("fragment committed: {receipt:?}");
            if let Some(failure) = later_failure {
                return Err(Box::new(ExampleOutcome::CommittedLaterFailure(failure)));
            }
        }
        syndic_storage::ProviderObservationStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            return Err(Box::new(ExampleOutcome::Indeterminate(failure)));
        }
    }
    match staging.control(ProviderObservationControl::EndField(item), &mut commit)? {
        syndic_storage::ProviderObservationStageOutcome::NotCommitted { evidence } => {
            return Err(Box::new(ExampleOutcome::NotCommitted(evidence)));
        }
        syndic_storage::ProviderObservationStageOutcome::Committed {
            receipt,
            later_failure,
            ..
        } => {
            println!("end field committed: {receipt:?}");
            if let Some(failure) = later_failure {
                return Err(Box::new(ExampleOutcome::CommittedLaterFailure(failure)));
            }
        }
        syndic_storage::ProviderObservationStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            return Err(Box::new(ExampleOutcome::Indeterminate(failure)));
        }
    }
    let sealed = match staging.seal(&mut commit)? {
        syndic_storage::ProviderObservationSealOutcome::NotCommitted { evidence } => {
            return Err(Box::new(ExampleOutcome::NotCommitted(evidence)));
        }
        syndic_storage::ProviderObservationSealOutcome::Committed {
            value,
            receipt,
            later_failure,
        } => {
            println!("seal committed: {receipt:?}");
            if let Some(failure) = later_failure {
                return Err(Box::new(ExampleOutcome::CommittedLaterFailure(failure)));
            }
            value
        }
        syndic_storage::ProviderObservationSealOutcome::Indeterminate { failure, custody } => {
            custody.install();
            return Err(Box::new(ExampleOutcome::Indeterminate(failure)));
        }
    };
    assert_eq!(sealed.identity(), identity);

    home.close()?;
    Ok(())
}

#[derive(Debug)]
enum ExampleOutcome {
    NotCommitted(CommandError),
    CommittedLaterFailure(CommandError),
    Indeterminate(CommandError),
}

impl std::fmt::Display for ExampleOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotCommitted(evidence) => write!(formatter, "stage did not commit: {evidence}"),
            Self::CommittedLaterFailure(failure) => {
                write!(formatter, "stage committed before later failure: {failure}")
            }
            Self::Indeterminate(failure) => write!(
                formatter,
                "stage outcome is indeterminate after reconciliation custody installation: {failure}"
            ),
        }
    }
}

impl std::error::Error for ExampleOutcome {}
