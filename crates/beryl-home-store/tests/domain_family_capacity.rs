#[path = "support/fjall.rs"]
mod fjall_support;
mod support;

use std::convert::Infallible;

#[cfg(feature = "test-faults")]
use std::io;

use beryl_home_store::{
    DomainDefinitionError, DomainReader, DomainRegistrationError, DomainSchemaVersion,
    KeyspaceSchemaVersion, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
};
use tempfile::tempdir;

use support::open_home;

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{decode_test_domain_metadata, encode_test_domain_metadata};

struct SeventyThreeFamilyDomain;
struct InvalidFamilyNameDomain;
struct EmptyFamilyNameDomain;

macro_rules! family_codecs {
    ($($codec:ident => $name:literal),+ $(,)?) => {
        $(
            struct $codec;

            impl RecordCodec<SeventyThreeFamilyDomain> for $codec {
                type Key = Vec<u8>;
                type Value = Vec<u8>;
                type Error = Infallible;

                const FAMILY: &'static str = $name;
                const VERSION: RecordVersion = RecordVersion::new(1);
                const MAX_KEY_BYTES: usize = 1;
                const MAX_VALUE_BYTES: usize = 4;

                fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                    Ok(key.clone())
                }

                fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                    Ok(encoded.to_vec())
                }

                fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                    Ok(value.clone())
                }

                fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                    Ok(encoded.to_vec())
                }
            }
        )+
    };
}

family_codecs!(
    Family00 => "f00", Family01 => "f01", Family02 => "f02", Family03 => "f03",
    Family04 => "f04", Family05 => "f05", Family06 => "f06", Family07 => "f07",
    Family08 => "f08", Family09 => "f09", Family10 => "f10", Family11 => "f11",
    Family12 => "f12", Family13 => "f13", Family14 => "f14", Family15 => "f15",
    Family16 => "f16", Family17 => "f17", Family18 => "f18", Family19 => "f19",
    Family20 => "f20", Family21 => "f21", Family22 => "f22", Family23 => "f23",
    Family24 => "f24", Family25 => "f25", Family26 => "f26", Family27 => "f27",
    Family28 => "f28", Family29 => "f29", Family30 => "f30", Family31 => "f31",
    Family32 => "f32", Family33 => "f33", Family34 => "f34", Family35 => "f35",
    Family36 => "f36", Family37 => "f37", Family38 => "f38", Family39 => "f39",
    Family40 => "f40", Family41 => "f41", Family42 => "f42", Family43 => "f43",
    Family44 => "f44", Family45 => "f45", Family46 => "f46", Family47 => "f47",
    Family48 => "f48", Family49 => "f49", Family50 => "f50", Family51 => "f51",
    Family52 => "f52", Family53 => "f53", Family54 => "f54", Family55 => "f55",
    Family56 => "f56", Family57 => "f57", Family58 => "f58", Family59 => "f59",
    Family60 => "f60", Family61 => "f61", Family62 => "f62", Family63 => "f63",
    Family64 => "f64", Family65 => "f65", Family66 => "f66", Family67 => "f67",
    Family68 => "f68", Family69 => "f69", Family70 => "f70", Family71 => "f71",
    Family72 => "f72",
);

impl StorageDomain for SeventyThreeFamilyDomain {
    const NAME: &'static str = "large";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[
        RecordFamily::new::<Family00>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family01>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family02>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family03>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family04>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family05>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family06>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family07>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family08>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family09>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family10>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family11>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family12>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family13>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family14>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family15>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family16>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family17>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family18>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family19>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family20>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family21>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family22>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family23>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family24>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family25>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family26>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family27>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family28>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family29>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family30>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family31>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family32>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family33>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family34>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family35>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family36>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family37>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family38>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family39>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family40>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family41>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family42>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family43>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family44>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family45>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family46>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family47>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family48>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family49>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family50>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family51>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family52>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family53>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family54>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family55>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family56>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family57>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family58>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family59>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family60>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family61>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family62>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family63>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family64>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family65>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family66>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family67>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family68>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family69>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family70>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family71>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<Family72>(KeyspaceSchemaVersion::new(1)),
    ];
    type ValidationError = Infallible;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

macro_rules! invalid_family_domain {
    ($domain:ident, $record:ident, $family:literal) => {
        struct $record;

        impl RecordCodec<$domain> for $record {
            type Key = Vec<u8>;
            type Value = Vec<u8>;
            type Error = Infallible;

            const FAMILY: &'static str = $family;
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 1;
            const MAX_VALUE_BYTES: usize = 4;

            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok(key.clone())
            }

            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                Ok(encoded.to_vec())
            }

            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                Ok(value.clone())
            }

            fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                Ok(encoded.to_vec())
            }
        }

        impl StorageDomain for $domain {
            const NAME: &'static str = "identifier";
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<$record>(KeyspaceSchemaVersion::new(1))];
            type ValidationError = Infallible;

            fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                Ok(())
            }
        }
    };
}

invalid_family_domain!(
    InvalidFamilyNameDomain,
    InvalidFamilyNameRecord,
    "invalid.name"
);
invalid_family_domain!(EmptyFamilyNameDomain, EmptyFamilyNameRecord, "");

#[test]
fn seventy_three_family_registration_reopens_exactly() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let domain = store.register_domain::<SeventyThreeFamilyDomain>().unwrap();
    assert_eq!(store.domain_revision(domain).unwrap().get(), 1);
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    let domain = reopened
        .register_domain::<SeventyThreeFamilyDomain>()
        .unwrap();
    assert_eq!(reopened.domain_revision(domain).unwrap().get(), 1);
    reopened.close().unwrap();
}

#[test]
fn malformed_or_empty_family_identifiers_reject_at_declaration_validation() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());

    for domain in [
        store
            .register_domain::<InvalidFamilyNameDomain>()
            .map(|_| ()),
        store.register_domain::<EmptyFamilyNameDomain>().map(|_| ()),
    ] {
        assert!(matches!(
            domain,
            Err(DomainRegistrationError::InvalidDefinition(
                DomainDefinitionError::InvalidName {
                    kind: "keyspace family",
                    ..
                }
            ))
        ));
    }

    store.close().unwrap();
}

#[cfg(feature = "test-faults")]
fn minimal_families(count: usize) -> Vec<(String, String)> {
    vec![("a".to_owned(), "d.a.a".to_owned()); count]
}

#[cfg(feature = "test-faults")]
fn assert_invalid_data(error: io::Error, message: &str) {
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.to_string(), message);
}

#[test]
#[cfg(feature = "test-faults")]
fn metadata_v1_small_golden_bytes_remain_exact() {
    let encoded = encode_test_domain_metadata(&minimal_families(1)).unwrap();
    let expected = vec![
        0x42, 0x52, 0x59, 0x4c, 0x44, 0x4f, 0x4d, 0x4e, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x01, 0x61, 0x05, 0x64,
        0x2e, 0x61, 0x2e, 0x61, 0x00, 0x00, 0x00, 0x01,
    ];

    assert_eq!(encoded, expected);
    decode_test_domain_metadata(&encoded).unwrap();
}

#[test]
#[cfg(feature = "test-faults")]
fn metadata_round_trips_at_the_derived_family_capacity() {
    let encoded = encode_test_domain_metadata(&minimal_families(680)).unwrap();

    assert_eq!(encoded.len(), 8_186);
    decode_test_domain_metadata(&encoded).unwrap();
}

#[test]
#[cfg(feature = "test-faults")]
fn metadata_rejects_681_families_with_the_count_error_before_traversal() {
    let encode_error = encode_test_domain_metadata(&minimal_families(681)).unwrap_err();
    assert_invalid_data(
        encode_error,
        "domain metadata contains too many keyspace families",
    );

    let mut encoded = encode_test_domain_metadata(&minimal_families(680)).unwrap();
    encoded[24..26].copy_from_slice(&681_u16.to_be_bytes());
    let decode_error = decode_test_domain_metadata(&encoded).unwrap_err();
    assert_invalid_data(
        decode_error,
        "domain metadata contains too many keyspace families",
    );
}

#[test]
#[cfg(feature = "test-faults")]
fn metadata_accepts_exactly_8192_bytes_and_rejects_8193_bytes() {
    let mut families = vec![("a".repeat(249), "p".repeat(255)); 15];
    families.push(("a".repeat(255), "p".repeat(255)));
    let encoded = encode_test_domain_metadata(&families).unwrap();

    assert_eq!(families.len(), 16);
    assert_eq!(encoded.len(), 8_192);
    decode_test_domain_metadata(&encoded).unwrap();

    families[0].0.push('a');
    let error = encode_test_domain_metadata(&families).unwrap_err();
    assert_invalid_data(error, "domain metadata exceeds its stored byte bound");
}

#[test]
#[cfg(feature = "test-faults")]
fn metadata_rejects_empty_and_oversized_stored_strings() {
    let empty_error =
        encode_test_domain_metadata(&[(String::new(), "d.a.a".to_owned())]).unwrap_err();
    assert_invalid_data(empty_error, "metadata string is empty");

    let oversized_error =
        encode_test_domain_metadata(&[("a".repeat(256), "d.a.a".to_owned())]).unwrap_err();
    assert_invalid_data(oversized_error, "metadata string exceeds 255 bytes");

    let mut encoded = encode_test_domain_metadata(&minimal_families(1)).unwrap();
    encoded[26] = 0;
    let decode_error = decode_test_domain_metadata(&encoded).unwrap_err();
    assert_invalid_data(decode_error, "metadata string is empty");
}
