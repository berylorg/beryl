use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

use super::super::code_panel::{CodePanelDisplayProjection, CodePanelWrapMode};

#[derive(Clone, Debug)]
pub(crate) struct CodePanelProjectionRequest {
    pub(super) owner_id: String,
    pub(super) fingerprint: ProjectionFingerprint,
    pub(super) scope_generation: u64,
    pub(super) source: String,
    pub(super) wrap_mode: CodePanelWrapMode,
}

impl CodePanelProjectionRequest {
    pub(crate) fn project(self) -> CodePanelProjectionCompletion {
        let started_at = Instant::now();
        let projection = CodePanelDisplayProjection::new(self.source.as_str(), self.wrap_mode);

        CodePanelProjectionCompletion {
            owner_id: self.owner_id,
            fingerprint: self.fingerprint,
            scope_generation: self.scope_generation,
            projection,
            elapsed: started_at.elapsed(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CodePanelProjectionCompletion {
    pub(super) owner_id: String,
    pub(super) fingerprint: ProjectionFingerprint,
    pub(super) scope_generation: u64,
    pub(super) projection: CodePanelDisplayProjection,
    pub(super) elapsed: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProjectionFingerprint {
    pub(super) len: usize,
    hash: u64,
    wrap_mode: CodePanelWrapMode,
}

impl ProjectionFingerprint {
    pub(super) fn new(source: &str, wrap_mode: CodePanelWrapMode) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        wrap_mode.hash(&mut hasher);
        Self {
            len: source.len(),
            hash: hasher.finish(),
            wrap_mode,
        }
    }
}
