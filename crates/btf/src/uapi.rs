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
#[derive(Debug, Clone, Copy)]
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

/// `struct btf_layout`
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub info_sz: u8,
    pub elem_sz: u8,
    pub flags: u16,
}

impl Layout {
    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let info_sz = reader.read_u8()?;
        let elem_sz = reader.read_u8()?;
        let flags = reader.read_u16(magic)?;

        Ok(Self {
            info_sz,
            elem_sz,
            flags,
        })
    }
}

/// A type id: which entry in the type section.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub u32);

impl TypeId {
    /// The void that no entry describes.
    pub const VOID: Self = Self(0);
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

/// The `u32` that follows a `BTF_KIND_INT`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct Int(pub u32);

impl Int {
    const ENCODING_SHIFT: u32 = 24;
    const ENCODING_MASK: u32 = 0x0f;
    const OFFSET_SHIFT: u32 = 16;
    const OFFSET_MASK: u32 = 0xff;
    const BITS_MASK: u32 = 0xff;
    const SIGNED: u32 = 1 << 0;
    const CHAR: u32 = 1 << 1;
    const BOOL: u32 = 1 << 2;

    /// Reads the encoding word.
    ///
    /// # Errors
    ///
    /// If the bytes run out.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        Ok(Self(reader.read_u32(magic)?))
    }

    /// How wide the integer actually is, which a bitfield makes smaller than the
    /// type's size.
    #[must_use]
    pub fn bits(&self) -> u32 {
        self.0 & Self::BITS_MASK
    }

    /// Where the bits start, for the bitfields the compiler describes this way
    /// rather than through a member offset.
    #[must_use]
    pub fn offset(&self) -> u32 {
        (self.0 >> Self::OFFSET_SHIFT) & Self::OFFSET_MASK
    }

    /// Signed, as opposed to plainly unsigned.
    #[must_use]
    pub fn is_signed(&self) -> bool {
        self.encoding() & Self::SIGNED != 0
    }

    /// Meant to be printed as a character.
    #[must_use]
    pub fn is_char(&self) -> bool {
        self.encoding() & Self::CHAR != 0
    }

    /// Meant to be read as a boolean.
    #[must_use]
    pub fn is_bool(&self) -> bool {
        self.encoding() & Self::BOOL != 0
    }

    fn encoding(self) -> u32 {
        (self.0 >> Self::ENCODING_SHIFT) & Self::ENCODING_MASK
    }
}

/// `struct btf_array`: the one record following a `BTF_KIND_ARRAY`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Array {
    pub elem_type_id: TypeId,
    pub index_type_id: TypeId,
    /// Zero for the flexible array member a struct can end with.
    pub nelems: u32,
}

impl Array {
    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let elem_type_id = TypeId(reader.read_u32(magic)?);
        let index_type_id = TypeId(reader.read_u32(magic)?);
        let nelems = reader.read_u32(magic)?;

        Ok(Self {
            elem_type_id,
            index_type_id,
            nelems,
        })
    }
}

/// `struct btf_member`: one per `vlen` after a `BTF_KIND_STRUCT` or `_UNION`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Member {
    pub name_off: u32,
    pub type_id: TypeId,
    /// A bit offset, and a bitfield width too when `kind_flag()` is true.
    pub offset: u32,
}

impl Member {
    const BITFIELD_SHIFT: u32 = 24;
    const BIT_OFFSET_MASK: u32 = 0x00ff_ffff;

    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let name_off = reader.read_u32(magic)?;
        let type_id = TypeId(reader.read_u32(magic)?);
        let offset = reader.read_u32(magic)?;

        Ok(Self {
            name_off,
            type_id,
            offset,
        })
    }

    /// Where the member starts. Only the low bits when the struct's kind flag is
    /// set; without it the whole word is the offset.
    #[must_use]
    pub fn bit_offset(&self, kind_flag: bool) -> u32 {
        if kind_flag {
            self.offset & Self::BIT_OFFSET_MASK
        } else {
            self.offset
        }
    }

    /// How wide a bitfield member is, and `None` for a member that is not one.
    ///
    /// Always `None` without the struct's kind flag, which is the older encoding
    /// that leaves the width to the member's own [`Int`] instead.
    #[must_use]
    pub fn bitfield_size(&self, kind_flag: bool) -> Option<u32> {
        Some(self.offset >> Self::BITFIELD_SHIFT).filter(|width| kind_flag && *width > 0)
    }
}

/// `struct btf_enum`: one per `vlen` after a `BTF_KIND_ENUM`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Enum {
    pub name_off: u32,
    pub val: i32,
}

impl Enum {
    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let name_off = reader.read_u32(magic)?;
        let val = reader.read_i32(magic)?;

        Ok(Self { name_off, val })
    }

    /// The value as a signed enum means it.
    #[must_use]
    pub fn signed(&self) -> i32 {
        self.val
    }

    /// The same bits as an unsigned enum means them, so 0xffffffff is four
    /// billion rather than minus one.
    #[must_use]
    pub fn unsigned(&self) -> u32 {
        self.val.cast_unsigned()
    }
}

/// `struct btf_enum64`: one per `vlen` after a `BTF_KIND_ENUM64`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Enum64 {
    pub name_off: u32,
    pub val_lo32: u32,
    pub val_hi32: u32,
}

impl Enum64 {
    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let name_off = reader.read_u32(magic)?;
        let val_lo32 = reader.read_u32(magic)?;
        let val_hi32 = reader.read_u32(magic)?;

        Ok(Self {
            name_off,
            val_lo32,
            val_hi32,
        })
    }

    /// The two halves as the one value they stand for, as an unsigned enum means
    /// it.
    #[must_use]
    pub fn unsigned(&self) -> u64 {
        u64::from(self.val_lo32) | (u64::from(self.val_hi32) << 32)
    }

    /// The same bits as a signed enum means them.
    #[must_use]
    pub fn signed(&self) -> i64 {
        self.unsigned().cast_signed()
    }
}

/// `struct btf_param`: one per `vlen` after a `BTF_KIND_FUNC_PROTO`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name_off: u32,
    pub type_id: TypeId,
}

impl Param {
    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let name_off = reader.read_u32(magic)?;
        let type_id = TypeId(reader.read_u32(magic)?);

        Ok(Self { name_off, type_id })
    }
}

/// `struct btf_var`: the one record following a `BTF_KIND_VAR`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Var {
    pub linkage: VarLinkage,
}

impl Var {
    /// # Errors
    ///
    /// If the bytes run out, or the linkage is one this build does not know.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let linkage_raw = reader.read_u32(magic)?;
        let linkage = VarLinkage::try_from(linkage_raw)?;

        Ok(Self { linkage })
    }
}

/// `BTF_VAR_*`: how far a variable is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VarLinkage {
    Static = 0,
    GlobalAllocated = 1,
    GlobalExtern = 2,
}

impl TryFrom<u32> for VarLinkage {
    type Error = Error;

    fn try_from(bits: u32) -> Result<Self> {
        match bits {
            0 => Ok(Self::Static),
            1 => Ok(Self::GlobalAllocated),
            2 => Ok(Self::GlobalExtern),
            other => Err(Error::new(
                ErrorKind::InvalidData,
                format!("unknown BTF variable linkage {other}"),
            )),
        }
    }
}

/// `enum btf_func_linkage`, which a `BTF_KIND_FUNC` keeps in `vlen` rather than
/// in a record of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FuncLinkage {
    Static = 0,
    Global = 1,
    Extern = 2,
}

impl TryFrom<u32> for FuncLinkage {
    type Error = Error;

    fn try_from(bits: u32) -> Result<Self> {
        Ok(match bits {
            0 => Self::Static,
            1 => Self::Global,
            2 => Self::Extern,
            other => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("unknown BTF function linkage {other}"),
                ));
            }
        })
    }
}

/// `struct btf_var_secinfo`: one per `vlen` after a `BTF_KIND_DATASEC`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VarSecInfo {
    pub type_id: TypeId,
    pub offset: u32,
    pub size: u32,
}

impl VarSecInfo {
    /// # Errors
    ///
    /// If the bytes run out part way.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let type_id = TypeId(reader.read_u32(magic)?);
        let offset = reader.read_u32(magic)?;
        let size = reader.read_u32(magic)?;

        Ok(Self {
            type_id,
            offset,
            size,
        })
    }
}

/// `struct btf_decl_tag`: the one record following a `BTF_KIND_DECL_TAG`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DeclTag {
    /// todo: figure out what is this thing ... but lets parse it eitherways
    pub component_idx: i32,
}

impl DeclTag {
    /// # Errors
    ///
    /// If the bytes run out.
    pub fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        Ok(Self {
            component_idx: reader.read_i32(magic)?,
        })
    }
}
