use gpui_text_input::{
    InlineObjectActivation, InlineObjectRealizationLoss, RealizedInlineObjectAnchor,
};

use super::MainWindowComposerSelectionIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerMarkerCommand {
    View,
    Remove,
}

pub const COMPOSER_MARKER_COMMANDS: [ComposerMarkerCommand; 2] =
    [ComposerMarkerCommand::View, ComposerMarkerCommand::Remove];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerPreviewCommand {
    Copy,
    Save,
}

pub const COMPOSER_PREVIEW_COMMANDS: [ComposerPreviewCommand; 2] =
    [ComposerPreviewCommand::Copy, ComposerPreviewCommand::Save];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerImagePresentationState {
    Pending,
    LocalUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerImagePreviewCommandState {
    DisabledPending,
    DisabledUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ComposerMarkerFocusTarget {
    OriginMarker(RealizedInlineObjectAnchor),
    ComposerEditor,
    ThreadSelector,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComposerMarkerMenu {
    selection: MainWindowComposerSelectionIdentity,
    anchor: RealizedInlineObjectAnchor,
}

impl ComposerMarkerMenu {
    pub const fn commands(&self) -> &[ComposerMarkerCommand; 2] {
        &COMPOSER_MARKER_COMMANDS
    }

    pub const fn anchor(&self) -> RealizedInlineObjectAnchor {
        self.anchor
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComposerImagePreviewShell {
    selection: MainWindowComposerSelectionIdentity,
    origin: RealizedInlineObjectAnchor,
    state: ComposerImagePresentationState,
}

impl ComposerImagePreviewShell {
    pub const fn origin(self) -> RealizedInlineObjectAnchor {
        self.origin
    }

    pub const fn state(self) -> ComposerImagePresentationState {
        self.state
    }

    pub const fn commands(self) -> &'static [ComposerPreviewCommand; 2] {
        &COMPOSER_PREVIEW_COMMANDS
    }

    pub const fn command_state(self) -> ComposerImagePreviewCommandState {
        match self.state {
            ComposerImagePresentationState::Pending => {
                ComposerImagePreviewCommandState::DisabledPending
            }
            ComposerImagePresentationState::LocalUnavailable => {
                ComposerImagePreviewCommandState::DisabledUnavailable
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerMarkerActivationDisposition {
    Opened,
    DuplicateSuppressed,
}

#[derive(Default)]
pub struct MainWindowComposerImageSurfaces {
    menu: Option<ComposerMarkerMenu>,
    preview: Option<ComposerImagePreviewShell>,
}

impl MainWindowComposerImageSurfaces {
    pub const fn menu(&self) -> Option<ComposerMarkerMenu> {
        self.menu
    }

    pub const fn preview(&self) -> Option<ComposerImagePreviewShell> {
        self.preview
    }

    pub fn activate_marker(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        activation: InlineObjectActivation,
    ) -> Result<ComposerMarkerActivationDisposition, ComposerMarkerSurfaceError> {
        validate_anchor(selection, activation.anchor)?;
        if let Some(menu) = self.menu.as_mut().filter(|menu| {
            menu.selection == selection && same_stable_anchor(menu.anchor, activation.anchor)
        }) {
            menu.anchor = activation.anchor;
            return Ok(ComposerMarkerActivationDisposition::DuplicateSuppressed);
        }
        self.preview = None;
        self.menu = Some(ComposerMarkerMenu {
            selection,
            anchor: activation.anchor,
        });
        Ok(ComposerMarkerActivationDisposition::Opened)
    }

    pub fn invoke_view(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        state: ComposerImagePresentationState,
    ) -> Result<(), ComposerMarkerSurfaceError> {
        let menu = self.menu.ok_or(ComposerMarkerSurfaceError::NoMenu)?;
        if menu.selection != selection {
            return Err(ComposerMarkerSurfaceError::StaleSelection);
        }
        self.menu = None;
        self.preview = Some(ComposerImagePreviewShell {
            selection,
            origin: menu.anchor,
            state,
        });
        Ok(())
    }

    pub fn invoke_remove(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<RealizedInlineObjectAnchor, ComposerMarkerSurfaceError> {
        let anchor = self.prepare_remove(selection)?;
        self.menu = None;
        Ok(anchor)
    }

    pub fn prepare_remove(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<RealizedInlineObjectAnchor, ComposerMarkerSurfaceError> {
        let menu = self.menu.ok_or(ComposerMarkerSurfaceError::NoMenu)?;
        if menu.selection != selection {
            return Err(ComposerMarkerSurfaceError::StaleSelection);
        }
        Ok(menu.anchor)
    }

    pub fn realization_lost(&mut self, loss: InlineObjectRealizationLoss) {
        if self.menu.is_some_and(|menu| menu.anchor == loss.anchor) {
            self.menu = None;
        }
        if self
            .preview
            .is_some_and(|preview| preview.origin == loss.anchor)
        {
            self.preview = None;
        }
    }

    pub fn dismiss_menu(
        &mut self,
        origin_eligible: bool,
        editor_eligible: bool,
    ) -> Option<ComposerMarkerFocusTarget> {
        let menu = self.menu.take()?;
        Some(focus_fallback(
            menu.anchor,
            origin_eligible,
            editor_eligible,
        ))
    }

    pub fn dismiss_preview(
        &mut self,
        origin_eligible: bool,
        editor_eligible: bool,
    ) -> Option<ComposerMarkerFocusTarget> {
        let preview = self.preview.take()?;
        Some(focus_fallback(
            preview.origin,
            origin_eligible,
            editor_eligible,
        ))
    }

    pub fn selection_changed(&mut self, selection: MainWindowComposerSelectionIdentity) {
        if self.menu.is_some_and(|menu| menu.selection != selection) {
            self.menu = None;
        }
        if self
            .preview
            .is_some_and(|preview| preview.selection != selection)
        {
            self.preview = None;
        }
    }

    pub fn clear(&mut self) {
        self.menu = None;
        self.preview = None;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ComposerMarkerSurfaceError {
    #[error("marker activation belongs to a stale composer selection")]
    StaleSelection,
    #[error("no marker command menu is open")]
    NoMenu,
}

fn validate_anchor(
    selection: MainWindowComposerSelectionIdentity,
    anchor: RealizedInlineObjectAnchor,
) -> Result<(), ComposerMarkerSurfaceError> {
    if anchor.binding != selection.binding().range_binding()
        || anchor.presentation_generation.get()
            != selection.binding().presentation_generation().get()
    {
        return Err(ComposerMarkerSurfaceError::StaleSelection);
    }
    Ok(())
}

fn focus_fallback(
    origin: RealizedInlineObjectAnchor,
    origin_eligible: bool,
    editor_eligible: bool,
) -> ComposerMarkerFocusTarget {
    if origin_eligible {
        ComposerMarkerFocusTarget::OriginMarker(origin)
    } else if editor_eligible {
        ComposerMarkerFocusTarget::ComposerEditor
    } else {
        ComposerMarkerFocusTarget::ThreadSelector
    }
}

fn same_stable_anchor(left: RealizedInlineObjectAnchor, right: RealizedInlineObjectAnchor) -> bool {
    left.binding == right.binding
        && left.object_id == right.object_id
        && left.order == right.order
        && left.presentation_generation == right.presentation_generation
}
