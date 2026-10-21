use std::io::{self, Read};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GeneratedPngHeader {
    bytes: [u8; 33],
    dimensions: [u32; 2],
}

#[derive(Debug)]
pub(super) enum HeaderError {
    Unsupported,
    Malformed,
    Read(io::Error),
}

impl GeneratedPngHeader {
    pub(super) fn read(mut source: impl Read) -> Result<Self, HeaderError> {
        let mut bytes = [0; 33];
        source.read_exact(&mut bytes[..8]).map_err(HeaderError::Read)?;
        if bytes[..8] != *b"\x89PNG\r\n\x1a\n" {
            return Err(HeaderError::Unsupported);
        }
        source.read_exact(&mut bytes[8..]).map_err(HeaderError::Read)?;
        if bytes[8..12] != 13u32.to_be_bytes() || bytes[12..16] != *b"IHDR" {
            return Err(HeaderError::Malformed);
        }
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        let depth = bytes[24];
        let colour = bytes[25];
        let legal_depth = match colour {
            0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
            2 | 4 | 6 => matches!(depth, 8 | 16),
            3 => matches!(depth, 1 | 2 | 4 | 8),
            _ => false,
        };
        if width == 0
            || height == 0
            || width > 0x7fff_ffff
            || height > 0x7fff_ffff
            || !legal_depth
            || bytes[26] != 0
            || bytes[27] != 0
            || bytes[28] > 1
            || crc32fast::hash(&bytes[12..29])
                != u32::from_be_bytes(bytes[29..33].try_into().unwrap())
        {
            return Err(HeaderError::Malformed);
        }
        Ok(Self {
            bytes,
            dimensions: [width, height],
        })
    }

    pub(super) fn dimensions(self) -> [u32; 2] {
        self.dimensions
    }

    pub(super) fn bind<R: Read>(self, source: R) -> HeaderBoundReader<R> {
        HeaderBoundReader {
            source,
            header: self.bytes,
            checked: 0,
            refused: false,
        }
    }
}

pub(super) struct HeaderBoundReader<R> {
    source: R,
    header: [u8; 33],
    checked: usize,
    refused: bool,
}

impl<R: Read> Read for HeaderBoundReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.refused {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "generated PNG header changed"));
        }
        if output.is_empty() {
            return Ok(0);
        }
        let count = self.source.read(output)?;
        let prefix = count.min(self.header.len() - self.checked);
        if (count == 0 && self.checked < self.header.len())
            || output[..prefix] != self.header[self.checked..self.checked + prefix]
        {
            self.refused = true;
            return Err(io::Error::new(io::ErrorKind::InvalidData, "generated PNG header changed"));
        }
        self.checked += prefix;
        Ok(count)
    }
}
