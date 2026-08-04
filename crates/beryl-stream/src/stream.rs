use thiserror::Error;

use crate::PageLease;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamIdentity([u8; 16]);

impl StreamIdentity {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReplayIdentity {
    stream: StreamIdentity,
    revision: u64,
}

impl ReplayIdentity {
    pub const fn new(stream: StreamIdentity, revision: u64) -> Self {
        Self { stream, revision }
    }

    pub const fn stream(self) -> StreamIdentity {
        self.stream
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }
}

pub struct SourcePage {
    identity: StreamIdentity,
    offset: u64,
    next_offset: u64,
    terminal: bool,
    lease: PageLease,
}

impl SourcePage {
    pub fn new(
        identity: StreamIdentity,
        offset: u64,
        lease: PageLease,
        terminal: bool,
    ) -> Result<Self, SourcePageError> {
        if lease.is_empty() && !terminal {
            return Err(SourcePageError::EmptyNonterminal);
        }
        let length = u64::try_from(lease.len()).map_err(|_| SourcePageError::OffsetOverflow)?;
        let next_offset = offset
            .checked_add(length)
            .ok_or(SourcePageError::OffsetOverflow)?;
        Ok(Self {
            identity,
            offset,
            next_offset,
            terminal,
            lease,
        })
    }

    pub const fn identity(&self) -> StreamIdentity {
        self.identity
    }

    pub const fn offset(&self) -> u64 {
        self.offset
    }

    pub const fn next_offset(&self) -> u64 {
        self.next_offset
    }

    pub const fn is_terminal(&self) -> bool {
        self.terminal
    }

    pub fn bytes(&self) -> &[u8] {
        self.lease.as_slice()
    }

    pub fn into_lease(self) -> PageLease {
        self.lease
    }
}

pub trait BoundedSource {
    type Error;

    fn identity(&self) -> StreamIdentity;

    fn read_page(&mut self, offset: u64, lease: PageLease) -> Result<SourcePage, Self::Error>;
}

pub trait ReplayableSource: BoundedSource {
    fn replay_identity(&self) -> ReplayIdentity;
}

pub trait BoundedSink {
    type Error;

    fn consume(&mut self, page: SourcePage) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamCursor {
    identity: StreamIdentity,
    next_offset: u64,
    terminal: bool,
}

impl StreamCursor {
    pub const fn new(identity: StreamIdentity) -> Self {
        Self {
            identity,
            next_offset: 0,
            terminal: false,
        }
    }

    pub const fn identity(self) -> StreamIdentity {
        self.identity
    }

    pub const fn next_offset(self) -> u64 {
        self.next_offset
    }

    pub const fn is_terminal(self) -> bool {
        self.terminal
    }

    pub fn accept(&mut self, page: &SourcePage) -> Result<(), StreamContractError> {
        if self.terminal {
            return Err(StreamContractError::AfterTerminal);
        }
        if page.identity != self.identity {
            return Err(StreamContractError::IdentityMismatch {
                expected: self.identity,
                actual: page.identity,
            });
        }
        if page.offset != self.next_offset {
            return Err(StreamContractError::OffsetMismatch {
                expected: self.next_offset,
                actual: page.offset,
            });
        }
        self.next_offset = page.next_offset;
        self.terminal = page.terminal;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SourcePageError {
    #[error("a nonterminal source page must make progress")]
    EmptyNonterminal,
    #[error("source page logical offset overflowed")]
    OffsetOverflow,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum StreamContractError {
    #[error("source page arrived after the terminal page")]
    AfterTerminal,
    #[error("source identity changed from {expected:?} to {actual:?}")]
    IdentityMismatch {
        expected: StreamIdentity,
        actual: StreamIdentity,
    },
    #[error("source page offset {actual} does not match expected offset {expected}")]
    OffsetMismatch { expected: u64, actual: u64 },
}
