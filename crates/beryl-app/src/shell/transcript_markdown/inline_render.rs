use super::{
    Image, Inline, MarkdownSourceMap, MarkdownSourceSpan, MathSpan, UnsupportedInline,
    markdown_inline_child_source_path, markdown_inline_source_path,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InlineRenderLine {
    pub(crate) fragments: Vec<InlineRenderFragment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InlineRenderFragment {
    pub(crate) text: String,
    pub(crate) style: InlineRenderStyle,
    pub(crate) source_span: Option<MarkdownSourceSpan>,
    pub(crate) display_source_span: Option<MarkdownSourceSpan>,
    pub(crate) copy_prefix: String,
    pub(crate) copy_suffix: String,
    pub(crate) copy_replacement: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InlineRenderStyle {
    pub(crate) role: InlineRenderRole,
    pub(crate) link: bool,
    pub(crate) emphasis: bool,
    pub(crate) strong: bool,
    pub(crate) fallback: bool,
    pub(crate) atom: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InlineRenderRole {
    Conversation,
    Emphasis,
    StrongEmphasis,
    Code,
}

impl Default for InlineRenderStyle {
    fn default() -> Self {
        Self {
            role: InlineRenderRole::Conversation,
            link: false,
            emphasis: false,
            strong: false,
            fallback: false,
            atom: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct StyleState {
    emphasis: bool,
    strong: bool,
    link: bool,
    fallback: bool,
    source_span: Option<MarkdownSourceSpan>,
}

impl StyleState {
    fn render_style(self) -> InlineRenderStyle {
        InlineRenderStyle {
            role: if self.fallback {
                InlineRenderRole::Code
            } else if self.strong {
                InlineRenderRole::StrongEmphasis
            } else if self.emphasis {
                InlineRenderRole::Emphasis
            } else {
                InlineRenderRole::Conversation
            },
            link: self.link,
            emphasis: !self.fallback && self.emphasis && !self.strong,
            strong: !self.fallback && self.strong,
            fallback: self.fallback,
            atom: false,
        }
    }

    fn emphasis(self) -> Self {
        Self {
            emphasis: true,
            ..self
        }
    }

    fn strong(self) -> Self {
        Self {
            strong: true,
            ..self
        }
    }

    fn link(self) -> Self {
        Self { link: true, ..self }
    }

    fn fallback(self) -> Self {
        Self {
            fallback: true,
            ..self
        }
    }

    fn source_span(self, source_span: Option<MarkdownSourceSpan>) -> Self {
        Self {
            source_span: source_span.or(self.source_span),
            ..self
        }
    }
}

pub(crate) fn inline_render_lines(inlines: &[Inline]) -> Vec<InlineRenderLine> {
    inline_render_lines_with_source_map(inlines, None, "")
}

pub(crate) fn inline_render_lines_with_source_map(
    inlines: &[Inline],
    source_map: Option<&MarkdownSourceMap>,
    parent_path: &str,
) -> Vec<InlineRenderLine> {
    inline_render_lines_with_copy_source(inlines, source_map, parent_path, None)
}

pub(crate) fn inline_render_lines_with_copy_source(
    inlines: &[Inline],
    source_map: Option<&MarkdownSourceMap>,
    parent_path: &str,
    markdown_source: Option<&str>,
) -> Vec<InlineRenderLine> {
    let mut builder = InlineLineBuilder::default();

    for (index, inline) in inlines.iter().enumerate() {
        let path = markdown_inline_source_path(parent_path, index);
        push_inline(
            inline,
            StyleState::default(),
            &mut builder,
            source_map,
            path.as_str(),
            markdown_source,
        );
    }

    builder.finish()
}

#[derive(Debug)]
struct InlineLineBuilder {
    lines: Vec<InlineRenderLine>,
}

impl Default for InlineLineBuilder {
    fn default() -> Self {
        Self {
            lines: vec![InlineRenderLine::default()],
        }
    }
}

impl InlineLineBuilder {
    fn push_text(
        &mut self,
        text: &str,
        style: InlineRenderStyle,
        source_span: Option<MarkdownSourceSpan>,
        markdown_source: Option<&str>,
    ) {
        for (index, chunk) in text.split('\n').enumerate() {
            if index > 0 {
                self.push_line_break();
            }
            if !chunk.is_empty() {
                let copy_source = markdown_copy_source(chunk, source_span, markdown_source);
                self.push_fragment(
                    chunk,
                    style,
                    source_span,
                    copy_source.display_source_span,
                    copy_source.copy_prefix,
                    copy_source.copy_suffix,
                );
            }
        }
    }

    fn push_fragment(
        &mut self,
        text: &str,
        style: InlineRenderStyle,
        source_span: Option<MarkdownSourceSpan>,
        display_source_span: Option<MarkdownSourceSpan>,
        copy_prefix: String,
        copy_suffix: String,
    ) {
        let line = self
            .lines
            .last_mut()
            .expect("inline render builder always keeps a current line");
        if let Some(previous) = line.fragments.last_mut()
            && previous.style == style
            && previous.source_span == source_span
            && previous.display_source_span == display_source_span
            && previous.copy_prefix == copy_prefix
            && previous.copy_suffix == copy_suffix
            && previous.copy_replacement.is_none()
        {
            previous.text.push_str(text);
            return;
        }

        line.fragments.push(InlineRenderFragment {
            text: text.to_string(),
            style,
            source_span,
            display_source_span,
            copy_prefix,
            copy_suffix,
            copy_replacement: None,
        });
    }

    fn push_line_break(&mut self) {
        self.lines.push(InlineRenderLine::default());
    }

    fn finish(self) -> Vec<InlineRenderLine> {
        self.lines
    }
}

fn push_inline(
    inline: &Inline,
    state: StyleState,
    builder: &mut InlineLineBuilder,
    source_map: Option<&MarkdownSourceMap>,
    path: &str,
    markdown_source: Option<&str>,
) {
    let node_source_span = source_map.and_then(|source_map| source_map.inline_span(path));
    let leaf_source_span = state.source_span.or(node_source_span);
    match inline {
        Inline::Text(text) => builder.push_text(
            text,
            state.render_style(),
            leaf_source_span,
            markdown_source,
        ),
        Inline::Emphasis(children) => push_children(
            children,
            state.source_span(node_source_span).emphasis(),
            builder,
            source_map,
            path,
            markdown_source,
        ),
        Inline::Strong(children) => push_children(
            children,
            state.source_span(node_source_span).strong(),
            builder,
            source_map,
            path,
            markdown_source,
        ),
        Inline::Code(source) => {
            let style = InlineRenderStyle {
                role: InlineRenderRole::Code,
                link: state.link,
                emphasis: state.emphasis && !state.strong,
                strong: state.strong,
                fallback: false,
                atom: false,
            };
            builder.push_text(
                source,
                style,
                node_source_span.or(state.source_span),
                markdown_source,
            );
        }
        Inline::Link(link) => {
            let link_state = state.source_span(node_source_span).link();
            if link.children().is_empty() {
                builder.push_text(
                    link.destination(),
                    link_state.render_style(),
                    node_source_span.or(state.source_span),
                    markdown_source,
                );
            } else {
                push_children(
                    link.children(),
                    link_state,
                    builder,
                    source_map,
                    path,
                    markdown_source,
                );
            }
        }
        Inline::Image(image) => builder.push_text(
            &image_fallback_text(image),
            state.fallback().render_style(),
            node_source_span.or(state.source_span),
            markdown_source,
        ),
        Inline::Math(math) => builder.push_text(
            &math_fallback_text(math),
            state.fallback().render_style(),
            node_source_span.or(state.source_span),
            markdown_source,
        ),
        Inline::SoftBreak | Inline::HardBreak => builder.push_line_break(),
        Inline::Unsupported(unsupported) => builder.push_text(
            &unsupported_fallback_text(unsupported),
            state.fallback().render_style(),
            node_source_span.or(state.source_span),
            markdown_source,
        ),
    }
}

fn push_children(
    children: &[Inline],
    state: StyleState,
    builder: &mut InlineLineBuilder,
    source_map: Option<&MarkdownSourceMap>,
    parent_path: &str,
    markdown_source: Option<&str>,
) {
    for (index, child) in children.iter().enumerate() {
        let path = markdown_inline_child_source_path(parent_path, index);
        push_inline(
            child,
            state,
            builder,
            source_map,
            path.as_str(),
            markdown_source,
        );
    }
}

struct MarkdownCopySource {
    display_source_span: Option<MarkdownSourceSpan>,
    copy_prefix: String,
    copy_suffix: String,
}

fn markdown_copy_source(
    display_text: &str,
    source_span: Option<MarkdownSourceSpan>,
    markdown_source: Option<&str>,
) -> MarkdownCopySource {
    if display_text.is_empty() {
        return MarkdownCopySource {
            display_source_span: None,
            copy_prefix: String::new(),
            copy_suffix: String::new(),
        };
    }

    let Some(source_text) =
        markdown_source.and_then(|source| source_span.and_then(|span| span.source_text(source)))
    else {
        return MarkdownCopySource {
            display_source_span: source_span,
            copy_prefix: String::new(),
            copy_suffix: String::new(),
        };
    };
    let Some(display_start) = source_text.find(display_text) else {
        return MarkdownCopySource {
            display_source_span: source_span,
            copy_prefix: String::new(),
            copy_suffix: String::new(),
        };
    };
    let display_end = display_start.saturating_add(display_text.len());
    let Some(prefix) = source_text.get(..display_start) else {
        return MarkdownCopySource {
            display_source_span: source_span,
            copy_prefix: String::new(),
            copy_suffix: String::new(),
        };
    };
    let Some(suffix) = source_text.get(display_end..) else {
        return MarkdownCopySource {
            display_source_span: source_span,
            copy_prefix: String::new(),
            copy_suffix: String::new(),
        };
    };

    let absolute_display_start = source_span.map(|span| span.start().saturating_add(display_start));
    let display_source_span = absolute_display_start
        .and_then(|start| MarkdownSourceSpan::new(start, start + display_text.len()));

    MarkdownCopySource {
        display_source_span,
        copy_prefix: prefix.to_string(),
        copy_suffix: suffix.to_string(),
    }
}

fn image_fallback_text(image: &Image) -> String {
    let mut fallback = format!("![{}]({}", image.alt(), image.destination());
    if let Some(title) = image.title() {
        fallback.push(' ');
        fallback.push('"');
        fallback.push_str(title);
        fallback.push('"');
    }
    fallback.push(')');
    fallback
}

fn math_fallback_text(math: &MathSpan) -> String {
    format!("$${}$$", math.source())
}

fn unsupported_fallback_text(unsupported: &UnsupportedInline) -> String {
    unsupported.source().to_string()
}
