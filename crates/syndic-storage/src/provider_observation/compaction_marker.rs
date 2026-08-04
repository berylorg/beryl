use beryl_model::CasItemId;

use crate::{CompactionMarkerLifecycle, SyndicTimestamp};

use super::{
    PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES, ProviderField, ProviderObservationBegin,
    ProviderObservationControl, ProviderObservationItemKind, ProviderObservationItemLifecycle,
    ProviderObservationStagingBytes, ProviderObservationValidatorError,
    ProviderObservationValidatorState, ProviderScalar, ProviderValueContext,
};

/// One fully validated context-compaction marker retained without unpublished durable staging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCompactionMarker {
    item_id: CasItemId,
    lifecycle: CompactionMarkerLifecycle,
    observed_at: SyndicTimestamp,
}

impl ProviderCompactionMarker {
    /// Returns the exact bounded external CAS item identity carried by the marker.
    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    /// Returns the marker's started or completed lifecycle.
    #[must_use]
    pub const fn lifecycle(&self) -> CompactionMarkerLifecycle {
        self.lifecycle
    }

    /// Returns the provider-observed lifecycle timestamp.
    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.observed_at
    }
}

/// Bounded resident parser for the source-free context-compaction marker schema.
///
/// Unlike the ordinary provider-observation stager, this parser never publishes an
/// unpublished build or chunk. It retains the full typed validator plus only the marker's
/// bounded external identity and lifecycle timestamp until the router authenticates its route.
pub struct ProviderCompactionMarkerStager {
    begin: ProviderObservationBegin,
    validator: ProviderObservationValidatorState,
    item_id: [u8; PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES],
    item_id_len: usize,
    observed_at: Option<SyndicTimestamp>,
}

impl ProviderCompactionMarkerStager {
    /// Begins resident staging for one exact context-compaction item lifecycle.
    pub fn begin(
        begin: ProviderObservationBegin,
    ) -> Result<Self, ProviderCompactionMarkerStagingError> {
        if !matches!(
            begin,
            ProviderObservationBegin::Item {
                kind: ProviderObservationItemKind::ContextCompaction,
                ..
            }
        ) {
            return Err(ProviderCompactionMarkerStagingError::ItemKindMismatch);
        }
        Ok(Self {
            begin,
            validator: ProviderObservationValidatorState::initial(),
            item_id: [0; PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES],
            item_id_len: 0,
            observed_at: None,
        })
    }

    /// Applies one typed structural control through the full provider validator.
    pub fn control(
        &mut self,
        control: ProviderObservationControl,
    ) -> Result<(), ProviderCompactionMarkerStagingError> {
        let mut validator = self.validator.clone();
        validator.control(self.begin, control)?;
        let observed_at = match control {
            ProviderObservationControl::Scalar {
                context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
                value: ProviderScalar::Unsigned(value),
            } => Some(SyndicTimestamp::from_unix_millis(value)),
            _ => self.observed_at,
        };
        self.validator = validator;
        self.observed_at = observed_at;
        Ok(())
    }

    /// Applies one bounded item-identity fragment without a durable callback.
    pub fn fragment(
        &mut self,
        fragment: ProviderObservationStagingBytes<'_>,
    ) -> Result<(), ProviderCompactionMarkerStagingError> {
        let mut validator = self.validator.clone();
        for byte in fragment.bytes() {
            validator.fragment_byte(fragment.context(), *byte)?;
        }
        let mut item_id = self.item_id;
        let mut item_id_len = self.item_id_len;
        if fragment.context() == ProviderValueContext::Field(ProviderField::ItemId) {
            let next = item_id_len
                .checked_add(fragment.bytes().len())
                .filter(|length| *length <= PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES)
                .ok_or(ProviderObservationValidatorError::InvalidIdentity)?;
            item_id[item_id_len..next].copy_from_slice(fragment.bytes());
            item_id_len = next;
        }
        self.validator = validator;
        self.item_id = item_id;
        self.item_id_len = item_id_len;
        Ok(())
    }

    /// Validates the complete schema and returns its compact typed marker.
    pub fn seal(self) -> Result<ProviderCompactionMarker, ProviderCompactionMarkerStagingError> {
        self.validator.finish(self.begin)?;
        let item_id = std::str::from_utf8(&self.item_id[..self.item_id_len])
            .ok()
            .and_then(|value| CasItemId::new(value).ok())
            .ok_or(ProviderCompactionMarkerStagingError::InvalidIdentity)?;
        let observed_at = self
            .observed_at
            .ok_or(ProviderCompactionMarkerStagingError::MissingLifecycleTimestamp)?;
        let lifecycle = match self.begin {
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Started,
                ..
            } => CompactionMarkerLifecycle::Started,
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Completed,
                ..
            } => CompactionMarkerLifecycle::Completed,
            ProviderObservationBegin::Delta { .. } => {
                return Err(ProviderCompactionMarkerStagingError::ItemKindMismatch);
            }
        };
        Ok(ProviderCompactionMarker {
            item_id,
            lifecycle,
            observed_at,
        })
    }

    /// Explicitly abandons this resident marker without a durable mutation.
    pub fn abandon(self) {}
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
/// Why resident context-compaction marker staging was rejected.
pub enum ProviderCompactionMarkerStagingError {
    /// The selected begin kind was not a context-compaction item.
    #[error("provider observation is not a context-compaction marker")]
    ItemKindMismatch,
    /// The observation failed the shared typed provider schema validator.
    #[error(transparent)]
    Validation(#[from] ProviderObservationValidatorError),
    /// The completed identity disagreed with the bounded CAS identity contract.
    #[error("context-compaction marker identity was not a valid bounded CAS item identity")]
    InvalidIdentity,
    /// The completed observation had no provider lifecycle timestamp.
    #[error("context-compaction marker omitted its lifecycle timestamp")]
    MissingLifecycleTimestamp,
}
