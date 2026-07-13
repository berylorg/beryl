use beryl_model::{
    AdmittedHostPath, Availability, ClaimRevision, PathFlavor, RootId, RuntimeId, SyndicThreadId,
    ThreadRevision, UnavailableReason, WindowId,
};

use crate::RecordRevision;

use super::{CatalogCodecError, Decoder, Encoder, invalid};
use crate::catalog::{
    CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimKind, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogLineageSummary, CatalogSearchFields, CatalogSourceRevisions,
    CatalogTitleCandidate, CatalogTitleFacts,
};

pub(super) fn encode_sources(encoder: &mut Encoder, value: CatalogSourceRevisions) {
    encoder.u64(value.thread().get());
    encoder.u64(value.thread_metadata().get());
    encoder.u64(value.runtime().get());
    encoder.u64(value.root().get());
    match value.claim() {
        Some(revision) => {
            encoder.u8(1);
            encoder.u64(revision.get());
        }
        None => encoder.u8(0),
    }
}

pub(super) fn decode_sources(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogSourceRevisions, CatalogCodecError> {
    let thread = ThreadRevision::new(decoder.u64()?)
        .map_err(|source| invalid("catalog Syndic source revision", source))?;
    let metadata = RecordRevision::new(decoder.u64()?)
        .map_err(|source| invalid("catalog metadata source revision", source))?;
    let runtime = RecordRevision::new(decoder.u64()?)
        .map_err(|source| invalid("catalog runtime source revision", source))?;
    let root = RecordRevision::new(decoder.u64()?)
        .map_err(|source| invalid("catalog root source revision", source))?;
    let claim = match decoder.u8()? {
        0 => None,
        1 => Some(
            ClaimRevision::new(decoder.u64()?)
                .map_err(|source| invalid("catalog claim source revision", source))?,
        ),
        tag => {
            return Err(CatalogCodecError::InvalidTag {
                kind: "catalog claim source option",
                tag,
            });
        }
    };
    Ok(CatalogSourceRevisions::new(
        thread, metadata, runtime, root, claim,
    ))
}

pub(super) fn encode_titles(encoder: &mut Encoder, value: &CatalogTitleFacts) {
    encode_title_candidate(encoder, value.generated());
    encode_title_candidate(encoder, value.syndic());
}

fn encode_title_candidate(encoder: &mut Encoder, value: Option<&CatalogTitleCandidate>) {
    match value {
        Some(value) => {
            encoder.u8(1);
            encoder.text(value.text());
            encoder.u64(value.source_thread_revision().get());
        }
        None => encoder.u8(0),
    }
}

pub(super) fn decode_titles(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogTitleFacts, CatalogCodecError> {
    Ok(CatalogTitleFacts::new(
        decode_title_candidate(decoder, "generated title")?,
        decode_title_candidate(decoder, "Syndic title")?,
    ))
}

fn decode_title_candidate(
    decoder: &mut Decoder<'_>,
    kind: &'static str,
) -> Result<Option<CatalogTitleCandidate>, CatalogCodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => {
            let text = decoder.text(kind)?;
            let revision = ThreadRevision::new(decoder.u64()?)
                .map_err(|source| invalid("catalog title source revision", source))?;
            CatalogTitleCandidate::new(text, revision)
                .map(Some)
                .map_err(|source| invalid(kind, source))
        }
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "catalog title option",
            tag,
        }),
    }
}

pub(super) fn encode_execution(encoder: &mut Encoder, value: &CatalogExecutionSummary) {
    encoder.fixed(value.runtime_id().as_bytes());
    encoder.fixed(value.root_id().as_bytes());
    encoder.text(value.environment_label());
    encode_path(encoder, value.configured_executable_path());
    encode_path(encoder, value.full_root_path());
    encode_availability(encoder, value.availability().runtime());
    encode_availability(encoder, value.availability().root());
}

pub(super) fn decode_execution(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogExecutionSummary, CatalogCodecError> {
    let runtime_id = RuntimeId::from_bytes(decoder.fixed()?);
    let root_id = RootId::from_bytes(decoder.fixed()?);
    let environment_label = decoder.text("catalog environment label")?;
    let executable = decode_path(decoder)?;
    let root = decode_path(decoder)?;
    let availability = CatalogAvailabilitySummary::new(
        decode_availability(decoder)?,
        decode_availability(decoder)?,
    );
    CatalogExecutionSummary::new(
        runtime_id,
        root_id,
        environment_label,
        executable,
        root,
        availability,
    )
    .map_err(|source| invalid("catalog execution summary", source))
}

fn encode_path(encoder: &mut Encoder, value: &AdmittedHostPath) {
    encoder.u8(match value.flavor() {
        PathFlavor::Windows => 0,
        PathFlavor::Posix => 1,
    });
    encoder.text(value.as_str());
}

fn decode_path(decoder: &mut Decoder<'_>) -> Result<AdmittedHostPath, CatalogCodecError> {
    let flavor = match decoder.u8()? {
        0 => PathFlavor::Windows,
        1 => PathFlavor::Posix,
        tag => {
            return Err(CatalogCodecError::InvalidTag {
                kind: "catalog host-path flavor",
                tag,
            });
        }
    };
    AdmittedHostPath::from_admitted(flavor, decoder.text("catalog host path")?)
        .map_err(|source| invalid("catalog host path", source))
}

fn encode_availability(encoder: &mut Encoder, value: Availability) {
    match value {
        Availability::Unknown => encoder.u8(0),
        Availability::Available => encoder.u8(1),
        Availability::Unavailable(reason) => {
            encoder.u8(2);
            encoder.u8(encode_unavailable_reason(reason));
        }
    }
}

fn decode_availability(decoder: &mut Decoder<'_>) -> Result<Availability, CatalogCodecError> {
    match decoder.u8()? {
        0 => Ok(Availability::Unknown),
        1 => Ok(Availability::Available),
        2 => decode_unavailable_reason(decoder.u8()?).map(Availability::Unavailable),
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "catalog availability",
            tag,
        }),
    }
}

fn encode_unavailable_reason(reason: UnavailableReason) -> u8 {
    match reason {
        UnavailableReason::NotFound => 0,
        UnavailableReason::AccessDenied => 1,
        UnavailableReason::EnvironmentUnavailable => 2,
        UnavailableReason::BackendUnavailable => 3,
        UnavailableReason::StoreUnavailable => 4,
        UnavailableReason::OpenElsewhere => 5,
        UnavailableReason::Unsupported => 6,
        UnavailableReason::Invalid => 7,
    }
}

fn decode_unavailable_reason(tag: u8) -> Result<UnavailableReason, CatalogCodecError> {
    match tag {
        0 => Ok(UnavailableReason::NotFound),
        1 => Ok(UnavailableReason::AccessDenied),
        2 => Ok(UnavailableReason::EnvironmentUnavailable),
        3 => Ok(UnavailableReason::BackendUnavailable),
        4 => Ok(UnavailableReason::StoreUnavailable),
        5 => Ok(UnavailableReason::OpenElsewhere),
        6 => Ok(UnavailableReason::Unsupported),
        7 => Ok(UnavailableReason::Invalid),
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "catalog unavailable reason",
            tag,
        }),
    }
}

pub(super) fn decode_archive(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogArchiveSummary, CatalogCodecError> {
    match decoder.u8()? {
        0 => Ok(CatalogArchiveSummary::Ordinary),
        1 => Ok(CatalogArchiveSummary::BranchDiscussionOpen),
        2 => Ok(CatalogArchiveSummary::BranchDiscussionArchived),
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "catalog archive summary",
            tag,
        }),
    }
}

pub(super) fn encode_claim(encoder: &mut Encoder, value: CatalogClaimSummary) {
    match value {
        CatalogClaimSummary::Unclaimed => encoder.u8(0),
        CatalogClaimSummary::Claimed { window_id, kind } => {
            encoder.u8(1);
            encoder.fixed(window_id.as_bytes());
            encoder.u8(match kind {
                CatalogClaimKind::Active => 0,
                CatalogClaimKind::Restoring => 1,
            });
        }
    }
}

pub(super) fn decode_claim(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogClaimSummary, CatalogCodecError> {
    match decoder.u8()? {
        0 => Ok(CatalogClaimSummary::Unclaimed),
        1 => {
            let window_id = WindowId::from_bytes(decoder.fixed()?);
            let kind = match decoder.u8()? {
                0 => CatalogClaimKind::Active,
                1 => CatalogClaimKind::Restoring,
                tag => {
                    return Err(CatalogCodecError::InvalidTag {
                        kind: "catalog claim kind",
                        tag,
                    });
                }
            };
            Ok(CatalogClaimSummary::claimed(window_id, kind))
        }
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "catalog claim summary",
            tag,
        }),
    }
}

pub(super) fn encode_lineage(encoder: &mut Encoder, value: CatalogLineageSummary) {
    match value {
        CatalogLineageSummary::TopLevel => encoder.u8(0),
        CatalogLineageSummary::Descendant {
            top_level_thread_id,
            parent_thread_id,
            depth,
        } => {
            encoder.u8(1);
            encoder.fixed(top_level_thread_id.as_bytes());
            encoder.fixed(parent_thread_id.as_bytes());
            encoder.u16(depth.get());
        }
    }
}

pub(super) fn decode_lineage(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogLineageSummary, CatalogCodecError> {
    match decoder.u8()? {
        0 => Ok(CatalogLineageSummary::TopLevel),
        1 => CatalogLineageSummary::descendant(
            SyndicThreadId::from_bytes(decoder.fixed()?),
            SyndicThreadId::from_bytes(decoder.fixed()?),
            decoder.u16()?,
        )
        .map_err(|source| invalid("catalog lineage", source)),
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "catalog lineage summary",
            tag,
        }),
    }
}

pub(super) fn encode_search(encoder: &mut Encoder, value: &CatalogSearchFields) {
    encoder.text(value.title());
    encoder.text(value.environment_label());
    encoder.text(value.configured_executable_path());
    encoder.text(value.full_root_path());
}

pub(super) fn decode_search(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogSearchFields, CatalogCodecError> {
    let title = decoder.text("normalized catalog title")?;
    let environment = decoder.text("normalized catalog environment label")?;
    let executable = decoder.text("normalized catalog executable path")?;
    let root = decoder.text("normalized catalog root path")?;
    CatalogSearchFields::from_admitted_normalized(title, environment, executable, root)
        .map_err(|source| invalid("catalog search fields", source))
}
