use beryl_home_store::CommandCancellation;
use std::io::{self, BufRead, Read, Seek, SeekFrom};

use super::{ImagePreparationError, check_cancelled, zeroed_bytes};

pub(super) struct PagedImageInput<'a, R> {
    inner: R,
    page: Vec<u8>,
    position: usize,
    filled: usize,
    cancelled: &'a CommandCancellation,
}

impl<'a, R> PagedImageInput<'a, R> {
    pub(super) fn new(
        inner: R,
        page_bytes: usize,
        cancelled: &'a CommandCancellation,
    ) -> Result<Self, ImagePreparationError> {
        Ok(Self {
            inner,
            page: zeroed_bytes(page_bytes)?,
            position: 0,
            filled: 0,
            cancelled,
        })
    }
}

impl<R: Read> BufRead for PagedImageInput<'_, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        check_cancelled(self.cancelled).map_err(io::Error::other)?;
        if self.position == self.filled {
            self.filled = self.inner.read(&mut self.page)?;
            self.position = 0;
        }
        Ok(&self.page[self.position..self.filled])
    }

    fn consume(&mut self, amount: usize) {
        self.position = self.position.saturating_add(amount).min(self.filled);
    }
}

impl<R: Read> Read for PagedImageInput<'_, R> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(destination.len());
        destination[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl<R: Read + Seek> Seek for PagedImageInput<'_, R> {
    fn seek(&mut self, requested: SeekFrom) -> io::Result<u64> {
        check_cancelled(self.cancelled).map_err(io::Error::other)?;
        let requested = match requested {
            SeekFrom::Current(offset) => {
                let buffered =
                    i64::try_from(self.filled - self.position).map_err(io::Error::other)?;
                SeekFrom::Current(offset.checked_sub(buffered).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "image seek overflow")
                })?)
            }
            offset => offset,
        };
        let position = self.inner.seek(requested)?;
        self.position = 0;
        self.filled = 0;
        Ok(position)
    }
}
