pub(crate) mod parse;
pub(crate) mod serialize;

pub(crate) use parse::parse;
pub(crate) use serialize::serialize;

use defmt::Format;
use lexical_core::{FormattedSize, NumberFormatBuilder};

// Terminology: Given "$PMTK183*38\r\n", the line is "PMTK183"

const CHECKSUM_FORMAT: u128 = NumberFormatBuilder::hexadecimal();

#[derive(Format, Debug, Clone, PartialEq, Eq)]
pub struct Checksum(u8);

impl Checksum {
    fn new(val: u8) -> Self {
        Self(val)
    }

    fn parse(ascii: &[u8; 2]) -> Result<Self, lexical_core::Error> {
        lexical_core::parse_with_options::<u8, { CHECKSUM_FORMAT }>(ascii, &Default::default())
            .map(Self::new)
    }

    fn compute_for(line: &[u8]) -> Self {
        let mut num = 0;
        for char in line {
            num ^= char;
        }
        Self::new(num)
    }

    fn to_ascii(&self) -> [u8; 2] {
        let mut buf = [0_u8; u8::FORMATTED_SIZE];
        let out = lexical_core::write_with_options::<u8, { CHECKSUM_FORMAT }>(
            self.0,
            &mut buf,
            &Default::default(),
        );

        match out.len() {
            1 => [b'0', out[0]],
            2 => [out[0], out[1]],
            _ => unreachable!(),
        }
    }
}

#[cfg(all(test, feature = "host-test"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_checksum() {
        let actual = Checksum::parse(b"38").unwrap();
        assert_eq!(actual, Checksum::new(56));
    }

    #[test]
    fn test_checksum_to_ascii() {
        let actual = Checksum::new(56).to_ascii();
        assert_eq!(&actual, b"38");

        // check leading zero produced
        let actual = Checksum::new(15).to_ascii();
        assert_eq!(&actual, b"0F");
    }

    #[test]
    fn test_compute_checksum_for_line() {
        let actual = Checksum::compute_for(b"PMTK314,1,1,1,1,1,5,0,0,0,0,0,0,0,0,0,0,0,0,0");
        assert_eq!(actual, Checksum::parse(b"2C").unwrap());

        // Test something that requires zero-padding
        let actual = Checksum::compute_for(b"PMTK527,0.20");
        assert_eq!(actual, Checksum::parse(b"02").unwrap());
    }
}
