use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    semantic_graph::{SemanticGraph, SemanticNodeId},
    threaded_decision::ThreadedDecisionRecordId,
};

const THREADED_DECISION_CONTEXT_HEADER: &str = "Beryl threaded-decision branch context:";

#[derive(Clone, Debug)]
pub(crate) struct ThreadedDecisionBootstrapContextInput<'a> {
    pub(crate) graph: &'a SemanticGraph,
    pub(crate) checklist_item_id: &'a SemanticNodeId,
    pub(crate) checklist_item_title: &'a str,
    pub(crate) checklist_item_summary: &'a str,
    pub(crate) planned_parent_topic_id: Option<&'a SemanticNodeId>,
    pub(crate) parent_thread_id: &'a ConversationThreadId,
    pub(crate) parent_thread_title: Option<&'a str>,
    pub(crate) parent_thread_summary: Option<&'a str>,
    pub(crate) child_thread_id: &'a ConversationThreadId,
    pub(crate) parent_context_turn_id: Option<&'a ConversationTurnId>,
    pub(crate) parent_context_source: Option<&'a str>,
    pub(crate) record_id: &'a ThreadedDecisionRecordId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadedDecisionContextProjection {
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphPathEntry {
    id: String,
    title: String,
    summary: String,
}

impl ThreadedDecisionContextProjection {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

pub(crate) fn threaded_decision_bootstrap_context(
    input: ThreadedDecisionBootstrapContextInput<'_>,
) -> ThreadedDecisionContextProjection {
    let item_title = normalized_display_text(input.checklist_item_title)
        .unwrap_or("Untitled decision")
        .to_string();
    let item_summary = normalized_display_text(input.checklist_item_summary);
    let path = graph_path_entries(
        input.graph,
        input.checklist_item_id,
        &item_title,
        item_summary.unwrap_or(""),
        input.planned_parent_topic_id,
    );

    let mut lines = vec![
        THREADED_DECISION_CONTEXT_HEADER.to_string(),
        format!(
            "- Decision checklist item: {} ({})",
            item_title,
            input.checklist_item_id.as_str()
        ),
    ];
    if let Some(summary) = item_summary {
        lines.push(format!("- Branch purpose: {summary}"));
    }
    if !path.is_empty() {
        let path_titles = path
            .iter()
            .map(|entry| {
                if entry.title.is_empty() {
                    entry.id.as_str()
                } else {
                    entry.title.as_str()
                }
            })
            .collect::<Vec<_>>();
        lines.push(format!("- Graph path: {}", path_titles.join(" > ")));

        let ancestor_summaries = path
            .iter()
            .take(path.len().saturating_sub(1))
            .filter_map(|entry| {
                (!entry.summary.is_empty()).then(|| {
                    let title = if entry.title.is_empty() {
                        entry.id.as_str()
                    } else {
                        entry.title.as_str()
                    };
                    format!("{title}: {}", entry.summary)
                })
            })
            .collect::<Vec<_>>();
        if !ancestor_summaries.is_empty() {
            lines.push("- Graph ancestor summaries:".to_string());
            lines.extend(
                ancestor_summaries
                    .into_iter()
                    .map(|summary| format!("  - {summary}")),
            );
        }
    }
    lines.push(format!(
        "- Parent thread: {}",
        input.parent_thread_id.as_str()
    ));
    if let Some(parent_title) = normalized_display_text(input.parent_thread_title.unwrap_or("")) {
        lines.push(format!("- Parent thread title: {parent_title}"));
    }
    if let Some(parent_summary) = normalized_display_text(input.parent_thread_summary.unwrap_or(""))
    {
        lines.push(format!("- Parent thread summary: {parent_summary}"));
    }
    lines.push(format!(
        "- Child decision thread: {}",
        input.child_thread_id.as_str()
    ));
    if let Some(parent_context_turn_id) = input.parent_context_turn_id {
        lines.push(format!(
            "- Parent context source turn: {}",
            parent_context_turn_id.as_str()
        ));
    }
    if let Some(parent_context_source) =
        normalized_display_text(input.parent_context_source.unwrap_or(""))
    {
        lines.push("- Parent context source content:".to_string());
        lines.extend(parent_context_source.lines().map(|line| {
            if line.trim().is_empty() {
                "  >".to_string()
            } else {
                format!("  > {line}")
            }
        }));
    }
    lines.push(format!("- Decision record: {}", input.record_id.as_str()));
    lines.push(
        "- Progress rule: This bootstrap turn records context and does not mark the decision in progress; progress starts with the first real user-authored exploratory turn in this child thread."
            .to_string(),
    );
    lines.push(
        "- Resolution workflow: Explore only this decision in the child thread. Use ordinary graph tools for navigation and upkeep only; do not use generic checklist/status writes to resolve the decision. When the decision is ready, use the dedicated threaded-decision resolution action/tool with outcome accepted or rejected and a handoff message."
            .to_string(),
    );

    ThreadedDecisionContextProjection {
        text: lines.join("\n"),
    }
}

fn graph_path_entries(
    graph: &SemanticGraph,
    checklist_item_id: &SemanticNodeId,
    checklist_item_title: &str,
    checklist_item_summary: &str,
    planned_parent_topic_id: Option<&SemanticNodeId>,
) -> Vec<GraphPathEntry> {
    if let Some(path) = graph.path_to_root(checklist_item_id) {
        return path
            .iter()
            .map(|node| GraphPathEntry {
                id: node.id().as_str().to_string(),
                title: node.title().trim().to_string(),
                summary: node.summary().trim().to_string(),
            })
            .collect();
    }

    let mut path = planned_parent_topic_id
        .and_then(|topic_id| graph.path_to_root(topic_id))
        .map(|path| {
            path.iter()
                .map(|node| GraphPathEntry {
                    id: node.id().as_str().to_string(),
                    title: node.title().trim().to_string(),
                    summary: node.summary().trim().to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    path.push(GraphPathEntry {
        id: checklist_item_id.as_str().to_string(),
        title: checklist_item_title.trim().to_string(),
        summary: checklist_item_summary.trim().to_string(),
    });
    path
}

fn normalized_display_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
