use std::str::Chars;

use unicode_normalization::UnicodeNormalization;

use super::CatalogValueError;

const TABLE: &[u8; 160_384] = include_bytes!("normalization/unicode-17-nfkc-cf.bin");
const TABLE_MAGIC: &[u8; 8] = b"BNKCF170";
const TABLE_HEADER_BYTES: usize = 16;
const TABLE_ENTRY_BYTES: usize = 12;
const TABLE_MAPPING_COUNT: usize = 10_583;
const TABLE_VALUE_COUNT: usize = 8_343;
const TABLE_VALUES_OFFSET: usize = TABLE_HEADER_BYTES + TABLE_MAPPING_COUNT * TABLE_ENTRY_BYTES;

/// Maximum original and normalized UTF-8 bytes accepted for one catalog query.
pub const CATALOG_QUERY_MAX_BYTES: usize = 64 * 1024;

const _: () = {
    let version = unicode_normalization::UNICODE_VERSION;
    assert!(version.0 == 17 && version.1 == 0 && version.2 == 0);
    assert!(TABLE_VALUES_OFFSET + TABLE_VALUE_COUNT * 4 == TABLE.len());
};

/// Durable catalog-search normalization identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogNormalizationProfile {
    version: u16,
    unicode_major: u8,
    unicode_minor: u8,
    unicode_patch: u8,
}

impl CatalogNormalizationProfile {
    #[must_use]
    pub const fn version(self) -> u16 {
        self.version
    }

    #[must_use]
    pub const fn unicode_version(self) -> (u8, u8, u8) {
        (self.unicode_major, self.unicode_minor, self.unicode_patch)
    }
}

/// V1 is Unicode R5 `toNFKC_Casefold` with fixed Unicode 17.0.0 data.
pub const CATALOG_NORMALIZATION_PROFILE: CatalogNormalizationProfile =
    CatalogNormalizationProfile {
        version: 1,
        unicode_major: 17,
        unicode_minor: 0,
        unicode_patch: 0,
    };

/// Query text normalized through the same fixed implementation as durable rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogNormalizedQuery(Box<str>);

impl CatalogNormalizedQuery {
    pub fn new(value: impl AsRef<str>) -> Result<Self, CatalogValueError> {
        let value = value.as_ref();
        if value.len() > CATALOG_QUERY_MAX_BYTES {
            return Err(CatalogValueError::TooLong {
                kind: "catalog query",
                maximum: CATALOG_QUERY_MAX_BYTES,
                actual: value.len(),
            });
        }
        normalize("normalized catalog query", value, CATALOG_QUERY_MAX_BYTES).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub(super) fn normalize(
    kind: &'static str,
    value: &str,
    maximum: usize,
) -> Result<Box<str>, CatalogValueError> {
    validate_table_header();
    let mut normalized = String::with_capacity(value.len().min(maximum));
    for scalar in NfkcCasefoldChars::new(value).nfc() {
        let actual =
            normalized
                .len()
                .checked_add(scalar.len_utf8())
                .ok_or(CatalogValueError::TooLong {
                    kind,
                    maximum,
                    actual: usize::MAX,
                })?;
        if actual > maximum {
            return Err(CatalogValueError::TooLong {
                kind,
                maximum,
                actual,
            });
        }
        normalized.push(scalar);
    }
    Ok(normalized.into_boxed_str())
}

struct NfkcCasefoldChars<'a> {
    input: Chars<'a>,
    mapped_offset: usize,
    mapped_remaining: usize,
}

impl<'a> NfkcCasefoldChars<'a> {
    fn new(value: &'a str) -> Self {
        Self {
            input: value.chars(),
            mapped_offset: 0,
            mapped_remaining: 0,
        }
    }
}

impl Iterator for NfkcCasefoldChars<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.mapped_remaining != 0 {
                let scalar = table_u32(TABLE_VALUES_OFFSET + self.mapped_offset * 4);
                self.mapped_offset += 1;
                self.mapped_remaining -= 1;
                return Some(char::from_u32(scalar).expect("generated mapping contains scalars"));
            }

            let scalar = self.input.next()?;
            let Some(mapping) = find_mapping(u32::from(scalar)) else {
                return Some(scalar);
            };
            self.mapped_offset = mapping.offset;
            self.mapped_remaining = mapping.length;
        }
    }
}

#[derive(Clone, Copy)]
struct Mapping {
    offset: usize,
    length: usize,
}

fn find_mapping(scalar: u32) -> Option<Mapping> {
    let mut lower = 0;
    let mut upper = TABLE_MAPPING_COUNT;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let entry = TABLE_HEADER_BYTES + middle * TABLE_ENTRY_BYTES;
        match table_u32(entry).cmp(&scalar) {
            std::cmp::Ordering::Less => lower = middle + 1,
            std::cmp::Ordering::Greater => upper = middle,
            std::cmp::Ordering::Equal => {
                return Some(Mapping {
                    offset: table_u32(entry + 4) as usize,
                    length: table_u16(entry + 8) as usize,
                });
            }
        }
    }
    None
}

fn validate_table_header() {
    assert_eq!(&TABLE[..8], TABLE_MAGIC, "invalid NFKC_CF table identity");
    assert_eq!(
        table_u32(8) as usize,
        TABLE_MAPPING_COUNT,
        "invalid NFKC_CF mapping count"
    );
    assert_eq!(
        table_u32(12) as usize,
        TABLE_VALUE_COUNT,
        "invalid NFKC_CF value count"
    );
}

fn table_u16(offset: usize) -> u16 {
    u16::from_le_bytes(
        TABLE[offset..offset + 2]
            .try_into()
            .expect("generated table u16 is in bounds"),
    )
}

fn table_u32(offset: usize) -> u32 {
    u32::from_le_bytes(
        TABLE[offset..offset + 4]
            .try_into()
            .expect("generated table u32 is in bounds"),
    )
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    fn folded(value: &str) -> String {
        normalize("test", value, 64 * 1024).unwrap().into()
    }

    #[test]
    fn unicode_17_r5_vectors_and_final_nfc_are_exact() {
        assert_eq!(folded("Straße"), "strasse");
        assert_eq!(folded("\u{0130}"), "i\u{0307}");
        assert_eq!(folded("\u{037a}"), " \u{03b9}");
        assert_eq!(folded("\u{3392}"), "mhz");
        assert_eq!(folded("\u{fb03}"), "ffi");
        assert_eq!(folded("\u{00ad}\u{200d}\u{fe0f}\u{2065}"), "");
        assert_eq!(folded("\u{0378}"), "\u{0378}");
        assert_eq!(folded("A\u{0341}"), "\u{00e1}");
        assert_eq!(folded("\u{0390}"), "\u{0390}");
    }

    #[test]
    fn profile_and_generated_table_are_fixed_to_unicode_17() {
        validate_table_header();
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(TABLE)),
            [
                0x62, 0xb4, 0xa3, 0xbe, 0x94, 0x2f, 0xc9, 0xeb, 0xb1, 0x30, 0x42, 0xf2, 0x55, 0x3e,
                0x48, 0x21, 0x01, 0x3f, 0x4e, 0x0c, 0x62, 0x5b, 0x10, 0x0a, 0xf4, 0xf3, 0x5d, 0xbb,
                0x90, 0xa3, 0x8e, 0xdb,
            ]
        );
        assert_eq!(CATALOG_NORMALIZATION_PROFILE.version(), 1);
        assert_eq!(
            CATALOG_NORMALIZATION_PROFILE.unicode_version(),
            unicode_normalization::UNICODE_VERSION
        );
    }
}
