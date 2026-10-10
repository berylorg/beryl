use super::*;
use crate::resource_structure::{
    construction::ConstructionError,
    frozen::FrozenRecords,
    query::{CellLookup, IndexedQuery},
    query_anchor::SourceAnchor,
    query_display::{DisplayRequest, Target},
    query_response::{OrdinalRange, Request, ResponseLimits},
};
use beryl_home_store::{
    CommandCancellation, DomainHandle, FrozenReadAccessError, HomeStore, ReadError,
};

#[path = "resource_structure_access.rs"]
mod access;
#[path = "resource_structure_anchor_access.rs"]
mod anchor_access;
#[path = "resource_structure_construction_access.rs"]
mod construction_access;
#[path = "resource_structure_selected_closure.rs"]
mod selected_closure;

fn complete(store: &HomeStore, handle: &DomainHandle<SyndicDomain>, binding: &Binding) {
    committed(store.execute_current(handle.current_command(AdmitBuild { key: key() })));
    for _ in 0..1_000 {
        let before = read::<BuildsCodec>(store, handle, &key()).unwrap();
        if before.lifecycle == Lifecycle::Completed {
            return;
        }
        let outcome = if before.checkpoint.pass == Pass::Sealed
            || before.checkpoint.phase == ParsePhase::Eof
        {
            store.execute_current(handle.current_command(ControlTransition {
                command: if before.checkpoint.pass == Pass::Sealed {
                    Command::Complete
                } else {
                    Command::PassChange
                },
                before,
            }))
        } else if binding.metadata.kind() == ResourceKind::Code {
            store.execute_current(handle.current_command(AdvanceCode {
                before,
                source_bytes: 65_536,
                descriptors: 64,
            }))
        } else {
            store.execute_current(handle.current_command(AdvanceTable {
                before,
                source_bytes: 65_536,
                descriptors: 64,
            }))
        };
        committed(outcome);
    }
    panic!("construction did not complete within the admitted fixture step limit");
}

#[test]
fn frozen_pending_selection_stays_pending_after_atomic_publication() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let binding = binding();
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: b"one\nthree\n".to_vec(),
    })));
    let pending = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    complete(&store, &handle, &binding);
    let records = FrozenRecords {
        store: &store,
        handle: &handle,
        read: &pending,
    };
    assert!(
        IndexedQuery::open(&records, &binding, None)
            .unwrap()
            .is_none()
    );
    let current = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let records = FrozenRecords {
        store: &store,
        handle: &handle,
        read: &current,
    };
    let mut query = IndexedQuery::open(&records, &binding, None)
        .unwrap()
        .unwrap();
    let response = query
        .display(
            &records,
            DisplayRequest {
                target: Target::Code(1),
                offset: 0,
            },
            None,
            65_536,
        )
        .unwrap();
    assert_eq!(response.fragment.unwrap().text, "three");
    assert_eq!(response.binding, binding);
    store.release_frozen_read(&pending).unwrap();
    store.release_frozen_read(&current).unwrap();
    assert!(matches!(
        query.display(
            &records,
            DisplayRequest {
                target: Target::Code(0),
                offset: 0
            },
            None,
            65_536
        ),
        Err(ConstructionError::Read(ReadError::FrozenRead(
            FrozenReadAccessError::Released
        )))
    ));
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    store.close().unwrap();
}

#[test]
fn frozen_late_code_and_anchor_do_not_read_corrupt_earlier_pages() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let bytes = b"x\n".repeat(4_096);
    let binding = fixture_support::code_build(&bytes, 4_096).binding;
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes,
    })));
    complete(&store, &handle, &binding);
    let page_key = PagesFamily::encode_key(&SequenceKey {
        build: key(),
        ordinal: 1,
    })
    .unwrap();
    store
        .inject_persisted_corrupt_record::<SyndicDomain, PagesCodec>(&handle, &page_key, &[0])
        .unwrap();
    let selected = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let records = FrozenRecords {
        store: &store,
        handle: &handle,
        read: &selected,
    };
    let mut query = IndexedQuery::open(&records, &binding, None)
        .unwrap()
        .unwrap();
    let root = query.root.digest;
    let response = query
        .display(
            &records,
            DisplayRequest {
                target: Target::Code(4_095),
                offset: 0,
            },
            None,
            65_536,
        )
        .unwrap();
    assert_eq!(response.fragment.unwrap().text, "x");
    assert_eq!(response.cursor.root, root);
    let position = binding.range.start + 4_095 * 2;
    let resolved = query
        .resolve_anchor(
            &records,
            SourceAnchor {
                source: binding.source,
                block_start: binding.range.start,
                kind: ResourceKind::Code,
                position,
                visual_offset: 17,
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(resolved.anchor.position, position);
    assert_eq!(resolved.anchor.visual_offset, 17);
    let mut earlier = IndexedQuery::open(&records, &binding, Some(root))
        .unwrap()
        .unwrap();
    assert!(earlier.line(&records, 0).is_err());
    store.release_frozen_read(&selected).unwrap();
    store.close().unwrap();
}

#[test]
fn frozen_table_rectangle_keeps_authored_empty_missing_alignment_and_source() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let bytes = b"a|b|c\n---|---:|:---:\nx|z||\ny|z\n";
    let binding = fixture_support::table_build(bytes, 2, 3).binding;
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: bytes.to_vec(),
    })));
    complete(&store, &handle, &binding);
    let selected = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let records = FrozenRecords {
        store: &store,
        handle: &handle,
        read: &selected,
    };
    let mut query = IndexedQuery::open(&records, &binding, None)
        .unwrap()
        .unwrap();
    assert!(matches!(
        query.cell(&records, RowRole::Body, 0, 2).unwrap(),
        CellLookup::Authored(_)
    ));
    assert!(matches!(
        query.cell(&records, RowRole::Body, 1, 2).unwrap(),
        CellLookup::Missing { .. }
    ));
    let mut query = IndexedQuery::open(&records, &binding, None)
        .unwrap()
        .unwrap();
    let response = query
        .range_display(
            &records,
            Request::Table {
                role: RowRole::Body,
                rows: OrdinalRange { start: 0, end: 2 },
                columns: OrdinalRange { start: 2, end: 3 },
            },
            None,
            ResponseLimits {
                descriptors: 2,
                bytes: 8_192,
            },
        )
        .unwrap();
    assert_eq!(response.binding, binding);
    assert!(response.request_complete);
    assert_eq!(response.items.len(), 2);
    let presentations = response.presentations.unwrap();
    assert_eq!(presentations[0].alignment, Some(Alignment::Center));
    assert_eq!(presentations[0].fragment.as_ref().unwrap().text, "");
    assert!(presentations[1].fragment.is_none());
    store.release_frozen_read(&selected).unwrap();
    store.close().unwrap();
}

#[test]
fn frozen_queries_refuse_released_and_foreign_read_handles() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let selected = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    store.release_frozen_read(&selected).unwrap();
    let records = FrozenRecords {
        store: &store,
        handle: &handle,
        read: &selected,
    };
    assert!(matches!(
        IndexedQuery::open(&records, &binding(), None),
        Err(ConstructionError::Read(ReadError::FrozenRead(
            FrozenReadAccessError::Released
        )))
    ));
    let other_home = TestHome::new();
    let (other_store, _) = open(&other_home, &FaultController::new());
    let foreign = other_store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let records = FrozenRecords {
        store: &store,
        handle: &handle,
        read: &foreign,
    };
    assert!(matches!(
        IndexedQuery::open(&records, &binding(), None),
        Err(ConstructionError::Read(ReadError::FrozenRead(
            FrozenReadAccessError::Foreign
        )))
    ));
    other_store.release_frozen_read(&foreign).unwrap();
    other_store.close().unwrap();
    store.close().unwrap();
}
