use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui::{App, AsyncApp, ClipboardItem, Entity, FontWeight, Pixels, ScrollHandle, px};

use crate::shell::{
    syntax_highlighting::{SyntaxHighlight, SyntaxHighlightCache, SyntaxHighlightRequest},
    theme_candidates::{self, ThemeCandidateFeedbackKind, ThemeCandidatePanelSnapshot},
    transcript_markdown::markdown_code_panel_id,
};

use super::super::code_panel_syntax::resolve_code_panel_syntax_highlight;
use super::super::{
    code_panel::{
        CodePanelDisplayProjectionInput, CodePanelHeader, CodePanelHeaderAction, CodePanelResize,
        CodePanelScrollChrome, CodePanelVerticalWheelOwnership, CodePanelWrapMode,
    },
    code_panel_projection_cache::{
        CodePanelProjectionCache, CodePanelProjectionRequest, CodePanelSourceRevision,
    },
    scrollbars::ScrollbarVisibilityState,
};
use super::{
    TRANSCRIPT_CODE_PANEL_MIN_HEIGHT, TranscriptCodeLayout, TranscriptFrameProfile, TranscriptPanel,
};

#[derive(Clone)]
pub(super) struct TranscriptCodePanelState {
    entity: Entity<TranscriptPanel>,
    soft_wrapped_panel_keys: Arc<HashSet<String>>,
    resized_panel_heights: Arc<HashMap<String, Pixels>>,
    scroll_handles: Rc<RefCell<HashMap<String, ScrollHandle>>>,
    scrollbar_visibility: Arc<HashMap<String, ScrollbarVisibilityState>>,
    selected_nested_code_panel_id: Arc<Option<String>>,
    theme_candidates: Arc<ThemeCandidatePanelSnapshot>,
    syntax_highlight_cache: Rc<RefCell<SyntaxHighlightCache>>,
    display_projection_cache: Rc<RefCell<CodePanelProjectionCache>>,
    rendered_panel_ids: Rc<RefCell<HashSet<String>>>,
    button_font_weight: FontWeight,
    profiler: Option<Rc<TranscriptFrameProfile>>,
}

#[derive(Clone)]
pub(super) struct TranscriptCodePanelControls {
    state: TranscriptCodePanelState,
    row_identity: String,
    block_path: String,
}

pub(super) struct TranscriptCodePanelProjection {
    pub(super) input: CodePanelDisplayProjectionInput,
    pub(super) source_revision: Option<CodePanelSourceRevision>,
}

impl TranscriptCodePanelState {
    pub(super) fn new(
        entity: Entity<TranscriptPanel>,
        soft_wrapped_panel_keys: Arc<HashSet<String>>,
        resized_panel_heights: Arc<HashMap<String, Pixels>>,
        scroll_handles: Rc<RefCell<HashMap<String, ScrollHandle>>>,
        scrollbar_visibility: Arc<HashMap<String, ScrollbarVisibilityState>>,
        selected_nested_code_panel_id: Arc<Option<String>>,
        theme_candidates: Arc<ThemeCandidatePanelSnapshot>,
        syntax_highlight_cache: Rc<RefCell<SyntaxHighlightCache>>,
        display_projection_cache: Rc<RefCell<CodePanelProjectionCache>>,
        rendered_panel_ids: Rc<RefCell<HashSet<String>>>,
        button_font_weight: FontWeight,
        profiler: Option<Rc<TranscriptFrameProfile>>,
    ) -> Self {
        Self {
            entity,
            soft_wrapped_panel_keys,
            resized_panel_heights,
            scroll_handles,
            scrollbar_visibility,
            selected_nested_code_panel_id,
            theme_candidates,
            syntax_highlight_cache,
            display_projection_cache,
            rendered_panel_ids,
            button_font_weight,
            profiler,
        }
    }

    pub(super) fn entity(&self) -> Entity<TranscriptPanel> {
        self.entity.clone()
    }

    pub(super) fn controls_for(
        &self,
        row_identity: impl Into<String>,
        block_path: impl Into<String>,
    ) -> TranscriptCodePanelControls {
        TranscriptCodePanelControls {
            state: self.clone(),
            row_identity: row_identity.into(),
            block_path: block_path.into(),
        }
    }
}

impl TranscriptCodePanelControls {
    pub(super) fn panel_id_for(&self, code_path: &str) -> String {
        let panel_id = markdown_code_panel_id(
            self.row_identity.as_str(),
            self.block_path.as_str(),
            code_path,
        );
        self.state
            .rendered_panel_ids
            .borrow_mut()
            .insert(panel_id.clone());
        panel_id
    }

    pub(super) fn wrap_mode(
        &self,
        panel_id: &str,
        code_layout: TranscriptCodeLayout,
    ) -> CodePanelWrapMode {
        if self.state.soft_wrapped_panel_keys.contains(panel_id) {
            CodePanelWrapMode::Smart {
                columns: code_layout.transcript_bordered_panel_columns,
            }
        } else {
            CodePanelWrapMode::NoWrap
        }
    }

    pub(super) fn header(
        &self,
        panel_id: &str,
        source_revision: Option<&CodePanelSourceRevision>,
    ) -> CodePanelHeader {
        let syntax_label = source_revision.and_then(CodePanelSourceRevision::syntax_label);
        let theme_candidate = theme_candidates::is_theme_candidate_language(syntax_label);
        let leading_actions =
            if let (true, Some(source_revision)) = (theme_candidate, source_revision) {
                self.theme_candidate_actions(panel_id, source_revision.copy_source())
            } else {
                Vec::new()
            };
        let title = theme_candidate
            .then(|| self.theme_candidate_header_title(panel_id))
            .flatten();
        let mut trailing_actions = vec![self.soft_wrap_action(panel_id)];
        if let Some(source_revision) = source_revision {
            trailing_actions.push(self.copy_action(source_revision.copy_source().to_string()));
        }

        CodePanelHeader {
            title,
            leading_actions,
            trailing_actions,
            button_font_weight: self.state.button_font_weight,
        }
    }

    pub(super) fn syntax_highlight(
        &self,
        panel_id: &str,
        source: &str,
        syntax_label: Option<&str>,
        cx: &mut App,
    ) -> Arc<SyntaxHighlight> {
        let entity = self.state.entity.clone();
        resolve_code_panel_syntax_highlight(
            &self.state.syntax_highlight_cache,
            panel_id,
            source,
            syntax_label,
            |request| schedule_syntax_highlight(entity, request, cx),
        )
    }

    pub(super) fn display_projection(
        &self,
        panel_id: &str,
        source_revision: CodePanelSourceRevision,
        wrap_mode: CodePanelWrapMode,
        cx: &mut App,
    ) -> TranscriptCodePanelProjection {
        let lookup = self.state.display_projection_cache.borrow_mut().lookup(
            panel_id,
            source_revision,
            wrap_mode,
        );
        if let Some(request) = lookup.projection_request {
            schedule_code_panel_projection(self.state.entity.clone(), request, cx);
        }

        lookup.ready.map_or(
            TranscriptCodePanelProjection {
                input: CodePanelDisplayProjectionInput::Pending,
                source_revision: None,
            },
            |ready| TranscriptCodePanelProjection {
                input: CodePanelDisplayProjectionInput::Ready(ready.projection),
                source_revision: Some(ready.source_revision),
            },
        )
    }

    pub(super) fn observe_code_panel_render(&self, elapsed: Duration) {
        if let Some(profiler) = self.state.profiler.as_ref() {
            profiler.observe_code_panel_render(elapsed);
        }
    }

    pub(super) fn observe_inline_text_construction(&self, elapsed: Duration) {
        if let Some(profiler) = self.state.profiler.as_ref() {
            profiler.observe_inline_text_construction(elapsed);
        }
    }

    pub(super) fn scroll_chrome(&self, panel_id: &str) -> CodePanelScrollChrome {
        let handle = self.scroll_handle(panel_id);
        let panel_key = panel_id.to_string();
        let activity_panel_key = panel_key.clone();
        let activity_entity = self.state.entity.clone();
        let select_panel_key = panel_key.clone();
        let select_entity = self.state.entity.clone();

        CodePanelScrollChrome {
            handle,
            scrollbar_visibility: self
                .state
                .scrollbar_visibility
                .get(panel_id)
                .cloned()
                .unwrap_or_default()
                .managed(TranscriptPanel::code_panel_scrollbar_update_callback(
                    self.state.entity.clone(),
                )),
            on_activity: Some(Arc::new(move |cx: &mut App| {
                activity_entity.update(cx, |view, cx| {
                    view.note_code_panel_scrollbar_activity(activity_panel_key.clone(), cx);
                });
            })),
            on_select: Some(Arc::new(move |cx: &mut App| {
                select_entity.update(cx, |view, cx| {
                    view.select_nested_code_panel(select_panel_key.clone(), cx);
                });
            })),
            vertical_wheel_ownership: self.vertical_wheel_ownership(panel_id),
        }
    }

    pub(super) fn resize(
        &self,
        panel_id: &str,
        code_layout: TranscriptCodeLayout,
    ) -> CodePanelResize {
        let panel_key = panel_id.to_string();
        let entity = self.state.entity.clone();
        CodePanelResize {
            current_height: self.state.resized_panel_heights.get(panel_id).copied(),
            min_height: px(TRANSCRIPT_CODE_PANEL_MIN_HEIGHT),
            max_height: Some(code_layout.resizable_panel_max_height),
            on_resize_start: Arc::new(move |panel_top, current_height, event, cx| {
                entity.update(cx, |view, cx| {
                    view.begin_code_panel_resize(
                        panel_key.clone(),
                        panel_top,
                        current_height,
                        event,
                        cx,
                    );
                });
            }),
        }
    }

    fn soft_wrap_action(&self, panel_id: &str) -> CodePanelHeaderAction {
        let panel_key = panel_id.to_string();
        let entity = self.state.entity.clone();
        CodePanelHeaderAction {
            key: "soft-wrap".to_string(),
            label: "Soft Wrap".to_string(),
            active: self.state.soft_wrapped_panel_keys.contains(panel_id),
            on_click: Arc::new(move |_, cx: &mut App| {
                entity.update(cx, |view, cx| {
                    view.toggle_code_panel_soft_wrap(panel_key.clone(), cx);
                });
            }),
        }
    }

    fn copy_action(&self, source: String) -> CodePanelHeaderAction {
        CodePanelHeaderAction {
            key: "copy".to_string(),
            label: "Copy".to_string(),
            active: false,
            on_click: Arc::new(move |_, cx: &mut App| {
                cx.write_to_clipboard(ClipboardItem::new_string(source.clone()));
            }),
        }
    }

    fn theme_candidate_actions(&self, panel_id: &str, source: &str) -> Vec<CodePanelHeaderAction> {
        let preview_active = self
            .state
            .theme_candidates
            .active_preview_panel_id()
            .is_some_and(|active| active == panel_id);
        let panel_key = panel_id.to_string();
        let preview_source = source.to_string();
        let entity = self.state.entity.clone();
        let preview_action = if preview_active {
            CodePanelHeaderAction {
                key: "theme-preview-stop".to_string(),
                label: "Stop Preview".to_string(),
                active: true,
                on_click: Arc::new(move |_, cx: &mut App| {
                    entity.update(cx, |view, cx| {
                        view.stop_theme_candidate_preview(panel_key.clone(), cx);
                    });
                }),
            }
        } else {
            CodePanelHeaderAction {
                key: "theme-preview".to_string(),
                label: "Preview".to_string(),
                active: false,
                on_click: Arc::new(move |_, cx: &mut App| {
                    entity.update(cx, |view, cx| {
                        view.preview_theme_candidate(panel_key.clone(), preview_source.clone(), cx);
                    });
                }),
            }
        };

        let install_panel_key = panel_id.to_string();
        let install_source = source.to_string();
        let install_entity = self.state.entity.clone();
        vec![
            preview_action,
            CodePanelHeaderAction {
                key: "theme-install".to_string(),
                label: "Install Theme".to_string(),
                active: false,
                on_click: Arc::new(move |window, cx: &mut App| {
                    install_entity.update(cx, |view, cx| {
                        view.install_theme_candidate(
                            install_panel_key.clone(),
                            install_source.clone(),
                            window,
                            cx,
                        );
                    });
                }),
            },
        ]
    }

    fn theme_candidate_header_title(&self, panel_id: &str) -> Option<String> {
        let feedback = self.state.theme_candidates.feedback(panel_id)?;
        let prefix = match feedback.kind() {
            ThemeCandidateFeedbackKind::Info => "beryl-theme",
            ThemeCandidateFeedbackKind::Success => "beryl-theme",
            ThemeCandidateFeedbackKind::Error => "beryl-theme error",
        };
        Some(format!("{prefix}: {}", feedback.message()))
    }

    fn scroll_handle(&self, panel_id: &str) -> ScrollHandle {
        self.state
            .scroll_handles
            .borrow_mut()
            .entry(panel_id.to_string())
            .or_insert_with(ScrollHandle::new)
            .clone()
    }

    fn vertical_wheel_ownership(&self, panel_id: &str) -> CodePanelVerticalWheelOwnership {
        if self.state.selected_nested_code_panel_id.as_ref().as_deref() == Some(panel_id) {
            CodePanelVerticalWheelOwnership::Panel
        } else {
            CodePanelVerticalWheelOwnership::Parent
        }
    }
}

fn schedule_syntax_highlight(
    panel: Entity<TranscriptPanel>,
    request: SyntaxHighlightRequest,
    cx: &mut App,
) {
    let highlight_task = cx
        .background_executor()
        .spawn(async move { request.highlight() });
    cx.spawn(move |cx: &mut AsyncApp| {
        let mut cx = cx.clone();
        async move {
            let completion = highlight_task.await;
            let _ = panel.update(&mut cx, |view, cx| {
                let result = view
                    .syntax_highlight_cache
                    .borrow_mut()
                    .complete_highlight(completion);
                if let Some(request) = result.follow_up_request {
                    schedule_syntax_highlight(cx.entity(), request, cx);
                }
                if result.display_changed {
                    cx.notify();
                }
            });
        }
    })
    .detach();
}

fn schedule_code_panel_projection(
    panel: Entity<TranscriptPanel>,
    request: CodePanelProjectionRequest,
    cx: &mut App,
) {
    let projection_task = cx
        .background_executor()
        .spawn(async move { request.project() });
    cx.spawn(move |cx: &mut AsyncApp| {
        let mut cx = cx.clone();
        async move {
            let completion = projection_task.await;
            let _ = panel.update(&mut cx, |view, cx| {
                let result = view
                    .code_panel_projection_cache
                    .borrow_mut()
                    .complete_projection(completion);
                if let Some(request) = result.follow_up_request {
                    schedule_code_panel_projection(cx.entity(), request, cx);
                }
                if result.display_changed {
                    cx.notify();
                }
            });
        }
    })
    .detach();
}
