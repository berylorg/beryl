use super::*;
use crate::{SyndicStorage, indexed_resource as api};

fn anchor(binding: &Binding, offset: u64) -> api::SourceAnchor {
    api::SourceAnchor {
        source: binding.source,
        block_start: binding.range.start,
        kind: binding.metadata.kind(),
        position: binding.range.start + offset,
        visual_offset: i64::MIN,
    }
}

#[test]
fn public_anchor_authenticates_root_charges_payload_and_borrows_snapshot() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let storage = SyndicStorage::reacquire(&store).unwrap();
    let binding = binding();
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: b"one\nthree\n".to_vec(),
    })));
    let pending = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let old = anchor(&binding, 6);
    complete(&store, &handle, &binding);
    assert!(matches!(
        storage
            .read_frozen_indexed_resource_anchor(&store, &pending, &binding, None, old, 65_536,)
            .unwrap(),
        api::IndexedResourceResult::Pending
    ));
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let api::IndexedResourceResult::Ready {
        root,
        response,
        encoded_bytes,
    } = storage
        .read_frozen_indexed_resource_anchor(&store, &frozen, &binding, None, old, 65_536)
        .unwrap()
    else {
        panic!("completed companion is pending")
    };
    assert_eq!(response.binding, binding);
    let resolved = response.resolved.as_ref().unwrap();
    assert_eq!(resolved.anchor, old);
    assert!(matches!(
        resolved.descriptor.descriptor,
        api::Descriptor::CodeLine { ordinal: 1, .. }
    ));
    assert_eq!(
        encoded_bytes,
        encode_root(&root).unwrap().len() + 9 + response.encode().unwrap().len()
    );
    assert!(matches!(
        storage.read_frozen_indexed_resource_anchor(
            &store,
            &frozen,
            &binding,
            Some([0xff; 32]),
            old,
            65_536,
        ),
        Err(api::IndexedResourceError::Collision)
    ));
    assert!(matches!(
        storage.read_frozen_indexed_resource_anchor(
            &store,
            &pending,
            &binding,
            Some(root.digest),
            old,
            65_536,
        ),
        Err(api::IndexedResourceError::Unavailable)
    ));
    assert!(matches!(
        storage.read_frozen_indexed_resource_anchor(
            &store,
            &frozen,
            &binding,
            Some(root.digest),
            old,
            encoded_bytes - 1,
        ),
        Err(api::IndexedResourceError::Limit)
    ));
    assert!(matches!(storage.read_frozen_indexed_resource_anchor(
        &store, &frozen, &binding, Some(root.digest), old, encoded_bytes,
    ).unwrap(), api::IndexedResourceResult::Ready { encoded_bytes: charge, .. } if charge == encoded_bytes));
    assert_eq!(store.retained_frozen_read_count().unwrap(), 2);
    store.release_frozen_read(&pending).unwrap();
    store.release_frozen_read(&frozen).unwrap();
    assert!(matches!(
        storage.read_frozen_indexed_resource_anchor(&store, &frozen, &binding, None, old, 65_536,),
        Err(api::IndexedResourceError::Read(ReadError::FrozenRead(
            FrozenReadAccessError::Released
        )))
    ));
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    store.close().unwrap();
}

#[test]
fn public_anchor_explicitly_invalidates_different_block_and_kind() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let storage = SyndicStorage::reacquire(&store).unwrap();
    let binding = binding();
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: b"one\nthree\n".to_vec(),
    })));
    complete(&store, &handle, &binding);
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let old = anchor(&binding, 6);
    for invalid in [
        api::SourceAnchor {
            block_start: old.block_start + 1,
            ..old
        },
        api::SourceAnchor {
            kind: ResourceKind::Table,
            ..old
        },
    ] {
        let api::IndexedResourceResult::Ready {
            response,
            encoded_bytes,
            ..
        } = storage
            .read_frozen_indexed_resource_anchor(&store, &frozen, &binding, None, invalid, 2_048)
            .unwrap()
        else {
            panic!("completed companion is pending")
        };
        assert!(response.resolved.is_none());
        assert_eq!(response.binding, binding);
        assert!(encoded_bytes <= 2_048);
    }
    assert!(matches!(
        storage.read_frozen_indexed_resource_anchor(
            &store,
            &frozen,
            &binding,
            None,
            api::SourceAnchor {
                position: old.source.logical_utf8_bytes() + 1,
                ..old
            },
            65_536,
        ),
        Err(api::IndexedResourceError::Unavailable)
    ));
    store.release_frozen_read(&frozen).unwrap();
    store.close().unwrap();
}

#[test]
fn public_table_anchor_preserves_authored_empty_and_whole_row_fallback() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let storage = SyndicStorage::reacquire(&store).unwrap();
    let bytes = b"|a|b|\n|---|---|\n||z|\n|x|y|extra|\n";
    let binding = fixture_support::table_build(bytes, 2, 2).binding;
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: bytes.to_vec(),
    })));
    complete(&store, &handle, &binding);
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    for offset in [17, 28] {
        let old = anchor(&binding, offset);
        let api::IndexedResourceResult::Ready {
            response,
            encoded_bytes,
            ..
        } = storage
            .read_frozen_indexed_resource_anchor(&store, &frozen, &binding, None, old, 2_048)
            .unwrap()
        else {
            panic!("completed companion is pending")
        };
        let resolved = response.resolved.unwrap();
        assert_eq!(resolved.anchor, old);
        assert!(encoded_bytes <= 2_048);
        if offset == 17 {
            assert!(matches!(resolved.descriptor.descriptor,
                api::Descriptor::TableCell { raw, .. } if raw.start == raw.end));
            assert_eq!(
                resolved.descriptor.absolute_raw,
                Some(ByteRange {
                    start: old.position,
                    end: old.position
                })
            );
        } else {
            assert!(matches!(
                resolved.descriptor.descriptor,
                api::Descriptor::TableRow {
                    disposition: RowDisposition::WholeRowFallback,
                    ordinal: 1,
                    ..
                }
            ));
        }
    }
    store.release_frozen_read(&frozen).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    store.close().unwrap();
}
