use super::*;
use crate::launch_cleanup::ManagedProcessOwner;
use crate::wsl_supervision::readers::WslGeneratedReader;
use std::io::Read;

pub struct GeneratedOutputSource {
    permit: GeneratedOutputPermit,
    cancellation: Arc<AtomicBool>,
    length: u64,
    backing: SourceBacking,
    closed: bool,
}
enum SourceBacking {
    Host {
        file: Arc<HostFile>,
        process: Arc<Mutex<std::process::Child>>,
    },
    Wsl(WslGeneratedReader),
}
pub struct GeneratedOutputCursor<'a> {
    source: &'a GeneratedOutputSource,
    offset: u64,
}
pub struct GeneratedOutputOpenFailure {
    error: GeneratedOutputError,
    source: Option<GeneratedOutputSource>,
}
pub struct GeneratedOutputCloseFailure {
    error: GeneratedOutputError,
    source: GeneratedOutputSource,
}

impl std::fmt::Debug for GeneratedOutputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedOutputSource")
            .field("length", &self.length)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for GeneratedOutputOpenFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedOutputOpenFailure")
            .field("error", &self.error)
            .field("retained_source", &self.source.is_some())
            .finish()
    }
}
impl std::fmt::Debug for GeneratedOutputCloseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedOutputCloseFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for GeneratedOutputOpenFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::fmt::Display for GeneratedOutputCloseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for GeneratedOutputOpenFailure {}
impl std::error::Error for GeneratedOutputCloseFailure {}
impl GeneratedOutputOpenFailure {
    pub fn error(&self) -> GeneratedOutputError {
        self.error
    }
    pub fn into_parts(self) -> (GeneratedOutputError, Option<GeneratedOutputSource>) {
        (self.error, self.source)
    }
    pub(crate) fn refused(error: GeneratedOutputError) -> Self {
        Self {
            error,
            source: None,
        }
    }
}
impl GeneratedOutputCloseFailure {
    pub fn error(&self) -> GeneratedOutputError {
        self.error
    }
    pub fn into_parts(self) -> (GeneratedOutputError, GeneratedOutputSource) {
        (self.error, self.source)
    }
}

pub(crate) fn open(
    permit: GeneratedOutputPermit,
    path: &RuntimeNativePath,
    allowance: GeneratedReaderAllowance,
    cancellation: Arc<AtomicBool>,
    process: &mut ManagedProcessOwner,
) -> Result<GeneratedOutputSource, GeneratedOutputOpenFailure> {
    let refused = GeneratedOutputOpenFailure::refused;
    if path.mode() != permit.launch.runtime_mode() || path.as_str().len() > 4096 {
        return Err(refused(GeneratedOutputError::InvalidPath));
    }
    if !permit.loaded.live.load(Ordering::Acquire)
        || permit.launch.source_files().retired.load(Ordering::Acquire)
    {
        return Err(refused(GeneratedOutputError::LostSourceAuthority));
    }
    if cancellation.load(Ordering::Acquire) {
        return Err(refused(GeneratedOutputError::Cancelled));
    }
    let _correlation = (
        &permit.target.resource,
        &permit.target.turn,
        &permit.target.item,
    );
    match process {
        ManagedProcessOwner::Host(process) => {
            if !host_path_is_native(path) {
                return Err(refused(GeneratedOutputError::InvalidPath));
            }
            let process = process
                .generated_source_witness()
                .ok_or_else(|| refused(GeneratedOutputError::LostSourceAuthority))?;
            if lock(&process)
                .try_wait()
                .map_err(|_| refused(GeneratedOutputError::Io))?
                .is_some()
            {
                return Err(refused(GeneratedOutputError::LostSourceAuthority));
            }
            let authority = permit.launch.source_files();
            let mut limit = lock(&authority.allowance);
            match *limit {
                Some(value) if value != allowance.0 => {
                    return Err(refused(GeneratedOutputError::Busy));
                }
                None => *limit = Some(allowance.0),
                _ => (),
            }
            let mut files = lock(&authority.files);
            files.retain(|file| file.strong_count() != 0);
            if files.len() >= allowance.0 as usize {
                return Err(refused(GeneratedOutputError::Busy));
            }
            let file = File::open(path.as_str()).map_err(|error| {
                refused(if error.kind() == io::ErrorKind::NotFound {
                    GeneratedOutputError::Missing
                } else {
                    GeneratedOutputError::Io
                })
            })?;
            let metadata = file
                .metadata()
                .map_err(|_| refused(GeneratedOutputError::Io))?;
            if !metadata.file_type().is_file() || metadata.len() == 0 {
                return Err(refused(GeneratedOutputError::Unsupported));
            }
            if authority.retired.load(Ordering::Acquire)
                || !permit.loaded.live.load(Ordering::Acquire)
            {
                return Err(refused(GeneratedOutputError::LostSourceAuthority));
            }
            let backing = Arc::new(HostFile {
                file: Mutex::new(Some(file)),
            });
            files.push(Arc::downgrade(&backing));
            let length = metadata.len();
            drop(files);
            drop(limit);
            Ok(GeneratedOutputSource {
                permit,
                cancellation,
                length,
                backing: SourceBacking::Host {
                    file: backing,
                    process,
                },
                closed: false,
            })
        }
        ManagedProcessOwner::Wsl(owner) => {
            let reader = owner.open_generated_reader(allowance).map_err(refused)?;
            let opened = reader.open(path.as_str(), &cancellation);
            let mut source = GeneratedOutputSource {
                permit,
                cancellation,
                length: 0,
                backing: SourceBacking::Wsl(reader),
                closed: false,
            };
            let opened = opened.and_then(|length| {
                source.active()?;
                Ok(length)
            });
            match opened {
                Ok(length) => {
                    source.length = length;
                    Ok(source)
                }
                Err(error) => match source.close_inner() {
                    Ok(()) => Err(refused(error)),
                    Err(_) => Err(GeneratedOutputOpenFailure {
                        error,
                        source: Some(source),
                    }),
                },
            }
        }
    }
}

pub(super) fn host_path_is_native(path: &RuntimeNativePath) -> bool {
    let native = if cfg!(target_os = "windows") {
        beryl_model::PathFlavor::Windows
    } else {
        beryl_model::PathFlavor::Posix
    };
    path.flavor() == native && std::path::Path::new(path.as_str()).is_absolute()
}

impl GeneratedOutputSource {
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn cursor(&self) -> GeneratedOutputCursor<'_> {
        GeneratedOutputCursor {
            source: self,
            offset: 0,
        }
    }
    pub fn close(mut self) -> Result<(), GeneratedOutputCloseFailure> {
        match self.close_inner() {
            Ok(()) => Ok(()),
            Err(error) => Err(GeneratedOutputCloseFailure {
                error,
                source: self,
            }),
        }
    }
    fn close_inner(&mut self) -> Result<(), GeneratedOutputError> {
        if self.closed {
            return Ok(());
        }
        match &self.backing {
            SourceBacking::Host { file, .. } => {
                lock(&file.file).take();
            }
            SourceBacking::Wsl(reader) => {
                reader.close(self.cancellation.load(Ordering::Acquire))?
            }
        }
        self.closed = true;
        Ok(())
    }
    fn active(&self) -> Result<(), GeneratedOutputError> {
        if self.closed {
            return Err(GeneratedOutputError::LostSourceAuthority);
        }
        if self.cancellation.load(Ordering::Acquire) {
            return Err(GeneratedOutputError::Cancelled);
        }
        if !self.permit.loaded.live.load(Ordering::Acquire)
            || self
                .permit
                .launch
                .source_files()
                .retired
                .load(Ordering::Acquire)
        {
            return Err(GeneratedOutputError::LostSourceAuthority);
        }
        if let SourceBacking::Host { process, .. } = &self.backing {
            if lock(process)
                .try_wait()
                .map_err(|_| GeneratedOutputError::Io)?
                .is_some()
            {
                return Err(GeneratedOutputError::LostSourceAuthority);
            }
        }
        if let SourceBacking::Wsl(reader) = &self.backing {
            if !reader.current() {
                return Err(GeneratedOutputError::LostSourceAuthority);
            }
        }
        Ok(())
    }
    fn read_at(&self, offset: u64, destination: &mut [u8]) -> Result<usize, GeneratedOutputError> {
        self.active()?;
        if destination.is_empty() {
            return Ok(0);
        }
        if offset > self.length {
            return Err(GeneratedOutputError::Changed);
        }
        let count = destination.len().min(beryl_wsl_supervisor::MAX_READER_PAGE);
        let destination = &mut destination[..count];
        let read = match &self.backing {
            SourceBacking::Host { file, .. } => {
                let file = lock(&file.file);
                let file = file
                    .as_ref()
                    .ok_or(GeneratedOutputError::LostSourceAuthority)?;
                host_read_at(file, destination, offset).map_err(|_| GeneratedOutputError::Io)?
            }
            SourceBacking::Wsl(reader) => reader.read(offset, destination, &self.cancellation)?,
        };
        self.active()?;
        if read as u64 > self.length - offset || (read == 0 && offset != self.length) {
            return Err(GeneratedOutputError::Changed);
        }
        Ok(read)
    }
}
impl Drop for GeneratedOutputSource {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}
impl Read for GeneratedOutputCursor<'_> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        let count = self
            .source
            .read_at(self.offset, destination)
            .map_err(GeneratedOutputError::into_io_error)?;
        self.offset = self
            .offset
            .checked_add(count as u64)
            .ok_or_else(|| GeneratedOutputError::Changed.into_io_error())?;
        Ok(count)
    }
}
#[cfg(target_os = "windows")]
fn host_read_at(file: &File, destination: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(destination, offset)
}
#[cfg(unix)]
fn host_read_at(file: &File, destination: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(destination, offset)
}
#[cfg(not(any(unix, target_os = "windows")))]
fn host_read_at(_file: &File, _destination: &mut [u8], _offset: u64) -> io::Result<usize> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}
