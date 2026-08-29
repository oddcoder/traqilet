use crate::{
    invalid,
    uapi::{
        Array as UArray, DeclTag as UDeclTag, Enum as UEnum, Enum64 as UEnum64, HeaderMagic, Int,
        Kind, Member as UMember, Param, Type as UType, TypeId, Var as UVar, VarLinkage, VarSecInfo,
    },
};
use std::io::{Read, Result};

/// One entry of the type section, as much of it as a type checker needs.
#[derive(Debug)]
pub struct Type {
    /// Where the name sits in the string section, and 0 for no name.
    pub name_off: u32,
    pub kind: TypeKind,
}

/// What an entry describes, and whatever that takes to say.
#[derive(Debug)]
pub enum TypeKind {
    /// Id 0, which no entry describes and every chain of ids ends at.
    Void,
    /// A pointer, which is one width whatever it points at.
    Ptr { target: TypeId },
    /// Another name for a type, which decides nothing about it.
    Typedef { target: TypeId },
    /// An integer, whose encoding says how wide it really is and how to read it.
    Int { size: u32, encoding: Int },
    /// A function, whose signature is the proto it names.
    Func { proto: TypeId },
    /// A const `target`
    Const { target: TypeId },
    /// A volatile `target`
    Volatile { target: TypeId },
    /// C's `restrict`: a promise that this pointer is the only way to reach what
    /// it points at. It exists for the compiler and decides nothing about layout.
    Restrict { target: TypeId },
    /// A string the compiler attached to a type, which sits in the chain where a
    /// qualifier would: `__user`, `__percpu`, `__rcu`, `address_space(1)`.
    TypeTag { target: TypeId },
    /// Forward declaration: `struct foo;` with no definition in this BTF.
    Fwd { union: bool },
    /// A floating point number, which BTF describes by its width alone.
    Float { size: u32 },
    /// A signature, which is anonymous and shared by every function having it.
    /// A variadic one ends with a param whose name and type are both zero.
    FuncProto { ret: TypeId, params: Vec<Param> },
    /// An array, whose length counts elements and not bytes, and is zero for the
    /// flexible member a struct can end with.
    Array {
        elem_type_id: TypeId,
        index_type_id: TypeId,
        nelems: u32,
    },
    /// A struct, whose members sit one after another.
    Struct { size: u32, members: Vec<Member> },
    /// A union, whose members all start at the same place.
    Union { size: u32, members: Vec<Member> },
    /// An unsigned enum.
    Enum {
        size: u32,
        values: Vec<EnumValue<u32>>,
    },
    /// A signed enum.
    SEnum {
        size: u32,
        values: Vec<EnumValue<i32>>,
    },
    /// An unsigned enum whose enumerators needed 64 bits.
    Enum64 {
        size: u32,
        values: Vec<EnumValue<u64>>,
    },
    /// A signed enum whose enumerators needed 64 bits.
    SEnum64 {
        size: u32,
        values: Vec<EnumValue<i64>>,
    },
    /// A global (maybe static?) variable.
    Var {
        type_id: TypeId,
        linkage: VarLinkage,
    },
    /// data section
    DataSec { size: u32, vars: Vec<VarSecInfo> },
    /// A string the compiler attached to a declaration, `component_idx` aiming it
    /// at one member or argument, or -1 at the declaration itself.
    DeclTag { target: TypeId, component_idx: i32 },
}

/// One enumerator of an enum, holding its value as wide and as signed as the enum
/// declared it.
#[derive(Debug)]
pub struct EnumValue<T> {
    pub name_off: u32,
    pub val: T,
}

/// One member of a struct or union, with the offset word already read apart.
#[derive(Debug)]
pub struct Member {
    pub name_off: u32,
    pub type_id: TypeId,
    /// Where the member starts, in bits.
    pub bit_offset: u32,
    /// How wide a bitfield member is, and `None` for one that is not a bitfield.
    pub bitfield_size: Option<u32>,
}

impl Type {
    /// Id 0, which no entry describes.
    pub const VOID: Self = Self {
        name_off: 0,
        kind: TypeKind::Void,
    };

    /// One entry, and whatever trailing records its kind carries after it.
    ///
    /// # Errors
    ///
    /// If the bytes run out, or the entry is of a kind this build cannot read.
    pub(crate) fn read<R: Read>(mut reader: R, magic: HeaderMagic) -> Result<Self> {
        let entry = UType::read(&mut reader, magic)?;
        let kind = TypeKind::read(&mut reader, magic, &entry)?;

        Ok(Self {
            name_off: entry.name_off,
            kind,
        })
    }
}

impl TypeKind {
    /// What an entry describes, and whatever trailing records that takes to say.
    ///
    /// # Errors
    ///
    /// If the bytes run out, or the entry is of a kind this build cannot read.
    fn read<R: Read>(mut reader: R, magic: HeaderMagic, entry: &UType) -> Result<Self> {
        let kind = match entry.kind()? {
            Kind::Unknown => {
                return Err(invalid("an entry has the void's own kind".to_owned()));
            }
            Kind::Ptr => Self::Ptr {
                target: TypeId(entry.size_or_type),
            },
            Kind::Typedef => Self::Typedef {
                target: TypeId(entry.size_or_type),
            },
            Kind::Int => Self::Int {
                size: entry.size_or_type,
                encoding: Int::read(&mut reader, magic)?,
            },
            Kind::Func => Self::Func {
                proto: TypeId(entry.size_or_type),
            },
            Kind::Const => Self::Const {
                target: TypeId(entry.size_or_type),
            },
            Kind::Volatile => Self::Volatile {
                target: TypeId(entry.size_or_type),
            },
            Kind::Restrict => Self::Restrict {
                target: TypeId(entry.size_or_type),
            },
            Kind::TypeTag => Self::TypeTag {
                target: TypeId(entry.size_or_type),
            },
            Kind::Fwd => Self::Fwd {
                union: entry.kind_flag(),
            },
            Kind::Float => Self::Float {
                size: entry.size_or_type,
            },
            Kind::Array => Self::array(&mut reader, magic)?,
            Kind::FuncProto => Self::func_proto(&mut reader, magic, entry)?,
            Kind::Struct => Self::struct_or_union(&mut reader, magic, entry, false)?,
            Kind::Union => Self::struct_or_union(&mut reader, magic, entry, true)?,
            Kind::Enum => Self::build_enum(&mut reader, magic, entry, false)?,
            Kind::Enum64 => Self::build_enum(&mut reader, magic, entry, true)?,
            Kind::Var => Self::var(&mut reader, magic, entry)?,
            Kind::DataSec => Self::data_sec(&mut reader, magic, entry)?,
            Kind::DeclTag => Self::decl_tag(&mut reader, magic, entry)?,
        };

        Ok(kind)
    }

    /// A variable, and the one record trailing it.
    ///
    /// # Errors
    ///
    /// If the bytes run out, or the linkage is one this build does not know.
    fn var<R: Read>(reader: R, magic: HeaderMagic, entry: &UType) -> Result<Self> {
        let uvar = UVar::read(reader, magic)?;

        Ok(Self::Var {
            type_id: TypeId(entry.size_or_type),
            linkage: uvar.linkage,
        })
    }

    /// A section, and one record per variable it holds.
    ///
    /// # Errors
    ///
    /// If the bytes run out part way.
    fn data_sec<R: Read>(mut reader: R, magic: HeaderMagic, entry: &UType) -> Result<Self> {
        let size = entry.size_or_type;
        let mut vars = Vec::new();
        for _ in 0..entry.vlen() {
            let var = VarSecInfo::read(&mut reader, magic)?;
            vars.push(var);
        }

        Ok(Self::DataSec { size, vars })
    }

    /// A tag, and the one record saying what it is on.
    ///
    /// # Errors
    ///
    /// If the bytes run out.
    fn decl_tag<R: Read>(reader: R, magic: HeaderMagic, entry: &UType) -> Result<Self> {
        let utag = UDeclTag::read(reader, magic)?;

        Ok(Self::DeclTag {
            target: TypeId(entry.size_or_type),
            component_idx: utag.component_idx,
        })
    }

    /// An array, and the one record trailing it. Its entry carries no size, the
    /// record having everything.
    ///
    /// # Errors
    ///
    /// If the bytes run out part way.
    fn array<R: Read>(reader: R, magic: HeaderMagic) -> Result<Self> {
        let uarray = UArray::read(reader, magic)?;

        Ok(Self::Array {
            elem_type_id: uarray.elem_type_id,
            index_type_id: uarray.index_type_id,
            nelems: uarray.nelems,
        })
    }

    /// A signature, and the params trailing it.
    ///
    /// # Errors
    ///
    /// If the bytes run out part way.
    fn func_proto<R: Read>(mut reader: R, magic: HeaderMagic, entry: &UType) -> Result<Self> {
        let ret = TypeId(entry.size_or_type);
        let mut params = Vec::new();
        for _ in 0..entry.vlen() {
            let param = Param::read(&mut reader, magic)?;
            params.push(param);
        }

        Ok(Self::FuncProto { ret, params })
    }

    /// One of the two, and the members trailing it.
    ///
    /// # Errors
    ///
    /// If the bytes run out part way.
    fn struct_or_union<R: Read>(
        mut reader: R,
        magic: HeaderMagic,
        entry: &UType,
        union: bool,
    ) -> Result<Self> {
        let bitfield = entry.kind_flag();
        let size = entry.size_or_type;

        // we do not preallocate because we do not trust the value
        // the file says.
        let mut members = Vec::new();

        for _ in 0..entry.vlen() {
            let umember = UMember::read(&mut reader, magic)?;
            let member = Member {
                name_off: umember.name_off,
                type_id: umember.type_id,
                bit_offset: umember.bit_offset(bitfield),
                bitfield_size: umember.bitfield_size(bitfield),
            };
            members.push(member);
        }

        Ok(if union {
            Self::Union { size, members }
        } else {
            Self::Struct { size, members }
        })
    }

    /// Whichever of the four an enum entry is: two widths, each signed or not.
    ///
    /// # Errors
    ///
    /// If the bytes run out part way.
    fn build_enum<R: Read>(
        mut reader: R,
        magic: HeaderMagic,
        entry: &UType,
        is_64: bool,
    ) -> Result<Self> {
        let size = entry.size_or_type;
        let vlen = entry.vlen();
        match (is_64, entry.kind_flag()) {
            (false, false) => Self::values(
                &mut reader,
                magic,
                size,
                vlen,
                |reader, magic| UEnum::read(reader, magic),
                |uenum| uenum.name_off,
                UEnum::unsigned,
                Self::r#enum,
            ),
            (false, true) => Self::values(
                &mut reader,
                magic,
                size,
                vlen,
                |reader, magic| UEnum::read(reader, magic),
                |uenum| uenum.name_off,
                UEnum::signed,
                Self::senum,
            ),
            (true, false) => Self::values(
                &mut reader,
                magic,
                size,
                vlen,
                |reader, magic| UEnum64::read(reader, magic),
                |uenum| uenum.name_off,
                UEnum64::unsigned,
                Self::enum64,
            ),
            (true, true) => Self::values(
                &mut reader,
                magic,
                size,
                vlen,
                |reader, magic| UEnum64::read(reader, magic),
                |uenum| uenum.name_off,
                UEnum64::signed,
                Self::senum64,
            ),
        }
    }

    fn r#enum(size: u32, values: Vec<EnumValue<u32>>) -> Self {
        Self::Enum { size, values }
    }

    fn senum(size: u32, values: Vec<EnumValue<i32>>) -> Self {
        Self::SEnum { size, values }
    }

    fn enum64(size: u32, values: Vec<EnumValue<u64>>) -> Self {
        Self::Enum64 { size, values }
    }

    fn senum64(size: u32, values: Vec<EnumValue<i64>>) -> Self {
        Self::SEnum64 { size, values }
    }

    /// I was bored, sorry
    ///
    /// # Errors
    ///
    /// If the bytes run out part way, or if you touch this code!
    #[expect(clippy::too_many_arguments, reason = "This one I like it!")]
    fn values<R: Read, S, T>(
        mut reader: R,
        magic: HeaderMagic,
        size: u32,
        vlen: u32,
        read: impl Fn(&mut R, HeaderMagic) -> Result<S>,
        name: impl Fn(&S) -> u32,
        val: impl Fn(&S) -> T,
        selfie: impl FnOnce(u32, Vec<EnumValue<T>>) -> Self,
    ) -> Result<Self> {
        let mut values = Vec::new();
        for _ in 0..vlen {
            let record = read(&mut reader, magic)?;
            let value = EnumValue {
                name_off: name(&record),
                val: val(&record),
            };
            values.push(value);
        }

        Ok(selfie(size, values))
    }
}
