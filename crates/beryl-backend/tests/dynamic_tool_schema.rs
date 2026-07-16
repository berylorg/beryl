use beryl_backend::{DynamicToolFunctionSpec, DynamicToolNamespaceSpec, DynamicToolSpec};
use serde_json::json;

#[test]
fn canonical_namespace_serializes_exact_tagged_function_shape() {
    let spec = DynamicToolSpec::from(DynamicToolNamespaceSpec::new(
        "beryl",
        "Beryl-owned tools.",
        vec![
            DynamicToolFunctionSpec::new(
                "inspect",
                "Inspect bounded state.",
                json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            )
            .with_defer_loading(false),
        ],
    ));

    assert_eq!(
        serde_json::to_value(spec).unwrap(),
        json!({
            "type": "namespace",
            "name": "beryl",
            "description": "Beryl-owned tools.",
            "tools": [{
                "type": "function",
                "name": "inspect",
                "description": "Inspect bounded state.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                },
                "deferLoading": false
            }]
        })
    );
}

#[test]
fn legacy_flat_and_mixed_specs_are_not_accepted() {
    let legacy = json!({
        "name": "inspect",
        "description": "Inspect bounded state.",
        "inputSchema": {},
        "namespace": "beryl"
    });
    assert!(serde_json::from_value::<DynamicToolSpec>(legacy).is_err());

    let mixed = json!({
        "type": "function",
        "name": "inspect",
        "description": "Inspect bounded state.",
        "inputSchema": {},
        "namespace": "beryl"
    });
    assert!(serde_json::from_value::<DynamicToolSpec>(mixed).is_err());
}

#[test]
fn canonical_top_level_function_round_trips() {
    let value = json!({
        "type": "function",
        "name": "inspect",
        "description": "Inspect bounded state.",
        "inputSchema": true
    });
    let spec: DynamicToolSpec = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(spec, DynamicToolSpec::Function(_)));
    assert_eq!(serde_json::to_value(spec).unwrap(), value);
}
