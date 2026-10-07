use sha2::{Digest, Sha256};
use std::io::{self, Read, Seek, SeekFrom};

const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

pub(super) fn digest(file: &mut (impl Read + Seek)) -> io::Result<[u8; 32]> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; 64];
    file.read_exact(&mut header)?;
    if &header[..4] != b"\x7fELF"
        || header[4..7] != [2, 1, 1]
        || u16::from_le_bytes([header[16], header[17]]) != 2
        || u16::from_le_bytes([header[18], header[19]]) != 62
        || u16::from_le_bytes([header[54], header[55]]) != 56
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported WSL supervisor executable",
        ));
    }
    let table = u64::from_le_bytes(header[32..40].try_into().unwrap());
    let count = u16::from_le_bytes([header[56], header[57]]);
    if count == 0
        || count > 128
        || table < 64
        || table
            .checked_add(u64::from(count) * 56)
            .is_none_or(|end| end > MAX_ARTIFACT_BYTES)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid ELF program table",
        ));
    }
    file.seek(SeekFrom::Start(table))?;
    for _ in 0..count {
        let mut entry = [0; 56];
        file.read_exact(&mut entry)?;
        if u32::from_le_bytes(entry[..4].try_into().unwrap()) == 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "WSL supervisor requires a dynamic interpreter",
            ));
        }
    }
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_ARTIFACT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "WSL supervisor artifact exceeds release bound",
            ));
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

pub(super) fn linux_path(path: &str) -> Option<String> {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    let bytes = path.as_bytes();
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }
    let mut result = format!("/mnt/{}", char::from(bytes[0].to_ascii_lowercase()));
    for component in path[3..].split('\\') {
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.contains(['\0', '/', ':'])
        {
            return None;
        }
        result.push('/');
        result.push_str(component);
    }
    Some(result)
}
