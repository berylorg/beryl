pub(super) const MAGIC: [u8; 4] = *b"PIV1";

pub(super) const OBSERVATION_STARTED: u8 = 0;
pub(super) const OBSERVATION_DELTA: u8 = 1;
pub(super) const OBSERVATION_COMPLETED: u8 = 2;

pub(super) const TEXT_INLINE: u8 = 0;
pub(super) const TEXT_REUSED: u8 = 1;

pub(super) const OPTION_NONE: u8 = 0;
pub(super) const OPTION_SOME: u8 = 1;
