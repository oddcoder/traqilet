use crate::uapi::HeaderMagic;
use std::io::{Read, Result};

macro_rules! ints {
    ($($name:ident -> $int:ty),* $(,)?) => {
        $(
            #[doc = concat!("The next `", stringify!($int), "`, in the given order.")]
            fn $name(&mut self, order: HeaderMagic) -> Result<$int> {
                let bytes = self.read_bytes()?;
                Ok(match order {
                    HeaderMagic::LE => <$int>::from_le_bytes(bytes),
                    HeaderMagic::BE => <$int>::from_be_bytes(bytes),
                })
            }
        )*
    };
}

pub(crate) trait ReadExt: Read {
    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0; N];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_bytes::<1>()?[0])
    }

    ints! {
        read_u32 -> u32,
    }
}

impl<R: Read + ?Sized> ReadExt for R {}

#[cfg(test)]
mod tests {
    use super::ReadExt;
    use crate::uapi::HeaderMagic::{BE, LE};
    use std::io::ErrorKind;

    #[test]
    fn each_field_reads_either_way_round() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0x05];

        let mut le = &bytes[..];
        assert_eq!(le.read_u8().unwrap(), 0x01);
        assert_eq!(le.read_u32(LE).unwrap(), 0x0504_0302);

        let mut be = &bytes[..];
        assert_eq!(be.read_u32(BE).unwrap(), 0x0102_0304);
        assert_eq!(be.read_u8().unwrap(), 0x05);
    }

    #[test]
    fn a_field_the_file_ends_inside_is_unexpected_eof() {
        let err = (&[0x01, 0x02, 0x03][..]).read_u32(LE).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    /// A reader that hands out one byte at a time, as a pipe would.
    #[test]
    fn a_field_split_across_reads_is_still_one_field() {
        struct Trickle<'a>(&'a [u8]);
        impl std::io::Read for Trickle<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let one = 1.min(buf.len());
                self.0.read(&mut buf[..one])
            }
        }

        let bytes = 0xdead_beefu32.to_le_bytes();
        assert_eq!(Trickle(&bytes).read_u32(LE).unwrap(), 0xdead_beef);
    }
}
