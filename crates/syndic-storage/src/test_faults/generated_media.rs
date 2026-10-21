use crate::{
    codec::Family,
    generated_media::{SourceValue, codec::*},
};

pub fn generated_media_source_codec_roundtrip_for_test(
    key: &[u8],
    payload: &[u8],
) -> Option<(Vec<u8>, Vec<u8>)> {
    let key = SourcesFamily::decode_key(key).ok()?;
    let value = SourcesFamily::decode_value(payload).ok()?;
    if !matches!(
        (&key, &value),
        (SourceKey::Allocator, SourceValue::Allocator(_))
            | (SourceKey::Resource(_), SourceValue::Source(_))
            | (SourceKey::Owner(_), SourceValue::Owner(_))
    ) {
        return None;
    }
    Some((
        SourcesFamily::encode_key(&key).ok()?,
        SourcesFamily::encode_value(&value).ok()?,
    ))
}

pub fn generated_media_pending_codec_roundtrip_for_test(
    key: &[u8],
    payload: &[u8],
) -> Option<(Vec<u8>, Vec<u8>)> {
    let key = PendingFamily::decode_key(key).ok()?;
    let resource = PendingFamily::decode_value(payload).ok()?;
    Some((
        PendingFamily::encode_key(&key).ok()?,
        PendingFamily::encode_value(&resource).ok()?,
    ))
}

pub fn generated_media_source_event_payload_for_test(event: &crate::SourceEventRecord) -> Vec<u8> {
    crate::codec::SourceEventsFamily::encode_value(event).unwrap()
}

pub fn generated_media_resource_codec_for_test(record: &crate::ResourceMetadataRecord) -> Vec<u8> {
    crate::codec::ResourcesFamily::encode_value(record).unwrap()
}

pub fn generated_projection_set_codec_for_test(record: &crate::ItemProjectionSetRecord) -> Vec<u8> {
    crate::codec::ItemProjectionSetsFamily::encode_value(record).unwrap()
}

pub fn decode_generated_projection_set_for_test(
    payload: &[u8],
) -> Option<crate::ItemProjectionSetRecord> {
    crate::codec::ItemProjectionSetsFamily::decode_value(payload).ok()
}

pub fn generated_projection_codec_for_test(record: &crate::ProjectionRecord) -> Vec<u8> {
    crate::codec::ProjectionsFamily::encode_value(record).unwrap()
}

pub fn decode_generated_projection_for_test(payload: &[u8]) -> Option<crate::ProjectionRecord> {
    crate::codec::ProjectionsFamily::decode_value(payload).ok()
}

pub fn generated_projection_set_record_version_for_test() -> u32 {
    crate::codec::ItemProjectionSetsFamily::RECORD_VERSION.get()
}

pub fn decode_generated_media_resource_for_test(
    payload: &[u8],
) -> Option<crate::ResourceMetadataRecord> {
    crate::codec::ResourcesFamily::decode_value(payload).ok()
}

pub fn replace_generated_source_owner_for_test(
    storage: &crate::SyndicStorage,
    revision: beryl_model::DomainRevision,
    snapshot: beryl_model::SyndicExecutionSnapshotId,
    owner: Option<beryl_model::GeneratedSourceIncarnation>,
) -> beryl_home_store::MutationContribution {
    storage
        .handle
        .contribution(revision, OwnerFixture { snapshot, owner })
}

struct OwnerFixture {
    snapshot: beryl_model::SyndicExecutionSnapshotId,
    owner: Option<beryl_model::GeneratedSourceIncarnation>,
}

impl beryl_home_store::DomainMutation<crate::domain::SyndicDomain> for OwnerFixture {
    type Error = crate::SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _: &beryl_home_store::DomainReader<'_, crate::domain::SyndicDomain>,
    ) -> Result<Self, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<
            '_,
            crate::domain::SyndicDomain,
        >,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<SourcesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self,
        mutations: &mut beryl_home_store::MutationBuilder<'_, crate::domain::SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let key = SourceKey::Owner(prepared.snapshot);
        match prepared.owner {
            Some(owner) => mutations.put::<SourcesCodec>(&key, &SourceValue::Owner(owner))?,
            None => mutations.delete::<SourcesCodec>(&key)?,
        }
        Ok(())
    }
}
