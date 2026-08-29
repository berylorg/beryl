use std::collections::HashSet;

use beryl_model::DomainRevision;

use super::*;

impl DomainBlueprint {
    pub(super) fn for_domain<D: StorageDomain>() -> Result<Self, DomainDefinitionError> {
        validate_component("domain", D::NAME)?;
        if D::FAMILIES.is_empty() {
            return Err(DomainDefinitionError::NoKeyspaces { domain: D::NAME });
        }

        let mut seen = HashSet::new();
        let mut families = Vec::with_capacity(D::FAMILIES.len());
        for family in D::FAMILIES {
            validate_component("keyspace family", family.name())?;
            if !seen.insert(family.name()) {
                return Err(DomainDefinitionError::DuplicateKeyspace {
                    domain: D::NAME,
                    family: family.name(),
                });
            }
            if family.max_key_bytes() == 0
                || family.max_key_bytes() > u16::MAX.into()
                || family.max_stored_value_bytes() < 4
                || family.max_stored_value_bytes() > u32::MAX as usize
            {
                return Err(DomainDefinitionError::InvalidRecordCodec {
                    domain: D::NAME,
                    family: family.name(),
                });
            }
            families.push(FamilyBlueprint {
                logical_name: family.name(),
                physical_name: physical_name(D::NAME, family.name()),
                schema: family.schema(),
                codec_type: family.codec_type(),
                max_key_bytes: family.max_key_bytes(),
                max_stored_value_bytes: family.max_stored_value_bytes(),
                validate_envelope: family.envelope_validator(),
            });
        }
        families.sort_by(|left, right| left.logical_name.cmp(right.logical_name));

        Ok(Self {
            name: D::NAME,
            schema: D::SCHEMA_VERSION,
            owner: DomainOwnerId::of::<D>(),
            attachment_type: TypeId::of::<D::RuntimeAttachment>(),
            attachment_factory: attachment::RuntimeAttachmentSlot::construct::<D>,
            families,
            reopen_validator: validate_reopen_typed::<D>,
            reconciler: reconcile_typed::<D>,
        })
    }

    pub(super) fn initial_metadata(&self) -> DomainMetadata {
        self.metadata(DomainRevision::new(1).expect("one is a valid initial revision"))
    }
}

fn validate_component(kind: &'static str, name: &str) -> Result<(), DomainDefinitionError> {
    let valid = !name.is_empty()
        && name.len() <= MAX_COMPONENT_BYTES
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
        });
    if valid {
        Ok(())
    } else {
        Err(DomainDefinitionError::InvalidName {
            kind,
            name: name.to_owned(),
        })
    }
}

fn physical_name(domain: &str, family: &str) -> String {
    format!("d.{domain}.{family}")
}

fn reconcile_typed<D: StorageDomain>(
    snapshot: &fjall::Snapshot,
    domain: &RegisteredDomain,
    descriptor: &crate::command::MaterializedDomainDescriptor,
) -> Result<DomainReconciliation, callback::ErasedCallbackError> {
    D::reconcile(&ReconciliationReader::new(snapshot, domain, descriptor))
        .map_err(callback::ErasedCallbackError::from_typed)
}

fn validate_reopen_typed<D: StorageDomain>(
    snapshot: &fjall::Snapshot,
    domain: &RegisteredDomain,
    sidecars: &crate::SidecarVerifier<'_>,
) -> Result<(), callback::ErasedCallbackError> {
    D::validate_reopen(&DomainReader::new(snapshot, domain), sidecars)
        .map_err(callback::ErasedCallbackError::from_typed)
}
