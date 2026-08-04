use std::{io, num::NonZeroU64};

use beryl_model::{DomainRevision, HomeRevision};

use crate::{DomainSchemaVersion, KeyspaceSchemaVersion};

pub(crate) const HEADER_KEYSPACE: &str = "_beryl_home";
pub(crate) const HEADER_KEY: &[u8] = b"header";
pub(crate) const HOME_REVISION_KEY: &[u8] = b"revision";
pub(crate) const DOMAINS_KEYSPACE: &str = "_beryl_domains";

const REVISION_MAGIC: &[u8; 8] = b"BRYLREVN";
const DOMAIN_MAGIC: &[u8; 8] = b"BRYLDOMN";
const METADATA_ENCODING: u32 = 1;
pub(crate) const MAX_HOME_HEADER_BYTES: usize = 64;
pub(crate) const HOME_REVISION_BYTES: usize = 20;
pub(crate) const MAX_DOMAIN_METADATA_BYTES: usize = 8 * 1_024;
const MAX_FAMILIES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedFamily {
    pub(crate) logical_name: String,
    pub(crate) physical_name: String,
    pub(crate) schema: KeyspaceSchemaVersion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DomainMetadata {
    pub(crate) schema: DomainSchemaVersion,
    pub(crate) revision: DomainRevision,
    pub(crate) families: Vec<PersistedFamily>,
}

pub(crate) fn encode_home_revision(revision: HomeRevision) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(HOME_REVISION_BYTES);
    encoded.extend_from_slice(REVISION_MAGIC);
    encoded.extend_from_slice(&METADATA_ENCODING.to_be_bytes());
    encoded.extend_from_slice(&revision.get().to_be_bytes());
    encoded
}

pub(crate) fn decode_home_revision(encoded: &[u8]) -> io::Result<HomeRevision> {
    if encoded.len() != HOME_REVISION_BYTES || &encoded[..8] != REVISION_MAGIC {
        return Err(invalid_data("home revision record has an invalid envelope"));
    }

    let encoding = read_u32(encoded, 8)?;
    if encoding != METADATA_ENCODING {
        return Err(invalid_data(
            "home revision record uses an unsupported encoding",
        ));
    }

    let raw = read_u64(encoded, 12)?;
    HomeRevision::new(raw).map_err(|_| invalid_data("home revision is zero"))
}

impl DomainMetadata {
    pub(crate) fn encode(&self) -> io::Result<Vec<u8>> {
        if self.families.len() > MAX_FAMILIES {
            return Err(invalid_data(
                "domain metadata contains too many keyspace families",
            ));
        }

        let mut encoded = Vec::new();
        encoded.extend_from_slice(DOMAIN_MAGIC);
        encoded.extend_from_slice(&METADATA_ENCODING.to_be_bytes());
        encoded.extend_from_slice(&self.schema.get().to_be_bytes());
        encoded.extend_from_slice(&self.revision.get().to_be_bytes());
        encoded.extend_from_slice(
            &u16::try_from(self.families.len())
                .map_err(|_| invalid_data("domain family count does not fit"))?
                .to_be_bytes(),
        );

        for family in &self.families {
            write_string(&mut encoded, &family.logical_name)?;
            write_string(&mut encoded, &family.physical_name)?;
            encoded.extend_from_slice(&family.schema.get().to_be_bytes());
        }

        if encoded.len() > MAX_DOMAIN_METADATA_BYTES {
            return Err(invalid_data(
                "domain metadata exceeds its stored byte bound",
            ));
        }
        Ok(encoded)
    }

    pub(crate) fn decode(encoded: &[u8]) -> io::Result<Self> {
        if encoded.len() > MAX_DOMAIN_METADATA_BYTES || encoded.len() < 26 {
            return Err(invalid_data("domain metadata has an invalid size"));
        }
        if &encoded[..8] != DOMAIN_MAGIC {
            return Err(invalid_data("domain metadata has an invalid envelope"));
        }

        let encoding = read_u32(encoded, 8)?;
        if encoding != METADATA_ENCODING {
            return Err(invalid_data("domain metadata uses an unsupported encoding"));
        }
        let schema = DomainSchemaVersion::new(read_nonzero_u32(encoded, 12, "domain schema")?);
        let revision = DomainRevision::from_nonzero(
            NonZeroU64::new(read_u64(encoded, 16)?)
                .ok_or_else(|| invalid_data("domain revision is zero"))?,
        );
        let count = usize::from(read_u16(encoded, 24)?);
        if count > MAX_FAMILIES {
            return Err(invalid_data(
                "domain metadata contains too many keyspace families",
            ));
        }

        let mut offset = 26;
        let mut families = Vec::with_capacity(count);
        for _ in 0..count {
            let logical_name = read_string(encoded, &mut offset)?;
            let physical_name = read_string(encoded, &mut offset)?;
            let schema_raw = read_nonzero_u32_at(encoded, &mut offset, "keyspace schema")?;
            families.push(PersistedFamily {
                logical_name,
                physical_name,
                schema: KeyspaceSchemaVersion::new(schema_raw),
            });
        }
        if offset != encoded.len() {
            return Err(invalid_data("domain metadata has trailing bytes"));
        }

        Ok(Self {
            schema,
            revision,
            families,
        })
    }
}

fn write_string(encoded: &mut Vec<u8>, value: &str) -> io::Result<()> {
    let bytes = value.as_bytes();
    let length =
        u8::try_from(bytes.len()).map_err(|_| invalid_data("metadata string exceeds 255 bytes"))?;
    encoded.push(length);
    encoded.extend_from_slice(bytes);
    Ok(())
}

fn read_string(encoded: &[u8], offset: &mut usize) -> io::Result<String> {
    let length = usize::from(
        *encoded
            .get(*offset)
            .ok_or_else(|| invalid_data("metadata string length is missing"))?,
    );
    *offset += 1;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| invalid_data("metadata string length overflows"))?;
    let bytes = encoded
        .get(*offset..end)
        .ok_or_else(|| invalid_data("metadata string is truncated"))?;
    *offset = end;
    String::from_utf8(bytes.to_vec()).map_err(|_| invalid_data("metadata string is not UTF-8"))
}

fn read_nonzero_u32(encoded: &[u8], offset: usize, name: &str) -> io::Result<u32> {
    let value = read_u32(encoded, offset)?;
    if value == 0 {
        return Err(invalid_data(format!("{name} is zero")));
    }
    Ok(value)
}

fn read_nonzero_u32_at(encoded: &[u8], offset: &mut usize, name: &str) -> io::Result<u32> {
    let value = read_nonzero_u32(encoded, *offset, name)?;
    *offset += 4;
    Ok(value)
}

fn read_u16(encoded: &[u8], offset: usize) -> io::Result<u16> {
    let bytes: [u8; 2] = encoded
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_data("metadata integer is truncated"))?
        .try_into()
        .expect("validated two-byte slice");
    Ok(u16::from_be_bytes(bytes))
}

fn read_u32(encoded: &[u8], offset: usize) -> io::Result<u32> {
    let bytes: [u8; 4] = encoded
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_data("metadata integer is truncated"))?
        .try_into()
        .expect("validated four-byte slice");
    Ok(u32::from_be_bytes(bytes))
}

fn read_u64(encoded: &[u8], offset: usize) -> io::Result<u64> {
    let bytes: [u8; 8] = encoded
        .get(offset..offset + 8)
        .ok_or_else(|| invalid_data("metadata integer is truncated"))?
        .try_into()
        .expect("validated eight-byte slice");
    Ok(u64::from_be_bytes(bytes))
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
