use super::support::BytesRecord;
use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainReader, DomainReconciliation,
    DomainRegistrationReader, DomainSchemaVersion, KeyspaceSchemaVersion, ReadError,
    ReconciliationReader, RecordFamily, StorageDomain,
};

pub(super) struct AlphaDomain;
pub(super) struct BetaDomain;

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub(super) struct ReconciliationReadError(#[from] ReadError);

impl DomainCallbackError for ReconciliationReadError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Ok(DomainCallbackSource::Read(self.0))
    }
}

macro_rules! domain {
    ($domain:ident, $name:literal) => {
        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<BytesRecord<Self>>(
                    KeyspaceSchemaVersion::new(1),
                )];
            type ValidationError = ReconciliationReadError;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = std::convert::Infallible;

            fn create_runtime_attachment(
                _: &DomainRegistrationReader<'_, Self>,
            ) -> Result<(), Self::RuntimeAttachmentError> {
                Ok(())
            }

            fn validate(_: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                Ok(())
            }

            fn reconcile(
                reader: &ReconciliationReader<'_, Self>,
            ) -> Result<DomainReconciliation, Self::ValidationError> {
                let mut side = None;
                for record in reader.records::<BytesRecord<Self>>()? {
                    let current = if record.current() == record.old() {
                        DomainReconciliation::ExactOld
                    } else if record.current() == record.new() {
                        DomainReconciliation::ExactNew
                    } else {
                        DomainReconciliation::Collision
                    };
                    if side.is_some_and(|previous| previous != current) {
                        return Ok(DomainReconciliation::Collision);
                    }
                    side = Some(current);
                }
                Ok(side.unwrap_or(DomainReconciliation::Collision))
            }
        }
    };
}

domain!(AlphaDomain, "current_alpha");
domain!(BetaDomain, "current_beta");
