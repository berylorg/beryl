use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use super::super::code_panel::{CodePanelDisplayProjection, CodePanelWrapMode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodePanelSourceRevision {
    display_source: Arc<str>,
    copy_source: Arc<str>,
    syntax_label: Option<Arc<str>>,
    copy_opening_fence: Arc<str>,
    copy_closing_fence: Arc<str>,
}

impl CodePanelSourceRevision {
    pub(crate) fn new(
        display_source: &str,
        copy_source: &str,
        syntax_label: Option<&str>,
        copy_opening_fence: &str,
        copy_closing_fence: &str,
    ) -> Self {
        Self {
            display_source: Arc::from(display_source),
            copy_source: Arc::from(copy_source),
            syntax_label: syntax_label.map(Arc::from),
            copy_opening_fence: Arc::from(copy_opening_fence),
            copy_closing_fence: Arc::from(copy_closing_fence),
        }
    }

    pub(crate) fn display_source(&self) -> &str {
        self.display_source.as_ref()
    }

    pub(crate) fn copy_source(&self) -> &str {
        self.copy_source.as_ref()
    }

    pub(crate) fn syntax_label(&self) -> Option<&str> {
        self.syntax_label.as_deref()
    }

    pub(crate) fn copy_opening_fence(&self) -> &str {
        self.copy_opening_fence.as_ref()
    }

    pub(crate) fn copy_closing_fence(&self) -> &str {
        self.copy_closing_fence.as_ref()
    }

    pub(super) fn estimated_retained_bytes(&self) -> usize {
        self.display_source
            .len()
            .saturating_add(self.copy_source.len())
            .saturating_add(self.syntax_label.as_ref().map_or(0, |label| label.len()))
            .saturating_add(self.copy_opening_fence.len())
            .saturating_add(self.copy_closing_fence.len())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CodePanelProjectionRequest {
    pub(super) owner_id: String,
    pub(super) fingerprint: ProjectionFingerprint,
    pub(super) scope_generation: u64,
    pub(super) source_revision: CodePanelSourceRevision,
    pub(super) wrap_mode: CodePanelWrapMode,
}

impl CodePanelProjectionRequest {
    pub(crate) fn project(self) -> CodePanelProjectionCompletion {
        let started_at = Instant::now();
        let projection =
            CodePanelDisplayProjection::new(self.source_revision.display_source(), self.wrap_mode);

        CodePanelProjectionCompletion {
            owner_id: self.owner_id,
            fingerprint: self.fingerprint,
            scope_generation: self.scope_generation,
            source_revision: self.source_revision,
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
    pub(super) source_revision: CodePanelSourceRevision,
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

    pub(super) fn wrap_mode(self) -> CodePanelWrapMode {
        self.wrap_mode
    }
}
