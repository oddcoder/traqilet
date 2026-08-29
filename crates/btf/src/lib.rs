//! BPF Type Format

mod btf;
mod read;
mod ty;
pub mod uapi;

pub use btf::Btf;
pub use ty::{EnumValue, Member, Type, TypeKind};

use std::io::{Error, ErrorKind};

pub(crate) fn invalid(msg: String) -> Error {
    Error::new(ErrorKind::InvalidData, msg)
}
