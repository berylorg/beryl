fn dynamic_container(kind: ContainerKind) -> DynamicToolArgumentContainer {
    match kind {
        ContainerKind::Object => DynamicToolArgumentContainer::Object,
        ContainerKind::Array => DynamicToolArgumentContainer::Array,
    }
}

fn dynamic_scalar_kind(
    kind: ScalarKind,
) -> Result<DynamicToolArgumentScalarKind, DynamicToolCallSchemaError> {
    match kind {
        ScalarKind::Name => Ok(DynamicToolArgumentScalarKind::ObjectName),
        ScalarKind::String => Ok(DynamicToolArgumentScalarKind::String),
        ScalarKind::Number => Ok(DynamicToolArgumentScalarKind::Number),
    }
}

fn parse_dynamic_request_number(
    value: &str,
) -> Result<DynamicToolCallRequestId, DynamicToolCallSchemaError> {
    value
        .parse::<i64>()
        .map(DynamicToolCallRequestId::Integer)
        .map_err(|_| DynamicToolCallSchemaError::InvalidRequestIdentity)
}
