use beryl_home_store::HomeStore;
use beryl_state::{AssetSidecarState, AssetState};
use gpui_text_input::{MutationPageItem, ObjectChange, RangeTextInputRequest};

use crate::composer_host::ComposerHostImageMarkerMetadata;

use super::MainWindowComposerSelectionIdentity;

pub struct MainWindowComposerMarkerMetadataAuthority {
    assets: AssetState,
}

pub(in crate::main_window) struct AuthenticatedComposerMarkerMetadataPage {
    selection: MainWindowComposerSelectionIdentity,
    mutation: Option<gpui_text_input::MutationPageKey>,
    metadata: Box<[ComposerHostImageMarkerMetadata]>,
}

impl MainWindowComposerMarkerMetadataAuthority {
    pub const fn new(assets: AssetState) -> Self {
        Self { assets }
    }

    pub(in crate::main_window) fn assets(&self) -> AssetState {
        self.assets.clone()
    }

    pub(in crate::main_window) fn authenticate(
        &self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        request: &RangeTextInputRequest,
        metadata: Box<[ComposerHostImageMarkerMetadata]>,
    ) -> Result<AuthenticatedComposerMarkerMetadataPage, String> {
        let page = match request {
            RangeTextInputRequest::MutationSourcePage(request)
            | RangeTextInputRequest::MutationProposalPage(request) => Some(request.page()),
            _ => None,
        };
        let mutation = page.map(|page| page.key());
        let mut supplied = Vec::new();
        if let Some(page) = page {
            for item in page.items() {
                if let MutationPageItem::Object(
                    ObjectChange::Insert { object } | ObjectChange::Replace { object, .. },
                ) = item
                {
                    supplied.push(object.id());
                }
            }
        }
        if metadata.len() != supplied.len() {
            return Err("composer marker metadata does not match the mutation page".to_owned());
        }
        let mut admitted = Vec::with_capacity(metadata.len());
        for value in metadata.iter().copied() {
            if admitted.contains(&value.object_id()) || !supplied.contains(&value.object_id()) {
                return Err("composer marker metadata identity is duplicate or foreign".to_owned());
            }
            let asset = self
                .assets
                .metadata(store, value.asset_id())
                .map_err(|error| format!("composer asset metadata read failed: {error}"))?
                .ok_or_else(|| "composer marker asset is not admitted".to_owned())?;
            if asset.asset_id() != value.asset_id()
                || asset.sidecar_state() != AssetSidecarState::Committed
            {
                return Err("composer marker asset metadata is not committed".to_owned());
            }
            admitted.push(value.object_id());
        }
        if supplied.iter().any(|id| !admitted.contains(id)) {
            return Err(
                "composer marker insertion or replacement omitted authenticated metadata"
                    .to_owned(),
            );
        }
        Ok(AuthenticatedComposerMarkerMetadataPage {
            selection,
            mutation,
            metadata,
        })
    }
}

impl AuthenticatedComposerMarkerMetadataPage {
    pub(in crate::main_window) fn into_metadata(
        self,
        selection: MainWindowComposerSelectionIdentity,
        request: &RangeTextInputRequest,
    ) -> Result<Box<[ComposerHostImageMarkerMetadata]>, String> {
        let mutation = match request {
            RangeTextInputRequest::MutationSourcePage(request)
            | RangeTextInputRequest::MutationProposalPage(request) => Some(request.page().key()),
            _ => None,
        };
        if self.selection != selection || self.mutation != mutation {
            return Err("composer marker metadata page is stale".to_owned());
        }
        Ok(self.metadata)
    }
}
