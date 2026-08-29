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

/// `BTF_KIND_*`: what an entry in the type section describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Kind {
    Unknown = 0,
    Int = 1,
    Ptr = 2,
    Array = 3,
    Struct = 4,
    Union = 5,
    Enum = 6,
    Fwd = 7,
    Typedef = 8,
    Volatile = 9,
    Const = 10,
    Restrict = 11,
    Func = 12,
    FuncProto = 13,
    Var = 14,
    DataSec = 15,
    Float = 16,
    DeclTag = 17,
    TypeTag = 18,
    Enum64 = 19,
}

impl TryFrom<u32> for Kind {
    type Error = Error;

    /// # Errors
    ///
    /// If the kind is newer than this build knows about.
    fn try_from(bits: u32) -> Result<Self> {
        Ok(match bits {
            0 => Self::Unknown,
            1 => Self::Int,
            2 => Self::Ptr,
            3 => Self::Array,
            4 => Self::Struct,
            5 => Self::Union,
            6 => Self::Enum,
            7 => Self::Fwd,
            8 => Self::Typedef,
            9 => Self::Volatile,
            10 => Self::Const,
            11 => Self::Restrict,
            12 => Self::Func,
            13 => Self::FuncProto,
            14 => Self::Var,
            15 => Self::DataSec,
            16 => Self::Float,
            17 => Self::DeclTag,
            18 => Self::TypeTag,
            19 => Self::Enum64,
            other => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("unknown BTF kind {other}"),
                ));
            }
        })
    }
}

/// `struct btf_type`: the twelve bytes every entry in the type section opens
/// with, before whatever its kind carries after it.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Type {
    pub name_off: u32,
    pub info: u32,
    pub size_or_type: u32,
}

impl Type {
    const KIND_SHIFT: u32 = 24;
    const KIND_MASK: u32 = 0x7f;
    const VLEN_MASK: u32 = 0x00ff_ffff;

    /// Reads one entry, less the trailing data its kind carries.
    ///
    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let name_off = reader.read_u32(magic)?;
        let info = reader.read_u32(magic)?;
        let size_or_type = reader.read_u32(magic)?;

        Ok(Self {
            name_off,
            info,
            size_or_type,
        })
    }

    /// The kind of this entry
    ///
    /// # Errors
    ///
    /// If the kind is newer than this build knows about.
    pub fn kind(&self) -> Result<Kind> {
        Kind::try_from((self.info >> Self::KIND_SHIFT) & Self::KIND_MASK)
    }

    /// How many members, parameters, enumerators or variables follow. Zero for the kinds that carry
    /// no list.
    #[must_use]
    pub fn vlen(&self) -> u32 {
        self.info & Self::VLEN_MASK
    }

    /// Used differently by struct, union, fwd, enum and enum64
    #[must_use]
    pub fn kind_flag(&self) -> bool {
        self.info >> 31 == 1
    }
}
