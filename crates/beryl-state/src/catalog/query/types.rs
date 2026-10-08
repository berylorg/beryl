use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogQueryScope {
    All,
    Runtime(RuntimeId),
    Root {
        runtime_id: RuntimeId,
        root_id: RootId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogQueryCriteria {
    scope: CatalogQueryScope,
    search: CatalogNormalizedQuery,
}
impl CatalogQueryCriteria {
    pub fn new(scope: CatalogQueryScope, search: CatalogNormalizedQuery) -> Self {
        Self { scope, search }
    }
    pub const fn scope(&self) -> CatalogQueryScope {
        self.scope
    }
    pub const fn search(&self) -> &CatalogNormalizedQuery {
        &self.search
    }
    pub(super) fn matches(&self, row: &CatalogRow) -> bool {
        let execution = row.facts().execution();
        let scope = match self.scope {
            CatalogQueryScope::All => true,
            CatalogQueryScope::Runtime(id) => execution.runtime_id() == id,
            CatalogQueryScope::Root {
                runtime_id,
                root_id,
            } => execution.runtime_id() == runtime_id && execution.root_id() == root_id,
        };
        scope && row.facts().search().matches(&self.search)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogQueryToken {
    pub(super) owner: u64,
    pub(super) generation: HomeGenerationIdentity,
    pub(super) id: u64,
}
impl CatalogQueryToken {
    pub const fn generation_identity(&self) -> HomeGenerationIdentity {
        self.generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogQueryCursor {
    pub(super) token: CatalogQueryToken,
    pub(super) after: CatalogRecencyCursor,
    pub(super) offset: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogQueryPageLimit {
    items: usize,
    bytes: usize,
}
impl CatalogQueryPageLimit {
    pub fn new(items: usize, bytes: usize) -> Result<Self, CatalogQueryError> {
        if items == 0
            || items > CATALOG_QUERY_PAGE_MAX_ITEMS
            || bytes == 0
            || bytes > CATALOG_QUERY_PAGE_MAX_BYTES
        {
            return Err(CatalogQueryError::Limit);
        }
        Ok(Self { items, bytes })
    }
    pub const fn maximum() -> Self {
        Self {
            items: CATALOG_QUERY_PAGE_MAX_ITEMS,
            bytes: CATALOG_QUERY_PAGE_MAX_BYTES,
        }
    }
    pub const fn max_items(self) -> usize {
        self.items
    }
    pub const fn max_bytes(self) -> usize {
        self.bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogQueryRow {
    pub(super) catalog: CatalogRow,
    pub(super) runtime_root: RuntimeRootCatalogSource,
}
impl CatalogQueryRow {
    pub const fn catalog(&self) -> &CatalogRow {
        &self.catalog
    }
    pub const fn runtime_root(&self) -> &RuntimeRootCatalogSource {
        &self.runtime_root
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogQueryScopePresentation {
    All,
    Runtime(RuntimeRecord),
    Root(RuntimeRootCatalogSource),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogQueryPage {
    pub(super) rows: Vec<CatalogQueryRow>,
    pub(super) offset: u64,
    pub(super) bytes: usize,
    pub(super) next: Option<CatalogQueryCursor>,
}
impl CatalogQueryPage {
    pub fn rows(&self) -> &[CatalogQueryRow] {
        &self.rows
    }
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    pub const fn charged_bytes(&self) -> usize {
        self.bytes
    }
    pub fn next_cursor(&self) -> Option<&CatalogQueryCursor> {
        self.next.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogQueryOpened {
    pub(super) token: CatalogQueryToken,
    pub(super) count: u64,
    pub(super) revision: DomainRevision,
    pub(super) home_revision: HomeRevision,
    pub(super) criteria: CatalogQueryCriteria,
    pub(super) scope: CatalogQueryScopePresentation,
    pub(super) first: CatalogQueryPage,
}
impl CatalogQueryOpened {
    pub const fn token(&self) -> &CatalogQueryToken {
        &self.token
    }
    pub const fn count(&self) -> u64 {
        self.count
    }
    pub const fn catalog_revision(&self) -> DomainRevision {
        self.revision
    }
    pub const fn home_revision(&self) -> HomeRevision {
        self.home_revision
    }
    pub const fn criteria(&self) -> &CatalogQueryCriteria {
        &self.criteria
    }
    pub const fn scope_presentation(&self) -> &CatalogQueryScopePresentation {
        &self.scope
    }
    pub const fn first_page(&self) -> &CatalogQueryPage {
        &self.first
    }
    pub fn into_parts(self) -> (CatalogQueryToken, u64, CatalogQueryPage) {
        (self.token, self.count, self.first)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogQueryPosition {
    pub(super) index: u64,
    pub(super) before: Option<CatalogQueryCursor>,
}
impl CatalogQueryPosition {
    pub const fn index(&self) -> u64 {
        self.index
    }
    pub fn cursor_before(&self) -> Option<&CatalogQueryCursor> {
        self.before.as_ref()
    }
}
