use crate::mutation::{point, required};
use crate::{
    CanonicalItemKind, ItemProjectionGeneration, ItemProjectionHeadRecord,
    ItemProjectionIndexRecord, ItemProjectionSetRecord, ItemProjectionSource,
    ProjectionFormatVersion, ProjectionLifecycle, ProjectionOrdinal, ProjectionPayload,
    ProjectionRecord, SyndicMutationError, SyndicStorage, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{
    CurrentDomainCommand, DomainMutation, DomainReader, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{DomainRevision, ProjectionRevision, SyndicItemId, SyndicResourceId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishGeneratedResourceProjection {
    item_id: SyndicItemId,
    expected_item_revision: ProjectionRevision,
    resource_id: SyndicResourceId,
    generation: ItemProjectionGeneration,
}

impl PublishGeneratedResourceProjection {
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        expected_item_revision: ProjectionRevision,
        resource_id: SyndicResourceId,
        generation: ItemProjectionGeneration,
    ) -> Self {
        Self {
            item_id,
            expected_item_revision,
            resource_id,
            generation,
        }
    }
}

impl SyndicStorage {
    #[must_use]
    pub fn current_publish_generated_resource_projection(
        &self,
        request: PublishGeneratedResourceProjection,
    ) -> CurrentDomainCommand {
        self.handle.current_command(PublishMutation { request })
    }

    #[must_use]
    pub fn publish_generated_resource_projection(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishGeneratedResourceProjection,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, PublishMutation { request })
    }
}

struct PublishMutation {
    request: PublishGeneratedResourceProjection,
}

struct Publication {
    projection: ProjectionRecord,
    membership: ItemProjectionIndexRecord,
    set: ItemProjectionSetRecord,
    head: ItemProjectionHeadRecord,
}

impl DomainMutation<SyndicDomain> for PublishMutation {
    type Error = SyndicMutationError;
    type Prepared = Publication;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let request = self.request;
        let item = required::<CanonicalItemsFamily>(reader, &request.item_id)?;
        let source = ItemProjectionSource::GeneratedResource(request.resource_id);
        if item.revision() != request.expected_item_revision
            || item.kind() != CanonicalItemKind::GeneratedMedia
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        super::lifecycle::validate_item_projection_source(reader, &item, source)?;
        let head = point::<ItemProjectionHeadsFamily>(reader, &item.id())?;
        if head.is_some_and(|head| {
            head.lifecycle() == ProjectionLifecycle::Current
                && head.source_item_revision() == item.revision()
        }) {
            return Err(SyndicMutationError::ProjectionAlreadyCurrent);
        }
        if super::lifecycle::latest_build(reader, item.id())?.is_some() {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        let latest = super::lifecycle::latest_set(reader, item.id())?;
        let expected = latest
            .as_ref()
            .map(|set| set.generation().checked_next())
            .transpose()?
            .unwrap_or(ItemProjectionGeneration::FIRST);
        if request.generation != expected
            || latest.as_ref().is_some_and(|set| {
                set.source() != source || set.source_item_revision() >= item.revision()
            })
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        let format = ProjectionFormatVersion::V1;
        let (id, revision) = crate::projection::generated_resource_projection_identity(
            format,
            item.id(),
            request.resource_id,
        );
        let ordinal = ProjectionOrdinal::new(1)?;
        let projection = ProjectionRecord::new(
            id,
            revision,
            item.id(),
            item.turn_id(),
            ordinal,
            ProjectionPayload::GeneratedResource {
                resource_id: request.resource_id,
            },
        );
        if point::<ProjectionsFamily>(reader, &id)?.is_some_and(|existing| existing != projection) {
            return Err(SyndicMutationError::ProjectionIdentityCollision);
        }
        let digest = crate::projection::advance_item_set_digest(
            crate::projection::item_set_digest_seed(),
            id,
            revision,
        );
        let set = ItemProjectionSetRecord::generated_resource(
            item.id(),
            request.generation,
            format,
            item.revision(),
            request.resource_id,
            digest,
        );
        let head_revision = head.map_or(
            Ok(ProjectionRevision::new(1).expect("initial head revision")),
            |head| head.revision().checked_next(),
        )?;
        Ok(Publication {
            projection,
            membership: ItemProjectionIndexRecord::new(
                item.id(),
                request.generation,
                ordinal,
                id,
                revision,
            ),
            set,
            head: ItemProjectionHeadRecord::new(
                item.id(),
                head_revision,
                item.revision(),
                request.generation,
                ProjectionLifecycle::Current,
            ),
        })
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ProjectionsCodec>(1)?;
        reservation.reserve_records::<ItemProjectionsCodec>(1)?;
        reservation.reserve_records::<ItemProjectionSetsCodec>(1)?;
        reservation.reserve_records::<ItemProjectionHeadsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ProjectionsCodec>(&prepared.projection.id(), &prepared.projection)?;
        mutations.put::<ItemProjectionsCodec>(
            &ItemProjectionKey {
                item: prepared.set.item_id(),
                generation: prepared.set.generation(),
                ordinal: prepared.membership.ordinal(),
            },
            &prepared.membership,
        )?;
        mutations.put::<ItemProjectionSetsCodec>(
            &ItemProjectionSetKey {
                item: prepared.set.item_id(),
                generation: prepared.set.generation(),
            },
            &prepared.set,
        )?;
        mutations.put::<ItemProjectionHeadsCodec>(&prepared.head.item_id(), &prepared.head)?;
        Ok(())
    }
}
