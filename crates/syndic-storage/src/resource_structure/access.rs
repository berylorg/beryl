use beryl_home_store::{FrozenHomeRead, HomeStore, ReadError};

use super::{
    codec::encode_root, construction::ConstructionError, frozen::FrozenRecords, query::IndexedQuery,
};
use crate::SyndicStorage;

pub use super::{
    Alignment, Binding, BuildKey, ByteRange, CellPresentation, Descriptor, ResourceKey, Root,
    RowDisposition, RowRole, SequenceKey,
    access_anchor::AnchorResponse,
    access_construction::{BuildState, BuildStatus, StopBuild},
    display::Fragment,
    query::IndexedDescriptor,
    query_anchor::{ResolvedAnchor, SourceAnchor},
    query_display::{DisplayCursor, DisplayRequest, DisplayResponse, Target},
    query_response::{
        Cursor, Item, OrdinalRange, Position, Presentation, Request, Response, ResponseLimits,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum IndexedResourceError {
    #[error("indexed resource read failed: {0}")]
    Read(#[from] ReadError),
    #[error("indexed resource demand exceeds its allowance")]
    Limit,
    #[error("indexed resource is unavailable for this exact source")]
    Unavailable,
    #[error("indexed resource source, root or continuation conflicts")]
    Collision,
    #[error("indexed resource closure is corrupt")]
    Corrupt,
}

impl From<ConstructionError> for IndexedResourceError {
    fn from(error: ConstructionError) -> Self {
        match error {
            ConstructionError::Read(source) => Self::Read(source),
            ConstructionError::Limit => Self::Limit,
            ConstructionError::Unavailable => Self::Unavailable,
            ConstructionError::Collision | ConstructionError::AlreadyCurrent => Self::Collision,
            ConstructionError::Write(_)
            | ConstructionError::Codec(_)
            | ConstructionError::Derivation(_)
            | ConstructionError::Corrupt => Self::Corrupt,
        }
    }
}

pub enum IndexedResourceResult<T> {
    Pending,
    Ready {
        root: Root,
        response: T,
        encoded_bytes: usize,
    },
}

pub(super) fn response_allowance(
    root: &Root,
    bytes: usize,
) -> Result<(usize, usize), IndexedResourceError> {
    if bytes == 0 || bytes > 65_536 {
        return Err(IndexedResourceError::Limit);
    }
    // Result tag and length-prefixed canonical root are part of the transport payload.
    let overhead = encode_root(root).map_err(ConstructionError::from)?.len() + 9;
    let remaining = bytes
        .checked_sub(overhead)
        .filter(|value| *value != 0)
        .ok_or(IndexedResourceError::Limit)?;
    Ok((remaining, overhead))
}

fn continuation_root(
    expected: Option<[u8; 32]>,
    cursor: Option<[u8; 32]>,
) -> Result<Option<[u8; 32]>, IndexedResourceError> {
    if expected
        .zip(cursor)
        .is_some_and(|(expected, cursor)| expected != cursor)
    {
        return Err(IndexedResourceError::Collision);
    }
    Ok(expected.or(cursor))
}

impl SyndicStorage {
    pub fn read_frozen_indexed_resource_range(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        binding: &Binding,
        expected_root: Option<[u8; 32]>,
        request: Request,
        cursor: Option<Cursor>,
        limits: ResponseLimits,
    ) -> Result<IndexedResourceResult<Response>, IndexedResourceError> {
        let expected_root =
            continuation_root(expected_root, cursor.map(|value| value.root_digest))?;
        let records = FrozenRecords {
            store,
            handle: &self.handle,
            read: frozen,
        };
        let Some(mut query) = IndexedQuery::open(&records, binding, expected_root)? else {
            return Ok(IndexedResourceResult::Pending);
        };
        let (bytes, overhead) = response_allowance(&query.root, limits.bytes)?;
        let response = query.range_display(
            &records,
            request,
            cursor,
            ResponseLimits {
                descriptors: limits.descriptors,
                bytes,
            },
        )?;
        let encoded_bytes = response.encoded_bytes + overhead;
        Ok(IndexedResourceResult::Ready {
            root: query.root,
            response,
            encoded_bytes,
        })
    }

    pub fn read_frozen_indexed_resource_display(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        binding: &Binding,
        expected_root: Option<[u8; 32]>,
        request: DisplayRequest,
        cursor: Option<DisplayCursor>,
        bytes: usize,
    ) -> Result<IndexedResourceResult<DisplayResponse>, IndexedResourceError> {
        let expected_root = continuation_root(expected_root, cursor.map(|value| value.root))?;
        let records = FrozenRecords {
            store,
            handle: &self.handle,
            read: frozen,
        };
        let Some(mut query) = IndexedQuery::open(&records, binding, expected_root)? else {
            return Ok(IndexedResourceResult::Pending);
        };
        let (bytes, overhead) = response_allowance(&query.root, bytes)?;
        let response = query.display(&records, request, cursor, bytes)?;
        let encoded_bytes = response.encoded_bytes + overhead;
        Ok(IndexedResourceResult::Ready {
            root: query.root,
            response,
            encoded_bytes,
        })
    }
}
