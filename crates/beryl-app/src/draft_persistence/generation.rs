use std::{fmt, num::NonZeroU64};

macro_rules! generation {
    ($(#[$meta:meta])* $name:ident, $label:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// First generation admitted by a newly seeded service.
            pub const FIRST: Self = Self(NonZeroU64::MIN);

            /// Returns the exact nonzero generation.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
            }

            pub(crate) fn next(self) -> Result<Self, GenerationExhausted> {
                self.0
                    .checked_add(1)
                    .map(Self)
                    .ok_or(GenerationExhausted { kind: $label })
            }
        }
    };
}

generation!(
    /// Generation of the exact home/thread/draft binding.
    DraftBindingGeneration,
    "draft binding"
);
generation!(
    /// Generation of caller-visible editor content.
    DraftEditGeneration,
    "draft edit"
);
generation!(
    /// Generation of the currently armed autosave deadline.
    DraftTimerGeneration,
    "draft timer"
);
generation!(
    /// Generation of one persistence request.
    DraftRequestGeneration,
    "draft request"
);

/// A monotonic app-owned generation could not advance without wrapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationExhausted {
    kind: &'static str,
}

impl fmt::Display for GenerationExhausted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} generation is exhausted", self.kind)
    }
}

impl std::error::Error for GenerationExhausted {}
