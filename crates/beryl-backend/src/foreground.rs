use std::num::NonZeroUsize;

/// Immutable local limits selected for one foreground backend candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForegroundSessionConfig {
    pre_bind_control_capacity: NonZeroUsize,
}

impl ForegroundSessionConfig {
    /// Creates one foreground profile with a finite pre-bind compact-control capacity.
    #[must_use]
    pub const fn new(pre_bind_control_capacity: NonZeroUsize) -> Self {
        Self {
            pre_bind_control_capacity,
        }
    }

    /// Returns the maximum controls retained before the ordered consumer is bound.
    #[must_use]
    pub const fn pre_bind_control_capacity(self) -> NonZeroUsize {
        self.pre_bind_control_capacity
    }
}

/// Content-free diagnostics for one session's bounded pre-bind control prefix.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreBindControlDiagnostics {
    /// Configured maximum number of retained controls.
    pub capacity: usize,
    /// Controls currently retained before ordered binding.
    pub current: usize,
    /// Greatest observed concurrent retained-control count.
    pub high_water: usize,
    /// Controls successfully admitted to the prefix.
    pub admissions: u64,
    /// Controls rejected because the prefix was full.
    pub full: u64,
}
