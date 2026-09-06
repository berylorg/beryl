//! Closed typed values and deterministic framing for pinned provider items.

pub(crate) mod codec;
mod error;
mod frame;
mod structured;
mod validate;
mod value;

pub use codec::*;
pub use error::*;
pub use frame::*;
pub use structured::*;
pub use value::*;
