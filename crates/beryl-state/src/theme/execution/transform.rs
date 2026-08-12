use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, Read, Write},
    num::{NonZeroU64, NonZeroUsize},
    rc::Rc,
};

use beryl_home_store::{HomeStore, ThemeFileIdentity, ThemeFileSelector, ThemeRepositorySnapshot};
use sha2::{Digest, Sha256};

use super::super::{
    InstalledThemeId, InstalledThemeSummary, THEME_MANIFEST_HEADER_MAX_BYTES,
    THEME_MANIFEST_LINE_MAX_BYTES, THEME_MANIFEST_PAGE_MAX_DECODED_BYTES,
    THEME_MANIFEST_PAGE_MAX_ENCODED_BYTES, ThemeDocumentDigest, ThemeManifestCursor,
    ThemeManifestDecoder, ThemeManifestEncodeError, ThemeManifestEncoder, ThemeManifestGeneration,
    ThemeManifestIdentity, ThemeManifestReadLimits, ThemeName, ThemePageLimits, ThemeService,
    physical::{
        PhysicalThemeLimits, PhysicalThemeReadErrors, PhysicalThemeReader, physical_file_identity,
    },
};
use super::{ThemeCommandFactError, ThemeRepositoryExecutionError, fact};

#[derive(Clone, Debug)]
pub(super) enum ManifestChange {
    Append {
        id: InstalledThemeId,
        name: ThemeName,
        required_member: Option<InstalledThemeId>,
    },
    Rename {
        expected: InstalledThemeSummary,
        name: ThemeName,
    },
    Delete {
        expected: InstalledThemeSummary,
    },
    Reorder {
        expected: InstalledThemeSummary,
        new_order: u64,
    },
}

impl ManifestChange {
    fn transform<W: Write>(
        &self,
        row: InstalledThemeSummary,
        state: &mut TransformState<W>,
    ) -> Result<(), ThemeRepositoryExecutionError> {
        match self {
            Self::Append {
                id,
                required_member,
                ..
            } => {
                if row.id() == id {
                    return Err(fact(ThemeCommandFactError::ThemeAlreadyInstalled));
                }
                if required_member.as_ref() == Some(row.id()) {
                    state.found = true;
                }
                state.emit(row)
            }
            Self::Rename { expected, name } => {
                if row.id() == expected.id() {
                    require_expected_row(&row, expected)?;
                    state.found = true;
                    state.emit(InstalledThemeSummary::new(
                        row.id().clone(),
                        name.clone(),
                        row.order(),
                    ))
                } else {
                    state.emit(row)
                }
            }
            Self::Delete { expected } => {
                if row.id() == expected.id() {
                    require_expected_row(&row, expected)?;
                    state.found = true;
                    Ok(())
                } else {
                    let order = row
                        .order()
                        .checked_sub(u64::from(state.found))
                        .ok_or_else(|| fact(ThemeCommandFactError::ExpectedRowMismatch))?;
                    state.emit(InstalledThemeSummary::new(
                        row.id().clone(),
                        row.name().clone(),
                        order,
                    ))
                }
            }
            Self::Reorder {
                expected,
                new_order,
            } => transform_reorder(row, expected, *new_order, state),
        }
    }

    fn finish<W: Write>(
        &self,
        state: &mut TransformState<W>,
    ) -> Result<(), ThemeRepositoryExecutionError> {
        match self {
            Self::Append {
                id,
                name,
                required_member,
            } => {
                if required_member.is_some() && !state.found {
                    return Err(fact(ThemeCommandFactError::ThemeNotInstalled));
                }
                state.emit(InstalledThemeSummary::new(
                    id.clone(),
                    name.clone(),
                    state.next_output,
                ))
            }
            Self::Rename { .. } | Self::Delete { .. } => {
                if !state.found {
                    return Err(fact(ThemeCommandFactError::ThemeNotInstalled));
                }
                Ok(())
            }
            Self::Reorder {
                expected,
                new_order,
            } => {
                if !state.found {
                    return Err(fact(ThemeCommandFactError::ThemeNotInstalled));
                }
                if *new_order > expected.order() && !state.inserted {
                    return Err(fact(ThemeCommandFactError::NewOrderOutOfRange));
                }
                Ok(())
            }
        }
    }
}

fn transform_reorder<W: Write>(
    row: InstalledThemeSummary,
    expected: &InstalledThemeSummary,
    new_order: u64,
    state: &mut TransformState<W>,
) -> Result<(), ThemeRepositoryExecutionError> {
    let old_order = expected.order();
    if new_order < old_order && row.order() == new_order {
        state.emit(InstalledThemeSummary::new(
            expected.id().clone(),
            expected.name().clone(),
            new_order,
        ))?;
        state.inserted = true;
    }
    if row.id() == expected.id() {
        require_expected_row(&row, expected)?;
        state.found = true;
        if new_order == old_order {
            state.emit(row)?;
            state.inserted = true;
        }
        return Ok(());
    }
    let output_order = if new_order < old_order && (new_order..old_order).contains(&row.order()) {
        row.order().checked_add(1)
    } else if new_order > old_order && row.order() > old_order && row.order() <= new_order {
        row.order().checked_sub(1)
    } else {
        Some(row.order())
    }
    .ok_or_else(|| fact(ThemeCommandFactError::NewOrderOutOfRange))?;
    let input_order = row.order();
    state.emit(InstalledThemeSummary::new(
        row.id().clone(),
        row.name().clone(),
        output_order,
    ))?;
    if new_order > old_order && input_order == new_order {
        state.emit(InstalledThemeSummary::new(
            expected.id().clone(),
            expected.name().clone(),
            new_order,
        ))?;
        state.inserted = true;
    }
    Ok(())
}

fn require_expected_row(
    actual: &InstalledThemeSummary,
    expected: &InstalledThemeSummary,
) -> Result<(), ThemeRepositoryExecutionError> {
    if actual != expected {
        return Err(fact(ThemeCommandFactError::ExpectedRowMismatch));
    }
    Ok(())
}

struct TransformState<W: Write> {
    encoder: ThemeManifestEncoder<W>,
    next_output: u64,
    found: bool,
    inserted: bool,
}

impl<W: Write> TransformState<W> {
    fn new(
        writer: W,
        generation: ThemeManifestGeneration,
    ) -> Result<Self, ThemeRepositoryExecutionError> {
        Ok(Self {
            encoder: ThemeManifestEncoder::new(writer, generation)
                .map_err(ThemeRepositoryExecutionError::ManifestEncode)?,
            next_output: 0,
            found: false,
            inserted: false,
        })
    }

    fn emit(&mut self, row: InstalledThemeSummary) -> Result<(), ThemeRepositoryExecutionError> {
        if row.order() != self.next_output {
            return Err(fact(ThemeCommandFactError::ExpectedRowMismatch));
        }
        self.encoder
            .write_theme(&row)
            .map_err(ThemeRepositoryExecutionError::ManifestEncode)?;
        self.next_output = self.next_output.checked_add(1).ok_or(
            ThemeRepositoryExecutionError::ManifestEncode(ThemeManifestEncodeError::OrderExhausted),
        )?;
        Ok(())
    }

    fn finish(
        self,
    ) -> Result<(W, super::super::ThemeManifestEncoding), ThemeRepositoryExecutionError> {
        self.encoder
            .finish()
            .map_err(ThemeRepositoryExecutionError::ManifestEncode)
    }
}

enum ManifestInput<'store> {
    Empty {
        done: bool,
    },
    Present {
        decoder: ThemeManifestDecoder<PhysicalThemeReader<'store>>,
        errors: PhysicalThemeReadErrors,
        cursor: ThemeManifestCursor,
        done: bool,
    },
}

impl ManifestInput<'_> {
    fn next_row(&mut self) -> Result<Option<InstalledThemeSummary>, ThemeRepositoryExecutionError> {
        match self {
            Self::Empty { done } => {
                *done = true;
                Ok(None)
            }
            Self::Present {
                decoder,
                errors,
                cursor,
                done,
            } => {
                if *done {
                    return Ok(None);
                }
                let limits = page_limits()?;
                let page = decoder.read_page(*cursor, limits).map_err(|source| {
                    errors.take().map_or(
                        ThemeRepositoryExecutionError::ManifestDecode(source),
                        ThemeRepositoryExecutionError::Repository,
                    )
                })?;
                let row = page.records().first().cloned();
                match page.next() {
                    Some(next) => *cursor = next,
                    None => *done = true,
                }
                Ok(row)
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn open_manifest_input<'store>(
    service: &ThemeService,
    store: &'store HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    max_manifest_source: NonZeroU64,
    limits: PhysicalThemeLimits,
) -> Result<ManifestInput<'store>, ThemeRepositoryExecutionError> {
    match physical_manifest {
        None => {
            if manifest.generation() != ThemeManifestGeneration::INITIAL {
                return Err(fact(ThemeCommandFactError::PhysicalManifestMismatch));
            }
            Ok(ManifestInput::Empty { done: false })
        }
        Some(expected) => {
            let reader = PhysicalThemeReader::new(
                store,
                snapshot,
                ThemeFileSelector::Manifest,
                expected,
                limits,
            )
            .map_err(ThemeRepositoryExecutionError::Repository)?;
            let errors = reader.errors();
            let mut decoder =
                ThemeManifestDecoder::open(reader, service.home(), manifest_read_limits()?)
                    .map_err(ThemeRepositoryExecutionError::ManifestDecode)?;
            decoder
                .bind_identity(manifest)
                .map_err(ThemeRepositoryExecutionError::ManifestDecode)?;
            if expected.length() > max_manifest_source.get() {
                return Err(fact(ThemeCommandFactError::PhysicalManifestMismatch));
            }
            let cursor = decoder.first_cursor();
            Ok(ManifestInput::Present {
                decoder,
                errors,
                cursor,
                done: false,
            })
        }
    }
}

fn manifest_read_limits() -> Result<ThemeManifestReadLimits, ThemeRepositoryExecutionError> {
    ThemeManifestReadLimits::new(
        NonZeroUsize::new(THEME_MANIFEST_LINE_MAX_BYTES)
            .ok_or(ThemeRepositoryExecutionError::InvalidLimits)?,
        NonZeroUsize::new(THEME_MANIFEST_HEADER_MAX_BYTES)
            .ok_or(ThemeRepositoryExecutionError::InvalidLimits)?,
        NonZeroUsize::new(THEME_MANIFEST_PAGE_MAX_ENCODED_BYTES)
            .ok_or(ThemeRepositoryExecutionError::InvalidLimits)?,
    )
    .map_err(ThemeRepositoryExecutionError::ManifestDecode)
}

fn page_limits() -> Result<ThemePageLimits, ThemeRepositoryExecutionError> {
    ThemePageLimits::new(
        NonZeroUsize::new(1).ok_or(ThemeRepositoryExecutionError::InvalidLimits)?,
        NonZeroUsize::new(THEME_MANIFEST_PAGE_MAX_DECODED_BYTES)
            .ok_or(ThemeRepositoryExecutionError::InvalidLimits)?,
    )
    .map_err(|_| ThemeRepositoryExecutionError::InvalidLimits)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn hash_manifest_transform(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    generation: ThemeManifestGeneration,
    change: ManifestChange,
    max_manifest_source: NonZeroU64,
) -> Result<ThemeFileIdentity, ThemeRepositoryExecutionError> {
    let limits = PhysicalThemeLimits::manifest(max_manifest_source)
        .map_err(|_| ThemeRepositoryExecutionError::InvalidLimits)?;
    let mut input = open_manifest_input(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        max_manifest_source,
        limits,
    )?;
    let writer = HashingWriter::new(max_manifest_source.get());
    let mut state = TransformState::new(writer, generation)?;
    while let Some(row) = input.next_row()? {
        change.transform(row, &mut state)?;
    }
    change.finish(&mut state)?;
    let (writer, _) = state.finish()?;
    writer.identity()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn manifest_transform_reader<'store>(
    service: &ThemeService,
    store: &'store HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    generation: ThemeManifestGeneration,
    change: ManifestChange,
    max_manifest_source: NonZeroU64,
    limits: PhysicalThemeLimits,
) -> Result<ManifestTransformReader<'store>, ThemeRepositoryExecutionError> {
    let input = open_manifest_input(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        max_manifest_source,
        limits,
    )?;
    ManifestTransformReader::new(input, change, generation, max_manifest_source.get())
}

struct HashingWriter {
    hasher: Sha256,
    length: u64,
    maximum: u64,
}

impl HashingWriter {
    fn new(maximum: u64) -> Self {
        Self {
            hasher: Sha256::new(),
            length: 0,
            maximum,
        }
    }

    fn identity(self) -> Result<ThemeFileIdentity, ThemeRepositoryExecutionError> {
        Ok(physical_file_identity(
            self.length,
            ThemeDocumentDigest::from_bytes(self.hasher.finalize().into()),
        ))
    }
}

impl Write for HashingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let amount = u64::try_from(bytes.len())
            .map_err(|_| io::Error::other("manifest encoded length overflow"))?;
        let length = self
            .length
            .checked_add(amount)
            .ok_or_else(|| io::Error::other("manifest encoded length overflow"))?;
        if length > self.maximum {
            return Err(io::Error::other("manifest source limit exceeded"));
        }
        self.hasher.update(bytes);
        self.length = length;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct QueueWriter {
    bytes: Rc<RefCell<VecDeque<u8>>>,
    length: u64,
    maximum: u64,
}

impl Write for QueueWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let amount = u64::try_from(bytes.len())
            .map_err(|_| io::Error::other("manifest encoded length overflow"))?;
        let length = self
            .length
            .checked_add(amount)
            .ok_or_else(|| io::Error::other("manifest encoded length overflow"))?;
        if length > self.maximum {
            return Err(io::Error::other("manifest source limit exceeded"));
        }
        self.bytes.borrow_mut().extend(bytes.iter().copied());
        self.length = length;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) struct ManifestTransformReader<'store> {
    input: ManifestInput<'store>,
    change: ManifestChange,
    state: Option<TransformState<QueueWriter>>,
    queue: Rc<RefCell<VecDeque<u8>>>,
    finished: bool,
}

impl<'store> ManifestTransformReader<'store> {
    fn new(
        input: ManifestInput<'store>,
        change: ManifestChange,
        generation: ThemeManifestGeneration,
        maximum: u64,
    ) -> Result<Self, ThemeRepositoryExecutionError> {
        let queue = Rc::new(RefCell::new(VecDeque::new()));
        let writer = QueueWriter {
            bytes: Rc::clone(&queue),
            length: 0,
            maximum,
        };
        Ok(Self {
            input,
            change,
            state: Some(TransformState::new(writer, generation)?),
            queue,
            finished: false,
        })
    }

    fn pump(&mut self) -> Result<(), ThemeRepositoryExecutionError> {
        if self.finished {
            return Ok(());
        }
        let state = self
            .state
            .as_mut()
            .ok_or(ThemeRepositoryExecutionError::InvalidLimits)?;
        if let Some(row) = self.input.next_row()? {
            self.change.transform(row, state)?;
            return Ok(());
        }
        self.change.finish(state)?;
        let state = self
            .state
            .take()
            .ok_or(ThemeRepositoryExecutionError::InvalidLimits)?;
        let _ = state.finish()?;
        self.finished = true;
        Ok(())
    }
}

impl Read for ManifestTransformReader<'_> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if destination.is_empty() {
            return Ok(0);
        }
        while self.queue.borrow().is_empty() && !self.finished {
            self.pump().map_err(io::Error::other)?;
        }
        let mut queue = self.queue.borrow_mut();
        let count = destination.len().min(queue.len());
        for slot in &mut destination[..count] {
            *slot = queue.pop_front().expect("count is bounded by queue length");
        }
        Ok(count)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn require_member(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    id: &InstalledThemeId,
    max_manifest_source: NonZeroU64,
) -> Result<(), ThemeRepositoryExecutionError> {
    let limits = PhysicalThemeLimits::manifest(max_manifest_source)
        .map_err(|_| ThemeRepositoryExecutionError::InvalidLimits)?;
    let mut input = open_manifest_input(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        max_manifest_source,
        limits,
    )?;
    while let Some(row) = input.next_row()? {
        if row.id() == id {
            return Ok(());
        }
    }
    Err(fact(ThemeCommandFactError::ThemeNotInstalled))
}
