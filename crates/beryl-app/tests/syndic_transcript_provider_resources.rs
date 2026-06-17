use std::ops::Range;

#[path = "support/syndic_transcript_contract.rs"]
mod syndic_transcript_contract;

use syndic_transcript_contract::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_contract::*;

const REVISION: ProviderRevision = ProviderRevision(32);

fn resource_id(name: &str) -> ResourceId {
    ResourceId(format!("resource-{name}"))
}

#[allow(clippy::too_many_arguments)]
fn metadata(
    resource_id: ResourceId,
    kind: ResourceKind,
    media_type: Option<&str>,
    byte_len: u64,
    digest: Option<&str>,
    line_count: Option<u64>,
    row_count: Option<u64>,
    column_count: Option<u64>,
    preview_range: Option<Range<u64>>,
) -> ResourceMetadata {
    ResourceMetadata {
        resource_id,
        revision: ProviderRevision(0),
        kind,
        media_type: media_type.map(str::to_string),
        byte_len,
        digest: digest.map(str::to_string),
        line_count,
        row_count,
        column_count,
        preview_range,
    }
}

fn read_metadata(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    resource_id: &ResourceId,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadResourceMetadata(ResourceMetadataRequest {
                resource_id: resource_id.clone(),
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn read_range(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    resource_id: &ResourceId,
    range: Range<u64>,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadResourceRange(ResourceRangeRequest {
                resource_id: resource_id.clone(),
                range,
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn expect_metadata(kind: TranscriptProviderResponseKind) -> ResourceMetadata {
    match kind {
        TranscriptProviderResponseKind::ResourceMetadata(metadata) => metadata,
        other => panic!("expected resource metadata, got {other:?}"),
    }
}

fn expect_range(kind: TranscriptProviderResponseKind) -> ResourceRangeResponse {
    match kind {
        TranscriptProviderResponseKind::ResourceRange(range) => range,
        other => panic!("expected resource range, got {other:?}"),
    }
}

fn expect_rejection(kind: TranscriptProviderResponseKind) -> TranscriptProviderRejection {
    match kind {
        TranscriptProviderResponseKind::Rejected(rejection) => rejection,
        other => panic!("expected resource rejection, got {other:?}"),
    }
}

fn expect_stale(kind: TranscriptProviderResponseKind) -> TranscriptProviderStale {
    match kind {
        TranscriptProviderResponseKind::Stale(stale) => stale,
        other => panic!("expected stale resource response, got {other:?}"),
    }
}

fn assert_rejection(
    rejection: TranscriptProviderRejection,
    target: TranscriptProviderTarget,
    reason: TranscriptProviderRejectionReason,
    message: Option<&str>,
) {
    assert_eq!(rejection.target, target);
    assert_eq!(rejection.reason, reason);
    assert_eq!(rejection.revision, Some(REVISION));
    assert_eq!(rejection.message.as_deref(), message);
}

#[test]
fn resource_metadata_preserves_shapes_revision_and_normalized_byte_lengths() {
    let code_id = resource_id("code");
    let table_id = resource_id("table");
    let image_id = resource_id("image");
    let code_bytes = b"fn main() {}\nprintln!();\n".to_vec();
    let table_bytes = b"name,count\nalpha,2\nbeta,3\n".to_vec();
    let image_bytes = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 1, 2, 3];
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_resource(
            metadata(
                code_id.clone(),
                ResourceKind::Code,
                Some("text/rust"),
                999,
                Some("sha256:code"),
                Some(2),
                None,
                None,
                Some(0..12),
            ),
            code_bytes.clone(),
        )
        .insert_resource(
            metadata(
                table_id.clone(),
                ResourceKind::Table,
                Some("text/csv"),
                999,
                Some("sha256:table"),
                None,
                Some(3),
                Some(2),
                Some(0..10),
            ),
            table_bytes.clone(),
        )
        .insert_resource(
            metadata(
                image_id.clone(),
                ResourceKind::Image,
                Some("image/png"),
                999,
                Some("sha256:image"),
                None,
                None,
                None,
                None,
            ),
            image_bytes.clone(),
        );

    let code = expect_metadata(read_metadata(&mut provider, 1, &code_id, None));
    assert_eq!(code.resource_id, code_id);
    assert_eq!(code.revision, REVISION);
    assert_eq!(code.kind, ResourceKind::Code);
    assert_eq!(code.media_type.as_deref(), Some("text/rust"));
    assert_eq!(code.byte_len, code_bytes.len() as u64);
    assert_eq!(code.digest.as_deref(), Some("sha256:code"));
    assert_eq!(code.line_count, Some(2));
    assert_eq!(code.row_count, None);
    assert_eq!(code.column_count, None);
    assert_eq!(code.preview_range, Some(0..12));

    let table = expect_metadata(read_metadata(&mut provider, 2, &table_id, Some(REVISION)));
    assert_eq!(table.resource_id, table_id);
    assert_eq!(table.revision, REVISION);
    assert_eq!(table.kind, ResourceKind::Table);
    assert_eq!(table.media_type.as_deref(), Some("text/csv"));
    assert_eq!(table.byte_len, table_bytes.len() as u64);
    assert_eq!(table.digest.as_deref(), Some("sha256:table"));
    assert_eq!(table.line_count, None);
    assert_eq!(table.row_count, Some(3));
    assert_eq!(table.column_count, Some(2));
    assert_eq!(table.preview_range, Some(0..10));

    let image = expect_metadata(read_metadata(&mut provider, 3, &image_id, None));
    assert_eq!(image.resource_id, image_id);
    assert_eq!(image.revision, REVISION);
    assert_eq!(image.kind, ResourceKind::Image);
    assert_eq!(image.media_type.as_deref(), Some("image/png"));
    assert_eq!(image.byte_len, image_bytes.len() as u64);
    assert_eq!(image.digest.as_deref(), Some("sha256:image"));
    assert_eq!(image.line_count, None);
    assert_eq!(image.row_count, None);
    assert_eq!(image.column_count, None);
    assert_eq!(image.preview_range, None);
}

#[test]
fn bounded_range_reads_return_requested_bytes_and_complete_flag() {
    let code_id = resource_id("code-range");
    let attachment_id = resource_id("attachment-range");
    let code_bytes = b"0123456789abcdef".to_vec();
    let attachment_bytes = b"attachment-bytes".to_vec();
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_resource(
            metadata(
                code_id.clone(),
                ResourceKind::Code,
                Some("text/plain"),
                0,
                Some("sha256:range-code"),
                Some(1),
                None,
                None,
                Some(0..4),
            ),
            code_bytes.clone(),
        )
        .insert_resource(
            metadata(
                attachment_id.clone(),
                ResourceKind::Attachment,
                Some("application/octet-stream"),
                0,
                Some("sha256:attachment"),
                None,
                None,
                None,
                Some(0..4),
            ),
            attachment_bytes,
        );

    let partial = expect_range(read_range(&mut provider, 4, &code_id, 2..6, Some(REVISION)));
    assert_eq!(partial.resource_id, code_id);
    assert_eq!(partial.revision, REVISION);
    assert_eq!(partial.kind, ResourceKind::Code);
    assert_eq!(partial.range, 2..6);
    assert_eq!(partial.bytes, b"2345".to_vec());
    assert!(!partial.complete);
    assert!(partial.bytes.len() < code_bytes.len());

    let terminal = expect_range(read_range(&mut provider, 5, &attachment_id, 10..16, None));
    assert_eq!(terminal.resource_id, attachment_id);
    assert_eq!(terminal.revision, REVISION);
    assert_eq!(terminal.kind, ResourceKind::Attachment);
    assert_eq!(terminal.range, 10..16);
    assert_eq!(terminal.bytes, b"-bytes".to_vec());
    assert!(terminal.complete);
}

#[test]
fn missing_unsupported_and_out_of_bounds_resources_are_rejections() {
    let missing_id = resource_id("missing");
    let unsupported_id = resource_id("unsupported");
    let bounded_id = resource_id("bounded");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_resource(
            metadata(
                unsupported_id.clone(),
                ResourceKind::Other("video".to_string()),
                Some("video/unsupported"),
                0,
                None,
                None,
                None,
                None,
                None,
            ),
            b"unsupported-video".to_vec(),
        )
        .reject_resource_with_message(
            unsupported_id.clone(),
            TranscriptProviderRejectionReason::UnsupportedResourceKind,
            "unsupported resource kind",
        )
        .insert_resource(
            metadata(
                bounded_id.clone(),
                ResourceKind::Table,
                Some("text/csv"),
                0,
                Some("sha256:bounded"),
                None,
                Some(1),
                Some(1),
                Some(0..1),
            ),
            b"x".to_vec(),
        );

    assert_rejection(
        expect_rejection(read_metadata(&mut provider, 6, &missing_id, None)),
        TranscriptProviderTarget::Resource(missing_id.clone()),
        TranscriptProviderRejectionReason::MissingResource,
        None,
    );
    assert_rejection(
        expect_rejection(read_range(&mut provider, 7, &missing_id, 0..1, None)),
        TranscriptProviderTarget::Resource(missing_id),
        TranscriptProviderRejectionReason::MissingResource,
        None,
    );
    assert_rejection(
        expect_rejection(read_metadata(&mut provider, 8, &unsupported_id, None)),
        TranscriptProviderTarget::Resource(unsupported_id.clone()),
        TranscriptProviderRejectionReason::UnsupportedResourceKind,
        Some("unsupported resource kind"),
    );
    assert_rejection(
        expect_rejection(read_range(&mut provider, 9, &unsupported_id, 0..4, None)),
        TranscriptProviderTarget::ResourceRange {
            resource_id: unsupported_id,
            range: 0..4,
        },
        TranscriptProviderRejectionReason::UnsupportedResourceKind,
        Some("unsupported resource kind"),
    );
    assert_rejection(
        expect_rejection(read_range(&mut provider, 10, &bounded_id, 2..3, None)),
        TranscriptProviderTarget::ResourceRange {
            resource_id: bounded_id.clone(),
            range: 2..3,
        },
        TranscriptProviderRejectionReason::RangeOutOfBounds,
        None,
    );
    assert_rejection(
        expect_rejection(read_range(&mut provider, 11, &bounded_id, 1..0, None)),
        TranscriptProviderTarget::ResourceRange {
            resource_id: bounded_id,
            range: 1..0,
        },
        TranscriptProviderRejectionReason::RangeOutOfBounds,
        None,
    );
}

#[test]
fn budget_and_policy_rejections_remain_explicit_for_metadata_and_ranges() {
    let budget_id = resource_id("budget");
    let policy_id = resource_id("policy");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .reject_resource_with_message(
            budget_id.clone(),
            TranscriptProviderRejectionReason::BudgetExceeded,
            "resource budget exceeded",
        )
        .reject_resource_with_message(
            policy_id.clone(),
            TranscriptProviderRejectionReason::PolicyDenied,
            "resource policy denied",
        );

    assert_rejection(
        expect_rejection(read_metadata(&mut provider, 12, &budget_id, Some(REVISION))),
        TranscriptProviderTarget::Resource(budget_id.clone()),
        TranscriptProviderRejectionReason::BudgetExceeded,
        Some("resource budget exceeded"),
    );
    assert_rejection(
        expect_rejection(read_range(&mut provider, 13, &budget_id, 0..16, None)),
        TranscriptProviderTarget::ResourceRange {
            resource_id: budget_id,
            range: 0..16,
        },
        TranscriptProviderRejectionReason::BudgetExceeded,
        Some("resource budget exceeded"),
    );
    assert_rejection(
        expect_rejection(read_metadata(&mut provider, 14, &policy_id, None)),
        TranscriptProviderTarget::Resource(policy_id.clone()),
        TranscriptProviderRejectionReason::PolicyDenied,
        Some("resource policy denied"),
    );
    assert_rejection(
        expect_rejection(read_range(
            &mut provider,
            15,
            &policy_id,
            0..16,
            Some(REVISION),
        )),
        TranscriptProviderTarget::ResourceRange {
            resource_id: policy_id,
            range: 0..16,
        },
        TranscriptProviderRejectionReason::PolicyDenied,
        Some("resource policy denied"),
    );
}

#[test]
fn observed_revision_controls_resource_response_identity() {
    let resource_id = resource_id("revision");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION).insert_resource(
        metadata(
            resource_id.clone(),
            ResourceKind::GeneratedImage,
            Some("image/png"),
            0,
            Some("sha256:revision"),
            None,
            None,
            None,
            Some(0..8),
        ),
        b"generated-image-bytes".to_vec(),
    );

    let current_metadata = expect_metadata(read_metadata(
        &mut provider,
        16,
        &resource_id,
        Some(REVISION),
    ));
    assert_eq!(current_metadata.revision, REVISION);

    let current_range = expect_range(read_range(
        &mut provider,
        17,
        &resource_id,
        0..8,
        Some(REVISION),
    ));
    assert_eq!(current_range.revision, REVISION);
    assert_eq!(current_range.bytes, b"generate".to_vec());

    let stale_metadata = expect_stale(read_metadata(
        &mut provider,
        18,
        &resource_id,
        Some(ProviderRevision(31)),
    ));
    assert_eq!(
        stale_metadata.target,
        TranscriptProviderTarget::Resource(resource_id.clone())
    );
    assert_eq!(stale_metadata.observed_revision, Some(ProviderRevision(31)));
    assert_eq!(stale_metadata.current_revision, REVISION);

    let stale_range = expect_stale(read_range(
        &mut provider,
        19,
        &resource_id,
        8..16,
        Some(ProviderRevision(31)),
    ));
    assert_eq!(
        stale_range.target,
        TranscriptProviderTarget::ResourceRange {
            resource_id,
            range: 8..16,
        }
    );
    assert_eq!(stale_range.observed_revision, Some(ProviderRevision(31)));
    assert_eq!(stale_range.current_revision, REVISION);
}
