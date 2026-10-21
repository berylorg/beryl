use std::{num::{NonZeroU64, NonZeroUsize}, sync::atomic::Ordering};

use beryl_backend::{GeneratedOutputError, GeneratedReaderAllowance};
use beryl_home_store::{AdmittedSidecar, SidecarByteLimit, SidecarError, SidecarNamespace};
use beryl_model::{AssetId, GeneratedMediaUnavailableReason as Reason, RuntimeNativePath};
use beryl_state::{AssetDimensions, AssetMediaType, GeneratedMediaFacts};
use syndic_storage::GeneratedMediaSourceProof;

use crate::{cas_projection::{AuthorizedGeneratedSource, GeneratedRuntimeSource}, image_processing::process::GeneratedSourceStageLease};
use super::{coordinator::{Result, Services, failure}, header::{GeneratedPngHeader, HeaderError}};

pub(super) enum Admission {
    Prepared(AdmittedSidecar, GeneratedMediaFacts),
    Unavailable(Reason),
    Pending,
}

struct SourceCustody<'a> {
    source: Option<GeneratedRuntimeSource>,
    services: &'a Services,
    _stages: GeneratedSourceStageLease,
}

impl Drop for SourceCustody<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() { self.services.shared.stop(); }
        if let Some(source) = self.source.take() { close(self.services, source); }
    }
}

fn close(services: &Services, mut source: GeneratedRuntimeSource) {
    loop {
        match source.close() {
            Ok(()) => return,
            Err(failure) => { (_, source) = failure.into_parts(); services.shared.cleanup_wait(); }
        }
    }
}

pub(super) fn classify(error: GeneratedOutputError) -> Option<Reason> {
    match error {
        GeneratedOutputError::Busy | GeneratedOutputError::Cancelled | GeneratedOutputError::CleanupPending => None,
        GeneratedOutputError::LostSourceAuthority => Some(Reason::LostSourceAuthority),
        GeneratedOutputError::Missing => Some(Reason::Missing),
        GeneratedOutputError::Unsupported => Some(Reason::Unsupported),
        GeneratedOutputError::Changed => Some(Reason::Changed),
        GeneratedOutputError::InvalidPath | GeneratedOutputError::Io | GeneratedOutputError::Protocol => Some(Reason::Unreadable),
    }
}

fn read_reason(error: &std::io::Error) -> Option<Reason> {
    if let Some(error) = error.get_ref().and_then(|e| e.downcast_ref::<GeneratedOutputError>()) {
        return classify(*error);
    }
    Some(match error.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => Reason::Changed,
        _ => Reason::Unreadable,
    })
}

pub(super) fn admit(services: &Services, proof: &GeneratedMediaSourceProof, authorized: AuthorizedGeneratedSource) -> Result<Admission> {
    let Some(saved_path) = proof.saved_path() else { return Ok(Admission::Pending); };
    let path = match saved_path {
        Ok(path) => path,
        Err(reason) => return Ok(Admission::Unavailable(reason)),
    };
    let root = proof.execution_snapshot().execution().root_path();
    let path = match RuntimeNativePath::from_admitted(root.mode().clone(), root.flavor(), path) {
        Ok(path) => path,
        Err(_) => return Ok(Admission::Unavailable(Reason::Unreadable)),
    };
    let stages = match services.process.acquire_generated_source_stages() {
        Ok(stages) => stages,
        Err(_) => {
            services.shared.capacity_wait();
            match services.process.acquire_generated_source_stages() {
                Ok(stages) => stages,
                Err(_) => return Ok(Admission::Pending),
            }
        }
    };
    let config = services.process.limits().configuration();
    let allowance = GeneratedReaderAllowance::new(config.generated_media_worker_slots).map_err(failure)?;
    let source = match authorized.open(path, allowance, services.shared.cancelled.clone()) {
        Ok(source) => source,
        Err(error) => {
            let (error, source) = error.into_parts();
            if let Some(source) = source { close(services, source); }
            drop(stages);
            return Ok(classify(error).map(Admission::Unavailable).unwrap_or(Admission::Pending));
        }
    };
    let custody = SourceCustody { source: Some(source), services, _stages: stages };
    let source = custody.source.as_ref().expect("owned generated source");
    let admitted = (|| {
        let Some(length) = NonZeroU64::new(source.length()) else { return Ok(Admission::Unavailable(Reason::Unsupported)); };
        let header = match GeneratedPngHeader::read(source.cursor()) {
            Ok(header) => header,
            Err(HeaderError::Unsupported | HeaderError::Malformed) => return Ok(Admission::Unavailable(Reason::Unsupported)),
            Err(HeaderError::Read(error)) => return Ok(read_reason(&error).map(Admission::Unavailable).unwrap_or(Admission::Pending)),
        };
        let sidecar = services.home.admit_sidecar_stream(
            SidecarNamespace::new("images").expect("fixed image namespace"), length.get(),
            SidecarByteLimit::new(NonZeroU64::MAX),
            NonZeroUsize::new(config.generated_media_page_bytes.min(config.sidecar_io_page_bytes)).expect("validated page bound"),
            || Ok(header.bind(source.cursor())),
            || services.shared.cancelled.load(Ordering::Acquire),
        );
        let sidecar = match sidecar {
            Ok(sidecar) => sidecar,
            Err(SidecarError::Cancelled) => return Ok(Admission::Pending),
            Err(SidecarError::Source { source }) => return Ok(read_reason(&source).map(Admission::Unavailable).unwrap_or(Admission::Pending)),
            Err(SidecarError::LengthMismatch { .. } | SidecarError::ContentMismatch) => return Ok(Admission::Unavailable(Reason::Changed)),
            Err(error) => return Err(failure(error)),
        };
        let asset = AssetId::sha256_v1(sidecar.address().digest().as_bytes(), length);
        let [width, height] = header.dimensions();
        let facts = GeneratedMediaFacts::new(asset, AssetMediaType::new("image/png").expect("fixed PNG media type"), Some(AssetDimensions::new(
            NonZeroU64::new(u64::from(width)).expect("validated width"), NonZeroU64::new(u64::from(height)).expect("validated height"),
        ))).map_err(failure)?;
        Ok(Admission::Prepared(sidecar, facts))
    })();
    drop(custody);
    admitted
}
