use crate::{
    Type, invalid,
    uapi::{Header, HeaderMagic, TypeId},
};
use std::{
    collections::HashMap,
    fs,
    io::{Error, ErrorKind, Result},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Btf {
    /// Indexed by id, so `types[0]` is [`Type::VOID`].
    types: Vec<Type>,
    strings: Box<[u8]>,
    /// Where the types came from
    source: Option<PathBuf>,
    by_name: HashMap<Box<str>, Vec<TypeId>>,
}

impl Btf {
    const KERNEL_BTF: &str = "/sys/kernel/btf/vmlinux";

    /// Types from bytes already in hand.
    ///
    /// # Errors
    ///
    /// If the bytes are not BTF, or a section of them lies outside them.
    pub fn new(bytes: &[u8]) -> Result<Self> {
        Self::load(bytes, None)
    }

    /// Types from a file, which its errors then name.
    ///
    /// # Errors
    ///
    /// If the file cannot be read, or does not hold BTF this build understands.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|err| with_path(err, Some(path)))?;

        Self::load(&bytes, Some(path.to_owned()))
    }

    /// The types the running kernel publishes for itself.
    ///
    /// # Errors
    ///
    /// If the kernel publishes none, or they do not parse.
    pub fn from_live_kernel() -> Result<Self> {
        let path = Path::new(Self::KERNEL_BTF);
        let bytes = fs::read(path).map_err(|err| {
            if err.kind() != ErrorKind::NotFound {
                return with_path(err, Some(path));
            }
            Error::new(
                ErrorKind::NotFound,
                format!(
                    "{} is missing: the kernel needs CONFIG_DEBUG_INFO_BTF, \
                         or load a matching BTF of its own",
                    Self::KERNEL_BTF
                ),
            )
        })?;

        Self::load(&bytes, Some(path.to_owned()))
    }

    fn load(bytes: &[u8], source: Option<PathBuf>) -> Result<Self> {
        let mut btf = Self::decode(bytes).map_err(|err| with_path(err, source.as_deref()))?;
        btf.source = source;

        Ok(btf)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let header = Header::read(bytes)?;

        // the sections are measured from the end of the header
        let body = bytes.get(header.hdr_len as usize..).ok_or_else(|| {
            let msg = format!("header length {} overflows", header.hdr_len);
            invalid(msg)
        })?;

        let types = section(body, header.type_off, header.type_len)
            .ok_or_else(|| invalid("the type section runs past the end".to_owned()))?;
        let strings = section(body, header.str_off, header.str_len)
            .ok_or_else(|| invalid("the string section runs past the end".to_owned()))?;
        if strings.first().is_some_and(|first| *first != 0) {
            let msg = "the string section does not open with the empty string";
            return Err(invalid(msg.to_owned()));
        }

        let mut btf = Self {
            types: Vec::new(),
            strings: strings.into(),
            source: None,
            by_name: HashMap::new(),
        };
        btf.parse(types, header.magic)?;
        Ok(btf)
    }

    /// Every entry of the type section, and then an index of the names they carry.
    ///
    /// # Errors
    ///
    /// If an entry is malformed, or is of a kind this build does not know.
    fn parse(&mut self, types: &[u8], magic: HeaderMagic) -> Result<()> {
        // entry 0 s always void
        self.types.push(Type::VOID);

        let mut rest = types;
        while !rest.is_empty() {
            let ty = Type::read(&mut rest, magic)?;

            let id = TypeId(self.types.len() as u32);
            self.index(id, &ty);
            self.types.push(ty);
        }

        Ok(())
    }

    /// Files a type under the hash of its name, if it has one to go by.
    fn index(&mut self, id: TypeId, ty: &Type) {
        let name = self.string_at(ty.name_off);
        if name.is_empty() {
            return;
        }
        let name = Box::<str>::from(name);
        self.by_name.entry(name).or_default().push(id);
    }

    /// Every id carrying this name, in the order the section declares them.
    #[must_use]
    pub fn find_all(&self, name: &str) -> &[TypeId] {
        self.by_name.get(name).map_or(&[], Vec::as_slice)
    }

    /// Where these types came from, for whatever has to explain itself to a user.
    #[must_use]
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// One past the highest id, so `1..len()` walks every type.
    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Whether the void at id 0 is all there is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.len() <= 1
    }

    /// The string at an offset, and empty when the offset is past the section or
    /// the bytes there are not UTF-8.
    #[must_use]
    pub fn string_at(&self, off: u32) -> &str {
        let Some(rest) = self.strings.get(off as usize..) else {
            return "";
        };
        let end = rest
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(rest.len());

        str::from_utf8(&rest[..end]).unwrap_or("")
    }
}

fn section(body: &[u8], off: u32, len: u32) -> Option<&[u8]> {
    if len == 0 {
        return Some(&[]);
    }
    let start = off as usize;
    let end = start + len as usize;
    body.get(start..end)
}

fn with_path(err: Error, source: Option<&Path>) -> Error {
    match source {
        Some(path) => Error::new(err.kind(), format!("{}: {err}", path.display())),
        None => err,
    }
}
