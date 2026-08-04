use beryl_backend::{
    StreamedInputDescriptor, StreamedInputHeader, StreamedInputSourceError, StreamedTextPage,
    StreamedTextSourceId,
};
use beryl_home_store::HomeStore;
use beryl_model::SealedAssetReferenceSetProof;
use beryl_state::{AssetOwnerHeadRecord, AssetState};
use syndic_storage::{ContentReference, SyndicStorage};

#[cfg(feature = "test-faults")]
use super::diagnostics::OrdinaryInputReplayDiagnostics;
use super::{
    InputReplayContext, InputReplayPrepareError, InputReplayRecord,
    marker::{MarkerReplayAuthority, MarkerReplayService},
    point_limit,
    prepared::{TextReplayAuthority, TextReplayService, check_cancelled},
};
use crate::cas_projection::{ProjectionCancellationToken, connection::StreamedInputBrokerService};

/// Immutable factory for independent replay sources over one exact durable input.
pub(in crate::cas_projection) struct InputReplayFactory {
    template: InputReplayAuthority,
}

/// One non-cloneable replay cursor over an immutable prepared input authority.
pub(in crate::cas_projection) struct InputReplayAuthority {
    kind: InputReplayKind,
}

enum InputReplayKind {
    MarkerFree(Box<TextReplayAuthority>),
    MarkerAware(Box<MarkerReplayAuthority>),
}

impl InputReplayFactory {
    #[allow(
        clippy::too_many_arguments,
        reason = "preparation keeps each exact durable authority explicit"
    )]
    pub(in crate::cas_projection) fn prepare(
        store: &HomeStore,
        storage: SyndicStorage,
        assets: AssetState,
        context: InputReplayContext,
        record: InputReplayRecord,
        content: ContentReference,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
        asset_owner_head: Option<AssetOwnerHeadRecord>,
        cancellation: &ProjectionCancellationToken,
        #[cfg(feature = "test-faults")] diagnostics: OrdinaryInputReplayDiagnostics,
    ) -> Result<Self, InputReplayPrepareError> {
        check_cancelled(cancellation)?;
        context.check_home(store)?;
        record.check_content(content)?;
        record.check_durable(store, storage)?;
        check_cancelled(cancellation)?;
        let manifest = storage
            .content_manifest(store, content.id(), point_limit())?
            .ok_or(InputReplayPrepareError::ContentMissing {
                content_id: content.id(),
            })?;
        if manifest.sealed_reference() != Some(content) {
            return Err(InputReplayPrepareError::ContentChanged {
                content_id: content.id(),
            });
        }
        check_cancelled(cancellation)?;
        let expected_owner_head = asset_owner_head.clone();
        let actual_owner_head = assets.owner_head(store, record.asset_owner())?;
        check_owner_head(&actual_owner_head, &expected_owner_head)?;
        check_cancelled(cancellation)?;
        let kind = if content.summary().image_marker_count() == 0 {
            if asset_reference_set.is_some() || expected_owner_head.is_some() {
                return Err(InputReplayPrepareError::AssetReferenceSetMismatch);
            }
            InputReplayKind::MarkerFree(Box::new(TextReplayAuthority::prepare(
                context.clone(),
                record.clone(),
                assets,
                content,
                #[cfg(feature = "test-faults")]
                diagnostics,
            )?))
        } else {
            let proof =
                asset_reference_set.ok_or(InputReplayPrepareError::AssetReferenceSetMissing)?;
            let owner_head = expected_owner_head
                .clone()
                .ok_or(InputReplayPrepareError::AssetOwnerHeadMissing)?;
            InputReplayKind::MarkerAware(Box::new(MarkerReplayAuthority::prepare(
                store,
                storage,
                assets,
                context.clone(),
                record.clone(),
                content,
                proof,
                owner_head,
                cancellation,
                #[cfg(feature = "test-faults")]
                diagnostics,
            )?))
        };
        check_cancelled(cancellation)?;
        record.check_durable(store, storage)?;
        context.check_home(store)?;
        let final_manifest = storage
            .content_manifest(store, content.id(), point_limit())?
            .ok_or(InputReplayPrepareError::ContentMissing {
                content_id: content.id(),
            })?;
        if final_manifest.sealed_reference() != Some(content) {
            return Err(InputReplayPrepareError::ContentChanged {
                content_id: content.id(),
            });
        }
        check_cancelled(cancellation)?;
        let final_owner_head = assets.owner_head(store, record.asset_owner())?;
        check_owner_head(&final_owner_head, &expected_owner_head)?;
        Ok(Self {
            template: InputReplayAuthority { kind },
        })
    }

    pub(in crate::cas_projection) fn header(&self) -> StreamedInputHeader {
        self.template.header()
    }

    pub(in crate::cas_projection) fn fresh_source(&self) -> InputReplayAuthority {
        self.template.fresh()
    }
}

fn check_owner_head(
    actual: &Option<AssetOwnerHeadRecord>,
    expected: &Option<AssetOwnerHeadRecord>,
) -> Result<(), InputReplayPrepareError> {
    if actual == expected {
        return Ok(());
    }
    Err(if actual.is_none() {
        InputReplayPrepareError::AssetOwnerHeadMissing
    } else {
        InputReplayPrepareError::AssetReferenceSetMismatch
    })
}

impl InputReplayAuthority {
    pub(in crate::cas_projection) fn header(&self) -> StreamedInputHeader {
        match &self.kind {
            InputReplayKind::MarkerFree(authority) => authority.header(),
            InputReplayKind::MarkerAware(authority) => authority.header(),
        }
    }

    fn fresh(&self) -> Self {
        let kind = match &self.kind {
            InputReplayKind::MarkerFree(authority) => {
                InputReplayKind::MarkerFree(Box::new(authority.fresh()))
            }
            InputReplayKind::MarkerAware(authority) => {
                InputReplayKind::MarkerAware(Box::new(authority.fresh()))
            }
        };
        Self { kind }
    }

    pub(in crate::cas_projection) fn service<'a>(
        &'a mut self,
        store: &'a HomeStore,
        storage: SyndicStorage,
        cancellation: &'a ProjectionCancellationToken,
    ) -> InputReplayService<'a> {
        match &mut self.kind {
            InputReplayKind::MarkerFree(authority) => {
                InputReplayService::MarkerFree(authority.service(store, storage, cancellation))
            }
            InputReplayKind::MarkerAware(authority) => {
                InputReplayService::MarkerAware(authority.service(store, storage, cancellation))
            }
        }
    }
}

pub(in crate::cas_projection) enum InputReplayService<'a> {
    MarkerFree(TextReplayService<'a>),
    MarkerAware(MarkerReplayService<'a>),
}

impl StreamedInputBrokerService for InputReplayService<'_> {
    fn header(&self) -> StreamedInputHeader {
        match self {
            Self::MarkerFree(service) => service.header(),
            Self::MarkerAware(service) => service.header(),
        }
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        #[cfg(feature = "test-faults")]
        let diagnostics = self.diagnostics().clone();
        #[cfg(feature = "test-faults")]
        diagnostics.record_source_request();
        let result = match self {
            Self::MarkerFree(service) => service.begin_pass(),
            Self::MarkerAware(service) => service.begin_pass(),
        };
        #[cfg(feature = "test-faults")]
        if result.is_ok() {
            diagnostics.record_pass_started();
        }
        result
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        #[cfg(feature = "test-faults")]
        let diagnostics = self.diagnostics().clone();
        #[cfg(feature = "test-faults")]
        diagnostics.record_source_request();
        let result = match self {
            Self::MarkerFree(service) => service.next_descriptor(),
            Self::MarkerAware(service) => service.next_descriptor(),
        };
        #[cfg(feature = "test-faults")]
        if matches!(result, Ok(Some(_))) {
            diagnostics.record_descriptor_emitted();
        }
        result
    }

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        #[cfg(feature = "test-faults")]
        let diagnostics = self.diagnostics().clone();
        #[cfg(feature = "test-faults")]
        diagnostics.record_source_request();
        #[cfg(feature = "test-faults")]
        let page_request = diagnostics.record_text_page_request();
        let result = match self {
            Self::MarkerFree(service) => service.read_text_page(source_id, start, max_utf8_bytes),
            Self::MarkerAware(service) => service.read_text_page(source_id, start, max_utf8_bytes),
        };
        #[cfg(feature = "test-faults")]
        let result = result.and_then(|page| {
            diagnostics
                .take_source_page_failure(page_request)
                .map_or(Ok(page), Err)
        });
        #[cfg(feature = "test-faults")]
        if let Ok(page) = &result {
            diagnostics.record_logical_text_bytes(page.text().len());
        }
        result
    }

    #[cfg(feature = "test-faults")]
    fn pause_text_page_handoff_for_lifecycle_test(&mut self) {
        let diagnostics = self.diagnostics().clone();
        diagnostics.pause_source_page_handoff(diagnostics.latest_text_page_request());
    }
}

#[cfg(feature = "test-faults")]
impl InputReplayService<'_> {
    fn diagnostics(&self) -> &OrdinaryInputReplayDiagnostics {
        match self {
            Self::MarkerFree(service) => service.diagnostics(),
            Self::MarkerAware(service) => service.diagnostics(),
        }
    }
}
