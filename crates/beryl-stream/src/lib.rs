//! Fixed-capacity pages, channels, and range streaming primitives for Beryl.
//!
//! [`PagePool`] owns a fixed number of reusable equal-capacity byte pages.
//! [`fixed_channel`] creates a fixed message-count channel with ordering,
//! backpressure, and close behavior. [`BoundedSource`], [`BoundedSink`], and
//! [`StreamCursor`] provide identity- and offset-checked range streaming
//! without materializing a complete logical value.
//!
//! Immediate sends distinguish [`SendError::Full`] from closure. Deadline-bounded sends report
//! [`SendError::Timeout`] when their deadline expires. Every unsuccessful send returns the caller's
//! exact message. If a requested duration cannot be represented as an [`std::time::Instant`]
//! deadline, a timed send or receive that would block times out immediately; an operation that can
//! complete immediately still succeeds.
//!
//! Capacities are local to each primitive. This crate does not provide a
//! process-wide resource policy or account for heap content owned by channel
//! messages.
//!
//! # Example
//!
//! ```
//! use std::num::NonZeroUsize;
//! use beryl_stream::{PagePool, fixed_channel};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//! let pages = PagePool::new(
//!     NonZeroUsize::new(4_096).unwrap(),
//!     NonZeroUsize::new(2).unwrap(),
//! )?;
//! let mut page = pages.try_lease()?;
//! page.buffer_mut()[..5].copy_from_slice(b"hello");
//! page.set_len(5)?;
//!
//! let (sender, receiver) = fixed_channel(NonZeroUsize::new(1).unwrap())?;
//! assert!(sender.try_send(page).is_ok());
//! assert_eq!(receiver.try_receive()?.as_slice(), b"hello");
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]

mod channel;
mod page;
mod stream;

pub use channel::{
    ChannelBuildError, ChannelDiagnostics, FixedChannel, FixedChannelObserver,
    FixedChannelReceiver, FixedChannelSender, ReceiveError, SendError, fixed_channel,
};
pub use page::{PageLease, PagePool, PagePoolDiagnostics, PagePoolError, PagePoolObserver};
pub use stream::{
    BoundedSink, BoundedSource, ReplayIdentity, ReplayableSource, SourcePage, SourcePageError,
    StreamContractError, StreamCursor, StreamIdentity,
};
