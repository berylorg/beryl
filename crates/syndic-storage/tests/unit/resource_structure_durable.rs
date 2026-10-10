use super::*;
use crate::codec::ResourcesCodec;
use crate::domain::SyndicDomain;
use crate::resource_structure::{
    advance::AdvanceCode,
    construction::{AdmitBuild, initial_build},
    table_advance::AdvanceTable,
    transition::ControlTransition,
};
use beryl_home_store::{
    CommandOutcome,
    test_faults::{FaultController, FaultPoint},
};
#[path = "resource_structure_durable_support.rs"]
mod support;
use support::*;
#[path = "resource_structure_admission.rs"]
mod admission;
#[path = "resource_structure_frozen.rs"]
mod frozen;
#[path = "resource_structure_recovery.rs"]
mod recovery;
use recovery::{BoundaryFault, execute_boundary};

fn run(binding: Binding, bytes: &[u8], fault: BoundaryFault) {
    let home = TestHome::new();
    let faults = FaultController::new();
    let (mut store, mut handle) = open(&home, &faults);
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: bytes.to_vec(),
    })));
    (store, handle) = execute_boundary(store, handle, &faults, fault, |store, handle| {
        store.execute_current(handle.current_command(AdmitBuild { key: key() }))
    });
    let mut before = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
    assert_eq!(
        before,
        initial_build(key(), binding.metadata.clone()).unwrap()
    );
    let mut steps = 0;
    while before.lifecycle == Lifecycle::Building {
        steps += 1;
        assert!(steps < 100);
        assert!(read::<HeadsCodec>(&store, &handle, &binding.resource).is_none());
        (store, handle) = execute_boundary(store, handle, &faults, fault, |store, handle| {
            if before.checkpoint.pass == Pass::Sealed || before.checkpoint.phase == ParsePhase::Eof
            {
                store.execute_current(handle.current_command(ControlTransition {
                    before: before.clone(),
                    command: if before.checkpoint.pass == Pass::Sealed {
                        Command::Complete
                    } else {
                        Command::PassChange
                    },
                }))
            } else if binding.metadata.kind() == ResourceKind::Code {
                store.execute_current(handle.current_command(AdvanceCode {
                    before: before.clone(),
                    source_bytes: 3,
                    descriptors: 1,
                }))
            } else {
                store.execute_current(handle.current_command(AdvanceTable {
                    before: before.clone(),
                    source_bytes: 3,
                    descriptors: 1,
                }))
            }
        });
        let next = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
        assert_eq!(next.revision, before.revision + 1);
        let progress = next.progress.unwrap();
        assert_eq!(
            read::<ProgressCodec>(&store, &handle, &progress.key)
                .unwrap()
                .digest,
            progress.digest
        );
        before = next;
    }
    let head = read::<HeadsCodec>(&store, &handle, &binding.resource).unwrap();
    let root = read::<RootsCodec>(&store, &handle, &key()).unwrap();
    let receipt = read::<ProgressCodec>(&store, &handle, &root.progress).unwrap();
    crate::resource_structure::construction::validate_selected(
        &binding, &head, &root, &before, &receipt,
    )
    .unwrap();
    assert_eq!(
        read::<ResourcesCodec>(&store, &handle, &binding.resource.id).unwrap(),
        binding.metadata
    );
    for ordinal in 1..=root.pages {
        let page_key = SequenceKey {
            build: key(),
            ordinal,
        };
        let page = read::<PagesCodec>(&store, &handle, &page_key).unwrap();
        assert_eq!(page.binding_digest, head.binding_digest);
        for (slot, descriptor) in page.descriptors.iter().enumerate() {
            let locator = match descriptor {
                Descriptor::CodeLine { ordinal, .. } => read::<CodeLinesCodec>(
                    &store,
                    &handle,
                    &CoordinateKey::CodeLine(key(), *ordinal),
                )
                .unwrap(),
                Descriptor::TableRow {
                    role, ordinal: row, ..
                } => read::<TableRowsCodec>(
                    &store,
                    &handle,
                    &CoordinateKey::TableRow(key(), *role, *row),
                )
                .unwrap(),
                Descriptor::TableCell {
                    role, row, column, ..
                } => read::<TableCellsCodec>(
                    &store,
                    &handle,
                    &CoordinateKey::TableCell(key(), *role, *row, *column),
                )
                .unwrap(),
                Descriptor::TableColumn { ordinal, .. } => read::<TableColumnsCodec>(
                    &store,
                    &handle,
                    &CoordinateKey::TableColumn(key(), *ordinal),
                )
                .unwrap(),
            };
            assert_eq!(
                (locator.page, locator.slot, locator.digest),
                (ordinal, slot as u64, page.digest)
            );
            if matches!(
                descriptor,
                Descriptor::CodeLine { .. }
                    | Descriptor::TableCell { .. }
                    | Descriptor::TableRow {
                        disposition: RowDisposition::WholeRowFallback,
                        ..
                    }
            ) {
                let raw = descriptor.raw().unwrap();
                let (tag, role, row, column) = descriptor.coordinate();
                let anchor_key = AnchorKey {
                    build: key(),
                    start: binding.range.start + raw.start,
                    tag,
                    role,
                    row,
                    column,
                };
                let anchor = read::<SourceAnchorsCodec>(&store, &handle, &anchor_key).unwrap();
                assert_eq!(
                    (anchor.page, anchor.slot, anchor.digest),
                    (ordinal, slot as u64, page.digest)
                );
                assert_eq!(anchor.end, binding.range.start + raw.end);
            }
        }
    }
    store.close().unwrap();
    let (store, handle) = open(&home, &faults);
    assert_eq!(
        read::<BuildsCodec>(&store, &handle, &key()).unwrap(),
        before
    );
    assert_eq!(
        read::<HeadsCodec>(&store, &handle, &binding.resource).unwrap(),
        head
    );
    store.close().unwrap();
}

#[test]
fn code_callbacks_publish_atomic_pages_and_reopen_exact_selection() {
    run(binding(), b"one\nthree\n", BoundaryFault::None);
}

#[test]
fn code_acknowledgement_loss_reconciles_every_progress_and_publication_boundary() {
    run(
        binding(),
        b"one\nthree\n",
        BoundaryFault::LostAcknowledgement,
    );
}

#[test]
fn table_callbacks_publish_all_passes_and_reopen_exact_selection() {
    let bytes = b"a|b\n---|---:\nc||\n";
    run(
        fixture_support::table_build(bytes, 1, 2).binding,
        bytes,
        BoundaryFault::None,
    );
}

#[test]
fn table_acknowledgement_loss_reconciles_every_pass_and_publication_boundary() {
    let bytes = b"a|b\n---|---:\nc||\n";
    run(
        fixture_support::table_build(bytes, 1, 2).binding,
        bytes,
        BoundaryFault::LostAcknowledgement,
    );
}

#[test]
fn code_precommit_failure_recovers_exact_old_closure_at_every_boundary() {
    run(binding(), b"one\nthree\n", BoundaryFault::BeforeCommit);
}

#[test]
fn table_precommit_failure_recovers_exact_old_closure_at_every_boundary() {
    let bytes = b"a|b\n---|---:\nc||\nx|y|z\n";
    run(
        fixture_support::table_build(bytes, 2, 2).binding,
        bytes,
        BoundaryFault::BeforeCommit,
    );
}

#[test]
fn capacity_and_stale_build_refusal_leave_the_entire_durable_frontier_unchanged() {
    let home = TestHome::new();
    let faults = FaultController::new();
    let (store, handle) = open(&home, &faults);
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding(),
        bytes: b"one\nthree\n".to_vec(),
    })));
    committed(store.execute_current(handle.current_command(AdmitBuild { key: key() })));
    let before = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
    let revision = store.home_revision().unwrap();
    for (source_bytes, descriptors) in [(65_537, 1), (3, 257), (3, 0)] {
        assert!(matches!(
            store.execute_current(handle.current_command(AdvanceCode {
                before: before.clone(),
                source_bytes,
                descriptors
            })),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(store.home_revision().unwrap(), revision);
        assert_eq!(
            read::<BuildsCodec>(&store, &handle, &key()).unwrap(),
            before
        );
    }
    committed(store.execute_current(handle.current_command(AdvanceCode {
        before: before.clone(),
        source_bytes: 3,
        descriptors: 1,
    })));
    let after = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        store.execute_current(handle.current_command(AdvanceCode {
            before,
            source_bytes: 3,
            descriptors: 1
        })),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(read::<BuildsCodec>(&store, &handle, &key()).unwrap(), after);
    assert!(read::<HeadsCodec>(&store, &handle, &key().resource).is_none());
    assert!(
        read::<PagesCodec>(
            &store,
            &handle,
            &SequenceKey {
                build: key(),
                ordinal: 1
            }
        )
        .is_none()
    );
    store.close().unwrap();
}

#[test]
fn precommit_failure_preserves_old_build_without_receipt_or_page_after_recovery() {
    let home = TestHome::new();
    let faults = FaultController::new();
    let (store, handle) = open(&home, &faults);
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding(),
        bytes: b"one\nthree\n".to_vec(),
    })));
    committed(store.execute_current(handle.current_command(AdmitBuild { key: key() })));
    let before = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        store.execute_current(handle.current_command(AdvanceCode {
            before: before.clone(),
            source_bytes: 10,
            descriptors: 2
        })),
        CommandOutcome::NotCommitted { .. }
    ));
    let store = store.recover_same_home().unwrap().publish().unwrap();
    let handle = store.domain_handle::<SyndicDomain>().unwrap();
    assert_eq!(
        read::<BuildsCodec>(&store, &handle, &key()).unwrap(),
        before
    );
    let first = SequenceKey {
        build: key(),
        ordinal: 1,
    };
    assert!(read::<ProgressCodec>(&store, &handle, &first).is_none());
    assert!(read::<PagesCodec>(&store, &handle, &first).is_none());
    assert!(read::<HeadsCodec>(&store, &handle, &key().resource).is_none());
    committed(store.execute_current(handle.current_command(AdvanceCode {
        before,
        source_bytes: 10,
        descriptors: 2,
    })));
    assert_eq!(
        read::<BuildsCodec>(&store, &handle, &key()).unwrap().counts[0],
        2
    );
    store.close().unwrap();
}

#[test]
fn terminal_control_acknowledgement_loss_retains_receipt_without_selecting_a_root() {
    for (command, lifecycle) in [
        (Command::Cancel, Lifecycle::Cancelled),
        (Command::Supersede, Lifecycle::Superseded),
        (Command::Refuse, Lifecycle::Refused),
    ] {
        let home = TestHome::new();
        let faults = FaultController::new();
        let (store, handle) = open(&home, &faults);
        committed(store.execute_current(handle.current_command(Seed {
            binding: binding(),
            bytes: b"one\nthree\n".to_vec(),
        })));
        committed(store.execute_current(handle.current_command(AdmitBuild { key: key() })));
        let before = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        reconcile(
            store.execute_current(handle.current_command(ControlTransition { before, command })),
            &store,
        );
        let after = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
        assert_eq!(after.lifecycle, lifecycle);
        let receipt = read::<ProgressCodec>(&store, &handle, &after.progress.unwrap().key).unwrap();
        assert_eq!(receipt.command, command);
        assert!(read::<RootsCodec>(&store, &handle, &key()).is_none());
        assert!(read::<HeadsCodec>(&store, &handle, &key().resource).is_none());
        store.close().unwrap();
    }
}

#[test]
fn terminal_precommit_failure_preserves_old_closure_before_exact_retry() {
    for (command, lifecycle) in [
        (Command::Cancel, Lifecycle::Cancelled),
        (Command::Supersede, Lifecycle::Superseded),
        (Command::Refuse, Lifecycle::Refused),
    ] {
        let home = TestHome::new();
        let faults = FaultController::new();
        let (store, handle) = open(&home, &faults);
        committed(store.execute_current(handle.current_command(Seed {
            binding: binding(),
            bytes: b"one\nthree\n".to_vec(),
        })));
        committed(store.execute_current(handle.current_command(AdmitBuild { key: key() })));
        let before = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
        let (store, handle) = execute_boundary(
            store,
            handle,
            &faults,
            BoundaryFault::BeforeCommit,
            |store, handle| {
                store.execute_current(handle.current_command(ControlTransition {
                    before: before.clone(),
                    command,
                }))
            },
        );
        let after = read::<BuildsCodec>(&store, &handle, &key()).unwrap();
        assert_eq!(after.lifecycle, lifecycle);
        assert_eq!(
            read::<ProgressCodec>(&store, &handle, &after.progress.unwrap().key)
                .unwrap()
                .command,
            command
        );
        assert!(read::<RootsCodec>(&store, &handle, &key()).is_none());
        assert!(read::<HeadsCodec>(&store, &handle, &key().resource).is_none());
        store.close().unwrap();
    }
}
