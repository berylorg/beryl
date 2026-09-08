use super::super::DraftPiecePrepareErrorV1;

pub(crate) const MAX_UNITS: u128 = 2 * u64::MAX as u128;
pub(crate) type MappingResult<T> = Result<T, DraftPiecePrepareErrorV1>;
pub(crate) fn invalid() -> DraftPiecePrepareErrorV1 {
    DraftPiecePrepareErrorV1::InvalidRoot
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Measure {
    pub source: u128,
    pub target: u128,
}

impl Measure {
    pub const ZERO: Self = Self {
        source: 0,
        target: 0,
    };
    pub fn valid(self, nonempty: bool) -> bool {
        self.source <= MAX_UNITS
            && self.target <= MAX_UNITS
            && (!nonempty || self.source != 0 || self.target != 0)
    }
    pub fn add(self, other: Self) -> MappingResult<Self> {
        let value = Self {
            source: self.source.checked_add(other.source).ok_or_else(invalid)?,
            target: self.target.checked_add(other.target).ok_or_else(invalid)?,
        };
        value.valid(false).then_some(value).ok_or_else(invalid)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Run {
    Copy(u128),
    Deleted(u128),
    Inserted(u128),
}
impl Run {
    pub fn length(self) -> u128 {
        match self {
            Self::Copy(n) | Self::Deleted(n) | Self::Inserted(n) => n,
        }
    }
    pub fn with_length(self, n: u128) -> Self {
        match self {
            Self::Copy(_) => Self::Copy(n),
            Self::Deleted(_) => Self::Deleted(n),
            Self::Inserted(_) => Self::Inserted(n),
        }
    }
    pub fn measure(self) -> Measure {
        match self {
            Self::Copy(n) => Measure {
                source: n,
                target: n,
            },
            Self::Deleted(n) => Measure {
                source: n,
                target: 0,
            },
            Self::Inserted(n) => Measure {
                source: 0,
                target: n,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Descriptor {
    pub id: [u8; 16],
    pub digest: [u8; 32],
    pub height: u8,
    pub measure: Measure,
}
impl Descriptor {
    pub fn valid(self) -> bool {
        (1..=22).contains(&self.height) && self.measure.valid(true)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MapRoot {
    Empty,
    Identity(u128),
    Stored(Descriptor),
}
impl MapRoot {
    pub fn initial(measure: Measure) -> MappingResult<Self> {
        if !measure.valid(false) || measure.source != measure.target {
            return Err(invalid());
        }
        Ok(if measure.source == 0 {
            Self::Empty
        } else {
            Self::Identity(measure.source)
        })
    }
    pub fn measure(self) -> Measure {
        match self {
            Self::Empty => Measure::ZERO,
            Self::Identity(n) => Measure {
                source: n,
                target: n,
            },
            Self::Stored(d) => d.measure,
        }
    }
    pub fn valid(self) -> bool {
        match self {
            Self::Empty => true,
            Self::Identity(n) => n > 0 && n <= MAX_UNITS,
            Self::Stored(d) => d.valid(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Entries {
    Runs(Vec<Run>),
    Children(Vec<Descriptor>),
}
impl Entries {
    pub fn len(&self) -> usize {
        match self {
            Self::Runs(v) => v.len(),
            Self::Children(v) => v.len(),
        }
    }
    pub fn measure(&self, height: u8) -> MappingResult<Measure> {
        let mut sum = Measure::ZERO;
        match self {
            Self::Runs(runs) if height == 1 => {
                for run in runs {
                    if run.length() == 0 {
                        return Err(invalid());
                    }
                    sum = sum.add(run.measure())?;
                }
            }
            Self::Children(children) if height > 1 => {
                for child in children {
                    if !child.valid() || child.height != height - 1 {
                        return Err(invalid());
                    }
                    sum = sum.add(child.measure)?;
                }
            }
            _ => return Err(invalid()),
        }
        Ok(sum)
    }
    pub fn split_off(&mut self, at: usize) -> Self {
        match self {
            Self::Runs(v) => Self::Runs(v.split_off(at)),
            Self::Children(v) => Self::Children(v.split_off(at)),
        }
    }
    pub fn append(&mut self, other: Self) -> MappingResult<()> {
        match (self, other) {
            (Self::Runs(a), Self::Runs(mut b)) => a.append(&mut b),
            (Self::Children(a), Self::Children(mut b)) => a.append(&mut b),
            _ => return Err(invalid()),
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Node {
    pub key: [u8; 64],
    pub height: u8,
    pub entries: Entries,
    pub digest: [u8; 32],
}
impl Node {
    pub fn descriptor(&self) -> MappingResult<Descriptor> {
        Ok(Descriptor {
            id: self.key[48..].try_into().map_err(|_| invalid())?,
            digest: self.digest,
            height: self.height,
            measure: self.entries.measure(self.height)?,
        })
    }
    pub fn shape(&self, root: bool) -> bool {
        let min = if root {
            if self.height == 1 { 1 } else { 2 }
        } else {
            8
        };
        (1..=22).contains(&self.height) && (min..=16).contains(&self.entries.len())
    }
}
