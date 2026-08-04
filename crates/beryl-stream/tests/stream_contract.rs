use std::{convert::Infallible, error::Error, io, num::NonZeroUsize};

use beryl_stream::{
    BoundedSink, BoundedSource, PageLease, PagePool, ReplayIdentity, ReplayableSource, SourcePage,
    SourcePageError, StreamContractError, StreamCursor, StreamIdentity,
};

fn nonzero(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("test capacity is nonzero")
}

struct SliceSource {
    identity: StreamIdentity,
    revision: u64,
    bytes: &'static [u8],
}

impl BoundedSource for SliceSource {
    type Error = Box<dyn Error>;

    fn identity(&self) -> StreamIdentity {
        self.identity
    }

    fn read_page(&mut self, offset: u64, mut lease: PageLease) -> Result<SourcePage, Self::Error> {
        let offset = usize::try_from(offset)?;
        let remaining = self.bytes.get(offset..).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "offset exceeds source length")
        })?;
        let length = remaining.len().min(lease.capacity());
        lease.buffer_mut()[..length].copy_from_slice(&remaining[..length]);
        lease.set_len(length)?;
        Ok(SourcePage::new(
            self.identity,
            u64::try_from(offset)?,
            lease,
            offset + length == self.bytes.len(),
        )?)
    }
}

impl ReplayableSource for SliceSource {
    fn replay_identity(&self) -> ReplayIdentity {
        ReplayIdentity::new(self.identity, self.revision)
    }
}

#[derive(Default)]
struct CollectSink(Vec<u8>);

impl BoundedSink for CollectSink {
    type Error = Infallible;

    fn consume(&mut self, page: SourcePage) -> Result<(), Self::Error> {
        self.0.extend_from_slice(page.bytes());
        Ok(())
    }
}

#[test]
fn source_sink_pages_preserve_identity_offsets_and_terminal_state() {
    let pool = PagePool::new(nonzero(4), nonzero(1)).unwrap();
    let identity = StreamIdentity::from_bytes([7; 16]);
    let mut source = SliceSource {
        identity,
        revision: 9,
        bytes: b"streamed-data",
    };
    assert_eq!(source.identity(), identity);
    assert_eq!(source.replay_identity(), ReplayIdentity::new(identity, 9));
    assert_eq!(source.replay_identity().stream(), identity);
    assert_eq!(source.replay_identity().revision(), 9);

    let mut cursor = StreamCursor::new(identity);
    let mut sink = CollectSink::default();
    while !cursor.is_terminal() {
        let page = source
            .read_page(cursor.next_offset(), pool.try_lease().unwrap())
            .unwrap();
        assert_eq!(page.offset(), cursor.next_offset());
        assert_eq!(page.identity(), identity);
        cursor.accept(&page).unwrap();
        sink.consume(page).unwrap();
    }

    assert_eq!(sink.0, b"streamed-data");
    assert_eq!(cursor.identity(), identity);
    assert_eq!(cursor.next_offset(), 13);
    assert_eq!(pool.diagnostics().available, 1);
}

fn one_byte_page(pool: &PagePool, identity: StreamIdentity, offset: u64) -> SourcePage {
    let mut lease = pool.try_lease().unwrap();
    lease.buffer_mut()[0] = 1;
    lease.set_len(1).unwrap();
    SourcePage::new(identity, offset, lease, false).unwrap()
}

#[test]
fn source_pages_reject_nonprogress_and_logical_offset_overflow() {
    let pool = PagePool::new(nonzero(1), nonzero(1)).unwrap();
    let identity = StreamIdentity::from_bytes([1; 16]);
    assert!(matches!(
        SourcePage::new(identity, 0, pool.try_lease().unwrap(), false),
        Err(SourcePageError::EmptyNonterminal)
    ));

    let mut lease = pool.try_lease().unwrap();
    lease.buffer_mut()[0] = 1;
    lease.set_len(1).unwrap();
    assert!(matches!(
        SourcePage::new(identity, u64::MAX, lease, true),
        Err(SourcePageError::OffsetOverflow)
    ));
    assert_eq!(pool.diagnostics().available, 1);
}

#[test]
fn cursor_rejects_identity_drift_gaps_overlap_and_post_terminal_pages() {
    let pool = PagePool::new(nonzero(1), nonzero(1)).unwrap();
    let identity = StreamIdentity::from_bytes([1; 16]);
    let other_identity = StreamIdentity::from_bytes([2; 16]);
    let mut cursor = StreamCursor::new(identity);

    let wrong = one_byte_page(&pool, other_identity, 0);
    assert!(matches!(
        cursor.accept(&wrong),
        Err(StreamContractError::IdentityMismatch {
            expected,
            actual
        }) if expected == identity && actual == other_identity
    ));
    drop(wrong);

    let gap = one_byte_page(&pool, identity, 2);
    assert_eq!(
        cursor.accept(&gap),
        Err(StreamContractError::OffsetMismatch {
            expected: 0,
            actual: 2,
        })
    );
    drop(gap);

    let first = one_byte_page(&pool, identity, 0);
    cursor.accept(&first).unwrap();
    drop(first);
    let overlap = one_byte_page(&pool, identity, 0);
    assert_eq!(
        cursor.accept(&overlap),
        Err(StreamContractError::OffsetMismatch {
            expected: 1,
            actual: 0,
        })
    );
    drop(overlap);

    let mut terminal_lease = pool.try_lease().unwrap();
    terminal_lease.buffer_mut()[0] = 3;
    terminal_lease.set_len(1).unwrap();
    let terminal = SourcePage::new(identity, 1, terminal_lease, true).unwrap();
    assert_eq!(terminal.next_offset(), 2);
    assert!(terminal.is_terminal());
    cursor.accept(&terminal).unwrap();
    assert_eq!(
        cursor.accept(&terminal),
        Err(StreamContractError::AfterTerminal)
    );
}

#[test]
fn empty_terminal_page_is_valid_and_returns_its_lease() {
    let pool = PagePool::new(nonzero(4), nonzero(1)).unwrap();
    let identity = StreamIdentity::from_bytes([3; 16]);
    let terminal = SourcePage::new(identity, 0, pool.try_lease().unwrap(), true).unwrap();
    assert!(terminal.bytes().is_empty());
    assert_eq!(terminal.offset(), terminal.next_offset());

    let lease = terminal.into_lease();
    assert!(lease.is_empty());
    drop(lease);
    assert_eq!(pool.diagnostics().available, 1);
}
