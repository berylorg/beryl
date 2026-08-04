use beryl_backend::{
    DynamicToolArgumentContainer, DynamicToolArgumentControl, DynamicToolArgumentScalarKind,
};

use super::*;

fn begin_resolution() -> BranchResolutionArgumentBuilder {
    let mut builder = BranchResolutionArgumentBuilder::new();
    builder.control(DynamicToolArgumentControl::ContainerStart(
        DynamicToolArgumentContainer::Object,
    ));
    builder.control(DynamicToolArgumentControl::ScalarStart(
        DynamicToolArgumentScalarKind::ObjectName,
    ));
    builder.fragment(
        DynamicToolArgumentScalarKind::ObjectName,
        0,
        b"resolution",
    );
    builder.control(DynamicToolArgumentControl::ScalarEnd(
        DynamicToolArgumentScalarKind::ObjectName,
    ));
    builder.control(DynamicToolArgumentControl::ScalarStart(
        DynamicToolArgumentScalarKind::String,
    ));
    builder
}

fn feed_fragmented(builder: &mut BranchResolutionArgumentBuilder, value: &[u8], chunk: usize) {
    for (index, fragment) in value.chunks(chunk).enumerate() {
        builder.fragment(
            DynamicToolArgumentScalarKind::String,
            u64::try_from(index * chunk).unwrap(),
            fragment,
        );
    }
}

fn finish_resolution(
    mut builder: BranchResolutionArgumentBuilder,
) -> Result<BranchDiscussionResolutionRequest, DynamicToolRejection> {
    builder.control(DynamicToolArgumentControl::ScalarEnd(
        DynamicToolArgumentScalarKind::String,
    ));
    builder.control(DynamicToolArgumentControl::ContainerEnd(
        DynamicToolArgumentContainer::Object,
    ));
    builder.seal()
}

#[test]
fn exact_scalar_and_utf8_boundaries_are_retained() {
    let scalar_boundary = "a".repeat(BRANCH_RESOLUTION_MAX_SCALARS);
    let mut scalar_builder = begin_resolution();
    feed_fragmented(&mut scalar_builder, scalar_boundary.as_bytes(), 257);
    let scalar_request = finish_resolution(scalar_builder).unwrap();
    assert_eq!(scalar_request.resolution(), scalar_boundary);

    let utf8_boundary = "\u{10ffff}".repeat(BRANCH_RESOLUTION_MAX_SCALARS);
    assert_eq!(utf8_boundary.len(), BRANCH_RESOLUTION_MAX_UTF8_BYTES);
    let mut utf8_builder = begin_resolution();
    feed_fragmented(&mut utf8_builder, utf8_boundary.as_bytes(), 4096);
    let utf8_request = finish_resolution(utf8_builder).unwrap();
    assert_eq!(utf8_request.resolution(), utf8_boundary);
}

#[test]
fn fragmented_scalar_and_byte_overflow_are_rejected() {
    let mut scalar_builder = begin_resolution();
    let exact = "a".repeat(BRANCH_RESOLUTION_MAX_SCALARS);
    feed_fragmented(&mut scalar_builder, exact.as_bytes(), 1024);
    scalar_builder.fragment(
        DynamicToolArgumentScalarKind::String,
        u64::try_from(exact.len()).unwrap(),
        b"a",
    );
    assert_eq!(
        finish_resolution(scalar_builder).err(),
        Some(DynamicToolSchemaRejection::StringTooLong.into())
    );

    let mut byte_builder = begin_resolution();
    let exact = "\u{10ffff}".repeat(BRANCH_RESOLUTION_MAX_SCALARS);
    feed_fragmented(&mut byte_builder, exact.as_bytes(), 4096);
    byte_builder.fragment(
        DynamicToolArgumentScalarKind::String,
        u64::try_from(exact.len()).unwrap(),
        b"a",
    );
    assert_eq!(
        finish_resolution(byte_builder).err(),
        Some(DynamicToolSchemaRejection::StringTooLong.into())
    );
}

#[test]
fn invalid_utf8_and_empty_products_are_rejected() {
    let mut invalid = begin_resolution();
    invalid.fragment(DynamicToolArgumentScalarKind::String, 0, &[0x80]);
    assert_eq!(
        finish_resolution(invalid).err(),
        Some(DynamicToolSchemaRejection::InvalidScalarFragment.into())
    );

    assert_eq!(
        finish_resolution(begin_resolution()).err(),
        Some(DynamicToolSchemaRejection::EmptyString.into())
    );
}
