use super::*;
use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainHandle, HomeStore, RecordCodec,
};

#[derive(Clone, Copy)]
pub(super) enum BoundaryFault {
    None,
    LostAcknowledgement,
    BeforeCommit,
}

pub(super) fn execute_boundary(
    mut store: HomeStore,
    mut handle: DomainHandle<SyndicDomain>,
    faults: &FaultController,
    fault: BoundaryFault,
    execute: impl Fn(&HomeStore, &DomainHandle<SyndicDomain>) -> CommandOutcome,
) -> (HomeStore, DomainHandle<SyndicDomain>) {
    match fault {
        BoundaryFault::None => committed(execute(&store, &handle)),
        BoundaryFault::LostAcknowledgement => {
            faults.fail_next(FaultPoint::AfterCommitBeforePersist);
            reconcile(execute(&store, &handle), &store);
        }
        BoundaryFault::BeforeCommit => {
            let old = closure(&store, &handle);
            let revision = store.home_revision().unwrap();
            faults.fail_next(FaultPoint::BeforeCommit);
            let outcome = execute(&store, &handle);
            assert!(
                matches!(outcome, CommandOutcome::NotCommitted { .. }),
                "{outcome:?}"
            );
            store = store.recover_same_home().unwrap().publish().unwrap();
            handle = store.domain_handle::<SyndicDomain>().unwrap();
            assert_eq!(store.home_revision().unwrap(), revision);
            assert_eq!(closure(&store, &handle), old);
            committed(execute(&store, &handle));
        }
    }
    (store, handle)
}

type EncodedRecords = Vec<(Vec<u8>, Vec<u8>)>;

fn family<R: RecordCodec<SyndicDomain>>(
    store: &HomeStore,
    handle: &DomainHandle<SyndicDomain>,
    first: R::Key,
    last: R::Key,
) -> EncodedRecords {
    let page = store
        .read_cursor::<SyndicDomain, R>(
            handle,
            &CursorRange::closed(first, last),
            CursorDirection::Forward,
            CursorReadLimits::new(128, 65_536).unwrap(),
        )
        .unwrap();
    assert!(!page.has_more(), "fixture exceeds bounded closure snapshot");
    page.records()
        .iter()
        .map(|record| {
            (
                R::encode_key(record.key()).unwrap(),
                R::encode_value(record.value()).unwrap(),
            )
        })
        .collect()
}

fn closure(store: &HomeStore, handle: &DomainHandle<SyndicDomain>) -> Vec<EncodedRecords> {
    let build = key();
    let first = SequenceKey { build, ordinal: 1 };
    let last = SequenceKey {
        build,
        ordinal: u64::MAX,
    };
    vec![
        family::<ResourcesCodec>(store, handle, build.resource.id, build.resource.id),
        family::<BuildsCodec>(store, handle, build, build),
        family::<ProgressCodec>(store, handle, first, last),
        family::<RootsCodec>(store, handle, build, build),
        family::<HeadsCodec>(store, handle, build.resource, build.resource),
        family::<PagesCodec>(store, handle, first, last),
        family::<CodeLinesCodec>(
            store,
            handle,
            CoordinateKey::CodeLine(build, 0),
            CoordinateKey::CodeLine(build, u64::MAX),
        ),
        family::<TableRowsCodec>(
            store,
            handle,
            CoordinateKey::TableRow(build, RowRole::Header, 0),
            CoordinateKey::TableRow(build, RowRole::Body, u64::MAX),
        ),
        family::<TableCellsCodec>(
            store,
            handle,
            CoordinateKey::TableCell(build, RowRole::Header, 0, 0),
            CoordinateKey::TableCell(build, RowRole::Body, u64::MAX, u64::MAX),
        ),
        family::<TableColumnsCodec>(
            store,
            handle,
            CoordinateKey::TableColumn(build, 0),
            CoordinateKey::TableColumn(build, u64::MAX),
        ),
        family::<SourceAnchorsCodec>(
            store,
            handle,
            AnchorKey {
                build,
                start: 0,
                tag: 0,
                role: RowRole::Header,
                row: 0,
                column: 0,
            },
            AnchorKey {
                build,
                start: u64::MAX,
                tag: 2,
                role: RowRole::Body,
                row: u64::MAX,
                column: u64::MAX,
            },
        ),
    ]
}
