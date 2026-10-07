use super::sys;
use crate::{MAX_PAYLOAD_LEN, Nonce};
use std::io;
use std::os::unix::ffi::OsStrExt;
pub(super) struct SecretBytes(pub(super) Vec<u8>);
impl std::ops::Deref for SecretBytes {
    type Target = Vec<u8>;
    fn deref(&self) -> &Vec<u8> {
        &self.0
    }
}
impl std::ops::DerefMut for SecretBytes {
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}
impl Drop for SecretBytes {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}
pub(super) struct Context {
    pub uid: u32,
    pub gid: u32,
    pub groups: Vec<u32>,
    pub environment: Vec<(Vec<u8>, Vec<u8>)>,
}
impl Drop for Context {
    fn drop(&mut self) {
        for (name, value) in &mut self.environment {
            for byte in name.iter_mut().chain(value.iter_mut()) {
                unsafe { std::ptr::write_volatile(byte, 0) };
            }
        }
    }
}
impl Context {
    pub(super) fn capture() -> io::Result<Self> {
        let count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
        sys::check(count)?;
        if count > 256 {
            return Err(io::Error::other("group bound"));
        }
        let mut groups = vec![0; count as usize];
        sys::check(unsafe { libc::getgroups(count, groups.as_mut_ptr()) })?;
        let mut context = Self {
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            groups,
            environment: Vec::new(),
        };
        let mut retained = 48 + context.groups.len() * 4;
        for (name, value) in std::env::vars_os() {
            if context.environment.len() == 128
                || name.as_bytes().len() > 256
                || value.as_bytes().len() > 8192
                || retained + 8 + name.as_bytes().len() + value.as_bytes().len() > MAX_PAYLOAD_LEN
            {
                return Err(io::Error::other("environment field or aggregate bound"));
            }
            retained += 8 + name.as_bytes().len() + value.as_bytes().len();
            context
                .environment
                .push((name.as_bytes().to_vec(), value.as_bytes().to_vec()));
        }
        context.encode(&[0; 32])?;
        Ok(context)
    }
    pub(super) fn encode(&self, nonce: &Nonce) -> io::Result<SecretBytes> {
        let mut bytes = SecretBytes(nonce.to_vec());
        for value in [self.uid, self.gid, self.groups.len() as u32] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in &self.groups {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        if self.environment.len() > 128 {
            return Err(io::Error::other("environment bound"));
        }
        bytes.extend_from_slice(&(self.environment.len() as u32).to_le_bytes());
        for (name, value) in &self.environment {
            if name.is_empty()
                || name.len() > 256
                || name.contains(&0)
                || name.contains(&b'=')
                || value.len() > 8192
                || value.contains(&0)
            {
                return Err(io::Error::other("environment field bound"));
            }
            for field in [name, value] {
                bytes.extend_from_slice(&(field.len() as u32).to_le_bytes());
                bytes.extend_from_slice(field);
            }
            if bytes.len() > MAX_PAYLOAD_LEN {
                return Err(io::Error::other("context aggregate bound"));
            }
        }
        Ok(bytes)
    }
    pub(super) fn decode(nonce: &Nonce, bytes: &[u8]) -> io::Result<Self> {
        struct Input<'a>(&'a [u8]);
        impl<'a> Input<'a> {
            fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
                if self.0.len() < count {
                    return Err(io::Error::other("truncated context"));
                }
                let (bytes, rest) = self.0.split_at(count);
                self.0 = rest;
                Ok(bytes)
            }
            fn number(&mut self) -> io::Result<u32> {
                Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
            }
            fn bytes(&mut self, maximum: usize) -> io::Result<Vec<u8>> {
                let count = self.number()? as usize;
                if count > maximum {
                    return Err(io::Error::other("context field bound"));
                }
                Ok(self.take(count)?.to_vec())
            }
        }
        let mut input = Input(bytes);
        if input.take(32)? != nonce {
            return Err(io::Error::other("context nonce mismatch"));
        }
        let uid = input.number()?;
        let gid = input.number()?;
        let count = input.number()? as usize;
        if count > 256 {
            return Err(io::Error::other("group bound"));
        }
        let mut groups = Vec::with_capacity(count);
        for _ in 0..count {
            groups.push(input.number()?);
        }
        let count = input.number()? as usize;
        if count > 128 {
            return Err(io::Error::other("environment bound"));
        }
        let mut context = Self {
            uid,
            gid,
            groups,
            environment: Vec::with_capacity(count),
        };
        for _ in 0..count {
            let name = input.bytes(256)?;
            let value = input.bytes(8192)?;
            context.environment.push((name, value));
        }
        if !input.0.is_empty() {
            return Err(io::Error::other("trailing context"));
        }
        context.encode(nonce)?;
        Ok(context)
    }
}
