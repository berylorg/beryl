use std::path::Path;

use beryl_model::BerylHomeId;

use crate::{
    DomainAttachmentAccessError, DomainAttachmentCapability, DomainHandle, DomainHandleError,
    DomainRegistrationError, HomeCloseError, HomeDurabilityTier, HomeGeneration,
    HomeHealthSnapshot, HomeOpenError, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    StorageDomain, domain::StoreInstanceId, fault::FaultController, health::FailureSeverity,
};

mod error;
mod requirements;

pub use error::{HomeCandidateError, HomeCandidateFailure, HomeDomainRequirementsError};
pub use requirements::HomeDomainRequirements;

#[cfg(feature = "test-faults")]
pub fn with_initial_candidate_store<R>(
    candidate: &HomeOpenCandidate,
    callback: impl FnOnce(&HomeStore) -> R,
) -> R {
    callback(&candidate.initial.store)
}

#[cfg(feature = "test-faults")]
pub fn with_initial_publication_store<R>(
    publication: &HomeOpenPublication,
    callback: impl FnOnce(&HomeStore) -> R,
) -> R {
    callback(&publication.initial.store)
}

#[derive(Debug)]
pub struct HomeOpenCandidate {
    initial: InitialHome,
}

#[derive(Debug)]
pub struct HomeOpenPublication {
    initial: InitialHome,
    requirements: HomeDomainRequirements,
}

#[derive(Debug)]
struct InitialHome {
    store: HomeStore,
    instance: StoreInstanceId,
    generation: HomeGeneration,
}

impl InitialHome {
    fn new(store: HomeStore) -> Self {
        let instance = store
            .generation
            .read()
            .expect("new home generation lock is not poisoned")
            .as_ref()
            .expect("new home generation is present")
            .instance_id;
        let generation = store
            .health()
            .generation()
            .expect("new home generation has an identity");
        Self {
            store,
            instance,
            generation,
        }
    }

    fn validate(&self, requirements: &HomeDomainRequirements) -> Result<(), HomeCandidateError> {
        let admission = self.store.health.admit_opening()?;
        let generation = self
            .store
            .generation
            .read()
            .map_err(|_| HomeCandidateError::GenerationUnavailable)?;
        let generation = generation
            .as_ref()
            .ok_or(HomeCandidateError::GenerationUnavailable)?;
        if generation.instance_id != self.instance || admission.generation() != self.generation {
            return Err(HomeCandidateError::GenerationMismatch);
        }
        requirements.validate(&generation.registry)?;
        admission.confirm_database(&generation.database, |source| {
            HomeCandidateError::StorageHealth {
                source: Box::new(source),
            }
        })
    }

    fn fail(&self) {
        self.store
            .health
            .signal_failure(FailureSeverity::Structural);
    }

    fn domain_handle<D: StorageDomain>(&self) -> Result<DomainHandle<D>, DomainHandleError> {
        self.store
            .domain_handle_admitted(self.store.health.admit_opening()?)
    }

    fn with_domain_attachment<D: StorageDomain, R>(
        &self,
        capability: &DomainAttachmentCapability<D>,
        callback: impl FnOnce(&D::RuntimeAttachment) -> R,
    ) -> Result<R, DomainAttachmentAccessError> {
        self.store.with_domain_attachment_admitted(
            self.store.health.admit_opening()?,
            capability,
            callback,
        )
    }
}

impl HomeOpenCandidate {
    pub fn open(options: HomeOpenOptions) -> Result<Self, HomeOpenError> {
        HomeStore::open_initial(options, FaultController::new()).map(|store| Self {
            initial: InitialHome::new(store),
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn open_with_faults(
        options: HomeOpenOptions,
        faults: FaultController,
    ) -> Result<Self, HomeOpenError> {
        HomeStore::open_initial(options, faults).map(|store| Self {
            initial: InitialHome::new(store),
        })
    }

    pub fn home_id(&self) -> BerylHomeId {
        self.initial.store.home_id()
    }

    pub fn schema(&self) -> HomeSchemaVersion {
        self.initial.store.schema()
    }

    pub fn generation(&self) -> HomeGeneration {
        self.initial.generation
    }

    pub fn health(&self) -> HomeHealthSnapshot {
        self.initial.store.health()
    }

    pub fn configured_path(&self) -> &Path {
        self.initial.store.configured_path()
    }

    pub fn canonical_path(&self) -> &Path {
        self.initial.store.canonical_path()
    }

    pub fn database_path(&self) -> &Path {
        self.initial.store.database_path()
    }

    pub fn durability_tier(&self) -> HomeDurabilityTier {
        self.initial.store.durability_tier()
    }

    pub fn register_domain<D: StorageDomain>(
        &mut self,
    ) -> Result<DomainHandle<D>, DomainRegistrationError> {
        self.register::<D>(false)
    }

    pub fn register_domain_with_schema_validation<D: StorageDomain>(
        &mut self,
    ) -> Result<DomainHandle<D>, DomainRegistrationError> {
        self.register::<D>(true)
    }

    fn register<D: StorageDomain>(
        &mut self,
        validate_schema: bool,
    ) -> Result<DomainHandle<D>, DomainRegistrationError> {
        let result = self
            .initial
            .store
            .register_initial_domain::<D>(validate_schema);
        if result.is_err() {
            self.initial.fail();
        }
        result
    }

    pub fn domain_handle<D: StorageDomain>(&self) -> Result<DomainHandle<D>, DomainHandleError> {
        self.initial.domain_handle()
    }

    pub fn with_domain_attachment<D: StorageDomain, R>(
        &self,
        capability: &DomainAttachmentCapability<D>,
        callback: impl FnOnce(&D::RuntimeAttachment) -> R,
    ) -> Result<R, DomainAttachmentAccessError> {
        self.initial.with_domain_attachment(capability, callback)
    }

    pub fn prepare_publication(
        self,
        requirements: HomeDomainRequirements,
    ) -> Result<HomeOpenPublication, HomeCandidateFailure<Self>> {
        match self.initial.validate(&requirements) {
            Ok(()) => Ok(HomeOpenPublication {
                initial: self.initial,
                requirements,
            }),
            Err(error) => {
                self.initial.fail();
                Err(HomeCandidateFailure::new(error, self))
            }
        }
    }

    pub fn close(self) -> Result<(), HomeCloseError> {
        self.initial.fail();
        self.initial.store.close()
    }
}

impl HomeOpenPublication {
    pub fn recovery_access(
        &mut self,
    ) -> Result<crate::HomeCandidateRecoveryAccess<'_>, HomeCandidateError> {
        crate::HomeCandidateRecoveryAccess::new(
            &self.initial.store,
            crate::HomeHealthState::Opening,
            self.initial.generation,
        )
    }

    pub fn home_id(&self) -> BerylHomeId {
        self.initial.store.home_id()
    }

    pub fn generation(&self) -> HomeGeneration {
        self.initial.generation
    }

    pub fn health(&self) -> HomeHealthSnapshot {
        self.initial.store.health()
    }

    pub fn durability_tier(&self) -> HomeDurabilityTier {
        self.initial.store.durability_tier()
    }

    pub fn domain_handle<D: StorageDomain>(&self) -> Result<DomainHandle<D>, DomainHandleError> {
        self.initial.domain_handle()
    }

    pub fn with_domain_attachment<D: StorageDomain, R>(
        &self,
        capability: &DomainAttachmentCapability<D>,
        callback: impl FnOnce(&D::RuntimeAttachment) -> R,
    ) -> Result<R, DomainAttachmentAccessError> {
        self.initial.with_domain_attachment(capability, callback)
    }

    pub fn publish(self) -> Result<HomeStore, HomeCandidateFailure<Self>> {
        let result = self.initial.validate(&self.requirements).and_then(|()| {
            let count = self.initial.store.reconciliation.pending_scope_count();
            if count != 0 {
                return Err(HomeCandidateError::PendingReconciliation { count });
            }
            self.initial
                .store
                .health
                .publish_opening(self.initial.generation)
                .map_err(HomeCandidateError::from)
        });
        match result {
            Ok(()) => Ok(self.initial.store),
            Err(error) => {
                if !matches!(error, HomeCandidateError::PendingReconciliation { .. }) {
                    self.initial.fail();
                }
                Err(HomeCandidateFailure::new(error, self))
            }
        }
    }

    pub fn close(self) -> Result<(), HomeCloseError> {
        self.initial.fail();
        self.initial.store.close()
    }
}
