use serde_json::{Value, json};

use crate::graph_tools::{MAX_GRAPH_NEIGHBORHOOD_CHILD_DEPTH, MAX_GRAPH_NEIGHBORHOOD_PARENT_DEPTH};

use super::arguments::{default_child_depth, default_parent_depth};
use super::{MAX_DYNAMIC_NODE_SUMMARY_CHARS, MAX_DYNAMIC_NODE_TITLE_CHARS};

pub(super) fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

pub(super) fn graph_neighborhood_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "anchorNodeId": graph_id_schema("Semantic node id to center the read on. Omit to return bounded root-level information."),
            "parentDepth": {
                "type": "integer",
                "minimum": 0,
                "maximum": MAX_GRAPH_NEIGHBORHOOD_PARENT_DEPTH,
                "default": default_parent_depth()
            },
            "childDepth": {
                "type": "integer",
                "minimum": 0,
                "maximum": MAX_GRAPH_NEIGHBORHOOD_CHILD_DEPTH,
                "default": default_child_depth()
            }
        },
        "additionalProperties": false,
        "examples": [{
            "anchorNodeId": "root",
            "parentDepth": 1,
            "childDepth": 1
        }]
    })
}

pub(super) fn checklist_read_schema() -> Value {
    json!({
        "type": "object",
        "required": ["checklistNodeId"],
        "properties": {
            "checklistNodeId": graph_id_schema("Checklist-capable semantic node id to read.")
        },
        "additionalProperties": false,
        "examples": [{
            "checklistNodeId": "release_checklist"
        }]
    })
}

pub(super) fn upsert_graph_node_schema() -> Value {
    json!({
        "type": "object",
        "required": ["nodeId", "parentId", "title", "summary", "topic", "checklist", "checklistItem"],
        "properties": {
            "nodeId": graph_id_schema("Stable semantic node id to create or update."),
            "parentId": nullable_graph_id_schema("Parent semantic node id. Use null when this node should be root-level."),
            "title": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_DYNAMIC_NODE_TITLE_CHARS,
                "description": "Short user-facing node title."
            },
            "summary": {
                "type": "string",
                "maxLength": MAX_DYNAMIC_NODE_SUMMARY_CHARS,
                "description": "Concise semantic summary for future work context."
            },
            "topic": {
                "type": "boolean",
                "description": "True when this node can be used as a work topic."
            },
            "checklist": {
                "type": "boolean",
                "description": "True when this node owns checklist-item child nodes."
            },
            "checklistItem": {
                "type": "boolean",
                "description": "True when this node is an actionable checklist item. Checklist items must also set topic=true."
            },
            "checklistItemStatus": checklist_item_status_schema_with_description(
                "Required when checklistItem=true. Omit for non-checklist-item nodes."
            )
        },
        "additionalProperties": false,
        "examples": [{
            "nodeId": "root",
            "parentId": null,
            "title": "Root",
            "summary": "Workspace root topic.",
            "topic": true,
            "checklist": false,
            "checklistItem": false
        }]
    })
}

pub(super) fn set_graph_node_parent_schema() -> Value {
    json!({
        "type": "object",
        "required": ["childId", "parentId"],
        "properties": {
            "childId": graph_id_schema("Semantic node id to move."),
            "parentId": nullable_graph_id_schema("New parent semantic node id. Use null to make the child root-level."),
            "index": {
                "type": "integer",
                "minimum": 0,
                "description": "Optional zero-based position among root-level nodes or the new parent's children."
            }
        },
        "additionalProperties": false,
        "examples": [{
            "childId": "root",
            "parentId": null
        }]
    })
}

pub(super) fn upsert_graph_soft_link_schema() -> Value {
    json!({
        "type": "object",
        "required": ["linkId", "sourceId", "targetId", "kind"],
        "properties": {
            "linkId": graph_id_schema("Stable soft-link id to create or update."),
            "sourceId": graph_id_schema("Source semantic node id."),
            "targetId": graph_id_schema("Target semantic node id."),
            "kind": graph_id_schema("Stable lowercase soft-link kind such as depends_on or informs.")
        },
        "additionalProperties": false,
        "examples": [{
            "linkId": "release_depends_on_docs",
            "sourceId": "release",
            "targetId": "docs",
            "kind": "depends_on"
        }]
    })
}

pub(super) fn set_checklist_item_status_schema() -> Value {
    json!({
        "type": "object",
        "required": ["nodeId", "status"],
        "properties": {
            "nodeId": graph_id_schema("Checklist-item semantic node id."),
            "status": checklist_item_status_schema()
        },
        "additionalProperties": false,
        "examples": [{
            "nodeId": "draft_release_notes",
            "status": "done"
        }]
    })
}

fn checklist_item_status_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["todo", "in_progress", "done"]
    })
}

fn checklist_item_status_schema_with_description(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["todo", "in_progress", "done"],
        "description": description
    })
}

fn graph_id_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "pattern": "^[a-z0-9_-]+$",
        "description": description
    })
}

fn nullable_graph_id_schema(description: &str) -> Value {
    json!({
        "type": ["string", "null"],
        "pattern": "^[a-z0-9_-]+$",
        "description": description
    })
}
