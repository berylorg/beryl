use super::*;
use crate::{SyndicStorage, indexed_resource as api};

fn demand() -> api::DisplayRequest {
    api::DisplayRequest {
        target: api::Target::Code(1),
        offset: 0,
    }
}

#[test]
fn public_frozen_reads_preserve_pending_snapshot_and_exact_root() {
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
    complete(&store, &handle, &binding);
    assert!(matches!(
        storage
            .read_frozen_indexed_resource_display(
                &store,
                &pending,
                &binding,
                None,
                demand(),
                None,
                65_536,
            )
            .unwrap(),
        api::IndexedResourceResult::Pending
    ));
    let current = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let api::IndexedResourceResult::Ready {
        root,
        response,
        encoded_bytes,
    } = storage
        .read_frozen_indexed_resource_display(
            &store,
            &current,
            &binding,
            None,
            demand(),
            None,
            65_536,
        )
        .unwrap()
    else {
        panic!("completed companion is pending")
    };
    assert_eq!(root.code_lines, 2);
    assert_eq!(root.digest, response.cursor.root);
    assert_eq!(response.binding, binding);
    assert_eq!(response.fragment.as_ref().unwrap().text, "three");
    assert_eq!(
        encoded_bytes,
        response.encode().unwrap().len() + encode_root(&root).unwrap().len() + 9
    );
    assert!(matches!(
        storage.read_frozen_indexed_resource_display(
            &store,
            &current,
            &binding,
            Some([0xff; 32]),
            demand(),
            None,
            65_536,
        ),
        Err(api::IndexedResourceError::Collision)
    ));
    assert!(matches!(
        storage.read_frozen_indexed_resource_display(
            &store,
            &pending,
            &binding,
            None,
            demand(),
            Some(response.cursor),
            65_536,
        ),
        Err(api::IndexedResourceError::Unavailable)
    ));
    assert!(matches!(
        storage.read_frozen_indexed_resource_display(
            &store,
            &current,
            &binding,
            Some([0xff; 32]),
            demand(),
            Some(response.cursor),
            65_536,
        ),
        Err(api::IndexedResourceError::Collision)
    ));
    store.release_frozen_read(&pending).unwrap();
    store.release_frozen_read(&current).unwrap();
    assert!(matches!(
        storage.read_frozen_indexed_resource_display(
            &store,
            &current,
            &binding,
            Some(root.digest),
            demand(),
            None,
            65_536,
        ),
        Err(api::IndexedResourceError::Read(ReadError::FrozenRead(
            FrozenReadAccessError::Released
        )))
    ));
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    store.close().unwrap();
}

#[test]
fn public_range_continuation_includes_root_in_every_bounded_response() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let storage = SyndicStorage::reacquire(&store).unwrap();
    let text = "😀".repeat(5_000);
    let bytes = format!("first\n{text}\n").into_bytes();
    let binding = fixture_support::code_build(&bytes, 2).binding;
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes,
    })));
    complete(&store, &handle, &binding);
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let request = api::Request::Code {
        lines: api::OrdinalRange { start: 1, end: 2 },
    };
    let limits = api::ResponseLimits {
        descriptors: 1,
        bytes: 2_048,
    };
    let mut cursor = None;
    let mut expected_root = None;
    let mut reconstructed = String::new();
    let mut finished = false;
    for _ in 0..100 {
        let api::IndexedResourceResult::Ready {
            root,
            response,
            encoded_bytes,
        } = storage
            .read_frozen_indexed_resource_range(
                &store,
                &frozen,
                &binding,
                expected_root,
                request,
                cursor,
                limits,
            )
            .unwrap()
        else {
            panic!("completed companion is pending")
        };
        assert!(encoded_bytes <= limits.bytes);
        assert_eq!(
            encoded_bytes,
            response.encode().unwrap().len() + encode_root(&root).unwrap().len() + 9
        );
        assert_eq!(response.cursor.root_digest, root.digest);
        assert_eq!(response.binding, binding);
        assert_eq!(response.items.len(), 1);
        let fragment = response.presentations.as_ref().unwrap()[0]
            .fragment
            .as_ref()
            .unwrap();
        assert!(fragment.text.len() <= 8_192);
        reconstructed.push_str(&fragment.text);
        if response.request_complete {
            assert!(response.resource_complete);
            finished = true;
            break;
        }
        assert!(
            response.cursor.fragment_offset
                > cursor.map_or(0, |value: api::Cursor| value.fragment_offset)
        );
        cursor = Some(response.cursor);
        expected_root = Some(root.digest);
    }
    assert!(finished);
    assert_eq!(reconstructed, text);
    assert!(matches!(
        storage.read_frozen_indexed_resource_range(
            &store,
            &frozen,
            &binding,
            expected_root,
            request,
            None,
            api::ResponseLimits {
                descriptors: 1,
                bytes: 1
            },
        ),
        Err(api::IndexedResourceError::Limit)
    ));
    store.release_frozen_read(&frozen).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    store.close().unwrap();
}

#[test]
fn public_table_rectangle_transports_root_and_empty_missing_source_facts() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let storage = SyndicStorage::reacquire(&store).unwrap();
    let bytes = b"a|b|c\n---|---:|:---:\nx|z||\ny|z\n";
    let binding = fixture_support::table_build(bytes, 2, 3).binding;
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: bytes.to_vec(),
    })));
    complete(&store, &handle, &binding);
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let api::IndexedResourceResult::Ready {
        root,
        response,
        encoded_bytes,
    } = storage
        .read_frozen_indexed_resource_range(
            &store,
            &frozen,
            &binding,
            None,
            api::Request::Table {
                role: api::RowRole::Body,
                rows: api::OrdinalRange { start: 0, end: 2 },
                columns: api::OrdinalRange { start: 2, end: 3 },
            },
            None,
            api::ResponseLimits {
                descriptors: 2,
                bytes: 8_192,
            },
        )
        .unwrap()
    else {
        panic!("completed companion is pending")
    };
    assert_eq!(root.body_rows, 2);
    assert_eq!(root.columns, 3);
    assert!(root.header.is_some());
    assert_eq!(root.digest, response.cursor.root_digest);
    assert_eq!(response.binding, binding);
    assert!(response.request_complete);
    assert!(encoded_bytes <= 8_192);
    assert_eq!(response.items.len(), 2);
    assert!(matches!(&response.items[0], api::Item::Descriptor(value)
        if matches!(value.descriptor, api::Descriptor::TableCell { raw, .. } if raw.start == raw.end)));
    assert!(matches!(
        response.items[1],
        api::Item::Missing {
            row: 1,
            column: 2,
            ..
        }
    ));
    let presentations = response.presentations.unwrap();
    assert_eq!(presentations[0].alignment, Some(api::Alignment::Center));
    assert_eq!(presentations[0].fragment.as_ref().unwrap().text, "");
    assert!(presentations[1].fragment.is_none());
    store.release_frozen_read(&frozen).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    store.close().unwrap();
}
