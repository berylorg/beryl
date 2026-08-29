use thiserror::Error;

use crate::StorageDomain;
use crate::metadata::{HOME_REVISION_BYTES, HOME_REVISION_KEY, MAX_DOMAIN_METADATA_BYTES};

/// Checked logical records and encoded key/value bytes for one atomic batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckedBatchFootprint {
    records: u64,
    encoded_key_bytes: u64,
    encoded_value_bytes: u64,
}

impl CheckedBatchFootprint {
    /// Creates one checked batch footprint.
    pub const fn new(records: u64, encoded_key_bytes: u64, encoded_value_bytes: u64) -> Self {
        Self {
            records,
            encoded_key_bytes,
            encoded_value_bytes,
        }
    }

    /// Returns the number of physical records.
    #[must_use]
    pub const fn records(self) -> u64 {
        self.records
    }

    /// Returns aggregate encoded key bytes.
    #[must_use]
    pub const fn encoded_key_bytes(self) -> u64 {
        self.encoded_key_bytes
    }

    /// Returns aggregate encoded value bytes.
    #[must_use]
    pub const fn encoded_value_bytes(self) -> u64 {
        self.encoded_value_bytes
    }

    /// Returns aggregate encoded key-plus-value bytes.
    pub fn encoded_key_value_bytes(self) -> Result<u64, DurableStartFootprintError> {
        self.encoded_key_bytes
            .checked_add(self.encoded_value_bytes)
            .ok_or(DurableStartFootprintError::ArithmeticOverflow)
    }

    /// Checked-adds another footprint.
    pub fn checked_add(self, other: Self) -> Result<Self, DurableStartFootprintError> {
        Ok(Self {
            records: self
                .records
                .checked_add(other.records)
                .ok_or(DurableStartFootprintError::ArithmeticOverflow)?,
            encoded_key_bytes: self
                .encoded_key_bytes
                .checked_add(other.encoded_key_bytes)
                .ok_or(DurableStartFootprintError::ArithmeticOverflow)?,
            encoded_value_bytes: self
                .encoded_value_bytes
                .checked_add(other.encoded_value_bytes)
                .ok_or(DurableStartFootprintError::ArithmeticOverflow)?,
        })
    }
}

/// Failure while deriving or composing a durable-start footprint.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DurableStartFootprintError {
    /// Checked footprint arithmetic overflowed.
    #[error("durable-start footprint arithmetic overflowed")]
    ArithmeticOverflow,
    /// A typed participant named a domain other than its fixed owner.
    #[error("durable-start participant has unexpected domain metadata `{actual}`")]
    UnexpectedParticipantDomain {
        /// The supplied registered domain name.
        actual: &'static str,
    },
    /// Syndic and asset participant operation kinds do not form one allowed start.
    #[error("durable-start participant kinds do not match")]
    MismatchedParticipantKinds,
    /// Fjall rejected the exact totals as unrepresentable journal framing.
    #[error("durable-start footprint is not representable by Fjall journal framing")]
    JournalFraming,
}

/// Home-store-owned metadata record contributed for one participating domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParticipatingDomainFootprint {
    domain_name: &'static str,
    batch: CheckedBatchFootprint,
}

impl ParticipatingDomainFootprint {
    /// Returns the checked metadata record footprint.
    #[must_use]
    pub const fn batch(self) -> CheckedBatchFootprint {
        self.batch
    }
}

/// Derives the exact metadata record footprint for one registered domain.
pub fn participating_domain_footprint<D: StorageDomain>()
-> Result<ParticipatingDomainFootprint, DurableStartFootprintError> {
    Ok(ParticipatingDomainFootprint {
        domain_name: D::NAME,
        batch: CheckedBatchFootprint::new(
            1,
            to_u64(D::NAME.len())?,
            to_u64(MAX_DOMAIN_METADATA_BYTES)?,
        ),
    })
}

fn to_u64(value: usize) -> Result<u64, DurableStartFootprintError> {
    u64::try_from(value).map_err(|_| DurableStartFootprintError::ArithmeticOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DurableStartOperation {
    IdleSubmission,
    AcceptedInputPromotion,
}

/// Typed Syndic contribution for one durable new-turn start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyndicDurableStartFootprint {
    operation: DurableStartOperation,
    mutation: CheckedBatchFootprint,
    metadata: ParticipatingDomainFootprint,
}

impl SyndicDurableStartFootprint {
    /// Constructs the typed idle-submission participant.
    pub fn idle_submission(
        mutation: CheckedBatchFootprint,
        metadata: ParticipatingDomainFootprint,
    ) -> Result<Self, DurableStartFootprintError> {
        Self::new(DurableStartOperation::IdleSubmission, mutation, metadata)
    }

    /// Constructs the typed accepted-input-promotion participant.
    pub fn accepted_input_promotion(
        mutation: CheckedBatchFootprint,
        metadata: ParticipatingDomainFootprint,
    ) -> Result<Self, DurableStartFootprintError> {
        Self::new(
            DurableStartOperation::AcceptedInputPromotion,
            mutation,
            metadata,
        )
    }

    fn new(
        operation: DurableStartOperation,
        mutation: CheckedBatchFootprint,
        metadata: ParticipatingDomainFootprint,
    ) -> Result<Self, DurableStartFootprintError> {
        if metadata.domain_name != "syndic" {
            return Err(DurableStartFootprintError::UnexpectedParticipantDomain {
                actual: metadata.domain_name,
            });
        }
        Ok(Self {
            operation,
            mutation,
            metadata,
        })
    }
}

/// Typed optional asset owner-transfer contribution for one durable new-turn start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetOwnerTransferFootprint {
    operation: DurableStartOperation,
    mutation: CheckedBatchFootprint,
    metadata: ParticipatingDomainFootprint,
}

impl AssetOwnerTransferFootprint {
    /// Constructs the draft-to-submitted-item owner-transfer participant.
    pub fn draft_to_submitted_item(
        mutation: CheckedBatchFootprint,
        metadata: ParticipatingDomainFootprint,
    ) -> Result<Self, DurableStartFootprintError> {
        Self::new(DurableStartOperation::IdleSubmission, mutation, metadata)
    }

    /// Constructs the accepted-input-to-submitted-item owner-transfer participant.
    pub fn accepted_input_to_submitted_item(
        mutation: CheckedBatchFootprint,
        metadata: ParticipatingDomainFootprint,
    ) -> Result<Self, DurableStartFootprintError> {
        Self::new(
            DurableStartOperation::AcceptedInputPromotion,
            mutation,
            metadata,
        )
    }

    fn new(
        operation: DurableStartOperation,
        mutation: CheckedBatchFootprint,
        metadata: ParticipatingDomainFootprint,
    ) -> Result<Self, DurableStartFootprintError> {
        if metadata.domain_name != "beryl-assets" {
            return Err(DurableStartFootprintError::UnexpectedParticipantDomain {
                actual: metadata.domain_name,
            });
        }
        Ok(Self {
            operation,
            mutation,
            metadata,
        })
    }
}

/// Complete logical and Fjall-journal envelope for one durable new-turn start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DurableStartFootprint {
    logical: CheckedBatchFootprint,
    journal_append_bytes: u64,
}

impl DurableStartFootprint {
    /// Composes one permitted Syndic start and its optional matching asset transfer.
    pub fn compose(
        syndic: SyndicDurableStartFootprint,
        asset: Option<AssetOwnerTransferFootprint>,
    ) -> Result<Self, DurableStartFootprintError> {
        if asset.is_some_and(|asset| asset.operation != syndic.operation) {
            return Err(DurableStartFootprintError::MismatchedParticipantKinds);
        }
        let mut logical = syndic.mutation.checked_add(syndic.metadata.batch)?;
        if let Some(asset) = asset {
            logical = logical
                .checked_add(asset.mutation)?
                .checked_add(asset.metadata.batch)?;
        }
        logical = logical.checked_add(CheckedBatchFootprint::new(
            1,
            to_u64(HOME_REVISION_KEY.len())?,
            to_u64(HOME_REVISION_BYTES)?,
        ))?;
        let journal_append_bytes = fjall::JournalAppendFootprint::try_from_batch_totals(
            logical.records,
            logical.encoded_key_bytes,
            logical.encoded_value_bytes,
        )
        .map_err(|_| DurableStartFootprintError::JournalFraming)?
        .max_encoded_bytes();
        Ok(Self {
            logical,
            journal_append_bytes,
        })
    }

    /// Returns the complete logical batch footprint.
    #[must_use]
    pub const fn logical(self) -> CheckedBatchFootprint {
        self.logical
    }

    /// Returns Fjall's conservative maximum encoded journal append bytes.
    #[must_use]
    pub const fn journal_append_bytes(self) -> u64 {
        self.journal_append_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MetadataTestDomain;

    impl StorageDomain for MetadataTestDomain {
        const NAME: &'static str = "footprint-test";
        const SCHEMA_VERSION: crate::DomainSchemaVersion = crate::DomainSchemaVersion::new(1);
        const FAMILIES: &'static [crate::RecordFamily<Self>] = &[];
        type ValidationError = std::convert::Infallible;
        type RuntimeAttachment = ();
        type RuntimeAttachmentError = std::convert::Infallible;

        fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
            Ok(())
        }

        fn validate(_reader: &crate::DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
            Ok(())
        }
    }

    #[test]
    fn declared_metadata_envelope_contains_the_actual_encoding_shape() {
        // `DomainMetadata::encode` has a fixed 26-byte prefix. Every persisted family adds
        // one-byte logical and physical name lengths, both names, and a four-byte schema.
        let actual = 26_usize;
        assert!(actual <= MAX_DOMAIN_METADATA_BYTES);
        let footprint = participating_domain_footprint::<MetadataTestDomain>()
            .expect("metadata footprint")
            .batch();
        assert_eq!(
            MAX_DOMAIN_METADATA_BYTES as u64,
            footprint.encoded_value_bytes()
        );
        assert!(actual as u64 <= footprint.encoded_value_bytes());
    }
}
