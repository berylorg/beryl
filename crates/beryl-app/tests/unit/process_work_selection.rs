use super::*;
use crate::cas_projection::ProcessWorkFacts;
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicThreadId,
};
use syndic_storage::{SyndicTimestamp, ThreadCatalogTitle, ThreadCatalogTitleSource};

fn row(seed: u8, title_len: usize) -> ProcessWorkRecord {
    ProcessWorkRecord {
        thread_id: SyndicThreadId::from_bytes([seed; 16]),
        title: Some(
            ThreadCatalogTitle::new("x".repeat(title_len), ThreadCatalogTitleSource::Generated)
                .unwrap(),
        ),
        execution: ExecutionBinding::new(
            RuntimeId::from_bytes([1; 16]),
            RootId::from_bytes([2; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\work\beryl",
            )
            .unwrap(),
        ),
        last_activity_at: SyndicTimestamp::from_unix_millis(100 - u64::from(seed / 2)),
        facts: ProcessWorkFacts {
            pending: true,
            ..Default::default()
        },
        attention: Vec::new(),
    }
}

#[test]
fn selected_prefix_matches_sorted_reference_for_all_arrival_orders_and_byte_gaps() {
    let rows = [row(1, 1), row(2, 220), row(3, 1), row(4, 80)];
    let total: usize = rows.iter().map(ProcessWorkRecord::bytes).sum();
    let budgets = [
        1,
        rows[0].bytes(),
        rows[0].bytes() + rows[2].bytes(),
        rows[0].bytes() + rows[1].bytes(),
        total - 1,
        total,
    ];
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let order = [a, b, c, d];
                    if order
                        .iter()
                        .enumerate()
                        .any(|(index, value)| order[..index].contains(value))
                    {
                        continue;
                    }
                    for max_records in 1..=4 {
                        for max_bytes in budgets {
                            for after in [None, Some(rows[0].key()), Some(rows[2].key())] {
                                let mut expected = Vec::new();
                                let mut bytes = 0;
                                let mut first_omitted = None;
                                for row in &rows {
                                    if after.is_some_and(|key| row.key() <= key) {
                                        continue;
                                    }
                                    if expected.len() == max_records
                                        || bytes + row.bytes() > max_bytes
                                    {
                                        first_omitted = Some(row.key());
                                        break;
                                    }
                                    bytes += row.bytes();
                                    expected.push(row.clone());
                                }
                                let mut selected = Selection::new(
                                    ProcessWorkPageLimits::new(max_records, max_bytes).unwrap(),
                                    after,
                                );
                                for index in order {
                                    selected.insert(rows[index].clone());
                                    assert!(
                                        selected.records.len() <= max_records
                                            && selected.bytes <= max_bytes
                                    );
                                }
                                assert_eq!(
                                    (selected.records, selected.bytes, selected.first_omitted),
                                    (expected, bytes, first_omitted),
                                    "order {order:?}, count {max_records}, bytes {max_bytes}, after {after:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
