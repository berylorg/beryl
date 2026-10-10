use super::*;
use crate::resource_structure::{
    construction::{ConstructionError, initial_build, validate_selected},
    transition::{prepare_control, validate_receipt},
};

fn frontier() -> Build {
    let mut build = build();
    build.revision = 2;
    build.transition = 1;
    build.progress = Some(Reference {
        key: SequenceKey {
            build: build.key,
            ordinal: 1,
        },
        digest: [0x81; 32],
    });
    build.next_page = 2;
    build.chain = [0x82; 32];
    build.counts = [2, 0, 0, 0];
    build.checkpoint = advance_code(&build.checkpoint, b"one\nthree\n", 10, 256)
        .unwrap()
        .checkpoint;
    build
}

#[test]
fn terminal_commands_preserve_frontier_and_close_receipt_digest_cycle() {
    for (command, lifecycle) in [
        (Command::Cancel, Lifecycle::Cancelled),
        (Command::Supersede, Lifecycle::Superseded),
        (Command::Refuse, Lifecycle::Refused),
    ] {
        for before in [build(), frontier()] {
            let result = prepare_control(&before, command).unwrap();
            assert_eq!(result.build.lifecycle, lifecycle);
            assert_eq!(result.build.checkpoint, before.checkpoint);
            assert_eq!(result.build.counts, before.counts);
            assert_eq!(result.build.chain, before.chain);
            assert_eq!(result.build.next_page, before.next_page);
            assert_eq!(result.receipt.prior, before.progress);
            assert_eq!(result.receipt.before_digest, build_digest(&before).unwrap());
            assert_eq!(
                result.receipt.after_digest,
                build_digest(&result.build).unwrap()
            );
            assert_eq!(result.receipt.after_revision, before.revision + 1);
            assert_eq!(result.build.progress.unwrap().digest, result.receipt.digest);
            assert!(result.receipt.page.is_none());
            assert!(result.selection.is_none());
            assert_eq!(
                decode_build(&encode_build(&result.build).unwrap()).unwrap(),
                result.build
            );
            assert_eq!(
                decode_progress(&encode_progress(&result.receipt).unwrap()).unwrap(),
                result.receipt
            );
            for next in [
                Command::Cancel,
                Command::Supersede,
                Command::Refuse,
                Command::PassChange,
                Command::Complete,
            ] {
                assert!(matches!(
                    prepare_control(&result.build, next),
                    Err(ConstructionError::Collision)
                ));
            }
        }
    }
}

#[test]
fn code_seal_and_completion_select_exact_metadata_without_new_page() {
    let before = frontier();
    let sealed = prepare_control(&before, Command::PassChange).unwrap();
    validate_receipt(&sealed.build, &sealed.receipt).unwrap();
    assert_eq!(sealed.build.checkpoint.pass, Pass::Sealed);
    assert_eq!(sealed.build.checkpoint.next_byte, 10);
    assert_eq!(sealed.build.checkpoint.next_row, 0);
    assert_eq!(sealed.build.lifecycle, Lifecycle::Building);
    assert!(sealed.selection.is_none());
    let complete = prepare_control(&sealed.build, Command::Complete).unwrap();
    let (root, head) = complete.selection.as_ref().unwrap();
    validate_selected(
        &before.binding,
        head,
        root,
        &complete.build,
        &complete.receipt,
    )
    .unwrap();
    assert_eq!(root.language.as_deref(), Some("rust"));
    assert_eq!(root.code_lines, 2);
    assert_eq!(root.pages, before.next_page - 1);
    assert_eq!(root.chain, before.chain);
    assert_eq!(root.counts, before.counts);
    assert_eq!(root.progress, complete.receipt.key);
    assert!(complete.receipt.page.is_none());
    assert_eq!(complete.receipt.root, Some(root.digest));
    assert_eq!(decode_root(&encode_root(root).unwrap()).unwrap(), *root);
    assert_eq!(decode_head(&encode_head(head).unwrap()).unwrap(), *head);
}

#[test]
fn pass_changes_refuse_unfinished_mismatched_or_repeated_frontiers() {
    assert!(prepare_control(&build(), Command::PassChange).is_err());
    assert!(prepare_control(&frontier(), Command::Complete).is_err());
    assert!(prepare_control(&frontier(), Command::Advance).is_err());
    let mut wrong = frontier();
    wrong.checkpoint.phase = ParsePhase::Consuming;
    assert!(prepare_control(&wrong, Command::PassChange).is_err());
    wrong = frontier();
    wrong.checkpoint.next_row = 1;
    wrong.counts[0] = 1;
    assert!(matches!(
        prepare_control(&wrong, Command::PassChange),
        Err(ConstructionError::Unavailable)
    ));
    let sealed = prepare_control(&frontier(), Command::PassChange).unwrap();
    assert!(prepare_control(&sealed.build, Command::PassChange).is_err());
    wrong = sealed.build;
    wrong.counts[0] = 1;
    assert!(matches!(
        prepare_control(&wrong, Command::Complete),
        Err(ConstructionError::Unavailable)
    ));
}

#[test]
fn selected_receipt_rejects_unequal_frontier_even_with_valid_own_digest() {
    let result = prepare_control(&frontier(), Command::PassChange).unwrap();
    validate_receipt(&result.build, &result.receipt).unwrap();
    let mut wrong = result.build.clone();
    wrong.chain = [0x99; 32];
    assert!(validate_receipt(&wrong, &result.receipt).is_err());
    wrong = result.build.clone();
    wrong.progress.as_mut().unwrap().digest = [0x99; 32];
    assert!(validate_receipt(&wrong, &result.receipt).is_err());
    let mut receipt = result.receipt.clone();
    receipt.after_digest = [0x99; 32];
    receipt.digest = progress_digest(&receipt).unwrap();
    wrong = result.build.clone();
    wrong.progress.as_mut().unwrap().digest = receipt.digest;
    assert!(validate_receipt(&wrong, &receipt).is_err());
    assert!(validate_receipt(&build(), &result.receipt).is_err());
}

#[test]
fn control_transitions_refuse_revision_overflow_and_unaccounted_eof_state() {
    let mut wrong = frontier();
    wrong.revision = u64::MAX;
    wrong.transition = u64::MAX - 1;
    wrong.progress.as_mut().unwrap().key.ordinal = wrong.transition;
    assert!(matches!(
        prepare_control(&wrong, Command::Cancel),
        Err(ConstructionError::Limit)
    ));
    let mut dirty = frontier();
    dirty.checkpoint.pass = Pass::Sealed;
    dirty.checkpoint.next_row = 0;
    dirty.checkpoint.syntax = b"x".to_vec();
    assert!(prepare_control(&dirty, Command::Complete).is_err());
    dirty = frontier();
    dirty.checkpoint.next_byte = 9;
    assert!(prepare_control(&dirty, Command::PassChange).is_err());
}

#[test]
fn table_pass_resets_and_completion_preserve_header_and_counts() {
    let original = binding();
    let metadata = ResourceMetadataRecord::new(
        key().resource.id,
        key().resource.revision,
        original.metadata.projection_id().unwrap(),
        original.metadata.item_id(),
        ResourceOrdinal::FIRST,
        ResourceKind::Table,
        "text/plain",
        ResourceBacking::TextRange {
            source: original.source,
            range: ProjectionSourceRange::new(5, 25).unwrap(),
        },
        [0x77; 32],
        None,
        ResourceStructure::table(1, 2, ProjectionSourceRange::new(0, 4).unwrap()).unwrap(),
    )
    .unwrap();
    let mut rows = initial_build(key(), metadata).unwrap();
    rows.revision = 2;
    rows.transition = 1;
    rows.progress = frontier().progress;
    rows.next_page = 2;
    rows.counts = [0, 2, 0, 0];
    rows.checkpoint.phase = ParsePhase::Eof;
    rows.checkpoint.next_byte = 20;
    rows.checkpoint.role = RowRole::Body;
    rows.checkpoint.next_row = 1;
    let cells = prepare_control(&rows, Command::PassChange).unwrap();
    assert_eq!(cells.build.checkpoint.pass, Pass::TableCells);
    assert_eq!(cells.build.checkpoint.next_byte, 0);
    assert_eq!(cells.build.checkpoint.role, RowRole::Header);
    let mut closed_cells = cells.build;
    closed_cells.counts[2] = 3;
    closed_cells.next_page = 3;
    closed_cells.checkpoint.phase = ParsePhase::Eof;
    closed_cells.checkpoint.next_byte = 20;
    closed_cells.checkpoint.role = RowRole::Body;
    closed_cells.checkpoint.next_row = 1;
    let columns = prepare_control(&closed_cells, Command::PassChange).unwrap();
    assert_eq!(columns.build.checkpoint.pass, Pass::Columns);
    assert_eq!(columns.build.checkpoint.next_byte, 4);
    let mut closed_columns = columns.build;
    closed_columns.next_page = 4;
    closed_columns.counts[3] = 2;
    closed_columns.checkpoint =
        advance_columns(&closed_columns.checkpoint, b"---|---\n", 2, 20, 256)
            .unwrap()
            .checkpoint;
    let sealed = prepare_control(&closed_columns, Command::PassChange).unwrap();
    assert_eq!(sealed.build.checkpoint.next_byte, 20);
    let complete = prepare_control(&sealed.build, Command::Complete).unwrap();
    let (root, head) = complete.selection.unwrap();
    validate_selected(
        &rows.binding,
        &head,
        &root,
        &complete.build,
        &complete.receipt,
    )
    .unwrap();
    assert_eq!(root.header, Some(range(0, 4)));
    assert_eq!(root.counts, [0, 2, 3, 2]);
    assert_eq!(root.body_rows, 1);
    assert_eq!(root.columns, 2);
    assert!(root.language.is_none());
}
