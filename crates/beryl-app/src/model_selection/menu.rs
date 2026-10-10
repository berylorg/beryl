use std::{collections::VecDeque, ops::Range, sync::Arc};

use super::{ModelContinuation, ModelOptionRecord, ModelPage, ModelQuery, ModelReadError};

pub(crate) const MODEL_ROW_HEIGHT: f32 = 30.;
pub(crate) const MODEL_OVERSCAN: usize = 0;
pub(crate) const RESIDENT_MODEL_PAGES: usize = 2;

pub(crate) struct ResidentModelPage {
    pub(crate) start: usize,
    pub(crate) page: Arc<ModelPage>,
    presentation: Box<[ModelRowPresentation]>,
}

pub(crate) struct ModelRowPresentation {
    pub(crate) id: Arc<str>,
    pub(crate) label: Arc<str>,
}

pub(crate) struct ModelMenuCollection {
    pub(crate) pages: VecDeque<ResidentModelPage>,
    pub(crate) observed: usize,
    pub(crate) complete: bool,
    pub(crate) selected: Option<(usize, ModelOptionRecord)>,
    pub(crate) focused: Option<(usize, ModelOptionRecord)>,
    pub(crate) requested_focus: Option<usize>,
    pub(crate) range: Range<usize>,
    pub(crate) realized: usize,
}

impl ModelMenuCollection {
    pub(crate) fn new() -> Self {
        Self {
            pages: VecDeque::new(),
            observed: 0,
            complete: false,
            selected: None,
            focused: None,
            requested_focus: None,
            range: 0..0,
            realized: 0,
        }
    }

    pub(crate) fn logical_count(&self) -> usize {
        self.observed.saturating_add(usize::from(!self.complete))
    }

    pub(crate) fn row(&self, index: usize) -> Option<(&Arc<ModelPage>, &ModelOptionRecord)> {
        self.pages.iter().find_map(|resident| {
            index
                .checked_sub(resident.start)
                .and_then(|offset| resident.page.records().get(offset))
                .map(|record| (&resident.page, record))
        })
    }

    pub(crate) fn presentation(&self, index: usize) -> Option<&ModelRowPresentation> {
        self.pages.iter().find_map(|resident| {
            index
                .checked_sub(resident.start)
                .and_then(|offset| resident.presentation.get(offset))
        })
    }

    pub(crate) fn install(
        &mut self,
        start: usize,
        page: Arc<ModelPage>,
        selected_model: Option<&str>,
    ) {
        let end = start.saturating_add(page.records().len());
        self.observed = self.observed.max(end);
        if page.continuation().is_none() {
            self.complete = true;
        }
        for (offset, record) in page.records().iter().enumerate() {
            let index = start + offset;
            if selected_model == Some(record.model.as_str()) {
                self.selected = Some((index, record.clone()));
            }
            if self.requested_focus == Some(index) {
                self.focused = Some((index, record.clone()));
                self.requested_focus = None;
            }
        }
        self.pages.retain(|resident| resident.start != start);
        while self.pages.len() >= RESIDENT_MODEL_PAGES {
            let evicted = self
                .pages
                .iter()
                .position(|resident| {
                    resident.start >= self.range.end
                        || resident.start + resident.page.records().len() <= self.range.start
                })
                .unwrap_or(0);
            self.pages.remove(evicted);
        }
        let presentation = page
            .records()
            .iter()
            .map(|record| ModelRowPresentation {
                id: Arc::from(format!("model-option-{}", record.id)),
                label: Arc::from(record.label.replace(['\r', '\n'], " ")),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        self.pages.push_back(ResidentModelPage {
            start,
            page,
            presentation,
        });
    }

    pub(crate) fn request_for(&self, target: usize) -> (usize, Option<ModelContinuation>) {
        self.pages
            .iter()
            .rev()
            .find_map(|resident| {
                let end = resident.start + resident.page.records().len();
                (end <= target)
                    .then(|| {
                        resident
                            .page
                            .continuation()
                            .cloned()
                            .map(|cursor| (end, Some(cursor)))
                    })
                    .flatten()
            })
            .unwrap_or((0, None))
    }

    pub(crate) fn reveal_focus(&mut self, index: usize) {
        if let Some((_, record)) = self.row(index) {
            self.focused = Some((index, record.clone()));
            self.requested_focus = None;
        } else {
            self.requested_focus = Some(index);
        }
    }
}

pub(crate) struct ModelPageArrival {
    pub(crate) start: usize,
    pub(crate) page: Arc<ModelPage>,
    pub(crate) selected: Option<(usize, ModelOptionRecord)>,
}

pub(crate) struct ModelPageFailure {
    pub(crate) start: usize,
    pub(crate) continuation: Option<ModelContinuation>,
    pub(crate) error: ModelReadError,
    pub(crate) retry_page: bool,
    pub(crate) waiting_election: bool,
}

fn election_contended(error: &ModelReadError) -> bool {
    matches!(
        error,
        ModelReadError::Busy
            | ModelReadError::Pending
            | ModelReadError::Source(
                crate::cas_projection::ModelSourceError::Publication
                    | crate::cas_projection::ModelSourceError::Runtime(
                        crate::cas_projection::RuntimeModelReadError::Busy
                    )
            )
    )
}

#[cfg(test)]
pub(crate) fn test_error_kind(error: &ModelReadError) -> String {
    use crate::cas_projection::{ModelSourceError, RuntimeModelReadError};
    match error {
        ModelReadError::Source(ModelSourceError::Runtime(RuntimeModelReadError::Backend(
            error,
        ))) => {
            let name = format!("{error:?}");
            format!(
                "Backend.{}",
                name.split([' ', '{', '(']).next().unwrap_or("Unknown")
            )
        }
        ModelReadError::Source(ModelSourceError::Runtime(error)) => format!("Runtime.{error:?}"),
        ModelReadError::Source(error) => format!("Source.{error:?}"),
        error => format!("Reader.{error:?}"),
    }
}

pub(crate) fn read_model_range(
    query: &Arc<ModelQuery>,
    mut start: usize,
    mut continuation: Option<ModelContinuation>,
    target: usize,
    retry: bool,
    selected_model: Option<&str>,
) -> Result<ModelPageArrival, ModelPageFailure> {
    let mut retry = retry;
    let mut selected = None;
    loop {
        #[cfg(test)]
        eprintln!("model range worker: validate start={start} retry={retry}");
        query.revalidate().map_err(|error| ModelPageFailure {
            start,
            continuation: continuation.clone(),
            waiting_election: election_contended(&error),
            error,
            retry_page: query.has_failed_page(),
        })?;
        #[cfg(test)]
        eprintln!("model range worker: validated start={start}");
        query
            .with_current(|| ())
            .map_err(|error| ModelPageFailure {
                start,
                continuation: continuation.clone(),
                waiting_election: election_contended(&error),
                error,
                retry_page: query.has_failed_page(),
            })?;
        let page = if retry {
            query.retry_page()
        } else {
            query.read_page(continuation.as_ref())
        }
        .map_err(|error| ModelPageFailure {
            start,
            continuation: continuation.clone(),
            error,
            retry_page: query.has_failed_page(),
            waiting_election: false,
        })?;
        #[cfg(test)]
        eprintln!(
            "model range worker: page start={start} records={}",
            page.records().len()
        );
        retry = false;
        if let Some((offset, record)) = page
            .records()
            .iter()
            .enumerate()
            .find(|(_, record)| selected_model == Some(record.model.as_str()))
        {
            selected = Some((start + offset, record.clone()));
        }
        let end = start + page.records().len();
        if target < end || page.continuation().is_none() {
            return Ok(ModelPageArrival {
                start,
                page: Arc::new(page),
                selected,
            });
        }
        continuation = page.continuation().cloned();
        start = end;
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/model_menu.rs"
    ));
}
