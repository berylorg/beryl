use beryl_model::{
    AdmittedHostPath, Availability, ClaimRevision, PathFlavor, ProjectionRevision, RootId,
    RuntimeId, SyndicPathDigest, SyndicThreadId, UnavailableReason, WindowId,
};

use crate::RecordRevision;

use super::{CatalogCodecError, Decoder, Encoder, invalid};
use crate::catalog::{
    CATALOG_NORMALIZATION_PROFILE, CatalogArchiveSummary, CatalogAvailabilitySummary,
    CatalogClaimKind, CatalogClaimSummary, CatalogExecutionSummary, CatalogLineageSummary,
    CatalogResolvedTitle, CatalogSearchFields, CatalogSourceRevisions, CatalogTitleSource,
};

pub(super) fn encode_sources(encoder: &mut Encoder, value: CatalogSourceRevisions) {
    encoder.u64(value.syndic_summary().get());
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
    let syndic_summary = ProjectionRevision::new(decoder.u64()?)
        .map_err(|source| invalid("catalog Syndic summary revision", source))?;
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
        syndic_summary,
        runtime,
        root,
        claim,
    ))
}

pub(super) fn encode_title(encoder: &mut Encoder, value: &CatalogResolvedTitle) {
    match value.source() {
        CatalogTitleSource::Absent => encoder.u8(0),
        CatalogTitleSource::Generated => {
            encoder.u8(1);
            encoder.text(value.text().expect("generated title has text"));
        }
        CatalogTitleSource::HistoryDerived => {
            encoder.u8(2);
            encoder.text(value.text().expect("history-derived title has text"));
        }
    }
}

pub(super) fn decode_title(
    decoder: &mut Decoder<'_>,
) -> Result<CatalogResolvedTitle, CatalogCodecError> {
    match decoder.u8()? {
        0 => Ok(CatalogResolvedTitle::absent()),
        1 => CatalogResolvedTitle::generated(decoder.text("generated catalog title")?)
            .map_err(|source| invalid("generated catalog title", source)),
        2 => CatalogResolvedTitle::history_derived(decoder.text("history-derived catalog title")?)
            .map_err(|source| invalid("history-derived catalog title", source)),
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "resolved catalog title source",
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
            parent_thread_id,
            depth,
            path_digest,
        } => {
            encoder.u8(1);
            encoder.fixed(parent_thread_id.as_bytes());
            encoder.u64(depth.get());
            encoder.fixed32(path_digest.as_bytes());
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
            decoder.u64()?,
            SyndicPathDigest::from_bytes(decoder.fixed32()?),
        )
        .map_err(|source| invalid("catalog lineage", source)),
        tag => Err(CatalogCodecError::InvalidTag {
            kind: "catalog lineage summary",
            tag,
        }),
    }
}

pub(super) fn encode_search(encoder: &mut Encoder, value: &CatalogSearchFields) {
    let profile = CATALOG_NORMALIZATION_PROFILE;
    encoder.u16(profile.version());
    let unicode = profile.unicode_version();
    encoder.u8(unicode.0);
    encoder.u8(unicode.1);
    encoder.u8(unicode.2);
    encoder.text(value.title());
    encoder.text(value.environment_label());
    encoder.text(value.configured_executable_path());
    encoder.text(value.full_root_path());
}

pub(super) fn decode_search(
    decoder: &mut Decoder<'_>,
    visible_title: &CatalogResolvedTitle,
    visible_execution: &CatalogExecutionSummary,
) -> Result<CatalogSearchFields, CatalogCodecError> {
    let version = decoder.u16()?;
    let unicode = (decoder.u8()?, decoder.u8()?, decoder.u8()?);
    let expected_profile = CATALOG_NORMALIZATION_PROFILE;
    if version != expected_profile.version() || unicode != expected_profile.unicode_version() {
        return Err(CatalogCodecError::InvalidNormalizationProfile { version, unicode });
    }
    let title = decoder.text("normalized catalog title")?;
    let environment = decoder.text("normalized catalog environment label")?;
    let executable = decoder.text("normalized catalog executable path")?;
    let root = decoder.text("normalized catalog root path")?;
    let expected = CatalogSearchFields::from_visible(visible_title, visible_execution)
        .map_err(|source| invalid("catalog search fields", source))?;
    if title != expected.title()
        || environment != expected.environment_label()
        || executable != expected.configured_executable_path()
        || root != expected.full_root_path()
    {
        return Err(CatalogCodecError::SearchFieldsMismatch);
    }
    Ok(expected)
}
