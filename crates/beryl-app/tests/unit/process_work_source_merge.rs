use super::*;

#[test]
fn source_consumes_one_thread_across_many_route_generation_pages_before_successor() {
    let thread = SyndicThreadId::from_bytes([1; 16]);
    let successor = SyndicThreadId::from_bytes([2; 16]);
    let mut reads = 0;
    let mut source = Source::new(|cursor: Option<usize>| {
        let page = cursor.unwrap_or(0);
        reads += 1;
        let records = match page {
            0..=3 => vec![
                (
                    thread,
                    ProcessWorkFacts {
                        queued: true,
                        ..Default::default()
                    }
                );
                256
            ],
            4 => vec![
                (
                    thread,
                    ProcessWorkFacts {
                        preparing: true,
                        ..Default::default()
                    },
                ),
                (
                    successor,
                    ProcessWorkFacts {
                        pending: true,
                        ..Default::default()
                    },
                ),
            ],
            _ => panic!("read after terminal page"),
        };
        Ok((records, (page < 4).then_some(page + 1)))
    });
    assert_eq!(source.peek().unwrap(), Some(thread));
    let mut facts = ProcessWorkFacts::default();
    source.consume(thread, &mut facts).unwrap();
    assert!(facts.queued && facts.preparing);
    assert_eq!(source.peek().unwrap(), Some(successor));
    assert!(source.page.len() <= 256);
    let mut successor_facts = ProcessWorkFacts::default();
    source.consume(successor, &mut successor_facts).unwrap();
    assert!(successor_facts.pending && !successor_facts.queued);
    assert_eq!(source.peek().unwrap(), None);
    drop(source);
    assert_eq!(reads, 5);
}
