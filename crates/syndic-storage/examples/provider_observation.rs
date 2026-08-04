use std::{io, path::PathBuf};

use beryl_home_store::{CommandError, HomeOpenOptions, HomeSchemaVersion, HomeStore};
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
    let mut commit = |batch: &ProviderObservationStageBatch| -> Result<(), CommandError> {
        home.execute_current(syndic.current_stage_provider_observation_batch(batch.clone()))?;
        Ok(())
    };

    let identity = ProviderObservationId::from_bytes([7; 16]);
    let mut staging = ProviderObservationStager::begin(
        identity,
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Completed,
            kind: ProviderObservationItemKind::ContextCompaction,
        },
        &mut commit,
    )?;
    staging.control(
        ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
            value: ProviderScalar::Unsigned(42),
        },
        &mut commit,
    )?;
    let item = ProviderValueContext::Field(ProviderField::ItemId);
    staging.control(ProviderObservationControl::BeginField(item), &mut commit)?;
    staging.fragment(
        ProviderObservationStagingBytes::new(item, b"provider-item")?,
        &mut commit,
    )?;
    staging.control(ProviderObservationControl::EndField(item), &mut commit)?;
    let sealed = staging.seal(&mut commit)?;
    assert_eq!(sealed.identity(), identity);

    home.close()?;
    Ok(())
}
