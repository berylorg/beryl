#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum AcceptedInputSchedulerExit {
    Clean,
    PersistentHomeFailure,
    Fatal,
}

impl AcceptedInputSchedulerExit {
    pub(in crate::cas_projection) const fn failed(self) -> bool {
        !matches!(self, Self::Clean)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::accepted_input_scheduler) enum SchedulerFailure {
    PersistentHomeFailure,
    Fatal,
}

impl SchedulerFailure {
    pub(in crate::cas_projection::accepted_input_scheduler) const fn merge(
        self,
        other: Self,
    ) -> Self {
        if matches!(self, Self::Fatal) || matches!(other, Self::Fatal) {
            Self::Fatal
        } else if matches!(self, Self::PersistentHomeFailure)
            || matches!(other, Self::PersistentHomeFailure)
        {
            Self::PersistentHomeFailure
        } else {
            Self::PersistentHomeFailure
        }
    }
}
