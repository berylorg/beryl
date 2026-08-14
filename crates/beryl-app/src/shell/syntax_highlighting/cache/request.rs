use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

use super::super::{
    highlight_syntax_for_language,
    model::{SyntaxHighlight, SyntaxLanguage},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SyntaxHighlightCacheKey {
    owner_id: String,
    pub(super) language: SyntaxLanguage,
}

impl SyntaxHighlightCacheKey {
    pub(crate) fn new(owner_id: impl Into<String>, language: SyntaxLanguage) -> Self {
        Self {
            owner_id: owner_id.into(),
            language,
        }
    }

    pub(crate) fn owner_id(&self) -> &str {
        self.owner_id.as_str()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SyntaxHighlightRequest {
    pub(super) key: SyntaxHighlightCacheKey,
    pub(super) fingerprint: SourceFingerprint,
    pub(super) scope_generation: u64,
    pub(super) source: String,
}

impl SyntaxHighlightRequest {
    pub(crate) fn highlight(self) -> SyntaxHighlightCompletion {
        let started_at = Instant::now();
        let highlight = highlight_syntax_for_language(self.source.as_str(), self.key.language);

        SyntaxHighlightCompletion {
            key: self.key,
            fingerprint: self.fingerprint,
            scope_generation: self.scope_generation,
            highlight,
            elapsed: started_at.elapsed(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SyntaxHighlightCompletion {
    pub(super) key: SyntaxHighlightCacheKey,
    pub(super) fingerprint: SourceFingerprint,
    pub(super) scope_generation: u64,
    pub(super) highlight: SyntaxHighlight,
    pub(super) elapsed: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceFingerprint {
    pub(super) len: usize,
    hash: u64,
}

impl SourceFingerprint {
    pub(super) fn new(source: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self {
            len: source.len(),
            hash: hasher.finish(),
        }
    }
}
