use super::TranscriptProviderResponseKind;
use beryl_model::ProjectionRevision;
use std::sync::Arc;
use syndic_storage::indexed_resource as index;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum IndexedResourceQuery {
    Range {
        request: index::Request,
        cursor: Option<index::Cursor>,
        descriptors: usize,
        bytes: usize,
    },
    Display {
        request: index::DisplayRequest,
        cursor: Option<index::DisplayCursor>,
        bytes: usize,
    },
    Anchor {
        anchor: index::SourceAnchor,
        bytes: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexedResourceDemand {
    pub(crate) revision: ProjectionRevision,
    pub(crate) root: Option<[u8; 32]>,
    pub(crate) query: IndexedResourceQuery,
}

pub(crate) enum IndexedResourceValue {
    Pending,
    Range(index::Response),
    Display(index::DisplayResponse),
    Anchor(index::AnchorResponse),
}

pub(crate) struct IndexedResourceResponse {
    pub(crate) root: Option<index::Root>,
    pub(crate) value: IndexedResourceValue,
    pub(crate) encoded_bytes: usize,
}

#[derive(Clone)]
pub(crate) enum ResourceDeliveryResponse {
    Raw(TranscriptProviderResponseKind),
    Indexed(Arc<IndexedResourceResponse>),
}
