use std::{
    any::{Any, TypeId},
    error::Error,
    fmt,
    marker::PhantomData,
};

use thiserror::Error;

use super::{DomainHandle, DomainOwnerId, StorageDomain, StoreInstanceId};
use crate::{HomeStore, store::StoreGeneration};

pub trait DomainRuntimeAttachment: Send + Sync + 'static {
    fn retire(&mut self) {}
}

impl DomainRuntimeAttachment for () {}

pub struct DomainAttachmentCapability<D: StorageDomain> {
    pub(crate) store: StoreInstanceId,
    pub(crate) slot: usize,
    pub(crate) owner: DomainOwnerId,
    pub(crate) attachment_type: TypeId,
    _domain: PhantomData<fn(D) -> D>,
}

impl<D: StorageDomain> Clone for DomainAttachmentCapability<D> {
    fn clone(&self) -> Self {
        Self {
            store: self.store,
            slot: self.slot,
            owner: self.owner,
            attachment_type: self.attachment_type,
            _domain: PhantomData,
        }
    }
}

#[cfg(feature = "test-faults")]
pub fn capability_with_test_attachment_type<D: StorageDomain, A: DomainRuntimeAttachment>(
    capability: &DomainAttachmentCapability<D>,
) -> DomainAttachmentCapability<D> {
    DomainAttachmentCapability {
        store: capability.store,
        slot: capability.slot,
        owner: capability.owner,
        attachment_type: TypeId::of::<A>(),
        _domain: PhantomData,
    }
}

impl<D: StorageDomain> fmt::Debug for DomainAttachmentCapability<D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DomainAttachmentCapability")
            .field("domain", &D::NAME)
            .finish_non_exhaustive()
    }
}

impl<D: StorageDomain> DomainHandle<D> {
    #[must_use]
    pub fn attachment_capability(&self) -> DomainAttachmentCapability<D> {
        DomainAttachmentCapability {
            store: self.store,
            slot: self.slot,
            owner: self.owner,
            attachment_type: TypeId::of::<D::RuntimeAttachment>(),
            _domain: PhantomData,
        }
    }
}

#[derive(Debug, Error)]
pub enum DomainAttachmentAccessError {
    #[error(transparent)]
    HealthGate(#[from] crate::HealthGateError),
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    #[error("the runtime attachment capability is stale or belongs to another home generation")]
    StaleOrForeign,
    #[error("domain `{domain}` is registered to another Rust owner type")]
    OwnerTypeMismatch { domain: &'static str },
    #[error("domain `{domain}` is registered with another runtime attachment type")]
    AttachmentTypeMismatch { domain: &'static str },
    #[error("domain `{domain}` runtime attachment access is closed")]
    AccessClosed { domain: &'static str },
    #[error("runtime attachment access could not confirm storage health: {source}")]
    StorageHealth {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

pub(crate) type ErasedAttachmentFactory =
    fn() -> Result<RuntimeAttachmentSlot, Box<dyn Error + Send + Sync>>;

trait ErasedRuntimeAttachment: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn retire(&mut self);
}

impl<T: DomainRuntimeAttachment> ErasedRuntimeAttachment for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn retire(&mut self) {
        DomainRuntimeAttachment::retire(self);
    }
}

pub(crate) struct RuntimeAttachmentSlot {
    attachment_type: TypeId,
    attachment: Option<Box<dyn ErasedRuntimeAttachment>>,
}

impl RuntimeAttachmentSlot {
    pub(crate) fn construct<D: StorageDomain>() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let attachment = D::create_runtime_attachment()
            .map_err(|source| Box::new(source) as Box<dyn Error + Send + Sync>)?;
        Ok(Self {
            attachment_type: TypeId::of::<D::RuntimeAttachment>(),
            attachment: Some(Box::new(attachment)),
        })
    }

    pub(crate) const fn attachment_type(&self) -> TypeId {
        self.attachment_type
    }

    pub(crate) fn get<D: StorageDomain>(
        &self,
    ) -> Result<&D::RuntimeAttachment, DomainAttachmentAccessError> {
        if self.attachment_type != TypeId::of::<D::RuntimeAttachment>() {
            return Err(DomainAttachmentAccessError::AttachmentTypeMismatch { domain: D::NAME });
        }
        let attachment = self
            .attachment
            .as_ref()
            .ok_or(DomainAttachmentAccessError::AccessClosed { domain: D::NAME })?;
        attachment
            .as_any()
            .downcast_ref::<D::RuntimeAttachment>()
            .ok_or(DomainAttachmentAccessError::AttachmentTypeMismatch { domain: D::NAME })
    }

    pub(crate) fn retire(&mut self) {
        if let Some(mut attachment) = self.attachment.take() {
            attachment.retire();
            drop(attachment);
        }
    }
}

impl Drop for RuntimeAttachmentSlot {
    fn drop(&mut self) {
        self.retire();
    }
}

impl StoreGeneration {
    pub(crate) fn with_domain_attachment<D: StorageDomain, R>(
        &self,
        capability: &DomainAttachmentCapability<D>,
        callback: impl FnOnce(&D::RuntimeAttachment) -> R,
    ) -> Result<R, DomainAttachmentAccessError> {
        if capability.store != self.instance_id {
            return Err(DomainAttachmentAccessError::StaleOrForeign);
        }
        if capability.owner != DomainOwnerId::of::<D>() {
            return Err(DomainAttachmentAccessError::OwnerTypeMismatch { domain: D::NAME });
        }
        if capability.attachment_type != TypeId::of::<D::RuntimeAttachment>() {
            return Err(DomainAttachmentAccessError::AttachmentTypeMismatch { domain: D::NAME });
        }
        let domain = self
            .registry
            .get(capability.slot)
            .ok_or(DomainAttachmentAccessError::StaleOrForeign)?;
        if domain.name != D::NAME || domain.schema != D::SCHEMA_VERSION {
            return Err(DomainAttachmentAccessError::StaleOrForeign);
        }
        if domain.owner != capability.owner {
            return Err(DomainAttachmentAccessError::OwnerTypeMismatch { domain: D::NAME });
        }
        if domain.attachment.attachment_type() != capability.attachment_type {
            return Err(DomainAttachmentAccessError::AttachmentTypeMismatch { domain: D::NAME });
        }
        let attachment = domain.attachment.get::<D>()?;
        Ok(callback(attachment))
    }
}

impl HomeStore {
    pub fn with_domain_attachment<D: StorageDomain, R>(
        &self,
        capability: &DomainAttachmentCapability<D>,
        callback: impl FnOnce(&D::RuntimeAttachment) -> R,
    ) -> Result<R, DomainAttachmentAccessError> {
        let admission = self.health.admit()?;
        let generation = self
            .generation
            .read()
            .map_err(|_| DomainAttachmentAccessError::GenerationPoisoned)?;
        let generation = generation
            .as_ref()
            .ok_or(DomainAttachmentAccessError::StaleOrForeign)?;
        admission.confirm_database(&generation.database, |source| {
            DomainAttachmentAccessError::StorageHealth {
                source: Box::new(source),
            }
        })?;
        generation.with_domain_attachment(capability, callback)
    }
}
