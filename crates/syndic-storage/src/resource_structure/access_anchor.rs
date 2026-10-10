use beryl_home_store::{FrozenHomeRead, HomeStore};

use super::{
    Binding,
    access::{IndexedResourceError, IndexedResourceResult, response_allowance},
    codec::{binding_bytes, enc_descriptor, enc_range, lp},
    construction::ConstructionError,
    frozen::FrozenRecords,
    query::IndexedQuery,
    query_anchor::{ResolvedAnchor, SourceAnchor},
};
use crate::{
    SyndicStorage,
    codec::parts::{Encoder, enc_projection_text_source, enc_resource_kind},
};

pub struct AnchorResponse {
    pub binding: Binding,
    pub resolved: Option<ResolvedAnchor>,
    pub encoded_bytes: usize,
}

impl AnchorResponse {
    pub(super) fn encode(&self) -> Result<Vec<u8>, ConstructionError> {
        let mut encoder = Encoder::new();
        lp(&mut encoder, &binding_bytes(&self.binding)?)?;
        encoder.u8(u8::from(self.resolved.is_some()));
        if let Some(resolved) = &self.resolved {
            enc_descriptor(&mut encoder, &resolved.descriptor.descriptor)?;
            encoder.u8(u8::from(resolved.descriptor.absolute_raw.is_some()));
            if let Some(raw) = resolved.descriptor.absolute_raw {
                enc_range(&mut encoder, raw)?;
            }
            let anchor = resolved.anchor;
            enc_projection_text_source(&mut encoder, anchor.source);
            encoder.u64(anchor.block_start);
            enc_resource_kind(&mut encoder, anchor.kind);
            encoder.u64(anchor.position);
            for byte in anchor.visual_offset.to_be_bytes() {
                encoder.u8(byte);
            }
        }
        Ok(encoder.finish())
    }
}

impl SyndicStorage {
    pub fn read_frozen_indexed_resource_anchor(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        binding: &Binding,
        expected_root: Option<[u8; 32]>,
        anchor: SourceAnchor,
        bytes: usize,
    ) -> Result<IndexedResourceResult<AnchorResponse>, IndexedResourceError> {
        let records = FrozenRecords {
            store,
            handle: &self.handle,
            read: frozen,
        };
        let Some(mut query) = IndexedQuery::open(&records, binding, expected_root)? else {
            return Ok(IndexedResourceResult::Pending);
        };
        let (allowance, overhead) = response_allowance(&query.root, bytes)?;
        let mut response = AnchorResponse {
            binding: binding.clone(),
            resolved: query.resolve_anchor(&records, anchor)?,
            encoded_bytes: 0,
        };
        response.encoded_bytes = response.encode()?.len();
        if response.encoded_bytes > allowance {
            return Err(IndexedResourceError::Limit);
        }
        let encoded_bytes = response.encoded_bytes + overhead;
        Ok(IndexedResourceResult::Ready {
            root: query.root,
            response,
            encoded_bytes,
        })
    }
}
