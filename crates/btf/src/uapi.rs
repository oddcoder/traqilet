//! BTF type encriched layer that is ABI compatiable with BTF file format
//!
//! `<https://github.com/torvalds/linux/commit/f7a6b9eaff3e6693ba3b19c5812e28538049bbf2>`
use crate::read::ReadExt;
use std::io::{Error, ErrorKind, Read, Result};

/// The first field of the header, and the byte order every field after it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum HeaderMagic {
    LE = 0xeb9f,
    BE = 0x9feb,
}

impl HeaderMagic {
    /// Reads the two byte magic.
    ///
    /// # Errors
    ///
    /// If the bytes run out, or they are not BTF's magic.
    pub fn read<R: Read>(mut reader: R) -> Result<Self> {
        const LE: u16 = HeaderMagic::LE as u16;
        const BE: u16 = HeaderMagic::BE as u16;

        let magic = reader.read_bytes()?;
        match u16::from_le_bytes(magic) {
            LE => Ok(Self::LE),
            BE => Ok(Self::BE),
            other => Err(Error::new(
                ErrorKind::InvalidData,
                format!("not BTF: magic is {other:#06x}, expected {LE:#06x}"),
            )),
        }
    }
}

#[repr(C)]
pub struct Header {
    pub magic: HeaderMagic,
    pub version: u8,
    pub flags: u8,
    pub hdr_len: u32,
    pub type_off: u32,
    pub type_len: u32,
    pub str_off: u32,
    pub str_len: u32,
    pub layout_off: u32,
    pub layout_len: u32,
}

impl Header {
    /// Reads the header
    ///
    /// # Errors
    ///
    /// If the bytes run out part way, or the magic is not BTF's.
    pub fn read<R: Read>(mut reader: R) -> Result<Self> {
        let magic = HeaderMagic::read(&mut reader)?;
        let version = reader.read_u8()?;
        let flags = reader.read_u8()?;
        let hdr_len = reader.read_u32(magic)?;
        let type_off = reader.read_u32(magic)?;
        let type_len = reader.read_u32(magic)?;
        let str_off = reader.read_u32(magic)?;
        let str_len = reader.read_u32(magic)?;
        let layout_off = reader.read_u32(magic)?;
        let layout_len = reader.read_u32(magic)?;

        Ok(Header {
            magic,
            version,
            flags,
            hdr_len,
            type_off,
            type_len,
            str_off,
            str_len,
            layout_off,
            layout_len,
        })
    }
}
