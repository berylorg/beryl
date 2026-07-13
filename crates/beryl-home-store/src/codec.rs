use std::{error::Error, fmt, marker::PhantomData, num::NonZeroU32, ops::Bound};

use crate::StorageDomain;

macro_rules! schema_version {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU32);

        impl $name {
            /// Constructs a nonzero schema version.
            ///
            /// # Panics
            ///
            /// Panics when `value` is zero. Schema versions are normally declared
            /// as compile-time constants, so an invalid declaration fails early.
            #[must_use]
            pub const fn new(value: u32) -> Self {
                assert!(value != 0, "schema versions must be nonzero");
                Self(NonZeroU32::new(value).expect("nonzero schema version"))
            }

            /// Returns the integer representation persisted by the owning schema.
            #[must_use]
            pub const fn get(self) -> u32 {
                self.0.get()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

schema_version!(
    /// Exact schema version of one registered logical domain.
    DomainSchemaVersion
);
schema_version!(
    /// Exact schema version of one physical keyspace family.
    KeyspaceSchemaVersion
);
schema_version!(
    /// Exact schema version carried by one stored record value.
    RecordVersion
);

/// Typed codec for one record family owned by a logical domain.
///
/// Implementations stay private to their domain package. Encoded keys and
/// values are consumed immediately inside `beryl-home-store`; application
/// callers receive only `Key` and `Value`.
pub trait RecordCodec<D: StorageDomain>: Send + Sync + 'static {
    /// Typed key accepted and returned by this record family.
    type Key: Send + Sync;
    /// Typed value accepted and returned by this record family.
    type Value: Send + Sync;
    /// Domain-owned codec failure.
    type Error: Error + Send + Sync + 'static;

    /// Logical keyspace-family name declared by `D`.
    const FAMILY: &'static str;
    /// Exact record version written and accepted by this codec.
    const VERSION: RecordVersion;
    /// Maximum encoded key size accepted from this codec.
    const MAX_KEY_BYTES: usize;
    /// Maximum encoded payload size, excluding the store-owned version prefix.
    const MAX_VALUE_BYTES: usize;

    /// Encodes one typed key for an internal point or cursor operation.
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error>;
    /// Decodes one internal key before returning it to the domain caller.
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error>;
    /// Encodes one typed value payload for a pending mutation.
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error>;
    /// Decodes one payload after the store validates the exact record version.
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error>;
}

/// Why a caller-supplied read bound is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadLimitError {
    /// Every read must permit at least one byte.
    ZeroBytes,
    /// Every cursor read must permit at least one item.
    ZeroItems,
}

impl fmt::Display for ReadLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroBytes => formatter.write_str("read byte limit must be nonzero"),
            Self::ZeroItems => formatter.write_str("cursor item limit must be nonzero"),
        }
    }
}

impl Error for ReadLimitError {}

/// Explicit stored-byte bound for one point read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointReadLimit {
    max_bytes: usize,
}

impl PointReadLimit {
    /// Constructs a nonzero point-read byte bound.
    pub fn new(max_bytes: usize) -> Result<Self, ReadLimitError> {
        if max_bytes == 0 {
            return Err(ReadLimitError::ZeroBytes);
        }
        Ok(Self { max_bytes })
    }

    /// Returns the maximum stored bytes this read may materialize.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }
}

/// Explicit item and stored-byte bounds for one cursor read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CursorReadLimits {
    max_items: usize,
    max_bytes: usize,
}

/// Direction in which a bounded cursor consumes its finite key range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorDirection {
    /// Lowest encoded key first.
    Forward,
    /// Highest encoded key first.
    Reverse,
}

/// Finite typed key range for one cursor read.
///
/// Both endpoints are always present. Callers paginate by using the last
/// returned typed key as an excluded endpoint in their next range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorRange<K> {
    start: Bound<K>,
    end: Bound<K>,
}

impl<K> CursorRange<K> {
    /// Constructs an inclusive range.
    #[must_use]
    pub fn closed(start: K, end: K) -> Self {
        Self {
            start: Bound::Included(start),
            end: Bound::Included(end),
        }
    }

    /// Constructs an inclusive-start, exclusive-end range.
    #[must_use]
    pub fn half_open(start: K, end: K) -> Self {
        Self {
            start: Bound::Included(start),
            end: Bound::Excluded(end),
        }
    }

    /// Constructs an exclusive-start, inclusive-end range.
    #[must_use]
    pub fn after(start: K, end: K) -> Self {
        Self {
            start: Bound::Excluded(start),
            end: Bound::Included(end),
        }
    }

    /// Constructs an exclusive range.
    #[must_use]
    pub fn open(start: K, end: K) -> Self {
        Self {
            start: Bound::Excluded(start),
            end: Bound::Excluded(end),
        }
    }

    pub(crate) fn bounds(&self) -> (&Bound<K>, &Bound<K>) {
        (&self.start, &self.end)
    }
}

/// One typed key/value pair returned by a cursor read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorRecord<K, V> {
    key: K,
    value: V,
}

impl<K, V> CursorRecord<K, V> {
    pub(crate) const fn new(key: K, value: V) -> Self {
        Self { key, value }
    }

    /// Returns the decoded record key.
    #[must_use]
    pub const fn key(&self) -> &K {
        &self.key
    }

    /// Returns the decoded record value.
    #[must_use]
    pub const fn value(&self) -> &V {
        &self.value
    }

    /// Consumes the record into its typed parts.
    #[must_use]
    pub fn into_parts(self) -> (K, V) {
        (self.key, self.value)
    }
}

/// Bounded page returned by a typed cursor read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorPage<K, V> {
    records: Vec<CursorRecord<K, V>>,
    stored_bytes: usize,
    has_more: bool,
    _not_raw: PhantomData<fn()>,
}

impl<K, V> CursorPage<K, V> {
    pub(crate) fn new(
        records: Vec<CursorRecord<K, V>>,
        stored_bytes: usize,
        has_more: bool,
    ) -> Self {
        Self {
            records,
            stored_bytes,
            has_more,
            _not_raw: PhantomData,
        }
    }

    /// Returns the decoded records in cursor order.
    #[must_use]
    pub fn records(&self) -> &[CursorRecord<K, V>] {
        &self.records
    }

    /// Returns the cumulative encoded key and value bytes read for this page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Returns whether another matching record exists beyond this page.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }

    /// Consumes the page into its decoded records.
    #[must_use]
    pub fn into_records(self) -> Vec<CursorRecord<K, V>> {
        self.records
    }
}

impl CursorReadLimits {
    /// Constructs nonzero cursor item and byte bounds.
    pub fn new(max_items: usize, max_bytes: usize) -> Result<Self, ReadLimitError> {
        if max_items == 0 {
            return Err(ReadLimitError::ZeroItems);
        }
        if max_bytes == 0 {
            return Err(ReadLimitError::ZeroBytes);
        }
        Ok(Self {
            max_items,
            max_bytes,
        })
    }

    /// Returns the maximum number of decoded records.
    #[must_use]
    pub const fn max_items(self) -> usize {
        self.max_items
    }

    /// Returns the maximum cumulative stored key and value bytes.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }
}
