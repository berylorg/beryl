use beryl_home_store::HomeStore;

use crate::{
    CanonicalItemPayload, CanonicalItemRecord, CasItemIndexRecord, CasItemSource, ContentLifecycle,
    ContentManifestRecord, ResourceMetadataRecord, SyndicReadError, codec::*,
    domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

/// One exact record-stabilized CAS item and its optional owned content or resource snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicCaptureItem {
    cas_index: CasItemIndexRecord,
    item: CanonicalItemRecord,
    content: Option<ContentManifestRecord>,
    resource: Option<ResourceMetadataRecord>,
    stored_bytes: usize,
}

impl SyndicCaptureItem {
    #[must_use]
    pub const fn cas_index(&self) -> &CasItemIndexRecord {
        &self.cas_index
    }

    #[must_use]
    pub const fn item(&self) -> &CanonicalItemRecord {
        &self.item
    }

    #[must_use]
    pub const fn content(&self) -> Option<&ContentManifestRecord> {
        self.content.as_ref()
    }

    #[must_use]
    pub const fn resource(&self) -> Option<&ResourceMetadataRecord> {
        self.resource.as_ref()
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}

impl SyndicStorage {
    /// Reads one live-capture item through a CAS-index/item/manifest/CAS-index stability proof.
    ///
    /// Unrelated Syndic mutations do not invalidate the result. A concurrent mutation of this
    /// exact item returns [`SyndicReadError::ConcurrentChange`].
    pub fn capture_item(
        &self,
        store: &HomeStore,
        source: &CasItemSource,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicCaptureItem>, SyndicReadError> {
        let key = CasItemKey::Record(
            source.turn().thread_id().clone(),
            source.turn().turn_id().clone(),
            source.item_id().clone(),
        );
        let Some(first) = self.point::<CasItemIndexFamily>(store, key.clone(), limit)? else {
            return match self.point::<CasItemIndexFamily>(store, key, limit)? {
                None => Ok(None),
                Some(_) => Err(concurrent()),
            };
        };
        let index = first.record().clone();
        let item = self.canonical_item(store, index.item_id(), limit)?.ok_or(
            SyndicReadError::Invariant("live-capture CAS item selects a missing canonical item"),
        )?;
        let content = match item.record().payload().content() {
            Some(content) => Some(self.content_manifest(store, content.id(), limit)?.ok_or(
                SyndicReadError::Invariant(
                    "live-capture canonical item selects a missing content manifest",
                ),
            )?),
            None => None,
        };
        let resource = match item.record().payload() {
            CanonicalItemPayload::GeneratedMedia(resource_id) => Some(
                self.resource(store, *resource_id, limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "live-capture generated item selects a missing resource",
                    ))?,
            ),
            _ => None,
        };
        let second = self
            .point::<CasItemIndexFamily>(store, key, limit)?
            .ok_or_else(concurrent)?;
        if second.record() != &index {
            return Err(concurrent());
        }
        let resource_second = match &resource {
            Some(resource) => Some(
                self.resource(store, resource.record().id(), limit)?
                    .ok_or_else(concurrent)?,
            ),
            None => None,
        };
        if resource_second
            .as_ref()
            .map(super::SyndicStoredRecord::record)
            != resource.as_ref().map(super::SyndicStoredRecord::record)
        {
            return Err(concurrent());
        }
        validate(
            source,
            &index,
            item.record(),
            content.as_ref().map(super::SyndicStoredRecord::record),
            resource.as_ref().map(super::SyndicStoredRecord::record),
        )?;
        let stored_bytes = first
            .stored_bytes()
            .checked_add(item.stored_bytes())
            .and_then(|bytes| {
                bytes.checked_add(
                    content
                        .as_ref()
                        .map_or(0, super::SyndicStoredRecord::stored_bytes),
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    resource
                        .as_ref()
                        .map_or(0, super::SyndicStoredRecord::stored_bytes),
                )
            })
            .and_then(|bytes| bytes.checked_add(second.stored_bytes()))
            .and_then(|bytes| {
                bytes.checked_add(
                    resource_second
                        .as_ref()
                        .map_or(0, super::SyndicStoredRecord::stored_bytes),
                )
            })
            .ok_or(SyndicReadError::Invariant(
                "live-capture item stored-byte accounting overflowed",
            ))?;
        Ok(Some(SyndicCaptureItem {
            cas_index: index,
            item: item.record().clone(),
            content: content.map(|record| record.record().clone()),
            resource: resource.map(|record| record.record().clone()),
            stored_bytes,
        }))
    }
}

fn validate(
    source: &CasItemSource,
    index: &CasItemIndexRecord,
    item: &CanonicalItemRecord,
    content: Option<&ContentManifestRecord>,
    resource: Option<&ResourceMetadataRecord>,
) -> Result<(), SyndicReadError> {
    if index.cas_thread_id() != source.turn().thread_id()
        || index.cas_turn_id() != source.turn().turn_id()
        || index.cas_item_id() != source.item_id()
        || index.item_id() != item.id()
        || index.item_revision() != item.revision()
        || item.cas_source() != Some(source)
    {
        return Err(SyndicReadError::Invariant(
            "live-capture CAS item, canonical item, and content disagree",
        ));
    }
    match (item.payload().content(), content) {
        (Some(expected), Some(content))
            if content.id() == expected.id()
                && content.owner() == Some(item.id())
                && matches!(
                    content.lifecycle(),
                    ContentLifecycle::Sealed | ContentLifecycle::Live | ContentLifecycle::Finalized
                )
                && content.current_reference() == Some(expected) => {}
        (None, None) => {}
        _ => {
            return Err(SyndicReadError::Invariant(
                "live-capture canonical item and content disagree",
            ));
        }
    }
    match (item.payload(), resource) {
        (CanonicalItemPayload::GeneratedMedia(expected), Some(resource))
            if resource.id() == *expected && resource.item_id() == item.id() => {}
        (CanonicalItemPayload::GeneratedMedia(_), _) | (_, Some(_)) => {
            return Err(SyndicReadError::Invariant(
                "live-capture canonical item and resource disagree",
            ));
        }
        (_, None) => {}
    }
    Ok(())
}

fn concurrent() -> SyndicReadError {
    SyndicReadError::ConcurrentChange {
        operation: "live-capture item read",
    }
}
