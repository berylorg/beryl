use std::{ops::Range, sync::Arc};

use beryl_model::SyndicItemId;

pub const ROW_HEIGHT: f32 = 28.;
pub const OVERSCAN: u64 = 4;
pub const DEFAULT_HEIGHT: f32 = 144.;
pub const MIN_HEIGHT: f32 = 56.;
pub const HANDLE_HEIGHT: f32 = 6.;
pub const FEEDBACK_HEIGHT: f32 = 32.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Query {
    pub owner: u64,
    pub period: u64,
    pub revision: u64,
    pub total: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Running,
    FinishedOk,
    FinishedError,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub id: SyndicItemId,
    pub status: Status,
    pub agent: Arc<str>,
    pub value: Arc<str>,
    pub tooltip: Arc<str>,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub start: u64,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Demand {
    pub query: Query,
    pub range: Range<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feedback {
    pub identity: u64,
    pub explanation: Arc<str>,
    pub pending: bool,
    pub retry_available: bool,
}

pub struct Model {
    pub query: Query,
    pub pages: Vec<Arc<Page>>,
    pub feedback: Option<Feedback>,
    pub inert: bool,
    pub first: u64,
    pub fraction: f32,
    pub top_attached: bool,
    pub pending: Option<Demand>,
    viewport_height: f32,
    anchor: Option<SyndicItemId>,
}

impl Model {
    pub fn new(query: Query) -> Self {
        Self {
            query,
            pages: Vec::new(),
            feedback: None,
            inert: true,
            first: 0,
            fraction: 0.,
            top_attached: true,
            pending: None,
            viewport_height: 0.,
            anchor: None,
        }
    }

    pub fn row(&self, ordinal: u64) -> Option<&Row> {
        self.pages.iter().find_map(|page| {
            let offset = ordinal.checked_sub(page.start)?;
            usize::try_from(offset)
                .ok()
                .and_then(|offset| page.rows.get(offset))
        })
    }

    pub fn replace(&mut self, query: Query, pages: Vec<Arc<Page>>) -> Result<(), &'static str> {
        validate_pages(query, &pages)?;
        let period_changed = self.query.owner != query.owner || self.query.period != query.period;
        let query_changed = self.query != query;
        if period_changed {
            self.first = 0;
            self.fraction = 0.;
            self.top_attached = true;
            self.anchor = None;
        } else if query_changed && !self.top_attached && self.anchor.is_none() {
            self.anchor = self.row(self.first).map(|row| row.id);
        }
        if query_changed {
            self.pending = None;
            self.feedback = None;
        }
        self.query = query;
        self.pages = pages;
        if self.top_attached {
            self.first = 0;
            self.fraction = 0.;
        } else if let Some(anchor) = self.anchor {
            if let Some(ordinal) = self.pages.iter().find_map(|page| {
                page.rows
                    .iter()
                    .position(|row| row.id == anchor)
                    .map(|index| page.start + index as u64)
            }) {
                self.first = ordinal;
                self.anchor = None;
            }
        }
        self.clamp();
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| self.covers(&pending.range))
        {
            self.pending = None;
        }
        Ok(())
    }

    pub fn set_feedback(&mut self, feedback: Option<Feedback>) -> Result<(), &'static str> {
        if feedback
            .as_ref()
            .is_some_and(|feedback| feedback.explanation.len() > 512)
        {
            return Err("Activity feedback exceeds its display bound");
        }
        self.feedback = feedback;
        Ok(())
    }

    pub fn set_viewport(&mut self, height: f32) {
        self.viewport_height = if height.is_finite() {
            height.max(0.)
        } else {
            0.
        };
        self.clamp();
    }

    pub fn viewport_height(&self) -> f32 {
        self.viewport_height
    }

    pub fn visible(&self) -> Range<u64> {
        let rows = ((self.viewport_height + self.fraction) / ROW_HEIGHT).ceil() as u64;
        self.first..self.first.saturating_add(rows).min(self.query.total)
    }

    pub fn realized_range(&self) -> Range<u64> {
        let visible = self.visible();
        visible.start.saturating_sub(OVERSCAN)
            ..visible.end.saturating_add(OVERSCAN).min(self.query.total)
    }

    pub fn request(&mut self) -> Option<Demand> {
        let range = self.realized_range();
        if self.inert || self.viewport_height == 0. || range.is_empty() || self.covers(&range) {
            return None;
        }
        let demand = Demand {
            query: self.query,
            range,
        };
        if self.pending.as_ref() == Some(&demand) {
            return None;
        }
        self.pending = Some(demand.clone());
        Some(demand)
    }

    pub fn covers(&self, range: &Range<u64>) -> bool {
        let mut next = range.start;
        for _ in 0..4 {
            if next >= range.end {
                return true;
            }
            let Some(page) = self.pages.iter().find(|page| {
                page.start <= next && page.start.saturating_add(page.rows.len() as u64) > next
            }) else {
                return false;
            };
            next = page.start.saturating_add(page.rows.len() as u64);
        }
        next >= range.end
    }

    pub fn scroll_by(&mut self, delta: f32) -> bool {
        if self.inert || !delta.is_finite() || delta == 0. {
            return false;
        }
        let old = (self.first, self.fraction);
        let local = f64::from(self.fraction) + f64::from(delta);
        let steps = (local / f64::from(ROW_HEIGHT)).floor() as i64;
        if steps < 0 && steps.unsigned_abs() > self.first {
            self.first = 0;
            self.fraction = 0.;
        } else {
            self.first = self.first.saturating_add_signed(steps);
            self.fraction = local.rem_euclid(f64::from(ROW_HEIGHT)) as f32;
        }
        self.clamp();
        self.top_attached = self.first == 0 && self.fraction == 0.;
        self.anchor = None;
        old != (self.first, self.fraction)
    }

    pub fn move_to_end(&mut self, end: bool) -> bool {
        if self.inert {
            return false;
        }
        let old = (self.first, self.fraction);
        (self.first, self.fraction) = if end { self.maximum() } else { (0, 0.) };
        self.top_attached = !end || self.first == 0 && self.fraction == 0.;
        self.anchor = None;
        old != (self.first, self.fraction)
    }

    pub fn offset(&self) -> f64 {
        self.first as f64 * f64::from(ROW_HEIGHT) + f64::from(self.fraction)
    }

    pub fn content_height(&self) -> f64 {
        self.query.total as f64 * f64::from(ROW_HEIGHT)
    }

    fn maximum(&self) -> (u64, f32) {
        let rows = (f64::from(self.viewport_height) / f64::from(ROW_HEIGHT)).ceil() as u64;
        if self.query.total < rows {
            return (0, 0.);
        }
        let remainder = self.viewport_height.rem_euclid(ROW_HEIGHT);
        (
            self.query.total - rows,
            if remainder == 0. {
                0.
            } else {
                ROW_HEIGHT - remainder
            },
        )
    }

    fn clamp(&mut self) {
        let maximum = self.maximum();
        if self.first > maximum.0 || self.first == maximum.0 && self.fraction > maximum.1 {
            (self.first, self.fraction) = maximum;
        }
    }
}

fn validate_pages(query: Query, pages: &[Arc<Page>]) -> Result<(), &'static str> {
    if pages.len() > 4 {
        return Err("Activity has more than four resident pages");
    }
    let mut ids = Vec::with_capacity(256);
    for (index, page) in pages.iter().enumerate() {
        let end = page
            .start
            .checked_add(page.rows.len() as u64)
            .ok_or("Activity page overflows")?;
        if page.rows.len() > 64 || end > query.total {
            return Err("Activity page exceeds its bounded query");
        }
        if pages[..index].iter().any(|prior| {
            page.start < prior.start.saturating_add(prior.rows.len() as u64) && prior.start < end
        }) {
            return Err("Activity resident pages overlap");
        }
        for row in &page.rows {
            if row.agent.len() > 2048 || row.value.len() > 1152 || row.tooltip.len() > 3072 {
                return Err("Activity row exceeds its display bound");
            }
            if ids.contains(&row.id) {
                return Err("Activity resident identity is repeated");
            }
            ids.push(row.id);
        }
    }
    Ok(())
}
