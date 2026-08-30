#[derive(Debug)]
enum TestError {
    Read(ReadError),
    Build(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::Build(error) => error.fmt(formatter),
        }
    }
}

impl Error for TestError {}

impl DomainCallbackError for TestError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            Self::Build(error) => Err(Self::Build(error)),
        }
    }
}

impl From<ReadError> for TestError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

struct SourceDomain;
struct WitnessDomain;
struct PassiveDomain;
struct SourceRecord;
struct WitnessRecord;
struct PassiveRecord;

macro_rules! codec {
    ($domain:ty, $record:ty) => {
        impl RecordCodec<$domain> for $record {
            type Key = u64;
            type Value = Vec<u8>;
            type Error = std::io::Error;
            const FAMILY: &'static str = "records";
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 8;
            const MAX_VALUE_BYTES: usize = 32;
            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok(key.to_be_bytes().to_vec())
            }
            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                Ok(u64::from_be_bytes(encoded.try_into().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid key")
                })?))
            }
            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                Ok(value.clone())
            }
            fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                Ok(encoded.to_vec())
            }
        }
    };
}

codec!(SourceDomain, SourceRecord);
codec!(PassiveDomain, PassiveRecord);

impl RecordCodec<WitnessDomain> for WitnessRecord {
    type Key = u64;
    type Value = Vec<u8>;
    type Error = std::io::Error;
    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 8;
    const MAX_VALUE_BYTES: usize = 32;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        if *key == INVALID_DERIVED_KEY {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid derived key",
            ));
        }
        if *key == OVERSIZED_DERIVED_KEY {
            return Ok(vec![0; Self::MAX_KEY_BYTES + 1]);
        }
        Ok(key.to_be_bytes().to_vec())
    }
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        Ok(u64::from_be_bytes(encoded.try_into().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid key")
        })?))
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        if value.as_slice() == [0xfe] {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid derived expected value",
            ));
        }
        Ok(value.clone())
    }
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        if encoded == 42_u64.to_be_bytes() {
            DERIVED_CURRENT_DECODE_CALLS.fetch_add(1, Ordering::SeqCst);
        }
        Ok(encoded.to_vec())
    }
}

fn classify<D, R>(reader: &ReconciliationReader<'_, D>) -> Result<DomainReconciliation, TestError>
where
    D: StorageDomain<ValidationError = TestError>,
    R: RecordCodec<D, Key = u64, Value = Vec<u8>>,
{
    let mut side = None;
    for record in reader.records::<R>()? {
        let current = if record.current() == record.old() {
            DomainReconciliation::ExactOld
        } else if record.current() == record.new() {
            DomainReconciliation::ExactNew
        } else {
            DomainReconciliation::Collision
        };
        if side.is_some_and(|side| side != current) {
            return Ok(DomainReconciliation::Collision);
        }
        side = Some(current);
    }
    Ok(side.unwrap_or(DomainReconciliation::Collision))
}

macro_rules! domain {
    ($domain:ty, $record:ty, $name:literal) => {
        impl StorageDomain for $domain {
            const NAME: &'static str = $name;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<$record>(KeyspaceSchemaVersion::new(1))];
            type ValidationError = TestError;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = std::convert::Infallible;

            fn create_runtime_attachment(
                _reader: &beryl_home_store::DomainRegistrationReader<'_, Self>,
            ) -> Result<(), Self::RuntimeAttachmentError> {
                Ok(())
            }

            fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                Ok(())
            }
            fn reconcile(
                reader: &ReconciliationReader<'_, Self>,
            ) -> Result<DomainReconciliation, Self::ValidationError> {
                classify::<Self, $record>(reader)
            }
        }
    };
}

domain!(SourceDomain, SourceRecord, "successor-source");
domain!(WitnessDomain, WitnessRecord, "successor-witness");
domain!(PassiveDomain, PassiveRecord, "successor-passive");
