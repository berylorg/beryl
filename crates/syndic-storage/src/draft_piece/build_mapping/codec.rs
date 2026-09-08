use super::model::*;
use crate::codec::{CodecError, ExactCodec, Family, parts::Decoder};
use beryl_home_store::RecordVersion;
use sha2::{Digest, Sha256};

pub(crate) struct DraftPieceBuildMappingFamily;
pub(crate) type DraftPieceBuildMappingCodec = ExactCodec<DraftPieceBuildMappingFamily>;

fn bad() -> CodecError {
    CodecError::InvalidLength("build coordinate mapping")
}
fn u128_value(d: &mut Decoder<'_>) -> Result<u128, CodecError> {
    Ok(u128::from_be_bytes(
        d.take(16)?.try_into().map_err(|_| bad())?,
    ))
}
pub(super) fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    hash.finalize().into()
}
fn entry_bytes(entries: &Entries) -> Vec<u8> {
    let mut out = Vec::new();
    match entries {
        Entries::Runs(runs) => {
            for run in runs {
                out.push(match run {
                    Run::Copy(_) => 0,
                    Run::Deleted(_) => 1,
                    Run::Inserted(_) => 2,
                });
                out.extend_from_slice(&run.length().to_be_bytes());
            }
        }
        Entries::Children(children) => {
            for child in children {
                out.extend_from_slice(&child.id);
                out.extend_from_slice(&child.digest);
                out.extend_from_slice(&child.measure.source.to_be_bytes());
                out.extend_from_slice(&child.measure.target.to_be_bytes());
            }
        }
    }
    out
}
pub(super) fn node_digest(owner: &[u8; 48], height: u8, entries: &Entries) -> [u8; 32] {
    hash(&[
        b"syndic/draft-piece-build-mapping-node/v1",
        owner,
        &[height],
        &(entries.len() as u64).to_be_bytes(),
        &entry_bytes(entries),
    ])
}
pub(super) fn record_id(owner: &[u8; 48], ordinal: u64, digest: &[u8; 32]) -> [u8; 16] {
    hash(&[
        b"syndic/draft-piece-build-mapping-record-id/v1",
        &owner[..16],
        &owner[16..32],
        &owner[32..],
        &ordinal.to_be_bytes(),
        digest,
    ])[..16]
        .try_into()
        .expect("fixed digest")
}

impl Family for DraftPieceBuildMappingFamily {
    type Key = [u8; 64];
    type Value = Node;
    const NAME: &'static str = "draft-piece-build-mapping";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 64;
    const MAX_VALUE_BYTES: usize = 1385;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.to_vec())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        bytes.try_into().map_err(|_| bad())
    }
    fn encode_value(node: &Node) -> Result<Vec<u8>, CodecError> {
        if !node.shape(true)
            || !node
                .entries
                .measure(node.height)
                .map_err(|_| bad())?
                .valid(true)
            || node.digest
                != node_digest(
                    node.key[..48].try_into().map_err(|_| bad())?,
                    node.height,
                    &node.entries,
                )
        {
            return Err(bad());
        }
        let mut out = node.key.to_vec();
        out.push(node.height);
        out.extend_from_slice(&(node.entries.len() as u64).to_be_bytes());
        out.extend_from_slice(&entry_bytes(&node.entries));
        out.extend_from_slice(&node.digest);
        Ok(out)
    }
    fn decode_value(bytes: &[u8]) -> Result<Node, CodecError> {
        if bytes.len() > Self::MAX_VALUE_BYTES {
            return Err(bad());
        }
        let mut d = Decoder::new(bytes);
        let key = d.take(64)?.try_into().map_err(|_| bad())?;
        let height = d.u8()?;
        let count = d.u64()?;
        if !(1..=22).contains(&height) || !(1..=16).contains(&count) || (height > 1 && count < 2) {
            return Err(bad());
        }
        let entries = if height == 1 {
            let mut runs = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let kind = d.u8()?;
                let n = u128_value(&mut d)?;
                runs.push(match kind {
                    0 => Run::Copy(n),
                    1 => Run::Deleted(n),
                    2 => Run::Inserted(n),
                    _ => return Err(bad()),
                });
            }
            Entries::Runs(runs)
        } else {
            let mut children = Vec::with_capacity(count as usize);
            for _ in 0..count {
                children.push(Descriptor {
                    id: d.fixed16()?,
                    digest: d.fixed32()?,
                    height: height - 1,
                    measure: Measure {
                        source: u128_value(&mut d)?,
                        target: u128_value(&mut d)?,
                    },
                });
            }
            Entries::Children(children)
        };
        let node = Node {
            key,
            height,
            entries,
            digest: d.fixed32()?,
        };
        d.finish()?;
        Self::encode_value(&node)?;
        Ok(node)
    }
}

pub(crate) fn encode_root(root: MapRoot) -> Result<Vec<u8>, CodecError> {
    if !root.valid() {
        return Err(bad());
    }
    let mut out = Vec::new();
    match root {
        MapRoot::Empty => out.push(0),
        MapRoot::Identity(n) => {
            out.push(1);
            out.extend_from_slice(&n.to_be_bytes());
        }
        MapRoot::Stored(d) => {
            out.push(2);
            out.extend_from_slice(&d.id);
            out.extend_from_slice(&d.digest);
            out.push(d.height);
            out.extend_from_slice(&d.measure.source.to_be_bytes());
            out.extend_from_slice(&d.measure.target.to_be_bytes());
        }
    }
    Ok(out)
}
pub(crate) fn decode_root(bytes: &[u8]) -> Result<MapRoot, CodecError> {
    let mut d = Decoder::new(bytes);
    let root = match d.u8()? {
        0 => MapRoot::Empty,
        1 => MapRoot::Identity(u128_value(&mut d)?),
        2 => MapRoot::Stored(Descriptor {
            id: d.fixed16()?,
            digest: d.fixed32()?,
            height: d.u8()?,
            measure: Measure {
                source: u128_value(&mut d)?,
                target: u128_value(&mut d)?,
            },
        }),
        _ => return Err(bad()),
    };
    d.finish()?;
    if !root.valid() {
        return Err(bad());
    }
    Ok(root)
}
